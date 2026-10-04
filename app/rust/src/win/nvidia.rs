//! Owned by WP-07: runtime-loaded NVIDIA APIs.

use super::{Error, Result, dll, process, record, setupapi, wide};
use std::{
    ffi::{CStr, c_char, c_void},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{FARPROC, FreeLibrary, HMODULE},
        System::LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
            LoadLibraryExW,
        },
    },
    core::{PCSTR, PCWSTR},
};

/// A successful vendor enumeration, including failures of optional enrichment.
pub struct Capture<T> {
    /// The values produced by the source.
    pub items: Vec<T>,
    /// Errors that must be carried into the section diagnostics.
    pub failures: Vec<(&'static str, Error)>,
}

/// NVIDIA identity in the same index/name/UUID notation as nvidia-smi -L.
#[derive(Debug, Default)]
pub struct Gpu {
    /// The NVIDIA enumeration index.
    pub index: u32,
    /// The driver-reported display name.
    pub name: String,
    /// Text following UUID, preserving the C# delimiter and whitespace.
    pub uuid_suffix: Option<String>,
    /// A domain-zero PCI bus, if the vendor source proved it.
    pub pci_bus: Option<u32>,
    /// Full domain-zero PCI address for an exact WMI devnode join.
    pub pci_address: Option<PciAddress>,
    /// Optional NVML board/module serial.
    pub serial: Option<String>,
    /// Optional NVML 64-bit physical device identifier, as sixteen hex digits.
    pub pdi: Option<String>,
    /// Optional NVML firmware version (context, not a unit identifier).
    pub vbios: Option<String>,
    /// Optional NVML board part number.
    pub board_part: Option<String>,
}

/// PCI location; Windows devnodes can prove only domain zero here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u32,
    pub device: u32,
    pub function: u32,
}

/// The raw NVAPI board identifier and the PCI bus belonging to its handle.
#[derive(Clone)]
pub struct Board {
    /// The PCI bus returned for this physical GPU handle.
    pub pci_bus: u32,
    /// All sixteen BoardNum bytes, before interpretation.
    pub bytes: [u8; 16],
}

// System32 stays on the shared loader. Its frozen API cannot load NVSMI paths;
// this owned variant supplies that required path without changing shared files.
enum VendorLibrary {
    System(dll::Library),
    Standard(HMODULE),
}

impl VendorLibrary {
    fn load(path: &Path) -> Result<Self> {
        Self::load_inner(path).map_err(classified)
    }

    fn load_inner(path: &Path) -> Result<Self> {
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| Error::msg("LoadLibraryExW", "expected an absolute NVIDIA DLL path"))?;
        if !path.is_absolute() {
            return Err(Error::msg("LoadLibraryExW", "DLL path is not absolute"));
        }
        if path == process::system32(name) {
            return dll::load_system_dll(name).map(Self::System);
        }
        if path != standard_path(name)? {
            return Err(Error::msg("LoadLibraryExW", "DLL is outside NVIDIA NVSMI"));
        }
        let path = wide::to_wide(&path.to_string_lossy());
        // SAFETY: An absolute terminated NVSMI path is live. Dependencies are
        // searched only in that installed driver's directory and System32.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(path.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| Error::from_win("LoadLibraryExW", e))?;
        Ok(Self::Standard(module))
    }

    fn address(&self, name: &CStr) -> Result<unsafe extern "system" fn() -> isize> {
        let address: FARPROC = match self {
            Self::System(library) => library.proc_address(name),
            Self::Standard(module) => {
                // SAFETY: self owns the loaded module; name is NUL terminated.
                unsafe { GetProcAddress(*module, PCSTR(name.as_ptr().cast())) }
            }
        };
        address.ok_or_else(|| Error::msg("GetProcAddress", "absent: optional NVIDIA export"))
    }
}

impl Drop for VendorLibrary {
    fn drop(&mut self) {
        if let Self::Standard(module) = self {
            // SAFETY: This is the sole module reference acquired by our loader.
            if let Err(error) = unsafe { FreeLibrary(*module) } {
                record(Error::from_win("FreeLibrary", error));
            }
        }
    }
}

macro_rules! symbol {
    ($library:expr, $name:expr, $ty:ty) => {{
        let address = $library.address($name)?;
        // SAFETY: These call sites specify the documented NVIDIA C ABI for the
        // named export. The owning library outlives every call and session.
        unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address) }
    }};
}

type End = unsafe extern "C" fn() -> i32;
struct Session<'a> {
    _library: &'a VendorLibrary,
    end: End,
    op: &'static str,
    active: bool,
}

impl Session<'_> {
    fn finish(&mut self) -> Result<()> {
        if self.active {
            self.active = false;
            // SAFETY: A successful initialize owns this session; its DLL is live.
            status(self.op, unsafe { (self.end)() })?;
        }
        Ok(())
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            record(error);
        }
    }
}

fn status(op: &'static str, code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error {
            op,
            code: code as u32,
            detail: format!("{}: NVIDIA status {code}", status_class(op, code)),
        })
    }
}

fn status_class(op: &str, code: i32) -> &'static str {
    match (op.starts_with("nvml"), code) {
        (true, 3) | (false, -3 | -104) => "unsupported",
        (true, 4) | (false, -137 | -175) => "access-denied",
        (true, 6 | 9 | 12 | 13) | (false, -6) => "absent",
        (true, 10) | (false, -120 | -191) => "timeout",
        _ => "malformed",
    }
}

const MAX_GPUS: usize = 256;

/// Adds a value-free failure class without changing the source operation.
pub fn classified(mut error: Error) -> Error {
    let detail = error.detail.to_ascii_lowercase();
    if ![
        "absent:",
        "unsupported:",
        "access-denied:",
        "timeout:",
        "malformed:",
        "ambiguous:",
        "placeholder:",
        "implausible:",
    ]
    .iter()
    .any(|class| detail.contains(class))
    {
        let class = match error.code {
            5 => "access-denied",
            2 | 3 | 126 | 127 => "absent",
            1460 => "timeout",
            _ if detail.contains("timed out") || detail.contains("deadline") => "timeout",
            _ => "malformed",
        };
        error.detail = format!("{class}: {}", error.detail);
    }
    error
}

fn repeated(bytes: &[u8]) -> bool {
    bytes
        .first()
        .is_some_and(|first| bytes.iter().all(|b| b == first))
}

fn pdi_value(value: u64) -> Result<String> {
    if repeated(&value.to_le_bytes()) {
        return Err(Error::msg(
            "nvmlDeviceGetPdi",
            "implausible: repeated PDI bytes",
        ));
    }
    Ok(format!("{value:016X}"))
}

fn valid_uuid(value: &str) -> bool {
    let Some(body) = value.strip_prefix("GPU-") else {
        return false;
    };
    body.len() == 36
        && body.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        && !repeated(&body.bytes().filter(|b| *b != b'-').collect::<Vec<_>>())
}

fn parse_driver_text(bytes: &[u8], op: &'static str) -> Result<String> {
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error::msg(op, "malformed: unterminated driver string"))?;
    let value = std::str::from_utf8(bytes.get(..end).unwrap_or_default())
        .map_err(|_| Error::msg(op, "malformed: invalid UTF-8 driver string"))?;
    if value.chars().any(char::is_control) {
        return Err(Error::msg(op, "malformed: non-printable driver string"));
    }
    if value.trim().is_empty()
        || (matches!(op, "nvmlDeviceGetSerial" | "nvmlDeviceGetBoardPartNumber")
            && value.trim().bytes().all(|byte| byte == b'0'))
    {
        return Err(Error::msg(
            op,
            "placeholder: empty or zero-only driver string",
        ));
    }
    if op == "nvmlDeviceGetUUID" && !valid_uuid(value) {
        return Err(Error::msg(op, "implausible: invalid GPU UUID"));
    }
    if op == "nvmlDeviceGetSerial" && repeated(value.trim().as_bytes()) {
        return Err(Error::msg(op, "implausible: repeated serial characters"));
    }
    Ok(value.to_owned())
}

/// Rejects uncertain enrichment while keeping every readable identity group.
fn finish_enumeration(capture: &mut Capture<Gpu>, expected: u32, final_count: Result<u32>) {
    let mismatch = match final_count {
        Ok(count) => count != expected || capture.items.len() != expected as usize,
        Err(error) => {
            capture.failures.push(("NVML count", error));
            true
        }
    };
    if mismatch {
        capture.failures.push((
            "NVML count",
            Error::msg("NVML count", "implausible: incomplete or changed GPU count"),
        ));
        for gpu in &mut capture.items {
            gpu.pci_bus = None;
            gpu.pci_address = None;
        }
    }
    sanitize_identities(capture);
}

fn sanitize_identities(capture: &mut Capture<Gpu>) {
    for gpu in &mut capture.items {
        if gpu
            .uuid_suffix
            .as_deref()
            .is_some_and(|suffix| !valid_uuid(suffix.strip_prefix(':').unwrap_or(suffix).trim()))
        {
            gpu.uuid_suffix = None;
            capture.failures.push((
                "NVIDIA UUID",
                Error::msg("NVIDIA UUID", "implausible: invalid GPU UUID"),
            ));
        }
        if gpu.name.trim().is_empty() || gpu.name.chars().any(char::is_control) {
            gpu.name = "Unknown".to_owned();
            capture.failures.push((
                "NVIDIA name",
                Error::msg("NVIDIA name", "malformed: empty or non-printable GPU name"),
            ));
        }
    }
    for field in 0..3 {
        let get = |gpu: &Gpu| {
            match field {
                0 => gpu.uuid_suffix.as_deref(),
                1 => gpu.serial.as_deref(),
                _ => gpu.pdi.as_deref(),
            }
            .map(|v| v.trim().to_ascii_uppercase())
        };
        let values: Vec<_> = capture.items.iter().map(get).collect();
        for (gpu, value) in capture.items.iter_mut().zip(&values) {
            if value.as_ref().is_some_and(|v| {
                values
                    .iter()
                    .filter(|other| other.as_ref() == Some(v))
                    .count()
                    > 1
            }) {
                let source = match field {
                    0 => "NVIDIA UUID",
                    1 => "NVML serial",
                    _ => "NVML PDI",
                };
                capture.failures.push((
                    source,
                    Error::msg(source, "implausible: duplicate identifier across GPUs"),
                ));
                match field {
                    0 => gpu.uuid_suffix = None,
                    1 => gpu.serial = None,
                    _ => gpu.pdi = None,
                }
            }
        }
    }
}

/// Resolves an absolute path in the standard driver's NVSMI installation.
pub fn standard_path(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\', ':', '\0']) || name == "." || name == ".." {
        return Err(Error::msg("NVSMI path", "malformed: expected a filename"));
    }
    let root = std::env::var_os("ProgramW6432")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| {
            Error::msg(
                "NVSMI path",
                "absent: ProgramW6432 is missing or not absolute",
            )
        })?;
    Ok(root.join("NVIDIA Corporation").join("NVSMI").join(name))
}

#[repr(C)]
struct PciInfo {
    legacy: [c_char; 16],
    domain: u32,
    bus: u32,
    device: u32,
    device_id: u32,
    subsystem_id: u32,
    bus_id: [c_char; 32],
}

type Device = *mut c_void;
type Count = unsafe extern "C" fn(*mut u32) -> i32;
type Handle = unsafe extern "C" fn(u32, *mut Device) -> i32;
type Text = unsafe extern "C" fn(Device, *mut c_char, u32) -> i32;
type Pci = unsafe extern "C" fn(Device, *mut PciInfo) -> i32;
#[repr(C)]
struct PdiInfo {
    version: u32,
    value: u64,
}
type Pdi = unsafe extern "C" fn(Device, *mut PdiInfo) -> i32;

fn parse_pdi(info: &PdiInfo) -> Result<String> {
    if info.version != std::mem::size_of::<PdiInfo>() as u32 | (1 << 24) {
        return Err(Error::msg(
            "nvmlDeviceGetPdi",
            "malformed: returned PDI structure version changed",
        ));
    }
    pdi_value(info.value)
}

fn optional_text(
    library: &VendorLibrary,
    symbol: &CStr,
    device: Device,
    op: &'static str,
) -> Result<String> {
    let function = symbol!(library, symbol, Text);
    driver_text(function, device, op)
}

fn optional_pdi(library: &VendorLibrary, device: Device) -> Result<String> {
    let function = symbol!(library, c"nvmlDeviceGetPdi", Pdi);
    let mut info = PdiInfo {
        // NVML_STRUCT_VERSION(Pdi, 1), not NVAPI's version encoding.
        version: std::mem::size_of::<PdiInfo>() as u32 | (1 << 24),
        value: 0,
    };
    // SAFETY: info is the aligned, versioned nvmlPdi_v1_t and device is live.
    status("nvmlDeviceGetPdi", unsafe { function(device, &mut info) })?;
    parse_pdi(&info)
}

fn optional_value<T>(
    capture: &mut Capture<Gpu>,
    index: u32,
    source: &'static str,
    value: Result<T>,
) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(error) => {
            let mut error = classified(error);
            // GPU indices are report positions, never captured identity values.
            error.detail.push_str(&format!(" (GPU {index})"));
            capture.failures.push((source, error));
            None
        }
    }
}

fn driver_text(function: Text, device: Device, op: &'static str) -> Result<String> {
    let mut bytes = [0_u8; 256];
    // SAFETY: The vendor handle is live and the output buffer has the stated size.
    status(op, unsafe {
        function(device, bytes.as_mut_ptr().cast(), bytes.len() as u32)
    })?;
    parse_driver_text(&bytes, op)
}

/// Enumerates NVIDIA GPUs using a fully qualified, optional NVML library.
pub fn nvml(path: &Path) -> Result<Capture<Gpu>> {
    let library = VendorLibrary::load(path)?;
    let initialize = symbol!(library, c"nvmlInit_v2", End);
    let shutdown = symbol!(library, c"nvmlShutdown", End);
    let count = symbol!(library, c"nvmlDeviceGetCount_v2", Count);
    let handle = symbol!(library, c"nvmlDeviceGetHandleByIndex_v2", Handle);
    // SAFETY: All required exports have the documented NVML signatures.
    status("nvmlInit_v2", unsafe { initialize() })?;
    let mut session = Session {
        _library: &library,
        end: shutdown,
        op: "nvmlShutdown",
        active: true,
    };
    let mut capture = Capture {
        items: Vec::new(),
        failures: Vec::new(),
    };
    let pci = match library.address(c"nvmlDeviceGetPciInfo_v3") {
        Ok(address) => {
            // SAFETY: NVML's v3 PCI export takes a device and a 68-byte nvmlPciInfo_t.
            Some(unsafe {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, Pci>(address)
            })
        }
        Err(error) => {
            capture.failures.push(("NVML PCI match", error));
            None
        }
    };
    let mut length = 0;
    // SAFETY: The initialized session and writable count output are live.
    status("nvmlDeviceGetCount_v2", unsafe { count(&mut length) })?;
    if length == 0 || length as usize > MAX_GPUS {
        return Err(Error::msg(
            "nvmlDeviceGetCount_v2",
            if length == 0 {
                "absent: no GPUs"
            } else {
                "implausible: invalid GPU count"
            },
        ));
    }
    let mut seen_handles = std::collections::HashSet::new();
    for index in 0..length {
        let mut device = std::ptr::null_mut();
        // SAFETY: index is within the returned count; device is writable.
        let result = status("nvmlDeviceGetHandleByIndex_v2", unsafe {
            handle(index, &mut device)
        });
        if optional_value(&mut capture, index, "NVML handle", result).is_none() {
            continue;
        }
        if device.is_null() || !seen_handles.insert(device) {
            capture.failures.push((
                "NVML handle",
                Error::msg(
                    "nvmlDeviceGetHandleByIndex_v2",
                    "implausible: null or duplicate GPU handle",
                ),
            ));
            continue;
        }
        let mut gpu = Gpu {
            index,
            name: optional_value(
                &mut capture,
                index,
                "NVML name",
                optional_text(&library, c"nvmlDeviceGetName", device, "nvmlDeviceGetName"),
            )
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| "Unknown".to_owned()),
            uuid_suffix: optional_value(
                &mut capture,
                index,
                "NVML UUID",
                optional_text(&library, c"nvmlDeviceGetUUID", device, "nvmlDeviceGetUUID"),
            )
            .map(|value| format!(": {value}")),
            serial: optional_value(
                &mut capture,
                index,
                "NVML serial",
                optional_text(
                    &library,
                    c"nvmlDeviceGetSerial",
                    device,
                    "nvmlDeviceGetSerial",
                ),
            ),
            pdi: optional_value(
                &mut capture,
                index,
                "NVML PDI",
                optional_pdi(&library, device),
            ),
            vbios: optional_value(
                &mut capture,
                index,
                "NVML VBIOS",
                optional_text(
                    &library,
                    c"nvmlDeviceGetVbiosVersion",
                    device,
                    "nvmlDeviceGetVbiosVersion",
                ),
            ),
            board_part: optional_value(
                &mut capture,
                index,
                "NVML board part",
                optional_text(
                    &library,
                    c"nvmlDeviceGetBoardPartNumber",
                    device,
                    "nvmlDeviceGetBoardPartNumber",
                ),
            ),
            ..Default::default()
        };
        if let Some(pci) = pci {
            let mut info = PciInfo {
                legacy: [0; 16],
                domain: 0,
                bus: 0,
                device: 0,
                device_id: 0,
                subsystem_id: 0,
                bus_id: [0; 32],
            };
            // SAFETY: info matches the published nvmlPciInfo_t layout, including both strings.
            match status("nvmlDeviceGetPciInfo_v3", unsafe { pci(device, &mut info) }) {
                Ok(()) if info.domain == 0 && info.bus <= 255 && info.device <= 31 => {
                    let end = info.bus_id.iter().position(|&b| b == 0);
                    let address = end.and_then(|end| {
                        let bytes: Vec<u8> =
                            info.bus_id.get(..end)?.iter().map(|&b| b as u8).collect();
                        std::str::from_utf8(&bytes)
                            .ok()
                            .and_then(|text| parse_pci_address(text).ok())
                    });
                    match address {
                        Some(address)
                            if address.bus == info.bus && address.device == info.device =>
                        {
                            gpu.pci_address = Some(address);
                            gpu.pci_bus = Some(address.bus);
                        }
                        _ => capture.failures.push((
                            "NVML PCI match",
                            Error::msg(
                                "NVML PCI match",
                                "malformed: invalid or inconsistent PCI BDF",
                            ),
                        )),
                    }
                }
                Ok(()) => capture.failures.push((
                    "NVML PCI match",
                    Error::msg(
                        "NVML PCI match",
                        "ambiguous: nonzero domain or invalid bus; NVAPI match not proven",
                    ),
                )),
                Err(error) => capture.failures.push(("NVML PCI match", error)),
            }
        }
        capture.items.push(gpu);
    }
    let mut final_count = 0;
    // SAFETY: The session is still initialized and the count output is writable.
    let count_result =
        status("nvmlDeviceGetCount_v2", unsafe { count(&mut final_count) }).map(|()| final_count);
    finish_enumeration(&mut capture, length, count_result);
    if let Err(error) = session.finish() {
        capture.failures.push(("NVML shutdown", error));
    }
    if capture.items.is_empty() {
        for (_, error) in capture.failures {
            record(error);
        }
        return Err(Error::msg("NVML", "absent: no readable GPU handles"));
    }
    Ok(capture)
}

fn parse_smi(text: &str) -> Result<Vec<Gpu>> {
    if text.len() > 1024 * 1024 {
        return Err(Error::msg(
            "nvidia-smi -L",
            "malformed: output exceeds size cap",
        ));
    }
    let mut gpus = Vec::new();
    // C# parity: Hardware/GpuInfo.cs:45-64. MIG/non-GPU lines are ignored,
    // names are trimmed before UUID's trailing ')' is removed (including CRLF).
    for line in text.split('\n').filter(|line| line.starts_with("GPU ")) {
        let (header, info) = line
            .split_once(':')
            .ok_or_else(|| Error::msg("nvidia-smi -L", "GPU line has no colon"))?;
        if gpus.len() >= MAX_GPUS {
            return Err(Error::msg(
                "nvidia-smi -L",
                "implausible: GPU count exceeds cap",
            ));
        }
        let index = header
            .get(4..)
            .unwrap_or_default()
            .trim()
            .parse::<u32>()
            .map_err(|_| Error::msg("nvidia-smi -L", "malformed: invalid GPU index"))?;
        if gpus.iter().any(|gpu: &Gpu| gpu.index == index) {
            return Err(Error::msg(
                "nvidia-smi -L",
                "ambiguous: duplicate GPU index",
            ));
        }
        let info = info.trim();
        let (name, uuid_suffix) = match info.split_once("(UUID") {
            Some((name, suffix)) => (name.trim(), Some(suffix.trim_end_matches(')').to_owned())),
            None => (info.trim(), None),
        };
        gpus.push(Gpu {
            index,
            name: name.to_owned(),
            uuid_suffix,
            ..Default::default()
        });
    }
    if gpus.is_empty() {
        return Err(Error::msg(
            "nvidia-smi -L",
            "absent: no GPU lines in output",
        ));
    }
    Ok(gpus)
}

fn parse_pci_address(text: &str) -> Result<PciAddress> {
    let invalid = || {
        Error::msg(
            "NVIDIA PCI BDF",
            "malformed: invalid or nonzero-domain PCI address",
        )
    };
    if text.len() > 32 {
        return Err(invalid());
    }
    let mut parts = text.split(':');
    let domain = parts.next().ok_or_else(invalid)?;
    let bus = parts.next().ok_or_else(invalid)?;
    let slot = parts.next().ok_or_else(invalid)?;
    if parts.next().is_some() {
        return Err(invalid());
    }
    let (device, function) = slot.split_once('.').ok_or_else(invalid)?;
    let hex = |part| u32::from_str_radix(part, 16).map_err(|_| invalid());
    let domain = hex(domain)?;
    let address = PciAddress {
        bus: hex(bus)?,
        device: hex(device)?,
        function: hex(function)?,
    };
    if domain != 0 || address.bus > 255 || address.device > 31 || address.function > 7 {
        return Err(invalid());
    }
    Ok(address)
}

fn zero_domain_location(text: &str, address: PciAddress) -> bool {
    let suffix = format!(
        "bus {}, device {}, function {}",
        address.bus, address.device, address.function
    );
    text == format!("PCI {suffix}") || text == format!("PCI segment 0 {suffix}")
}

fn location_address(bytes: &[u8], required: u32, address: PciAddress) -> Result<PciAddress> {
    let invalid = || {
        Error::msg(
            "GPU PCI domain",
            "malformed: invalid location string size or encoding",
        )
    };
    if required < 2 || !required.is_multiple_of(2) {
        return Err(invalid());
    }
    let bytes = bytes.get(..required as usize).ok_or_else(invalid)?;
    let words: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    if words.last() != Some(&0) {
        return Err(invalid());
    }
    let location = String::from_utf16(
        words
            .get(..words.len().saturating_sub(1))
            .unwrap_or_default(),
    )
    .map_err(|_| invalid())?;
    if !zero_domain_location(&location, address) {
        return Err(Error::msg(
            "GPU PCI domain",
            "ambiguous: zero domain and exact BDF not proven",
        ));
    }
    Ok(address)
}

/// Gets PCI BDFs from the exact NVIDIA WMI instance IDs through SetupAPI.
/// No instance ID or PCI address is embedded in a failure message.
pub fn adapter_pci_addresses(ids: &[&str]) -> Result<Capture<(usize, PciAddress)>> {
    if ids.len() > MAX_GPUS {
        return Err(Error::msg(
            "WMI PCI match",
            "implausible: GPU count exceeds cap",
        ));
    }
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SP_DEVINFO_DATA, SPDRP_ADDRESS, SPDRP_BUSNUMBER, SetupDiGetDevicePropertyW,
        SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDeviceInfoW,
    };
    use windows::Win32::Devices::Properties::{
        DEVPKEY_Device_LocationInfo, DEVPROP_TYPE_STRING, DEVPROPTYPE,
    };
    use windows::Win32::System::Registry::REG_DWORD;
    let set = setupapi::DevInfoSet::enum_present_all()?;
    let mut capture = Capture {
        items: Vec::new(),
        failures: Vec::new(),
    };
    for (index, id) in ids.iter().enumerate() {
        let result = (|| {
            let id = wide::to_wide(id);
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            // SAFETY: The snapshot is live, the ID is terminated, and data has the SDK size.
            unsafe {
                SetupDiOpenDeviceInfoW(set.as_raw(), PCWSTR(id.as_ptr()), None, 0, Some(&mut data))
            }
            .map_err(|e| Error::from_win("SetupDiOpenDeviceInfoW (GPU PCI)", e))?;
            let read = |property| -> Result<u32> {
                let mut bytes = [0_u8; 4];
                let mut kind = 0;
                let mut required = 0;
                // SAFETY: data belongs to the live snapshot and all outputs are writable.
                unsafe {
                    SetupDiGetDeviceRegistryPropertyW(
                        set.as_raw(),
                        &data,
                        property,
                        Some(&mut kind),
                        Some(&mut bytes),
                        Some(&mut required),
                    )
                }
                .map_err(|e| Error::from_win("SetupDiGetDeviceRegistryPropertyW (GPU PCI)", e))?;
                if kind != REG_DWORD.0 || required != 4 {
                    return Err(Error::msg(
                        "GPU PCI location",
                        "invalid DWORD property type or size",
                    ));
                }
                Ok(u32::from_le_bytes(bytes))
            };
            let bus = read(SPDRP_BUSNUMBER)?;
            // For PCI, SPDRP_ADDRESS is device in high word, function in low word.
            let address = read(SPDRP_ADDRESS)?;
            let device = address >> 16;
            let function = address & 0xffff;
            if bus > 255 || device > 31 || function > 7 {
                return Err(Error::msg(
                    "GPU PCI location",
                    "invalid PCI bus, device or function",
                ));
            }
            let address = PciAddress {
                bus,
                device,
                function,
            };
            let mut bytes = [0_u8; 512];
            let mut kind = DEVPROPTYPE::default();
            let mut required = 0;
            // SAFETY: data belongs to this snapshot; byte buffer and output sizes are live.
            unsafe {
                SetupDiGetDevicePropertyW(
                    set.as_raw(),
                    &data,
                    &DEVPKEY_Device_LocationInfo,
                    &mut kind,
                    Some(&mut bytes),
                    Some(&mut required),
                    0,
                )
            }
            .map_err(|e| Error::from_win("SetupDiGetDevicePropertyW (GPU PCI domain)", e))?;
            if kind != DEVPROP_TYPE_STRING
                || required < 2
                || required as usize > bytes.len()
                || required % 2 != 0
            {
                return Err(Error::msg(
                    "GPU PCI domain",
                    "invalid location string type or size",
                ));
            }
            location_address(&bytes, required, address)
        })();
        match result {
            Ok(address) => capture.items.push((index, address)),
            Err(error) => capture.failures.push(("WMI PCI match", error)),
        }
    }
    Ok(capture)
}

fn pci_buses(text: &str, gpus: &mut [Gpu]) -> Result<()> {
    if text.len() > 1024 * 1024 {
        return Err(Error::msg(
            "nvidia-smi PCI match",
            "malformed: output exceeds size cap",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut buses = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if buses.len() >= MAX_GPUS {
            return Err(Error::msg(
                "nvidia-smi PCI match",
                "implausible: GPU count exceeds cap",
            ));
        }
        let (index, address) = line
            .split_once(',')
            .ok_or_else(|| Error::msg("nvidia-smi PCI match", "invalid CSV row"))?;
        let index = index
            .trim()
            .parse::<u32>()
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        if !seen.insert(index) || !gpus.iter().any(|gpu| gpu.index == index) {
            return Err(Error::msg(
                "nvidia-smi PCI match",
                "ambiguous: invalid PCI address or GPU index",
            ));
        }
        let address = parse_pci_address(address.trim())?;
        if buses
            .iter()
            .any(|(_, prior): &(u32, PciAddress)| prior.bus == address.bus)
        {
            return Err(Error::msg(
                "nvidia-smi PCI match",
                "ambiguous: duplicate PCI bus",
            ));
        }
        buses.push((index, address));
    }
    if seen.len() != gpus.len() {
        return Err(Error::msg(
            "nvidia-smi PCI match",
            "implausible: incomplete GPU list",
        ));
    }
    for gpu in gpus {
        gpu.pci_address = buses
            .iter()
            .find(|(index, _)| *index == gpu.index)
            .map(|(_, address)| *address);
        gpu.pci_bus = gpu.pci_address.map(|address| address.bus);
    }
    Ok(())
}

/// Runs nvidia-smi by absolute path with a shared fifteen-second deadline.
pub fn smi(path: &Path, budget: Duration) -> Result<Capture<Gpu>> {
    let start = Instant::now();
    let deadline = budget;
    let cancel = process::Cancel::new();
    let run = |args: &[&str]| -> Result<String> {
        let remaining = deadline.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(Error::msg("nvidia-smi", "timeout: shared deadline expired"));
        }
        let output = process::run(path, args, remaining, &cancel)
            .map_err(|e| classified(Error::msg("nvidia-smi", e)))?;
        if output.code != 0 {
            return Err(Error::msg(
                "nvidia-smi",
                format!("absent: process exited with code {}", output.code),
            ));
        }
        Ok(output.stdout)
    };
    let mut capture = Capture {
        items: parse_smi(&run(&["-L"])?)?,
        failures: Vec::new(),
    };
    sanitize_identities(&mut capture);
    match run(&[
        "--query-gpu=index,pci.bus_id",
        "--format=csv,noheader,nounits",
    ])
    .and_then(|text| pci_buses(&text, &mut capture.items))
    {
        Ok(()) => (),
        Err(error) => capture.failures.push(("nvidia-smi PCI match", error)),
    }
    Ok(capture)
}

#[repr(C)]
struct BoardInfo {
    version: u32,
    bytes: [u8; 16],
}
type Query = unsafe extern "C" fn(u32) -> *const c_void;
type EnumGpus = unsafe extern "C" fn(*mut Device, *mut u32) -> i32;
type Bus = unsafe extern "C" fn(Device, *mut u32) -> i32;
type BoardFn = unsafe extern "C" fn(Device, *mut BoardInfo) -> i32;

macro_rules! interface {
    ($query:expr, $id:expr, $ty:ty) => {{
        // SAFETY: nvapi_QueryInterface is loaded from the live System32 library.
        let address = unsafe { $query($id) };
        if address.is_null() {
            return Err(Error::msg(
                "nvapi_QueryInterface",
                format!("absent: missing interface 0x{:08X}", $id),
            ));
        }
        // SAFETY: Each numeric ID and C signature matches NVIDIA's published interface table.
        unsafe { std::mem::transmute::<*const c_void, $ty>(address) }
    }};
}

/// Enumerates NVAPI board bytes with bus IDs, retaining per-GPU enrichment errors.
pub fn boards() -> Result<Capture<Board>> {
    let library = VendorLibrary::load(&process::system32("nvapi64.dll"))?;
    let query = symbol!(library, c"nvapi_QueryInterface", Query);
    // C# parity: Services/Win32/NvApi.cs:16-19,32-62. Cdecl and version 20 | 1<<16.
    let initialize = interface!(query, 0x0150_E828_u32, End);
    let unload = interface!(query, 0xD22B_DD7E_u32, End);
    let enumerate = interface!(query, 0xE5AC_921F_u32, EnumGpus);
    let board = interface!(query, 0x22D5_4523_u32, BoardFn);
    let bus = interface!(query, 0x1BE0_B8E5_u32, Bus);
    // SAFETY: The resolved initialize export has no arguments.
    status("NvAPI_Initialize", unsafe { initialize() })?;
    let mut session = Session {
        _library: &library,
        end: unload,
        op: "NvAPI_Unload",
        active: true,
    };
    let mut handles = [std::ptr::null_mut(); 64];
    let mut length = 0;
    // SAFETY: NVAPI requires a 64-handle array and writable count output.
    status("NvAPI_EnumPhysicalGPUs", unsafe {
        enumerate(handles.as_mut_ptr(), &mut length)
    })?;
    if length > handles.len() as u32 {
        return Err(Error::msg(
            "NvAPI_EnumPhysicalGPUs",
            "implausible: invalid GPU count",
        ));
    }
    let mut capture = Capture {
        items: Vec::new(),
        failures: Vec::new(),
    };
    let mut seen_handles = std::collections::HashSet::new();
    for &handle in handles.get(..length as usize).unwrap_or_default() {
        let result = (|| {
            if handle.is_null() || !seen_handles.insert(handle) {
                return Err(Error::msg(
                    "NvAPI_EnumPhysicalGPUs",
                    "implausible: null or duplicate GPU handle",
                ));
            }
            let mut pci_bus = 0;
            let mut info = BoardInfo {
                version: 20 | (1 << 16),
                bytes: [0; 16],
            };
            // SAFETY: The enumerated handle and writable bus output are live.
            status("NvAPI_GPU_GetBusId", unsafe { bus(handle, &mut pci_bus) })?;
            if pci_bus > 255 {
                return Err(Error::msg(
                    "NvAPI_GPU_GetBusId",
                    "malformed: invalid PCI bus",
                ));
            }
            // SAFETY: info is the 20-byte version-1 NV_BOARD_INFO, with sixteen inline bytes.
            if let Err(error) = status("NvAPI_GPU_GetBoardInfo", unsafe {
                board(handle, &mut info)
            }) {
                // Keep the successfully identified bus in the complete enumeration.
                // Failed optional board bytes must not suppress another GPU's board.
                info.bytes = [0; 16];
                capture.failures.push(("NVAPI board", error));
            }
            Ok(Board {
                pci_bus,
                bytes: info.bytes,
            })
        })();
        match result {
            Ok(value) => capture.items.push(value),
            Err(error) => capture.failures.push(("NVAPI board", error)),
        }
    }
    if let Err(error) = session.finish() {
        capture.failures.push(("NVAPI unload", error));
    }
    Ok(capture)
}

/// Formats raw board bytes as C# ASCII, or uppercase hex without separators (AD-13).
pub fn board_value(bytes: &[u8; 16]) -> Option<String> {
    if repeated(bytes) {
        return None;
    }
    if bytes.iter().all(|&b| b == 0 || (0x20..=0x7e).contains(&b)) {
        // C# parity: Services/Win32/NvApi.cs:119-121. Trim only trailing NULs;
        // reject whitespace and ASCII '0', preserving embedded NULs and spaces.
        let value = String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_owned();
        if value.trim().is_empty()
            || value.chars().all(|c| c == '0')
            || repeated(value.trim().as_bytes())
        {
            None
        } else {
            Some(value)
        }
    } else {
        Some(bytes.iter().map(|b| format!("{b:02X}")).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabricated_gpu(index: u32) -> Gpu {
        Gpu {
            index,
            name: format!("NVIDIA Test GPU {index}"),
            uuid_suffix: Some(format!(": GPU-9e521d74-03ba-4c68-a27f-81d639b504c{index}")),
            pci_bus: Some(index + 1),
            pci_address: Some(PciAddress {
                bus: index + 1,
                device: 0,
                function: 0,
            }),
            serial: Some(format!("03248271963{index}")),
            pdi: Some(format!("08F47A2196BC3D5{index}")),
            vbios: Some("94.04.3A.00.71".to_owned()),
            board_part: Some("900-1G141-2530-000".to_owned()),
        }
    }

    #[test]
    fn nvml_optional_fields_reject_garbage_and_keep_partial_results() {
        assert_eq!(status_class("NvAPI_Initialize", -6), "absent");
        assert_eq!(status_class("NvAPI_GPU_GetBoardInfo", -104), "unsupported");
        assert_eq!(status_class("NvAPI_GPU_GetBoardInfo", -100), "malformed");
        assert_eq!(
            status_class("NvAPI_GPU_GetBoardInfo", -175),
            "access-denied"
        );
        assert_eq!(status_class("NvAPI_GPU_GetBoardInfo", -191), "timeout");
        assert_eq!(std::mem::size_of::<PdiInfo>(), 16);
        assert_eq!(std::mem::offset_of!(PdiInfo, value), 8);
        assert_eq!(
            std::mem::size_of::<PdiInfo>() as u32 | (1 << 24),
            0x0100_0010
        );
        assert_eq!(
            pdi_value(0x08F4_7A21_96BC_3D50).expect("fabricated PDI"),
            "08F47A2196BC3D50"
        );
        let mut capture = Capture {
            items: vec![fabricated_gpu(0), fabricated_gpu(1)],
            failures: Vec::new(),
        };
        let malformed_pdi = parse_pdi(&PdiInfo {
            version: 0,
            value: 0x08F4_7A21_96BC_3D50,
        });
        assert!(optional_value(&mut capture, 0, "NVML PDI", malformed_pdi).is_none());
        assert!(
            capture
                .failures
                .last()
                .expect("diagnostic")
                .1
                .detail
                .contains("malformed")
        );
        for bytes in [
            b"\0".as_slice(),
            b" 000 \0",
            b"111111\0",
            b"bad\nserial\0",
            &[0xff, 0],
            b"unterminated",
        ] {
            let result = parse_driver_text(bytes, "nvmlDeviceGetSerial");
            capture.items[0].serial = optional_value(&mut capture, 0, "NVML serial", result);
            assert!(capture.items[0].serial.is_none());
        }
        assert_eq!(
            parse_driver_text(b" 032482719635 \0", "nvmlDeviceGetSerial").expect("serial"),
            " 032482719635 "
        );
        for value in [0, u64::MAX, 0x1111_1111_1111_1111] {
            let result = pdi_value(value);
            capture.items[0].pdi = optional_value(&mut capture, 0, "NVML PDI", result);
            assert!(capture.items[0].pdi.is_none());
        }
        for (result, class) in [
            (
                Err(Error::msg("GetProcAddress", "absent: export missing")),
                "absent",
            ),
            (
                status("nvmlDeviceGetBoardPartNumber", 3).map(|()| String::new()),
                "unsupported",
            ),
            (
                status("nvmlDeviceGetBoardPartNumber", 4).map(|()| String::new()),
                "access-denied",
            ),
        ] {
            capture.items[0].board_part =
                optional_value(&mut capture, 0, "NVML board part", result);
            assert!(capture.items[0].board_part.is_none());
            assert!(
                capture
                    .failures
                    .last()
                    .expect("diagnostic")
                    .1
                    .detail
                    .contains(class)
            );
        }
        assert_eq!(capture.items[1].serial.as_deref(), Some("032482719631"));
        assert!(
            capture
                .items
                .iter()
                .all(|gpu| gpu.uuid_suffix.is_some() && gpu.vbios.is_some())
        );
        assert!(
            capture
                .failures
                .iter()
                .all(|(_, error)| !error.detail.contains("03248271963"))
        );
    }

    #[test]
    fn nvml_changed_counts_and_duplicate_ids_fail_closed() {
        for final_count in [
            Ok(3),
            Err(Error::msg(
                "nvmlDeviceGetCount_v2",
                "timeout: fabricated failure",
            )),
        ] {
            let mut capture = Capture {
                items: vec![fabricated_gpu(0), fabricated_gpu(1)],
                failures: Vec::new(),
            };
            finish_enumeration(&mut capture, 2, final_count);
            assert!(
                capture
                    .items
                    .iter()
                    .all(|gpu| gpu.pci_bus.is_none() && gpu.pci_address.is_none())
            );
            assert!(
                capture
                    .items
                    .iter()
                    .all(|gpu| gpu.serial.is_some() && gpu.uuid_suffix.is_some())
            );
            assert!(!capture.failures.is_empty());
        }
        // One unreadable handle preserves the other device but forbids a board join.
        let mut capture = Capture {
            items: vec![fabricated_gpu(1)],
            failures: vec![(
                "NVML handle",
                Error::msg(
                    "nvmlDeviceGetHandleByIndex_v2",
                    "absent: fabricated failure",
                ),
            )],
        };
        finish_enumeration(&mut capture, 2, Ok(2));
        assert_eq!(capture.items[0].index, 1);
        assert!(capture.items[0].pci_bus.is_none());
        assert!(capture.items[0].serial.is_some());
        let mut capture = Capture {
            items: vec![fabricated_gpu(0), fabricated_gpu(1)],
            failures: Vec::new(),
        };
        finish_enumeration(&mut capture, 2, Ok(2));
        assert!(capture.failures.is_empty());
        assert!(capture.items.iter().all(|gpu| gpu.pci_bus.is_some()));
        capture.items[1].uuid_suffix = capture.items[0].uuid_suffix.clone();
        capture.items[1].serial = capture.items[0].serial.clone();
        capture.items[1].pdi = capture.items[0].pdi.clone();
        sanitize_identities(&mut capture);
        assert!(
            capture
                .items
                .iter()
                .all(|gpu| gpu.uuid_suffix.is_none() && gpu.serial.is_none() && gpu.pdi.is_none())
        );
        assert!(
            capture
                .items
                .iter()
                .all(|gpu| gpu.vbios.is_some() && gpu.board_part.is_some())
        );
        assert_eq!(capture.failures.len(), 6);
    }

    #[test]
    fn smi_invalid_uuid_and_capped_lists_preserve_readable_groups() {
        for uuid in [
            "wrong",
            "GPU-00000000-0000-0000-0000-000000000000",
            "GPU-9e521d74-03ba-4c68-a27f-81d639b504cg",
        ] {
            let mut capture = Capture {
                items: parse_smi(&format!("GPU 0: NVIDIA Test (UUID: {uuid})")).expect("identity"),
                failures: Vec::new(),
            };
            sanitize_identities(&mut capture);
            assert_eq!(capture.items[0].name, "NVIDIA Test");
            assert!(capture.items[0].uuid_suffix.is_none());
            assert!(capture.failures[0].1.detail.contains("implausible"));
            assert!(!capture.failures[0].1.detail.contains(uuid));
        }
        let text: String = (0..=MAX_GPUS)
            .map(|i| format!("GPU {i}: NVIDIA Test\n"))
            .collect();
        assert!(
            parse_smi(&text)
                .expect_err("cap")
                .detail
                .contains("implausible")
        );
        assert!(
            parse_smi("No devices were found")
                .expect_err("absent")
                .detail
                .contains("absent")
        );
        let mut capture = Capture {
            items: vec![fabricated_gpu(0)],
            failures: Vec::new(),
        };
        capture.items[0].name = "bad\nname".to_owned();
        sanitize_identities(&mut capture);
        assert_eq!(capture.items[0].name, "Unknown");
        assert!(capture.items[0].uuid_suffix.is_some() && capture.items[0].serial.is_some());
        assert!(capture.failures[0].1.detail.contains("malformed"));
    }

    #[test]
    fn pci_location_encoding_and_duplicate_buses_leave_diagnostics() {
        let address = PciAddress {
            bus: 1,
            device: 0,
            function: 0,
        };
        let encode = |s: &str| {
            s.encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        };
        let bytes = encode("PCI bus 1, device 0, function 0");
        assert_eq!(
            location_address(&bytes, bytes.len() as u32, address).expect("location"),
            address
        );
        for bytes in [
            encode("PCI-Bus 1, Gerät 0, Funktion 0"),
            encode("unknown"),
            encode("PCI segment 1 bus 1, device 0, function 0"),
            vec![0, 0xd8, 0, 0],
            vec![1],
            vec![65, 0],
        ] {
            let mut capture = Capture {
                items: Vec::new(),
                failures: Vec::new(),
            };
            assert!(
                optional_value(
                    &mut capture,
                    0,
                    "WMI PCI match",
                    location_address(&bytes, bytes.len() as u32, address)
                )
                .is_none()
            );
            assert_eq!(capture.failures.len(), 1);
        }
        assert!(location_address(&bytes, u32::MAX, address).is_err());
        let mut gpus = [fabricated_gpu(0), fabricated_gpu(1)];
        let before: Vec<_> = gpus
            .iter()
            .map(|gpu| (gpu.name.clone(), gpu.uuid_suffix.clone(), gpu.pci_address))
            .collect();
        assert!(
            pci_buses("0, 0000:01:00.0\n1, 0000:01:01.0", &mut gpus)
                .expect_err("duplicate bus")
                .detail
                .contains("ambiguous")
        );
        assert_eq!(
            gpus.iter()
                .map(|gpu| (gpu.name.clone(), gpu.uuid_suffix.clone(), gpu.pci_address))
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn nvidia_smi_parser_keeps_csharp_crlf_and_uuid_delimiters() {
        let address = PciAddress {
            bus: 1,
            device: 2,
            function: 3,
        };
        assert_eq!(
            parse_pci_address("00000000:01:02.3").expect("PCI BDF"),
            address
        );
        for text in [
            "0001:01:02.3",
            "0000:100:02.3",
            "0000:01:20.0",
            "0000:01:00.8",
            "invalid",
        ] {
            assert!(parse_pci_address(text).is_err());
        }
        assert!(zero_domain_location(
            "PCI bus 1, device 2, function 3",
            address
        ));
        assert!(zero_domain_location(
            "PCI segment 0 bus 1, device 2, function 3",
            address
        ));
        for text in [
            "PCI segment 1 bus 1, device 2, function 3",
            "PCI bus 1, device 2, function 4",
            "unknown",
            "PCI bus 1, device 2, function 3 extra",
        ] {
            assert!(!zero_domain_location(text, address));
        }
        let fixture = include_str!("../../tests/fixtures/wp-07/nvidia-smi.fixture");
        for input in [fixture.to_owned(), fixture.replace('\n', "\r\n")] {
            let mut gpus = parse_smi(&input).expect("fabricated SMI capture");
            assert_eq!(gpus.len(), 2);
            assert_eq!(gpus[0].index, 0);
            assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 5080");
            assert_eq!(
                gpus[0].uuid_suffix.as_deref(),
                Some(": GPU-358d91ef-2174-43eb-9221-d36a721cd603")
            );
            assert_eq!(
                gpus[1].uuid_suffix.as_deref(),
                Some(": GPU-b12e8307-f86e-410c-9a08-7d3452d12c9a")
            );
            pci_buses("1, 00000000:03:00.0\r\n0, 00000000:01:00.0\r\n", &mut gpus)
                .expect("PCI addresses");
            assert_eq!((gpus[0].pci_bus, gpus[1].pci_bus), (Some(1), Some(3)));
        }
        assert_eq!(
            parse_smi("GPU 2: Legacy NVIDIA GPU\r\n").expect("UUID is optional")[0].uuid_suffix,
            None
        );
        for input in [
            "",
            "No devices were found",
            "GPU 0",
            "GPU invalid: name",
            "GPU 0: A\nGPU 0: B",
        ] {
            assert!(parse_smi(input).is_err(), "{input:?}");
        }
        for input in [
            "0, 0000:01:00.0\n0, 0000:02:00.0",
            "0, broken",
            "0, 0001:01:00.0",
            "0, 0000:100:00.0",
            "0, 0000:01:20.0",
            "0, 0000:01:00.8",
            "9, 0000:01:00.0",
            "",
        ] {
            let mut gpus = parse_smi(fixture).expect("fixture");
            assert!(pci_buses(input, &mut gpus).is_err(), "{input:?}");
            assert!(gpus.iter().all(|gpu| gpu.pci_bus.is_none()));
        }
    }

    #[test]
    fn nvidia_board_bytes_preserve_ascii_and_reject_zero_placeholders() {
        assert_eq!(std::mem::size_of::<BoardInfo>(), 20);
        assert_eq!(std::mem::size_of::<PciInfo>(), 68);
        assert_eq!(board_value(&[0; 16]), None);
        assert_eq!(board_value(&[b'0'; 16]), None);
        assert_eq!(board_value(b"000000\0\0\0\0\0\0\0\0\0\0"), None);
        assert_eq!(board_value(&[b' '; 16]), None);
        assert_eq!(board_value(&[0xff; 16]), None);
        assert_eq!(board_value(&[b'1'; 16]), None);
        assert_eq!(
            board_value(b"042571983612\0\0\0\0"),
            Some("042571983612".to_owned())
        );
        assert_eq!(
            board_value(b"12\0XY           "),
            Some("12\0XY           ".to_owned())
        );
        let bytes = [
            0x80, 0x00, 0x7f, 0x10, 0x23, 0x9a, 0xbc, 0xde, 0x10, 0x47, 0x59, 0x61, 0x73, 0x85,
            0x97, 0xa9,
        ];
        assert_eq!(
            board_value(&bytes),
            Some("80007F10239ABCDE10475961738597A9".to_owned())
        );
    }
}
