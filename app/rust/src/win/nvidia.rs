//! Owned by WP-07: runtime-loaded NVIDIA APIs.

use super::{Error, Result, dll, process, record, wide};
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
#[derive(Debug)]
pub struct Gpu {
    /// The NVIDIA enumeration index.
    pub index: u32,
    /// The driver-reported display name.
    pub name: String,
    /// Text following UUID, preserving the C# delimiter and whitespace.
    pub uuid_suffix: Option<String>,
    /// A domain-zero PCI bus, if the vendor source proved it.
    pub pci_bus: Option<u32>,
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
        address.ok_or_else(|| Error::msg("GetProcAddress", name.to_string_lossy()))
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
            detail: format!("NVIDIA status {code}"),
        })
    }
}

/// Resolves an absolute path in the standard driver's NVSMI installation.
pub fn standard_path(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\', ':', '\0']) || name == "." || name == ".." {
        return Err(Error::msg("NVSMI path", "expected a filename"));
    }
    let root = std::env::var_os("ProgramW6432")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| Error::msg("NVSMI path", "ProgramW6432 is missing or not absolute"))?;
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

fn driver_text(function: Text, device: Device, op: &'static str) -> Result<String> {
    let mut bytes = [0_u8; 256];
    // SAFETY: The vendor handle is live and the output buffer has the stated size.
    status(op, unsafe {
        function(device, bytes.as_mut_ptr().cast(), bytes.len() as u32)
    })?;
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error::msg(op, "unterminated driver string"))?;
    let value = std::str::from_utf8(&bytes[..end]).map_err(|e| Error::msg(op, e.to_string()))?;
    if value.trim().is_empty() {
        return Err(Error::msg(op, "empty driver string"));
    }
    Ok(value.to_owned())
}

/// Enumerates NVIDIA GPUs using a fully qualified, optional NVML library.
pub fn nvml(path: &Path) -> Result<Capture<Gpu>> {
    let library = VendorLibrary::load(path)?;
    let initialize = symbol!(library, c"nvmlInit_v2", End);
    let shutdown = symbol!(library, c"nvmlShutdown", End);
    let count = symbol!(library, c"nvmlDeviceGetCount_v2", Count);
    let handle = symbol!(library, c"nvmlDeviceGetHandleByIndex_v2", Handle);
    let name = symbol!(library, c"nvmlDeviceGetName", Text);
    let uuid = symbol!(library, c"nvmlDeviceGetUUID", Text);
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
    if length == 0 || length > 256 {
        return Err(Error::msg(
            "nvmlDeviceGetCount_v2",
            "no GPUs or invalid GPU count",
        ));
    }
    for index in 0..length {
        let mut device = std::ptr::null_mut();
        // SAFETY: index is within the returned count; device is writable.
        status("nvmlDeviceGetHandleByIndex_v2", unsafe {
            handle(index, &mut device)
        })?;
        if device.is_null() {
            return Err(Error::msg(
                "nvmlDeviceGetHandleByIndex_v2",
                "null GPU handle",
            ));
        }
        let mut gpu = Gpu {
            index,
            name: driver_text(name, device, "nvmlDeviceGetName")?
                .trim()
                .to_owned(),
            uuid_suffix: Some(format!(
                ": {}",
                driver_text(uuid, device, "nvmlDeviceGetUUID")?
            )),
            pci_bus: None,
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
                Ok(()) if info.domain == 0 && info.bus <= 255 => gpu.pci_bus = Some(info.bus),
                Ok(()) => capture.failures.push((
                    "NVML PCI match",
                    Error::msg(
                        "NVML PCI match",
                        "nonzero domain or invalid bus; NVAPI match not proven",
                    ),
                )),
                Err(error) => capture.failures.push(("NVML PCI match", error)),
            }
        }
        capture.items.push(gpu);
    }
    if let Err(error) = session.finish() {
        capture.failures.push(("NVML shutdown", error));
    }
    Ok(capture)
}

fn parse_smi(text: &str) -> Result<Vec<Gpu>> {
    let mut gpus = Vec::new();
    // C# parity: Hardware/GpuInfo.cs:45-64. MIG/non-GPU lines are ignored,
    // names are trimmed before UUID's trailing ')' is removed (including CRLF).
    for line in text.split('\n').filter(|line| line.starts_with("GPU ")) {
        let (header, info) = line
            .split_once(':')
            .ok_or_else(|| Error::msg("nvidia-smi -L", "GPU line has no colon"))?;
        let index = header[4..]
            .trim()
            .parse::<u32>()
            .map_err(|e| Error::msg("nvidia-smi -L", e.to_string()))?;
        if gpus.iter().any(|gpu: &Gpu| gpu.index == index) {
            return Err(Error::msg("nvidia-smi -L", "duplicate GPU index"));
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
            pci_bus: None,
        });
    }
    if gpus.is_empty() {
        return Err(Error::msg("nvidia-smi -L", "no GPU lines in output"));
    }
    Ok(gpus)
}

fn pci_buses(text: &str, gpus: &mut [Gpu]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let mut buses = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (index, address) = line
            .split_once(',')
            .ok_or_else(|| Error::msg("nvidia-smi PCI match", "invalid CSV row"))?;
        let index = index
            .trim()
            .parse::<u32>()
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        let parts: Vec<_> = address.trim().split(':').collect();
        if parts.len() != 3 || !seen.insert(index) || !gpus.iter().any(|gpu| gpu.index == index) {
            return Err(Error::msg(
                "nvidia-smi PCI match",
                "invalid PCI address or GPU index",
            ));
        }
        let domain = u32::from_str_radix(parts[0], 16)
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        let bus = u32::from_str_radix(parts[1], 16)
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        let (device, function) = parts[2]
            .split_once('.')
            .ok_or_else(|| Error::msg("nvidia-smi PCI match", "missing PCI function"))?;
        let device = u32::from_str_radix(device, 16)
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        let function = u32::from_str_radix(function, 16)
            .map_err(|e| Error::msg("nvidia-smi PCI match", e.to_string()))?;
        if domain != 0 || bus > 255 || device > 31 || function > 7 {
            return Err(Error::msg(
                "nvidia-smi PCI match",
                "PCI address cannot be matched to NVAPI",
            ));
        }
        buses.push((index, bus));
    }
    if seen.len() != gpus.len() {
        return Err(Error::msg("nvidia-smi PCI match", "incomplete GPU list"));
    }
    for gpu in gpus {
        gpu.pci_bus = buses
            .iter()
            .find(|(index, _)| *index == gpu.index)
            .map(|(_, bus)| *bus);
    }
    Ok(())
}

/// Runs nvidia-smi by absolute path with a shared fifteen-second deadline.
pub fn smi(path: &Path) -> Result<Capture<Gpu>> {
    let start = Instant::now();
    let deadline = Duration::from_secs(15);
    let cancel = process::Cancel::new();
    let run = |args: &[&str]| -> Result<String> {
        let output = process::run(
            path,
            args,
            deadline.saturating_sub(start.elapsed()),
            &cancel,
        )
        .map_err(|e| Error::msg("nvidia-smi", e))?;
        if output.code != 0 {
            return Err(Error::msg(
                "nvidia-smi",
                format!(
                    "Process exited with code {}. {}",
                    output.code,
                    output.stderr.trim()
                ),
            ));
        }
        Ok(output.stdout)
    };
    let mut capture = Capture {
        items: parse_smi(&run(&["-L"])?)?,
        failures: Vec::new(),
    };
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
                format!("missing interface 0x{:08X}", $id),
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
        return Err(Error::msg("NvAPI_EnumPhysicalGPUs", "invalid GPU count"));
    }
    let mut capture = Capture {
        items: Vec::new(),
        failures: Vec::new(),
    };
    for &handle in &handles[..length as usize] {
        let result = (|| {
            if handle.is_null() {
                return Err(Error::msg("NvAPI_EnumPhysicalGPUs", "null GPU handle"));
            }
            let mut pci_bus = 0;
            let mut info = BoardInfo {
                version: 20 | (1 << 16),
                bytes: [0; 16],
            };
            // SAFETY: The enumerated handle and writable bus output are live.
            status("NvAPI_GPU_GetBusId", unsafe { bus(handle, &mut pci_bus) })?;
            // SAFETY: info is the 20-byte version-1 NV_BOARD_INFO, with sixteen inline bytes.
            status("NvAPI_GPU_GetBoardInfo", unsafe {
                board(handle, &mut info)
            })?;
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
    if bytes.iter().all(|&b| b == 0 || (0x20..=0x7e).contains(&b)) {
        // C# parity: Services/Win32/NvApi.cs:119-121. Trim only trailing NULs;
        // reject whitespace and ASCII '0', preserving embedded NULs and spaces.
        let value = String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_owned();
        if value.trim().is_empty() || value.chars().all(|c| c == '0') {
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

    #[test]
    fn nvidia_smi_parser_keeps_csharp_crlf_and_uuid_delimiters() {
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
