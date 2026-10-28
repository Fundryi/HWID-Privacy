//! Read-only anti-cheat snapshot. Presence and permission are deliberately separate.
//!
//! Names are version-dependent evidence, not proof of a game's identity or complete coverage.
//! This engine does not alter services, inspect process memory, or enter collection/report paths.

mod database;
pub use database::{Database, Gate, SOURCE_PATH as DATABASE_SOURCE};

use super::{Error, OwnedHandle, Result, dll, record, wide};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    ERROR_MORE_DATA, ERROR_NO_MORE_FILES, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SUCCESS,
    GetLastError, HANDLE, LUID, SetLastError,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, ImpersonateSelf, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
    RevertToSelf, SE_PRIVILEGE_ENABLED, SecurityImpersonation, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    OpenServiceW, QueryServiceStatusEx, SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_ENUMERATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_DRIVER, SERVICE_QUERY_STATUS,
    SERVICE_STATE_ALL, SERVICE_STATUS_PROCESS, SERVICE_WIN32,
};
use windows::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};
use windows::core::{BOOL, PCWSTR};

const BUDGET: Duration = Duration::from_secs(2);
const MAX_SERVICES: usize = 8192;
const MAX_MODULES: usize = 4096;
const MAX_PROCESSES: usize = 65536;
const MAX_GROWTH: usize = 3;
static WORKER_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer {
    Service,
    Driver,
    Process,
    Database,
    Worker,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Evidence {
    pub layer: Layer,
    /// Recognized service name or executable/module basename only.
    pub matched_name: String,
}

#[derive(Clone, Debug)]
pub struct DetectedAntiCheat {
    pub id: String,
    pub name: String,
    pub gate: Gate,
    pub kernel_mode: bool,
    pub boot_resident: bool,
    /// Associated titles, never asserted to be running just because a service matched.
    pub games_sample: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug)]
pub struct FailedLayer {
    pub layer: Layer,
    pub error: Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Clear,
    Detected,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateDecision {
    Clear,
    Blocked,
    Unknown,
}

#[derive(Clone, Debug, Default)]
pub struct Detection {
    pub database_version: Option<u32>,
    pub detected: Vec<DetectedAntiCheat>,
    pub failed_layers: Vec<FailedLayer>,
}

impl Detection {
    /// Hits take precedence for presence; failed layers are still retained.
    pub fn presence(&self) -> Presence {
        if !self.detected.is_empty() {
            Presence::Detected
        } else if self.database_version != Some(1) || !self.failed_layers.is_empty() {
            Presence::Unknown
        } else {
            Presence::Clear
        }
    }

    /// MUST_GATE and SHOULD_GATE block, without a bypass. DONT_GATE does not hide failures.
    pub fn gate_decision(&self) -> GateDecision {
        if self.detected.iter().any(|entry| entry.gate != Gate::Dont) {
            GateDecision::Blocked
        } else if self.database_version != Some(1) || !self.failed_layers.is_empty() {
            GateDecision::Unknown
        } else {
            GateDecision::Clear
        }
    }

    fn fail(&mut self, layer: Layer, error: Error) {
        self.failed_layers.push(FailedLayer { layer, error });
    }

    fn unknown(layer: Layer, error: Error) -> Self {
        let mut result = Self::default();
        result.fail(layer, error);
        result
    }
}

/// Performs a fresh pass with at most two seconds of caller wait and one unfinished worker.
/// Busy, timeout, panic, database and enumeration failures never become an empty clear snapshot.
pub fn detect() -> Detection {
    bounded_detection(&WORKER_BUSY, BUDGET, detect_worker)
}

struct BusyGuard(&'static AtomicBool);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn bounded_detection(
    busy: &'static AtomicBool,
    budget: Duration,
    work: impl FnOnce(Instant) -> Detection + Send + 'static,
) -> Detection {
    let start = Instant::now();
    if busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Detection::unknown(
            Layer::Worker,
            Error::msg("anti-cheat worker", "check already in progress"),
        );
    }
    let guard = BusyGuard(busy);
    let (send, receive) = mpsc::sync_channel(1);
    if let Err(error) = std::thread::Builder::new()
        .name("anti-cheat gate".into())
        .spawn(move || {
            let result = super::catch_panic(|| work(start + budget)).unwrap_or_else(|_| {
                Detection::unknown(
                    Layer::Worker,
                    Error::msg("anti-cheat worker", "worker panicked"),
                )
            });
            // All query handles/privileges are gone before another worker is admitted.
            drop(guard);
            let _ = send.send(result);
        })
    {
        return Detection::unknown(
            Layer::Worker,
            Error::msg("thread::spawn", error.to_string()),
        );
    }
    receive
        .recv_timeout(budget.saturating_sub(start.elapsed()))
        .unwrap_or_else(|error| {
            Detection::unknown(
                Layer::Worker,
                Error::msg(
                    "anti-cheat worker",
                    match error {
                        mpsc::RecvTimeoutError::Timeout => "two-second deadline expired",
                        mpsc::RecvTimeoutError::Disconnected => "result channel disconnected",
                    },
                ),
            )
        })
}

struct Matches<'a> {
    database: &'a Database,
    result: Detection,
}

impl<'a> Matches<'a> {
    fn new(database: &'a Database) -> Self {
        Self {
            database,
            result: Detection {
                database_version: Some(database.version),
                ..Detection::default()
            },
        }
    }

    fn observe(&mut self, layer: Layer, name: &str) {
        for entry in &self.database.anticheats {
            let matched = match layer {
                Layer::Service => entry
                    .services
                    .iter()
                    .any(|matcher| service_matches(matcher, name)),
                Layer::Driver => entry
                    .drivers
                    .iter()
                    .any(|d| d.file.eq_ignore_ascii_case(name)),
                Layer::Process => entry
                    .processes
                    .iter()
                    .any(|matcher| matcher.eq_ignore_ascii_case(name)),
                _ => false,
            };
            if !matched {
                continue;
            }
            let index = self
                .result
                .detected
                .iter()
                .position(|e| e.id == entry.id)
                .unwrap_or_else(|| {
                    self.result.detected.push(DetectedAntiCheat {
                        id: entry.id.clone(),
                        name: entry.name.clone(),
                        gate: entry.gate,
                        kernel_mode: entry.kernel_mode,
                        boot_resident: entry.boot_resident,
                        games_sample: entry.games_sample.clone(),
                        evidence: Vec::new(),
                    });
                    self.result.detected.len() - 1
                });
            let evidence = &mut self.result.detected[index].evidence;
            if !evidence
                .iter()
                .any(|e| e.layer == layer && e.matched_name.eq_ignore_ascii_case(name))
            {
                evidence.push(Evidence {
                    layer,
                    matched_name: name.into(),
                });
            }
        }
    }

    fn service(&mut self, name: &str, state: u32) -> Result<()> {
        match state {
            1 => Ok(()), // SERVICE_STOPPED: installation alone is not live evidence.
            2..=7 => {
                self.observe(Layer::Service, name);
                Ok(())
            }
            _ => Err(Error::msg("QueryServiceStatusEx", "invalid service state")),
        }
    }
}

fn service_matches(matcher: &str, name: &str) -> bool {
    // Only an explicit terminal '*' opts into service-family matching. The v1 EAC entry
    // lists shared legacy/EOS names, not per-game prefixes: EasyAntiCheat_MyGame is NOT
    // inferred. Unknown aliases require a researched database change and real verification.
    match matcher.strip_suffix('*') {
        Some(prefix) => name
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix)),
        None => matcher.eq_ignore_ascii_case(name),
    }
}

fn deadline_check(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(Error::msg("anti-cheat enumeration", "deadline expired"))
    } else {
        Ok(())
    }
}

fn detect_worker(deadline: Instant) -> Detection {
    let database = match Database::parse(database::EMBEDDED) {
        Ok(database) => database,
        Err(error) => {
            return Detection::unknown(Layer::Database, Error::msg("anti-cheat database", error));
        }
    };
    let mut matches = Matches::new(&database);
    for (layer, enumerate) in [
        (
            Layer::Service,
            services as fn(&mut Matches<'_>, Instant) -> Result<()>,
        ),
        (Layer::Driver, drivers),
        (Layer::Process, processes),
    ] {
        if let Err(error) =
            deadline_check(deadline).and_then(|()| enumerate(&mut matches, deadline))
        {
            matches.result.fail(layer, error);
        }
    }
    matches.result
}

struct ServiceHandle(SC_HANDLE);
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: This is a uniquely owned SCM/OpenService handle, never a CloseHandle handle.
        if let Err(error) = unsafe { CloseServiceHandle(self.0) } {
            record(Error::from_win("CloseServiceHandle", error));
        }
    }
}

fn services(matches: &mut Matches<'_>, deadline: Instant) -> Result<()> {
    // SAFETY: Null names select the local SCM; only connect/enumerate rights are requested.
    let manager = ServiceHandle(
        unsafe {
            OpenSCManagerW(
                PCWSTR::null(),
                PCWSTR::null(),
                SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
            )
        }
        .map_err(|e| Error::from_win("OpenSCManagerW", e))?,
    );
    // EnumServicesStatusEx can silently omit services whose query ACL denies access.
    // Always probe every database exact name as well, even if enumeration failed.
    let enumerated = enumerate_services(manager.0, matches, deadline);
    let database = matches.database;
    let mut probed = std::collections::HashSet::new();
    let mut probe_error = None;
    for name in database
        .anticheats
        .iter()
        .flat_map(|e| &e.services)
        .filter(|n| !n.ends_with('*'))
    {
        if !probed.insert(name.to_ascii_lowercase()) {
            continue;
        }
        if let Err(error) =
            deadline_check(deadline).and_then(|()| probe_service(manager.0, name, matches))
        {
            probe_error.get_or_insert(error);
            if Instant::now() >= deadline {
                break;
            }
        }
    }
    // Preserve both failure codes; evidence already collected by either path survives.
    if let Err(error) = enumerated {
        if let Some(probe_error) = probe_error {
            matches.result.fail(Layer::Service, probe_error);
        }
        return Err(error);
    }
    probe_error.map_or(Ok(()), Err)
}

fn enumerate_services(
    manager: SC_HANDLE,
    matches: &mut Matches<'_>,
    deadline: Instant,
) -> Result<()> {
    // usize storage supplies ENUM_SERVICE_STATUS_PROCESSW alignment; strings share this buffer.
    let mut storage = vec![0_usize; 64 * 1024 / size_of::<usize>()];
    let mut resume = 0;
    let mut total = 0;
    for _ in 0..=MAX_GROWTH {
        deadline_check(deadline)?;
        let mut needed = 0;
        let mut returned = 0;
        // SAFETY: The slice covers initialized, aligned owned storage and never outlives it.
        let buffer = unsafe {
            std::slice::from_raw_parts_mut(
                storage.as_mut_ptr().cast::<u8>(),
                size_of_val(storage.as_slice()),
            )
        };
        // SAFETY: The SCM handle and all writable buffers/counts are live; no service is changed.
        let status = unsafe {
            EnumServicesStatusExW(
                manager,
                SC_ENUM_PROCESS_INFO,
                SERVICE_DRIVER | SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(buffer),
                &mut needed,
                &mut returned,
                Some(&mut resume),
                PCWSTR::null(),
            )
        }
        .map_err(|e| Error::from_win("EnumServicesStatusExW", e));
        if let Err(error) = &status
            && error.code != ERROR_MORE_DATA.0
        {
            return Err(error.clone());
        }
        let count = returned as usize;
        total += count;
        if total > MAX_SERVICES || count > buffer.len() / size_of::<ENUM_SERVICE_STATUS_PROCESSW>()
        {
            return Err(Error::msg(
                "EnumServicesStatusExW",
                "service count limit or malformed buffer",
            ));
        }
        // SAFETY: Alignment and count were checked; the API initialized the returned records.
        let entries = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                count,
            )
        };
        for entry in entries {
            deadline_check(deadline)?;
            let name = service_name(buffer, entry.lpServiceName)?;
            matches.service(&name, entry.ServiceStatusProcess.dwCurrentState.0)?;
        }
        if status.is_ok() {
            if resume != 0 {
                return Err(Error::msg(
                    "EnumServicesStatusExW",
                    "inconsistent resume handle",
                ));
            }
            return Ok(());
        }
        if needed == 0 {
            return Err(Error::msg(
                "EnumServicesStatusExW",
                "incomplete enumeration",
            ));
        }
        let next = (needed as usize).max(buffer.len()).min(256 * 1024);
        storage.resize(next.div_ceil(size_of::<usize>()), 0);
    }
    Err(Error::msg(
        "EnumServicesStatusExW",
        "enumeration growth/page limit exhausted",
    ))
}

fn service_name(buffer: &[u8], pointer: windows::core::PWSTR) -> Result<String> {
    let start = buffer.as_ptr() as usize;
    let address = pointer.0 as usize;
    let offset = address
        .checked_sub(start)
        .filter(|offset| *offset < buffer.len())
        .ok_or_else(|| Error::msg("EnumServicesStatusExW", "service name outside buffer"))?;
    if !address.is_multiple_of(align_of::<u16>()) {
        return Err(Error::msg(
            "EnumServicesStatusExW",
            "unaligned service name",
        ));
    }
    // SAFETY: The aligned pointer is inside the owned buffer; length stays within that buffer.
    let units =
        unsafe { std::slice::from_raw_parts(pointer.0, ((buffer.len() - offset) / 2).min(257)) };
    let length = units
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| Error::msg("EnumServicesStatusExW", "unterminated service name"))?;
    String::from_utf16(&units[..length])
        .map_err(|_| Error::msg("EnumServicesStatusExW", "invalid service name encoding"))
}

fn probe_service(manager: SC_HANDLE, name: &str, matches: &mut Matches<'_>) -> Result<()> {
    let name_wide = wide::to_wide(name);
    // SAFETY: The terminated database name and manager handle are live; access is query-only.
    let handle =
        match unsafe { OpenServiceW(manager, PCWSTR(name_wide.as_ptr()), SERVICE_QUERY_STATUS) } {
            Ok(handle) => ServiceHandle(handle),
            Err(error) => {
                let error = Error::from_win("OpenServiceW", error);
                if error.code == ERROR_SERVICE_DOES_NOT_EXIST.0 {
                    return Ok(());
                }
                return Err(error);
            }
        };
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    // SAFETY: The byte slice spans a live initialized SDK structure of exactly its advertised size.
    let buffer = unsafe {
        std::slice::from_raw_parts_mut(
            (&raw mut status).cast::<u8>(),
            size_of::<SERVICE_STATUS_PROCESS>(),
        )
    };
    // SAFETY: The service and writable structure/count stay live through this read-only call.
    unsafe { QueryServiceStatusEx(handle.0, SC_STATUS_PROCESS_INFO, Some(buffer), &mut needed) }
        .map_err(|e| Error::from_win("QueryServiceStatusEx", e))?;
    // pcbBytesNeeded is defined only on ERROR_INSUFFICIENT_BUFFER, not on success.
    matches.service(name, status.dwCurrentState.0)
}

struct Impersonation(bool);
impl Impersonation {
    fn restore(&mut self) -> Result<()> {
        // SAFETY: Only our dedicated worker impersonates; reverting discards its private token.
        unsafe { RevertToSelf() }.map_err(|e| Error::from_win("RevertToSelf", e))?;
        self.0 = false;
        Ok(())
    }
}
impl Drop for Impersonation {
    fn drop(&mut self) {
        if self.0
            && let Err(error) = self.restore()
        {
            record(error);
        }
    }
}

fn drivers(matches: &mut Matches<'_>, deadline: Instant) -> Result<()> {
    // SAFETY: Called only on the new gate worker, which has no inherited impersonation token.
    // SeDebugPrivilege is enabled on a private thread token, never the shared process token.
    unsafe { ImpersonateSelf(SecurityImpersonation) }
        .map_err(|e| Error::from_win("ImpersonateSelf", e))?;
    let mut impersonation = Impersonation(true);
    let result = (|| {
        let mut token = HANDLE::default();
        // SAFETY: The borrowed thread handle is valid; token is a writable output.
        unsafe {
            OpenThreadToken(
                GetCurrentThread(),
                TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
                true,
                &mut token,
            )
        }
        .map_err(|e| Error::from_win("OpenThreadToken", e))?;
        // SAFETY: OpenThreadToken returned a uniquely owned CloseHandle-compatible token handle.
        let token = unsafe { OwnedHandle::from_raw(token) }?;
        let mut luid = LUID::default();
        // SAFETY: The literal is terminated and luid is writable for the SDK structure.
        unsafe {
            LookupPrivilegeValueW(
                PCWSTR::null(),
                windows::core::w!("SeDebugPrivilege"),
                &mut luid,
            )
        }
        .map_err(|e| Error::from_win("LookupPrivilegeValueW", e))?;
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: The owned private token and privileges are live. Its original process state
        // is restored by discarding impersonation on every return/panic, without a global race.
        unsafe {
            SetLastError(ERROR_SUCCESS);
            AdjustTokenPrivileges(token.as_raw(), false, Some(&privileges), 0, None, None)
        }
        .map_err(|e| Error::from_win("AdjustTokenPrivileges", e))?;
        // SAFETY: Captures ERROR_NOT_ALL_ASSIGNED immediately, before any other Win32 call.
        if unsafe { GetLastError() } != ERROR_SUCCESS {
            return Err(Error::last("AdjustTokenPrivileges"));
        }
        enumerate_drivers(matches, deadline)
    })();
    // A restoration failure also makes this layer unknown, even if module enumeration succeeded.
    impersonation.restore()?;
    result
}

fn module_count(needed: u32, capacity: usize) -> Result<usize> {
    let bytes = needed as usize;
    if bytes == 0 || !bytes.is_multiple_of(size_of::<*mut c_void>()) {
        return Err(Error::msg(
            "K32EnumDeviceDrivers",
            "empty or malformed module list",
        ));
    }
    let count = bytes / size_of::<*mut c_void>();
    if count > MAX_MODULES || count > capacity {
        return Err(Error::msg(
            "K32EnumDeviceDrivers",
            "module count exceeds buffer or limit",
        ));
    }
    Ok(count)
}

fn enumerate_drivers(matches: &mut Matches<'_>, deadline: Instant) -> Result<()> {
    type Enumerate = unsafe extern "system" fn(*mut *mut c_void, u32, *mut u32) -> BOOL;
    type BaseName = unsafe extern "system" fn(*const c_void, *mut u16, u32) -> u32;
    let library = dll::load_system_dll("kernel32.dll")?;
    let enumerate = library
        .proc_address(c"K32EnumDeviceDrivers")
        .ok_or_else(|| Error::msg("GetProcAddress", "K32EnumDeviceDrivers unavailable"))?;
    let basename = library
        .proc_address(c"K32GetDeviceDriverBaseNameW")
        .ok_or_else(|| Error::msg("GetProcAddress", "K32GetDeviceDriverBaseNameW unavailable"))?;
    // SAFETY: These documented K32 exports have the exact Win32 ABI declared above;
    // the owning System32 library remains live through every invocation.
    let (enumerate, basename): (Enumerate, BaseName) = unsafe {
        (
            std::mem::transmute::<unsafe extern "system" fn() -> isize, Enumerate>(enumerate),
            std::mem::transmute::<unsafe extern "system" fn() -> isize, BaseName>(basename),
        )
    };
    let mut modules = vec![std::ptr::null_mut(); 128];
    for growth in 0..=MAX_GROWTH {
        deadline_check(deadline)?;
        let mut needed = 0;
        // SAFETY: The owned pointer array and byte count agree; needed is writable.
        if !unsafe {
            enumerate(
                modules.as_mut_ptr(),
                size_of_val(modules.as_slice()) as u32,
                &mut needed,
            )
        }
        .as_bool()
        {
            return Err(Error::last("K32EnumDeviceDrivers"));
        }
        if needed as usize > size_of_val(modules.as_slice()) {
            let count = module_count(needed, MAX_MODULES)?;
            if growth == MAX_GROWTH {
                break;
            }
            modules.resize(count, std::ptr::null_mut());
            continue;
        }
        let count = module_count(needed, modules.len())?;
        return match_driver_names(matches, &modules[..count], deadline, |base| {
            let mut name = [0_u16; 1024];
            // SAFETY: base is a non-null opaque value returned by K32, not dereferenced here;
            // the writable basename buffer and owning library remain live.
            let length = unsafe { basename(base, name.as_mut_ptr(), name.len() as u32) } as usize;
            if length == 0 {
                Err(Error::last("K32GetDeviceDriverBaseNameW"))
            } else if length >= name.len() {
                Err(Error::msg(
                    "K32GetDeviceDriverBaseNameW",
                    "truncated basename",
                ))
            } else {
                String::from_utf16(&name[..length]).map_err(|_| {
                    Error::msg("K32GetDeviceDriverBaseNameW", "invalid basename encoding")
                })
            }
        });
    }
    Err(Error::msg(
        "K32EnumDeviceDrivers",
        "module growth limit exhausted",
    ))
}

fn match_driver_names(
    matches: &mut Matches<'_>,
    modules: &[*mut c_void],
    deadline: Instant,
    mut basename: impl FnMut(*mut c_void) -> Result<String>,
) -> Result<()> {
    let mut failure = None;
    for base in modules {
        deadline_check(deadline)?;
        // Windows 11 24H2 can report success with null bases without SeDebugPrivilege.
        // Never pass a null base to the basename API or interpret it as absence.
        let name = if base.is_null() {
            Err(Error::msg(
                "K32EnumDeviceDrivers",
                "null module base (incomplete enumeration)",
            ))
        } else {
            basename(*base)
        };
        match name {
            Ok(name) => matches.observe(Layer::Driver, &name),
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn processes(matches: &mut Matches<'_>, deadline: Instant) -> Result<()> {
    // SAFETY: This takes a read-only system process snapshot; no target process is opened.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| Error::from_win("CreateToolhelp32Snapshot", e))?;
    // SAFETY: The successful snapshot is uniquely owned and CloseHandle-compatible.
    let snapshot = unsafe { OwnedHandle::from_raw(snapshot) }?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: The snapshot and correctly sized, writable SDK entry remain live.
    unsafe { Process32FirstW(snapshot.as_raw(), &mut entry) }
        .map_err(|e| Error::from_win("Process32FirstW", e))?;
    for _ in 0..MAX_PROCESSES {
        deadline_check(deadline)?;
        let length = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .ok_or_else(|| Error::msg("Process32NextW", "unterminated executable name"))?;
        let name = String::from_utf16(&entry.szExeFile[..length])
            .map_err(|_| Error::msg("Process32NextW", "invalid executable name encoding"))?;
        matches.observe(Layer::Process, &name);
        // SAFETY: The same snapshot and initialized SDK entry remain live for iteration.
        if let Err(error) = unsafe { Process32NextW(snapshot.as_raw(), &mut entry) } {
            let error = Error::from_win("Process32NextW", error);
            return if error.code == ERROR_NO_MORE_FILES.0 {
                Ok(())
            } else {
                Err(error)
            };
        }
    }
    Err(Error::msg(
        "Process32NextW",
        "process count limit exhausted",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_database_matcher_is_case_insensitive_and_evidence_is_deduplicated() {
        let database = Database::parse(database::EMBEDDED).unwrap();
        for entry in &database.anticheats {
            for (layer, name) in entry
                .services
                .iter()
                .map(|n| (Layer::Service, n))
                .chain(entry.drivers.iter().map(|d| (Layer::Driver, &d.file)))
                .chain(entry.processes.iter().map(|n| (Layer::Process, n)))
            {
                let mut matches = Matches::new(&database);
                matches.observe(layer, &name.to_ascii_uppercase());
                matches.observe(layer, &name.to_ascii_lowercase());
                let found = matches
                    .result
                    .detected
                    .iter()
                    .find(|e| e.id == entry.id)
                    .unwrap();
                assert_eq!(found.name, entry.name);
                assert_eq!(found.gate, entry.gate);
                assert_eq!(found.evidence.len(), 1);
                assert_eq!(matches.result.presence(), Presence::Detected);
            }
        }
    }

    #[test]
    fn exact_eac_names_and_only_explicit_service_prefixes_match() {
        let database = Database::parse(database::EMBEDDED).unwrap();
        let mut matches = Matches::new(&database);
        for name in [
            "EasyAntiCheat_MyGame",
            "EasyAntiCheat_EOS_extra",
            "notEasyAntiCheat",
            "C:\\EasyAntiCheat.exe",
        ] {
            matches.observe(Layer::Service, name);
            matches.observe(Layer::Process, name);
        }
        assert!(matches.result.detected.is_empty());
        matches.observe(Layer::Service, "eAsYaNtIcHeAt_EoS");
        assert_eq!(matches.result.detected[0].id, "eac");
        assert!(service_matches("VerifiedFamily_*", "verifiedfamily_MyGame"));
        assert!(!service_matches(
            "VerifiedFamily_*",
            "NotVerifiedFamily_MyGame"
        ));
        assert!(!service_matches("VerifiedFamily_", "VerifiedFamily_MyGame"));
    }

    #[test]
    fn stopped_services_do_not_match_but_all_transitional_states_do() {
        let database = Database::parse(database::EMBEDDED).unwrap();
        for state in 1..=7 {
            let mut matches = Matches::new(&database);
            matches.service("vgk", state).unwrap();
            assert_eq!(
                matches.result.gate_decision(),
                if state == 1 {
                    GateDecision::Clear
                } else {
                    GateDecision::Blocked
                }
            );
        }
        let mut matches = Matches::new(&database);
        assert!(matches.service("vgk", 0).is_err());
        assert!(matches.service("vgk", 8).is_err());
        matches.service("vgk", 1).unwrap();
        matches.observe(Layer::Driver, "vgk.sys");
        assert_eq!(matches.result.gate_decision(), GateDecision::Blocked);
        assert!(matches.result.detected[0].boot_resident);
    }

    #[test]
    fn policy_keeps_presence_failures_and_permission_separate() {
        let database = Database::parse(database::EMBEDDED).unwrap();
        assert_eq!(Detection::default().gate_decision(), GateDecision::Unknown);
        for (name, gate) in [
            ("steam.exe", GateDecision::Clear),
            ("RobloxPlayerBeta.exe", GateDecision::Blocked),
            ("vgc.exe", GateDecision::Blocked),
        ] {
            let mut matches = Matches::new(&database);
            matches.observe(Layer::Process, name);
            assert_eq!(matches.result.gate_decision(), gate);
            matches
                .result
                .fail(Layer::Driver, Error::msg("fixture", "access denied"));
            assert_eq!(matches.result.presence(), Presence::Detected);
            assert_eq!(
                matches.result.gate_decision(),
                if gate == GateDecision::Clear {
                    GateDecision::Unknown
                } else {
                    gate
                }
            );
            assert_eq!(matches.result.failed_layers.len(), 1);
        }
        let mut matches = Matches::new(&database);
        assert_eq!(matches.result.presence(), Presence::Clear);
        matches.observe(Layer::Process, "steam.exe");
        matches.observe(Layer::Driver, "vgk.sys");
        assert_eq!(matches.result.detected.len(), 2);
        assert_eq!(matches.result.gate_decision(), GateDecision::Blocked);
        for layer in [Layer::Service, Layer::Driver, Layer::Process] {
            let mut matches = Matches::new(&database);
            matches
                .result
                .fail(layer, Error::msg("fixture", "partial enumeration"));
            assert_eq!(matches.result.presence(), Presence::Unknown);
            assert_eq!(matches.result.gate_decision(), GateDecision::Unknown);
        }
    }

    #[test]
    fn malformed_enumeration_buffers_cannot_be_clear() {
        assert!(module_count(0, MAX_MODULES).is_err());
        assert!(module_count(1, MAX_MODULES).is_err());
        assert!(
            module_count(
                ((MAX_MODULES + 1) * size_of::<*mut c_void>()) as u32,
                MAX_MODULES
            )
            .is_err()
        );
        assert!(module_count(16, 1).is_err());
        assert_eq!(module_count(16, 2).unwrap(), 2);
        let buffer = [0_usize; 8];
        // SAFETY: The initialized aligned storage remains live throughout the bounded parser calls.
        let bytes = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size_of_val(&buffer))
        };
        assert!(service_name(bytes, windows::core::PWSTR::null()).is_err());
        assert!(
            service_name(
                bytes,
                windows::core::PWSTR(
                    buffer
                        .as_ptr()
                        .cast::<u16>()
                        .wrapping_add(1)
                        .cast_mut()
                        .cast::<u8>()
                        .wrapping_add(1)
                        .cast::<u16>()
                )
            )
            .is_err()
        );
        let bad = [usize::MAX; 8];
        // SAFETY: The byte slice exactly covers live initialized storage; it has no NUL terminator.
        let bytes =
            unsafe { std::slice::from_raw_parts(bad.as_ptr().cast::<u8>(), size_of_val(&bad)) };
        assert!(service_name(bytes, windows::core::PWSTR(bad.as_ptr().cast_mut().cast())).is_err());
    }

    #[test]
    fn null_and_partial_module_lists_are_unknown_and_keep_valid_hits() {
        let database = Database::parse(database::EMBEDDED).unwrap();
        let deadline = Instant::now() + BUDGET;
        let mut matches = Matches::new(&database);
        let error = match_driver_names(&mut matches, &[std::ptr::null_mut(); 3], deadline, |_| {
            panic!("null bases must not reach the basename API")
        })
        .unwrap_err();
        matches.result.fail(Layer::Driver, error);
        assert_eq!(matches.result.presence(), Presence::Unknown);
        assert_eq!(matches.result.gate_decision(), GateDecision::Unknown);
        let mut matches = Matches::new(&database);
        let base = std::ptr::without_provenance_mut::<c_void>(0x1000);
        let error = match_driver_names(
            &mut matches,
            &[std::ptr::null_mut(), base],
            deadline,
            |_| Ok("vgk.sys".into()),
        )
        .unwrap_err();
        matches.result.fail(Layer::Driver, error);
        assert_eq!(matches.result.presence(), Presence::Detected);
        assert_eq!(matches.result.gate_decision(), GateDecision::Blocked);
        assert_eq!(matches.result.detected[0].evidence.len(), 1);
        let mut matches = Matches::new(&database);
        let error = match_driver_names(&mut matches, &[base], deadline, |_| {
            Err(Error::msg("basename fixture", "query denied"))
        })
        .unwrap_err();
        matches.result.fail(Layer::Driver, error);
        assert_eq!(matches.result.gate_decision(), GateDecision::Unknown);
    }

    #[test]
    fn timeout_busy_and_panics_refuse_permission_without_extra_workers() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let (release, wait) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let result = bounded_detection(&BUSY, Duration::from_millis(25), move |_| {
            wait.recv().unwrap();
            done.send(()).unwrap();
            Detection {
                database_version: Some(1),
                ..Detection::default()
            }
        });
        assert_eq!(result.gate_decision(), GateDecision::Unknown);
        assert!(BUSY.load(Ordering::Acquire));
        let busy = bounded_detection(&BUSY, BUDGET, |_| panic!("must not start another worker"));
        assert_eq!(busy.gate_decision(), GateDecision::Unknown);
        assert!(busy.failed_layers[0].error.detail.contains("progress"));
        release.send(()).unwrap();
        finished.recv_timeout(BUDGET).unwrap();
        let deadline = Instant::now() + BUDGET;
        while BUSY.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!BUSY.load(Ordering::Acquire));
        assert_eq!(result.gate_decision(), GateDecision::Unknown); // A late clear result cannot replace it.
        let panicked = bounded_detection(&BUSY, BUDGET, |_| panic!("fabricated worker failure"));
        assert_eq!(panicked.gate_decision(), GateDecision::Unknown);
        assert!(!BUSY.load(Ordering::Acquire));
    }

    #[test]
    #[ignore = "live Windows API check; complete detection needs elevation and owner-provided AC/version machines"]
    fn live_read_only_detection() {
        let elevated = super::super::security::is_admin();
        let _ = detect(); // Warm SCM/RPC infrastructure before checking handle stability.
        let handles = || {
            let mut count = 0;
            // SAFETY: The current-process pseudo handle is borrowed; count is writable.
            unsafe {
                windows::Win32::System::Threading::GetProcessHandleCount(
                    windows::Win32::System::Threading::GetCurrentProcess(),
                    &mut count,
                )
            }
            .unwrap();
            count
        };
        let before = handles();
        for _ in 0..3 {
            let start = Instant::now();
            let result = bounded_detection(&WORKER_BUSY, BUDGET, |deadline| {
                let result = detect_worker(deadline);
                let mut token = HANDLE::default();
                // SAFETY: This fresh worker should have restored its original lack of a thread
                // token. The writable output is owned/closed if the assertion finds a leak.
                let remaining =
                    unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token) };
                if remaining.is_ok() {
                    // SAFETY: A successful OpenThreadToken returns a uniquely owned token handle.
                    drop(unsafe { OwnedHandle::from_raw(token) }.unwrap());
                    panic!("gate worker retained impersonation");
                }
                assert_eq!(
                    Error::from_win("OpenThreadToken", remaining.unwrap_err()).code,
                    windows::Win32::Foundation::ERROR_NO_TOKEN.0
                );
                result
            });
            println!(
                "database={:?}, presence={:?}, permission={:?}, matches={}, failures={}, elapsed={:?}",
                result.database_version,
                result.presence(),
                result.gate_decision(),
                result.detected.len(),
                result.failed_layers.len(),
                start.elapsed()
            );
            println!("elevated={elevated}, errors={:?}", result.failed_layers);
            if elevated {
                assert!(
                    result.failed_layers.is_empty(),
                    "{:#?}",
                    result.failed_layers
                );
            } else {
                assert_ne!(result.gate_decision(), GateDecision::Clear);
                assert!(
                    result
                        .failed_layers
                        .iter()
                        .any(|e| e.layer == Layer::Driver)
                );
            }
            assert!(start.elapsed() < Duration::from_secs(3));
        }
        // The send can wake us just before its worker exits; allow that thread to finish closing.
        let deadline = Instant::now() + BUDGET;
        while handles() > before && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(handles() <= before, "gate query handles accumulated");
    }
}
