//! WMI queries and no-input methods, with connections owned by their calling thread.
//!
//! Backend: the `wmi` crate (exec_query, exec_method, get_property). Raw `IWbemClassObject::Get`
//! only keeps array CIM types (empty arrays). The crate waits with WBEM_INFINITE, so the
//! provider deadline in `hw::collect_all` bounds every call.

use super::{Error, Result, record};
use ::wmi::{IWbemClassWrapper, Variant, WMIConnection, WMIError};
use std::{cell::RefCell, collections::HashMap};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, RPC_E_TOO_LATE};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx, CoInitializeSecurity,
    CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::{Variant::VARIANT, Wmi::*};
use windows::core::{HRESULT, HSTRING, PCWSTR};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Namespace {
    Cimv2,
    Wmi,
    MicrosoftTpm,
    Storage,
}

impl Namespace {
    fn path(self) -> &'static str {
        match self {
            Self::Cimv2 => r"root\cimv2",
            Self::Wmi => r"root\wmi",
            Self::MicrosoftTpm => r"root\cimv2\Security\MicrosoftTpm",
            Self::Storage => r"root\Microsoft\Windows\Storage",
        }
    }
}

#[derive(Debug, Default)]
pub struct Row {
    // Preserve the WMI property order for method-output Boolean fallbacks.
    _values: Vec<(String, Variant)>,
    // Variant::Array loses its element type when empty; retain the CIM metadata.
    array_types: HashMap<String, &'static str>,
}

struct Apartment;

impl Apartment {
    fn new() -> Result<Self> {
        // SAFETY: Reserved argument is null; the thread-local guard balances success.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let initialized = if initialized == RPC_E_CHANGED_MODE {
            // Retain the caller's STA until cached proxies have been released.
            // SAFETY: Reserved argument is null; this successful init is balanced by Drop.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        } else {
            initialized
        };
        initialized
            .ok()
            .map_err(|e| Error::from_win("CoInitializeEx", e))?;
        let apartment = Self;
        // SAFETY: COM is initialized; reserved arguments are null and security is default.
        if let Err(error) = unsafe {
            CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_DEFAULT,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
                None,
            )
        } {
            // A host/main-thread security policy is already effective in this case.
            if error.code() != RPC_E_TOO_LATE {
                return Err(Error::from_win("CoInitializeSecurity", error));
            }
        }
        Ok(apartment)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: The guard remains on its initializing thread and owns one COM init.
        unsafe { CoUninitialize() };
    }
}

#[derive(Default)]
struct Connections {
    // Field order releases every COM proxy before balancing the apartment.
    namespaces: HashMap<Namespace, WMIConnection>,
    apartment: Option<Apartment>,
}

thread_local! {
    static CONNECTIONS: RefCell<Connections> = RefCell::new(Connections::default());
}

fn with_connection<T>(
    ns: Namespace,
    action: impl FnOnce(&WMIConnection) -> Result<T>,
) -> Result<T> {
    CONNECTIONS.with(|cache| {
        let mut cache = cache
            .try_borrow_mut()
            .map_err(|e| Error::msg("WMI connection", e.to_string()))?;
        if cache.apartment.is_none() {
            // Avoid the crate's unbalanced CoIncrementMTAUsage cookie on each new worker.
            cache.apartment = Some(Apartment::new()?);
        }
        let connection = match cache.namespaces.entry(ns) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let connection = WMIConnection::with_namespace_path(ns.path())
                    .map_err(|e| wmi_error("WMI ConnectServer", e))?;
                entry.insert(connection)
            }
        };
        action(connection)
    })
}

fn wmi_error(op: &'static str, error: WMIError) -> Error {
    match error {
        WMIError::HResultError { hres } => {
            Error::from_win(op, windows::core::Error::from_hresult(HRESULT(hres)))
        }
        other => Error::msg(op, other.to_string()),
    }
}

/// Queries a namespace with WQL using a connection local to the calling thread.
pub fn query(_ns: Namespace, _wql: &str) -> Result<Vec<Row>> {
    with_connection(_ns, |connection| {
        // raw_query<HashMap<...>> omits __PATH and discards empty-array CIM types.
        // Use the same enumerator/property reader, retaining both for callers.
        connection
            .exec_query(_wql)
            .map_err(|e| wmi_error("WMI query", e))?
            .map(|object| Row::from_object(object.map_err(|e| wmi_error("WMI Next", e))?, true))
            .collect()
    })
}

/// Calls a no-input WMI method and rejects a nonzero ReturnValue.
pub fn call_method(_ns: Namespace, _object_path: &str, _method: &str) -> Result<Row> {
    let object_path = _object_path;
    let method = _method;
    with_connection(_ns, |connection| {
        // C# parity: Hardware/TpmInfo.cs:116-118 (no-input method, out parameters).
        let output = connection
            .exec_method(object_path, method, None)
            .map_err(|e| wmi_error("WMI ExecMethod", e))?
            .ok_or_else(|| {
                Error::msg("WMI ExecMethod", format!("{method}: no output parameters"))
            })?;
        let value = output
            .get_property("ReturnValue")
            .map_err(|e| wmi_error("WMI ReturnValue", e))?;
        check_return_value(&value, method)?;
        Row::from_object(output, false)
    })
}

/// Calls an input-aware method on a thread-local connection; preserves input CIM types.
/// The caller must bound its wait (the underlying COM method is synchronous).
pub fn call_method_with_inputs(
    ns: Namespace,
    class: &str,
    object_path: &str,
    method: &str,
    inputs: &[(&str, Variant)],
) -> Result<Row> {
    with_connection(ns, |connection| {
        // GetMethod requires a class definition, not the instance object.
        let signature = connection
            .get_object(class)
            .and_then(|class| class.get_method(method))
            .map_err(|e| wmi_error("WMI method signature", e))?
            .ok_or_else(|| Error::msg("WMI method signature", "no input parameters"))?;
        let parameters = signature
            .spawn_instance()
            .map_err(|e| wmi_error("WMI method inputs", e))?;
        for (name, value) in inputs {
            parameters
                .put_property(name, value.clone())
                .map_err(|e| wmi_error("WMI method input", e))?;
        }
        let output = connection
            .exec_method(object_path, method, Some(&parameters))
            .map_err(|e| wmi_error("WMI ExecMethod", e))?
            .ok_or_else(|| Error::msg("WMI ExecMethod", "no output parameters"))?;
        // Some native providers expose a void method with output parameters even
        // when their documentation shows uint32. On the dev machine this monitor
        // method has only BlockContent/BlockType; CIM synthesizes ReturnValue = 0.
        // COM failure is propagated, and an explicit ReturnValue is checked.
        if output
            .list_properties()
            .map_err(|e| wmi_error("WMI GetNames", e))?
            .iter()
            .any(|name| name.eq_ignore_ascii_case("ReturnValue"))
        {
            let value = output
                .get_property("ReturnValue")
                .map_err(|e| wmi_error("WMI ReturnValue", e))?;
            check_return_value(&value, method)?;
        }
        Row::from_object(output, false)
    })
    .map_err(|mut error| {
        // Conversion errors from input-aware methods must never echo supplied IDs.
        error.detail = "input-aware WMI method failed".into();
        error
    })
}

fn check_return_value(value: &Variant, method: &str) -> Result<()> {
    let code = unsigned(value)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            Error::msg(
                "WMI ReturnValue",
                format!("{method}: missing or invalid UInt32 ReturnValue"),
            )
        })?;
    if code != 0 {
        return Err(Error {
            op: "WMI ExecMethod",
            code,
            detail: format!("{method} returned a nonzero ReturnValue"),
        });
    }
    Ok(())
}

fn unsigned(value: &Variant) -> Option<u64> {
    match value {
        Variant::UI1(n) => Some(u64::from(*n)),
        Variant::UI2(n) => Some(u64::from(*n)),
        Variant::UI4(n) => Some(u64::from(*n)),
        Variant::UI8(n) => Some(*n),
        Variant::I1(n) => u64::try_from(*n).ok(),
        Variant::I2(n) => u64::try_from(*n).ok(),
        Variant::I4(n) => u64::try_from(*n).ok(),
        Variant::I8(n) => u64::try_from(*n).ok(),
        Variant::String(n) => n.trim().parse().ok(),
        _ => None,
    }
}

fn array_type(object: &IWbemClassWrapper, name: &str) -> Result<&'static str> {
    let name = HSTRING::from(name);
    let mut value = VARIANT::default();
    let mut kind = 0;
    // SAFETY: object is a live COM proxy, name is terminated, outputs are initialized;
    // VARIANT's Drop clears its allocation even when Get/conversion fails.
    unsafe {
        object
            .inner
            .Get(PCWSTR(name.as_ptr()), 0, &mut value, Some(&mut kind), None)
    }
    .map_err(|e| Error::from_win("WMI array type", e))?;
    let kind = CIMTYPE_ENUMERATION(kind & !CIM_FLAG_ARRAY.0);
    Ok(match kind {
        CIM_SINT8 => "System.SByte[]",
        CIM_SINT16 => "System.Int16[]",
        CIM_SINT32 => "System.Int32[]",
        CIM_SINT64 => "System.Int64[]",
        CIM_UINT8 => "System.Byte[]",
        CIM_UINT16 => "System.UInt16[]",
        CIM_UINT32 => "System.UInt32[]",
        CIM_UINT64 => "System.UInt64[]",
        CIM_REAL32 => "System.Single[]",
        CIM_REAL64 => "System.Double[]",
        CIM_BOOLEAN => "System.Boolean[]",
        CIM_CHAR16 => "System.Char[]",
        CIM_STRING | CIM_DATETIME | CIM_REFERENCE => "System.String[]",
        CIM_OBJECT => "System.Management.ManagementBaseObject[]",
        _ => {
            return Err(Error::msg(
                "WMI array type",
                format!("unsupported CIM array type {}", kind.0),
            ));
        }
    })
}

impl Row {
    fn from_object(object: IWbemClassWrapper, paths: bool) -> Result<Self> {
        let mut row = Self::default();
        let mut names = object
            .list_properties()
            .map_err(|e| wmi_error("WMI GetNames", e))?;
        if paths {
            names.extend(["__PATH".into(), "__RELPATH".into()]);
        }
        for name in names {
            let value: Result<Variant> = (|| {
                let value = object
                    .get_property(&name)
                    .map_err(|e| wmi_error("WMI Get", e))?;
                if matches!(value, Variant::Array(_)) {
                    row.array_types
                        .insert(name.clone(), array_type(&object, &name)?);
                }
                Ok(value)
            })();
            match value {
                Ok(value) => row._values.push((name, value)),
                Err(mut error) => {
                    error.detail = format!("{name}: {}", error.detail);
                    record(error);
                }
            }
        }
        Ok(row)
    }

    fn property(&self, name: &str) -> Option<(&str, &Variant)> {
        self._values
            .iter()
            .find(|(key, _)| key == name)
            .or_else(|| {
                self._values
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
            })
            .map(|(key, value)| (key.as_str(), value))
    }

    /// Returns .NET ToString text, including `Some("")`; None means null or absent.
    pub fn str(&self, _name: &str) -> Option<String> {
        let (name, value) = self.property(_name)?;
        // C# parity: Hardware/RamInfo.cs:75-79 (untrimmed .NET ToString values).
        Some(match value {
            Variant::Null | Variant::Empty => return None,
            Variant::String(s) => s.clone(),
            Variant::Bool(b) => if *b { "True" } else { "False" }.into(),
            Variant::I1(n) => n.to_string(),
            Variant::I2(n) => n.to_string(),
            Variant::I4(n) => n.to_string(),
            Variant::I8(n) => n.to_string(),
            Variant::UI1(n) => n.to_string(),
            Variant::UI2(n) => n.to_string(),
            Variant::UI4(n) => n.to_string(),
            Variant::UI8(n) => n.to_string(),
            Variant::R4(n) => net_float(
                n.to_string(),
                format!("{n:e}"),
                *n != 0.0 && (n.abs() < 1e-4 || n.abs() >= 1e9),
            ),
            Variant::R8(n) => net_float(
                n.to_string(),
                format!("{n:e}"),
                *n != 0.0 && (n.abs() < 1e-4 || n.abs() >= 1e17),
            ),
            Variant::Array(_) => self
                .array_types
                .get(name)
                .copied()
                .unwrap_or("System.Object[]")
                .into(),
            Variant::Unknown(_) => "System.__ComObject".into(),
            Variant::Object(_) => "System.Management.ManagementBaseObject".into(),
        })
    }
    /// Reads an unsigned 32-bit property without truncation.
    pub fn u32(&self, _name: &str) -> Option<u32> {
        self.u64(_name).and_then(|n| u32::try_from(n).ok())
    }
    /// Reads an unsigned 64-bit property without truncation.
    pub fn u64(&self, _name: &str) -> Option<u64> {
        unsigned(self.property(_name)?.1)
    }
    /// Reads a boolean property.
    pub fn bool(&self, _name: &str) -> Option<bool> {
        match self.property(_name)?.1 {
            Variant::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// Returns the first Boolean in WMI order, excluding a name without case sensitivity.
    pub fn first_bool_except(&self, excluded: &str) -> Option<bool> {
        // C# parity: Hardware/TpmInfo.cs:131-136 (Boolean out-parameter fallback).
        self._values.iter().find_map(|(name, value)| {
            if name.eq_ignore_ascii_case(excluded) {
                return None;
            }
            match value {
                Variant::Bool(value) => Some(*value),
                _ => None,
            }
        })
    }
    /// Reads a UInt16 array, including WmiMonitorID text arrays.
    pub fn u16_array(&self, _name: &str) -> Option<Vec<u16>> {
        let (name, value) = self.property(_name)?;
        if self
            .array_types
            .get(name)
            .is_some_and(|kind| *kind != "System.UInt16[]")
        {
            return None;
        }
        match value {
            // C# parity: Hardware/MonitorInfo.cs:63 (UInt16[] kept intact, including NULs).
            Variant::Array(values) => values
                .iter()
                .map(|v| match v {
                    Variant::UI2(n) => Some(*n),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    /// Reads only a CIM UInt8 array; never coerces integers, strings or mixed arrays.
    pub fn u8_array(&self, name: &str) -> Option<Vec<u8>> {
        let (name, value) = self.property(name)?;
        if self.array_types.get(name) != Some(&"System.Byte[]") {
            return None;
        }
        match value {
            Variant::Array(values) => values
                .iter()
                .map(|v| match v {
                    Variant::UI1(n) => Some(*n),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }
}

fn net_float(fixed: String, scientific: String, exponential: bool) -> String {
    // C# parity: HWIDChecker.csproj:17 (invariant .NET general-number formatting).
    match fixed.as_str() {
        "inf" => return "Infinity".into(),
        "-inf" => return "-Infinity".into(),
        _ => {}
    }
    if exponential && let Some((mantissa, exponent)) = scientific.split_once('e') {
        let (sign, digits) = match exponent.strip_prefix('-') {
            Some(digits) => ('-', digits),
            None => ('+', exponent),
        };
        return format!("{mantissa}E{sign}{digits:0>2}");
    }
    fixed
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Apartment, Namespace, Row, call_method, check_return_value, query};
    use ::wmi::{Variant, WMIConnection};
    use std::{collections::HashMap, sync::mpsc, thread, time::Duration};

    // The crate uses WBEM_INFINITE. Keep live spike failures bounded independently
    // of the application provider deadline, without launching the elevated exe.
    fn live(check: impl FnOnce() -> std::result::Result<String, String> + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _apartment = Apartment::new().unwrap();
            tx.send(check()).unwrap();
        });
        println!(
            "{}",
            rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap()
        );
    }

    #[test]
    #[ignore = "live spike requires the Windows hardware providers; works without admin"]
    fn spike_plain_namespaces() {
        for (ns, wql) in [
            (r"root\cimv2", "SELECT Caption FROM Win32_OperatingSystem"),
            (r"root\wmi", "SELECT UserFriendlyName FROM WmiMonitorID"),
            (
                r"root\cimv2\Security\MicrosoftTpm",
                "SELECT SpecVersion FROM Win32_Tpm",
            ),
            (
                r"root\Microsoft\Windows\Storage",
                "SELECT DeviceId FROM MSFT_PhysicalDisk",
            ),
        ] {
            live(move || {
                let result = WMIConnection::with_namespace_path(ns)
                    .and_then(|con| con.raw_query::<HashMap<String, Variant>>(wql));
                match result {
                    Ok(rows) => Ok(format!("{ns}: {} row(s)", rows.len())),
                    Err(::wmi::WMIError::HResultError { hres })
                        if ns.ends_with("MicrosoftTpm")
                            && [0x80041003_u32, 0x80070005].contains(&(hres as u32)) =>
                    {
                        Ok(format!(
                            "{ns}: access denied without admin (0x{:08X})",
                            hres as u32
                        ))
                    }
                    Err(error) => Err(format!("{ns}: {error}")),
                }
            });
        }
    }

    #[test]
    #[ignore = "live spike requires an attached WMI monitor; works without admin"]
    fn spike_monitor_uint16_array() {
        live(|| {
            let con = WMIConnection::with_namespace_path(r"root\wmi").map_err(|e| e.to_string())?;
            let rows: Vec<HashMap<String, Variant>> = con
                .raw_query("SELECT UserFriendlyName FROM WmiMonitorID")
                .map_err(|e| e.to_string())?;
            if rows.is_empty() {
                return Err("no monitor available for UInt16[] spike".into());
            }
            for row in &rows {
                match row.get("UserFriendlyName") {
                    Some(Variant::Array(values))
                        if values.iter().all(|v| matches!(v, Variant::UI2(_))) => {}
                    _ => return Err("UserFriendlyName did not arrive as Array<UI2>".into()),
                }
            }
            Ok(format!("UInt16[]: {} monitor array(s) decoded", rows.len()))
        });
    }

    #[test]
    fn spike_invalid_class_and_namespace() {
        live(|| {
            let con = WMIConnection::new().map_err(|e| e.to_string())?;
            let error = con
                .raw_query::<HashMap<String, Variant>>(
                    "SELECT * FROM HWIDChecker_FabricatedMissingClass",
                )
                .unwrap_err();
            assert!(
                matches!(error, ::wmi::WMIError::HResultError { hres } if hres as u32 == 0x80041010)
            );
            let error =
                WMIConnection::with_namespace_path(r"root\HWIDChecker_FabricatedMissingNamespace")
                    .unwrap_err();
            assert!(
                matches!(error, ::wmi::WMIError::HResultError { hres } if hres as u32 == 0x8004100E)
            );
            Ok("invalid class and namespace: prompt HRESULT errors".into())
        });
    }

    #[test]
    #[ignore = "requires administrator privileges and an installed TPM"]
    fn spike_tpm_method_out_parameters_admin() {
        live(|| {
            let con = WMIConnection::with_namespace_path(r"root\cimv2\Security\MicrosoftTpm")
                .map_err(|e| e.to_string())?;
            let object = con
                .exec_query("SELECT * FROM Win32_Tpm")
                .map_err(|e| e.to_string())?
                .next()
                .ok_or("no TPM")?
                .map_err(|e| e.to_string())?;
            let path = object.path().map_err(|e| e.to_string())?;
            let output = con
                .exec_method(path, "IsEnabled", None)
                .map_err(|e| e.to_string())?
                .ok_or("no method output")?;
            assert_eq!(
                output
                    .get_property("ReturnValue")
                    .map_err(|e| e.to_string())?,
                Variant::UI4(0)
            );
            assert!(matches!(
                output
                    .get_property("IsEnabled")
                    .map_err(|e| e.to_string())?,
                Variant::Bool(_)
            ));
            let path = object.path().map_err(|e| e.to_string())?;
            let output = call_method(Namespace::MicrosoftTpm, &path, "IsEnabled")
                .map_err(|e| e.to_string())?;
            assert_eq!(output.u32("ReturnValue"), Some(0));
            assert!(output.bool("IsEnabled").is_some());
            Ok("Win32_Tpm.IsEnabled: ReturnValue=0, boolean out parameter decoded".into())
        });
    }

    #[test]
    #[ignore = "owner must stop winmgmt first in an elevated shell; test never changes service state"]
    fn spike_wmi_service_stopped_admin() {
        live(|| {
            let result = WMIConnection::new().and_then(|con| {
                con.raw_query::<HashMap<String, Variant>>(
                    "SELECT Caption FROM Win32_OperatingSystem",
                )
            });
            match result {
                Err(error) => Ok(format!("stopped WMI service: {error}")),
                Ok(_) => Err(
                    "winmgmt is available; owner must stop it before running this ignored test"
                        .into(),
                ),
            }
        });
    }

    fn scalar(value: Variant) -> Row {
        Row {
            _values: vec![("Value".into(), value)],
            ..Row::default()
        }
    }

    #[test]
    fn dotnet_scalar_text_and_checked_accessors() {
        for (value, text) in [
            (
                Variant::String("  Fabricated Ω 😀\0BSTR  ".into()),
                "  Fabricated Ω 😀\0BSTR  ".into(),
            ),
            (Variant::String(String::new()), String::new()),
            (Variant::Bool(true), "True".into()),
            (Variant::Bool(false), "False".into()),
            (Variant::I1(i8::MIN), "-128".into()),
            (Variant::I2(i16::MIN), "-32768".into()),
            (Variant::I4(i32::MIN), "-2147483648".into()),
            (Variant::I8(i64::MIN), "-9223372036854775808".into()),
            (Variant::UI1(u8::MAX), "255".into()),
            (Variant::UI2(u16::MAX), "65535".into()),
            (Variant::UI4(u32::MAX), "4294967295".into()),
            (Variant::UI8(u64::MAX), "18446744073709551615".into()),
            (
                Variant::String("20260102030405.000000+000".into()),
                "20260102030405.000000+000".into(),
            ),
        ] {
            assert_eq!(scalar(value).str("vAlUe"), Some(text));
        }
        assert_eq!(scalar(Variant::Null).str("Value"), None);
        assert_eq!(scalar(Variant::Empty).str("Value"), None);
        assert_eq!(Row::default().str("Missing"), None);
        assert_eq!(scalar(Variant::UI8(u64::MAX)).u64("Value"), Some(u64::MAX));
        assert_eq!(scalar(Variant::UI8(u64::MAX)).u32("Value"), None);
        assert_eq!(scalar(Variant::I4(-1)).u64("Value"), None);
        assert_eq!(
            scalar(Variant::UI8(u64::from(u32::MAX))).u32("Value"),
            Some(u32::MAX)
        );
        assert_eq!(
            scalar(Variant::String("  +4294967295  ".into())).u32("Value"),
            Some(u32::MAX)
        );
        assert_eq!(
            scalar(Variant::String("18446744073709551616".into())).u64("Value"),
            None
        );
        assert_eq!(scalar(Variant::R8(42.0)).u64("Value"), None);
        assert_eq!(scalar(Variant::Bool(true)).bool("Value"), Some(true));
        assert_eq!(scalar(Variant::UI1(1)).bool("Value"), None);
        let row = Row {
            _values: vec![
                ("ReturnValue".into(), Variant::Bool(true)),
                ("Number".into(), Variant::UI4(1)),
                ("ZFirst".into(), Variant::Bool(false)),
                ("ALater".into(), Variant::Bool(true)),
            ],
            ..Row::default()
        };
        assert_eq!(row.first_bool_except("returnvalue"), Some(false));
        assert_eq!(row.first_bool_except("zfirst"), Some(true));
        assert_eq!(
            scalar(Variant::UI4(0)).first_bool_except("ReturnValue"),
            None
        );
        assert_eq!(scalar(Variant::Bool(true)).first_bool_except("VALUE"), None);
    }

    #[test]
    fn dotnet_float_text_from_net10_oracle() {
        // Measured against .NET 10.0.11 InvariantCulture, as in HWIDChecker.csproj:17.
        for (value, text) in [
            (Variant::R4(0.0001), "0.0001"),
            (Variant::R4(0.00001), "1E-05"),
            (Variant::R4(1e8), "100000000"),
            (Variant::R4(1e9), "1E+09"),
            (Variant::R8(1e16), "10000000000000000"),
            (Variant::R8(1e17), "1E+17"),
            (Variant::R4(f32::from_bits(1)), "1E-45"),
            (Variant::R8(f64::from_bits(1)), "5E-324"),
            (Variant::R8(f64::NAN), "NaN"),
            (Variant::R8(f64::INFINITY), "Infinity"),
            (Variant::R8(f64::NEG_INFINITY), "-Infinity"),
            (Variant::R8(-0.0), "-0"),
        ] {
            assert_eq!(scalar(value).str("Value").as_deref(), Some(text));
        }
    }

    #[test]
    fn uint16_arrays_keep_nuls_and_reject_other_types() {
        let mut row = scalar(Variant::Array(vec![
            Variant::UI2(65),
            Variant::UI2(0),
            Variant::UI2(937),
        ]));
        row.array_types.insert("Value".into(), "System.UInt16[]");
        assert_eq!(row.u16_array("value"), Some(vec![65, 0, 937]));
        assert_eq!(row.str("value").as_deref(), Some("System.UInt16[]"));
        row._values[0].1 = Variant::Array(vec![]);
        assert_eq!(row.str("Value").as_deref(), Some("System.UInt16[]"));
        assert_eq!(row.u16_array("Value"), Some(vec![]));
        row.array_types.insert("Value".into(), "System.UInt32[]");
        assert_eq!(row.u16_array("Value"), None);
        assert_eq!(
            scalar(Variant::Array(vec![Variant::UI4(65)])).u16_array("Value"),
            None
        );
        assert_eq!(scalar(Variant::Null).u16_array("Value"), None);
        let mut bytes = scalar(Variant::Array(vec![Variant::UI1(0), Variant::UI1(255)]));
        bytes.array_types.insert("Value".into(), "System.Byte[]");
        assert_eq!(bytes.u8_array("value"), Some(vec![0, 255]));
        bytes._values[0].1 = Variant::Array(vec![]);
        assert_eq!(bytes.u8_array("Value"), Some(vec![]));
        bytes.array_types.insert("Value".into(), "System.UInt16[]");
        assert_eq!(bytes.u8_array("Value"), None);
        bytes.array_types.insert("Value".into(), "System.Byte[]");
        bytes._values[0].1 = Variant::Array(vec![Variant::UI1(1), Variant::UI2(2)]);
        assert_eq!(bytes.u8_array("Value"), None);
        assert_eq!(scalar(Variant::Array(vec![])).u8_array("Value"), None);
    }

    #[test]
    fn method_return_value_is_mandatory_and_preserved() {
        assert!(check_return_value(&Variant::UI4(0), "IsEnabled").is_ok());
        let error = check_return_value(&Variant::UI4(0x80280001), "IsEnabled").unwrap_err();
        assert_eq!(error.code, 0x80280001);
        assert!(error.to_string().contains("IsEnabled"));
        for value in [
            Variant::Null,
            Variant::Empty,
            Variant::Bool(false),
            Variant::I4(-1),
            Variant::UI8(u64::MAX),
        ] {
            assert!(check_return_value(&value, "IsEnabled").is_err());
        }
    }

    #[test]
    #[ignore = "live wrapper check requires Windows WMI and an attached monitor; works without admin"]
    fn live_wrapper_queries_cache_arrays_and_method_outputs() {
        live(|| {
            for ns in [Namespace::Cimv2, Namespace::Wmi, Namespace::Storage] {
                let wql = match ns {
                    Namespace::Cimv2 => "SELECT Caption FROM Win32_OperatingSystem",
                    Namespace::Wmi => "SELECT UserFriendlyName FROM WmiMonitorID",
                    Namespace::Storage => "SELECT DeviceId FROM MSFT_PhysicalDisk",
                    _ => unreachable!(),
                };
                for _ in 0..2 {
                    let rows = query(ns, wql).map_err(|e| e.to_string())?;
                    assert!(!rows.is_empty());
                    if ns == Namespace::Wmi {
                        for row in rows {
                            assert!(row.u16_array("UserFriendlyName").is_some());
                            assert_eq!(
                                row.str("UserFriendlyName").as_deref(),
                                Some("System.UInt16[]")
                            );
                        }
                    }
                }
            }
            super::CONNECTIONS.with(|cache| assert_eq!(cache.borrow().namespaces.len(), 3));
            // GetOwnerSid reads only this non-elevated test process's identity.
            let rows = query(
                Namespace::Cimv2,
                &format!(
                    "SELECT * FROM Win32_Process WHERE ProcessId = {}",
                    std::process::id()
                ),
            )
            .map_err(|e| e.to_string())?;
            let path = rows
                .first()
                .and_then(|row| row.str("__PATH"))
                .ok_or("missing __PATH")?;
            let output =
                call_method(Namespace::Cimv2, &path, "GetOwnerSid").map_err(|e| e.to_string())?;
            assert_eq!(output.u32("ReturnValue"), Some(0));
            assert!(output.str("Sid").is_some());
            Ok("wrapper: namespace queries, cache, UInt16[] and read-only method out parameter passed".into())
        });
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let sta = super::super::initialize_com().unwrap();
            assert!(
                !query(
                    Namespace::Cimv2,
                    "SELECT Caption FROM Win32_OperatingSystem"
                )
                .unwrap()
                .is_empty()
            );
            drop(sta);
            // The cache's own STA reference keeps COM valid after the caller's guard drops.
            assert!(
                !query(
                    Namespace::Cimv2,
                    "SELECT Caption FROM Win32_OperatingSystem"
                )
                .unwrap()
                .is_empty()
            );
            tx.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(60)).unwrap();
        println!("wrapper: caller-owned STA survives caller guard release");
    }
}
