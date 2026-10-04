//! Read-only Core Audio endpoint inventory. COM objects never leave their worker.

use super::{Error, Result, record, wide};
use std::{marker::PhantomData, rc::Rc};
use windows::Win32::{
    Devices::FunctionDiscovery::{
        PKEY_Device_FriendlyName, PKEY_Device_InstanceId, PKEY_DeviceInterface_FriendlyName,
    },
    Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE},
    Media::Audio::{
        DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
        DEVICE_STATE_UNPLUGGED, IDeviceTopology, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, eCapture, eRender,
    },
    System::{
        Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoCreateInstance,
            CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ,
            StructuredStorage::{PROPVARIANT, PropVariantClear},
        },
        Variant::{VT_CLSID, VT_EMPTY, VT_LPWSTR, VT_NULL},
    },
    UI::Shell::PropertiesSystem::IPropertyStore,
};
use windows::core::{GUID, PCWSTR, PWSTR};

// Microsoft.Windows.SDK.CPP 10.0.28000.2705, um/mmdeviceapi.h. This key is
// absent from windows 0.62.2; Windows 11 24H2+ may return VT_EMPTY even here.
const STABLE_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 12,
};

#[derive(Debug)]
pub struct Endpoint {
    pub direction: &'static str,
    pub id: Option<String>,
    pub name: Option<String>,
    pub adapter: Option<String>,
    pub instance_id: Option<String>,
    pub stable_id: Option<String>,
}

#[derive(Default)]
pub struct Scan {
    pub endpoints: Vec<Endpoint>,
    pub failures: Vec<Error>,
    pub inactive: Vec<(&'static str, u32)>,
    pub incomplete: bool,
}

struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    fn new() -> Result<Self> {
        // SAFETY: Null reserved argument; this thread-bound guard balances success.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let initialized = if initialized == RPC_E_CHANGED_MODE {
            // SAFETY: Retain an existing STA, matching win/wmi.rs, and balance success.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        } else {
            initialized
        };
        initialized
            .ok()
            .map_err(|e| api_error("CoInitializeEx", e))?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: One successful initialization on this thread; proxies drop first.
        unsafe { CoUninitialize() };
    }
}

struct Property(PROPVARIANT);

impl Drop for Property {
    fn drop(&mut self) {
        // SAFETY: GetValue transferred one initialized PROPVARIANT to this guard.
        if let Err(e) = unsafe { PropVariantClear(&mut self.0) } {
            record(api_error("PropVariantClear", e));
        }
    }
}

// Avoid IErrorInfo descriptions: a COM server could include an identity in them.
fn api_error(op: &'static str, error: windows::core::Error) -> Error {
    let code = error.code().0 as u32;
    let code = if code & 0xffff0000 == 0x80070000 {
        code & 0xffff
    } else {
        code
    };
    let detail = match code {
        5 => "access denied",
        1168 => "property or endpoint absent",
        50 | 0x80004001 => "operation unsupported",
        1460 | 0x800705b4 => "operation timed out",
        _ => "Core Audio operation failed",
    };
    Error {
        op,
        code,
        detail: detail.into(),
    }
}

// SAFETY contract: pointer comes from Core Audio and remains owned/alive while read.
unsafe fn string(pointer: PWSTR, op: &'static str) -> Result<String> {
    if pointer.is_null() {
        return Err(Error::msg(op, "malformed null string"));
    }
    // Core Audio promises a terminated UTF-16 string. Cap traversal/output rather
    // than accepting an arbitrarily large provider value; preserve all characters.
    for len in 0..32_768 {
        // SAFETY: The caller supplies a valid SDK-owned terminated string.
        if unsafe { *pointer.0.add(len) } == 0 {
            // SAFETY: Every unit through len has just been visited; the owner is alive.
            return String::from_utf16(unsafe { std::slice::from_raw_parts(pointer.0, len) })
                .map_err(|_| Error::msg(op, "malformed UTF-16 string"));
        }
    }
    Err(Error::msg(op, "string exceeds 32767 UTF-16 units"))
}

fn property(store: &IPropertyStore, key: &PROPERTYKEY, op: &'static str) -> Result<Option<String>> {
    // SAFETY: The store is live on its apartment; key is a valid PROPERTYKEY.
    let value = Property(unsafe { store.GetValue(key) }.map_err(|e| api_error(op, e))?);
    // SAFETY: GetValue initialized the discriminant and its corresponding union.
    let raw = unsafe { &value.0.Anonymous.Anonymous };
    if raw.vt == VT_EMPTY || raw.vt == VT_NULL {
        return Ok(None);
    }
    if raw.vt == VT_CLSID && key == &STABLE_ID {
        // SAFETY: VT_CLSID selects puuid; the property guard retains its allocation.
        let pointer = unsafe { raw.Anonymous.puuid };
        if pointer.is_null() {
            return Err(Error::msg(op, "malformed null GUID"));
        }
        // SAFETY: GetValue owns one GUID until the property guard clears it.
        let guid = unsafe { *pointer };
        return Ok((guid != GUID::zeroed()).then(|| format!("{{{guid:?}}}")));
    }
    if raw.vt != VT_LPWSTR {
        return Err(Error::msg(
            op,
            "unsupported property type (expected VT_LPWSTR)",
        ));
    }
    // SAFETY: The discriminator is VT_LPWSTR; the guard owns the string until return.
    let text = unsafe { string(raw.Anonymous.pwszVal, op) }?;
    Ok((!text.is_empty()).then_some(text))
}

fn adapter_instance_id(device: &IMMDevice, enumerator: &IMMDeviceEnumerator) -> Result<String> {
    // SAFETY: The endpoint is live in this apartment. Only topology is activated;
    // this does not activate an audio client/stream or change device state.
    let topology: IDeviceTopology = unsafe { device.Activate(CLSCTX_INPROC_SERVER, None) }
        .map_err(|e| api_error("IMMDevice::Activate(IDeviceTopology)", e))?;
    // SAFETY: Core Audio endpoint topologies expose their adapter connection at index 0.
    let connector = unsafe { topology.GetConnector(0) }
        .map_err(|e| api_error("IDeviceTopology::GetConnector", e))?;
    // SAFETY: The live connector transfers a CoTaskMem-allocated interface-path string.
    let pointer = unsafe { connector.GetDeviceIdConnectedTo() }
        .map_err(|e| api_error("IConnector::GetDeviceIdConnectedTo", e))?;
    // SAFETY: A successful call owns a terminated string until CoTaskMemFree.
    let path = unsafe { string(pointer, "IConnector::GetDeviceIdConnectedTo") };
    // SAFETY: This allocation uses CoTaskMem, even if decoding failed.
    unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
    let path = wide::to_wide(&path?);
    // SAFETY: GetDeviceIdConnectedTo returns an MMDevice ID accepted by GetDevice.
    // It may have a {2}. prefix; do not parse that opaque token as a SetupAPI path.
    let adapter = unsafe { enumerator.GetDevice(PCWSTR(path.as_ptr())) }
        .map_err(|e| api_error("IMMDeviceEnumerator::GetDevice (audio adapter)", e))?;
    // SAFETY: This reads the topology adapter's property store, not the endpoint's.
    let store = unsafe { adapter.OpenPropertyStore(STGM_READ) }
        .map_err(|e| api_error("IMMDevice::OpenPropertyStore (audio adapter)", e))?;
    if let Some(id) = property(
        &store,
        &PKEY_Device_InstanceId,
        "PKEY_Device_InstanceId (audio adapter)",
    )? {
        return Ok(id);
    }
    Err(Error::msg(
        "PKEY_Device_InstanceId (audio adapter)",
        "property absent or empty",
    ))
}

fn read_endpoint(
    device: &IMMDevice,
    enumerator: &IMMDeviceEnumerator,
    direction: &'static str,
    scan: &mut Scan,
) {
    let mut endpoint = Endpoint {
        direction,
        id: None,
        name: None,
        adapter: None,
        instance_id: None,
        stable_id: None,
    };
    // SAFETY: device belongs to this apartment. GetId transfers a CoTaskMem string.
    match unsafe { device.GetId() } {
        Ok(pointer) => {
            // SAFETY: The successful call owns a live SDK-terminated UTF-16 string.
            let value = unsafe { string(pointer, "IMMDevice::GetId") };
            // SAFETY: GetId requires CoTaskMemFree, including when decoding failed.
            unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
            match value {
                Ok(id) if !id.is_empty() => endpoint.id = Some(id),
                Ok(_) => scan
                    .failures
                    .push(Error::msg("IMMDevice::GetId", "empty endpoint ID")),
                Err(e) => scan.failures.push(e),
            }
        }
        Err(e) => scan.failures.push(api_error("IMMDevice::GetId", e)),
    }
    // SAFETY: Open only the read-only property store; no stream activation or writes.
    match unsafe { device.OpenPropertyStore(STGM_READ) } {
        Ok(store) => {
            for (key, op, field) in [
                (
                    &PKEY_Device_FriendlyName,
                    "PKEY_Device_FriendlyName",
                    &mut endpoint.name,
                ),
                (
                    &PKEY_DeviceInterface_FriendlyName,
                    "PKEY_DeviceInterface_FriendlyName",
                    &mut endpoint.adapter,
                ),
                (
                    &PKEY_Device_InstanceId,
                    "PKEY_Device_InstanceId",
                    &mut endpoint.instance_id,
                ),
                (
                    &STABLE_ID,
                    "PKEY_AudioEndpoint_StableId",
                    &mut endpoint.stable_id,
                ),
            ] {
                match property(&store, key, op) {
                    Ok(Some(value)) => *field = Some(value),
                    Ok(None) => scan
                        .failures
                        .push(Error::msg(op, "property absent or empty")),
                    Err(e) => scan.failures.push(e),
                }
            }
        }
        Err(e) => scan
            .failures
            .push(api_error("IMMDevice::OpenPropertyStore", e)),
    }
    // Some property stores identify the endpoint's SWD devnode, not the adapter.
    // Never use that software devnode as an adapter grouping key.
    if endpoint
        .instance_id
        .as_ref()
        .is_some_and(|id| id.to_uppercase().starts_with("SWD\\MMDEVAPI\\"))
    {
        endpoint.instance_id = None;
        scan.failures.push(Error::msg(
            "PKEY_Device_InstanceId",
            "endpoint devnode is not an adapter",
        ));
    }
    if endpoint.instance_id.is_none() {
        match adapter_instance_id(device, enumerator) {
            Ok(id) => endpoint.instance_id = Some(id),
            Err(error) => scan.failures.push(error),
        }
    }
    scan.endpoints.push(endpoint);
}

/// Enumerates active render/capture endpoints; inactive counts are diagnostics only.
/// Synchronous COM calls are bounded by the collector's existing 60-second deadline.
pub fn endpoints() -> Result<Scan> {
    let _apartment = Apartment::new()?;
    // SAFETY: COM is initialized; creation requests only the local enumerator interface.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| api_error("CoCreateInstance(MMDeviceEnumerator)", e))?;
    let mut scan = Scan::default();
    for (flow, direction) in [(eRender, "Render"), (eCapture, "Capture")] {
        // SAFETY: Enumerator belongs to this apartment; only active devices are requested.
        match unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) } {
            Ok(devices) => {
                // SAFETY: The collection is live and owned on this thread.
                match unsafe { devices.GetCount() } {
                    Ok(count) => {
                        for index in 0..count {
                            // SAFETY: index is below the collection's advertised count.
                            match unsafe { devices.Item(index) } {
                                Ok(device) => {
                                    read_endpoint(&device, &enumerator, direction, &mut scan)
                                }
                                Err(e) => {
                                    scan.incomplete = true;
                                    scan.failures
                                        .push(api_error("IMMDeviceCollection::Item", e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        scan.incomplete = true;
                        scan.failures
                            .push(api_error("IMMDeviceCollection::GetCount", e));
                    }
                }
            }
            Err(e) => {
                scan.incomplete = true;
                scan.failures.push(api_error(
                    "IMMDeviceEnumerator::EnumAudioEndpoints(active)",
                    e,
                ));
            }
        }
        // SAFETY: Diagnostic-only inventory of inactive devices; no endpoint properties read.
        match unsafe {
            enumerator.EnumAudioEndpoints(
                flow,
                DEVICE_STATE(
                    DEVICE_STATE_DISABLED.0 | DEVICE_STATE_NOTPRESENT.0 | DEVICE_STATE_UNPLUGGED.0,
                ),
            )
        } {
            Ok(devices) => {
                // SAFETY: Collection is live on this thread.
                match unsafe { devices.GetCount() } {
                    Ok(count) => scan.inactive.push((direction, count)),
                    Err(e) => scan
                        .failures
                        .push(api_error("IMMDeviceCollection::GetCount(inactive)", e)),
                }
            }
            Err(e) => scan.failures.push(api_error(
                "IMMDeviceEnumerator::EnumAudioEndpoints(inactive)",
                e,
            )),
        }
    }
    Ok(scan)
}
