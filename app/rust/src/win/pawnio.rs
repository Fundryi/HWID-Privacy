//! Opt-in, client-only PawnIO 2.2.0 / official SmbusI801 0.2.11 transport.
//! No SCM, installer, download, driver start/stop or EEPROM-programming operations.
//! Live bring-up and the wider service lifecycle remain unverified/deferred.

use super::{Error as WinError, anticheat, spd};
use std::{fmt, time::Instant};

pub const DEVICE_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";
const VERSION: u32 = 0x0002_0200;
// pawnio_um.h: CTL_CODE(41394, function, METHOD_BUFFERED, FILE_ANY_ACCESS).
const LOAD: u32 = (41394 << 16) | (0x821 << 2);
const EXECUTE: u32 = (41394 << 16) | (0x841 << 2);
const QUERY_VERSION: u32 = (41394 << 16) | (0x861 << 2);
const MODULE_HASH: &str = "a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c";
const MODULE_SIZE: usize = 55_540;
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Debug)]
pub enum Error {
    NotInstalled,
    ModuleUnavailable,
    OpenFailed(WinError),
    VersionUnsupported(u32),
    UnsupportedController,
    ReadFailed(WinError),
    Malformed(&'static str),
    Timeout,
    CleanupIncomplete,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("PawnIO device not installed or not running"),
            Self::ModuleUnavailable => f.write_str("pinned signed SMBus module unavailable"),
            Self::OpenFailed(error) => write!(f, "PawnIO open failed ({}0x{:08X})", if error.code == 5 { "access denied, " } else { "" }, error.code),
            Self::VersionUnsupported(version) => write!(f, "unsupported PawnIO version 0x{version:08X}"),
            Self::UnsupportedController => f.write_str("unsupported SMBus controller or SPD hub mode"),
            Self::ReadFailed(error) => write!(f, "PawnIO read failed ({}0x{:08X})", if error.code == 5 { "access denied, " } else { "" }, error.code),
            Self::Malformed(detail) => write!(f, "malformed PawnIO/SPD data: {detail}"),
            Self::Timeout => f.write_str("PawnIO session or transaction timed out"),
            Self::CleanupIncomplete => f.write_str("Kernel reader cleanup incomplete. Restart Windows before using anti-cheat or another driver scan."),
        }
    }
}
impl std::error::Error for Error {}

impl Error {
    pub fn diagnostic(&self) -> WinError {
        let mut error = WinError::msg("PawnIO SPD", self.to_string());
        if let Self::OpenFailed(inner) | Self::ReadFailed(inner) = self {
            error.code = inner.code;
        }
        error
    }
    fn no_response(&self) -> bool {
        matches!(self, Self::ReadFailed(error) if error.code == 433)
    }
    fn aborts_controller(&self) -> bool {
        matches!(
            self,
            Self::ReadFailed(_) | Self::Timeout | Self::CleanupIncomplete
        )
    }
}

#[derive(Clone, Debug)]
pub struct Module {
    pub address: u8,
    pub manufacturing: spd::Manufacturing,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub modules: Vec<Module>,
    pub failures: Vec<Error>,
    pub gate: Option<anticheat::Detection>,
}

/// Tests implement this exact byte-buffer boundary, never a real device handle.
trait Device {
    fn ioctl(
        &mut self,
        code: u32,
        input: &[u8],
        output: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, Error>;
}

struct Client<D> {
    device: D,
    deadline: Instant,
}

impl<D: Device> Client<D> {
    fn query(device: D, deadline: Instant) -> Result<Self, Error> {
        let mut client = Self { device, deadline };
        let bytes = client.ioctl(QUERY_VERSION, &[], 4)?;
        let version = u32::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| Error::Malformed("version reply size"))?,
        );
        if version != VERSION {
            return Err(Error::VersionUnsupported(version));
        }
        Ok(client)
    }
    fn initialize(&mut self, module: &[u8]) -> Result<(), Error> {
        self.ioctl(LOAD, module, 0)?;
        // Upstream has no generic capability IOCTL: query the selected module's identity.
        let identity = self.execute("ioctl_identity", &[], 3)?;
        if identity[0] != u64::from(u32::from_le_bytes(*b"i801")) || identity[2] & 0xffff != 0x8086
        {
            return Err(Error::UnsupportedController);
        }
        Ok(())
    }
    fn ioctl(&mut self, code: u32, input: &[u8], output: usize) -> Result<Vec<u8>, Error> {
        if Instant::now() >= self.deadline {
            return Err(Error::Timeout);
        }
        let bytes = self.device.ioctl(code, input, output, self.deadline)?;
        if bytes.len() != output {
            return Err(Error::Malformed("IOCTL reply size"));
        }
        Ok(bytes)
    }
    fn execute(
        &mut self,
        name: &'static str,
        cells: &[u64],
        output: usize,
    ) -> Result<Vec<u64>, Error> {
        if name.len() >= 32 || cells.len() > 5 || output > 3 {
            return Err(Error::Malformed("execute request bounds"));
        }
        let mut input = vec![0; 32];
        input[..name.len()].copy_from_slice(name.as_bytes());
        for cell in cells {
            input.extend_from_slice(&cell.to_le_bytes());
        }
        let bytes = self.ioctl(EXECUTE, &input, output * 8)?;
        Ok(bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|cell| u64::from_le_bytes(*cell))
            .collect())
    }
    fn read_byte(&mut self, address: u8, command: u8) -> Result<u8, Error> {
        let reply = self.execute(
            "ioctl_smbus_xfer",
            &[u64::from(address), 1, u64::from(command), 2],
            1,
        )?;
        u8::try_from(reply[0]).map_err(|_| Error::Malformed("SMBus byte exceeds eight bits"))
    }
    fn ddr4_page(&mut self, page: u8) -> Result<(), Error> {
        // EE1004 SPA0/SPA1 send-byte; not an EEPROM write.
        self.execute("ioctl_smbus_xfer", &[0x36 + u64::from(page), 0, 0, 1, 0], 0)?;
        Ok(())
    }
    fn hub_page(&mut self, address: u8, value: u8) -> Result<(), Error> {
        // SPD5118 MR11 only: preserve mode/reserved bits; no NVM address is writable here.
        self.execute(
            "ioctl_smbus_xfer",
            &[u64::from(address), 0, 0x0b, 2, u64::from(value)],
            0,
        )?;
        Ok(())
    }
    fn slot(&mut self, address: u8) -> Result<Option<spd::Manufacturing>, Error> {
        if !(0x50..=0x57).contains(&address) {
            return Err(Error::Malformed("SPD address outside slot range"));
        }
        let first = match self.read_byte(address, 0) {
            Ok(byte) => byte,
            Err(error) if error.no_response() => return Ok(None),
            Err(error) => return Err(error),
        };
        // SPD5118 MR0:MR1 is big-endian 0x5118. Other DDR5 hub types stay unsupported.
        if first == 0x51 && self.read_byte(address, 1)? == 0x18 {
            let original = self.read_byte(address, 0x0b)?;
            if original & 0xf8 != 0 {
                return Err(Error::UnsupportedController);
            }
            let read = (|| {
                let mut bytes = vec![0; 1024];
                for page in 0..8 {
                    self.hub_page(address, page)?;
                    for offset in 0..128 {
                        bytes[usize::from(page) * 128 + offset] =
                            self.read_byte(address, 0x80 | offset as u8)?;
                    }
                }
                spd::decode(&bytes).map_err(|_| Error::Malformed("DDR5 layout or required CRC"))
            })();
            // Restore the page on every exit, even when the read's budget has expired.
            self.restore(|client| client.hub_page(address, original))?;
            return read.map(Some);
        }
        // Only an EE1004-looking header permits DDR4 page-selection commands.
        let original_page = match self.execute("ioctl_smbus_xfer", &[0x36, 1, 0, 1], 1) {
            Ok(_) => 0,
            Err(error) if error.no_response() => 1,
            Err(error) => return Err(error),
        };
        if original_page == 0 && first != 0x23 && first != 0x24 {
            return Err(Error::UnsupportedController);
        }
        let read = (|| {
            let mut bytes = vec![0; 512];
            for page in 0..2 {
                self.ddr4_page(page)?;
                for offset in 0..256 {
                    bytes[usize::from(page) * 256 + offset] =
                        self.read_byte(address, offset as u8)?;
                }
            }
            spd::decode(&bytes).map_err(|_| Error::Malformed("DDR4 layout or required CRC"))
        })();
        self.restore(|client| client.ddr4_page(original_page))?;
        read.map(Some)
    }
    fn restore(&mut self, work: impl FnOnce(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        let deadline = self.deadline;
        self.deadline = self
            .deadline
            .max(Instant::now() + std::time::Duration::from_millis(500));
        let result = work(self).map_err(|_| Error::CleanupIncomplete);
        self.deadline = deadline;
        result
    }
}

fn collect_with<D: Device>(
    enabled: bool,
    initial: &anticheat::Detection,
    open: impl FnOnce() -> Result<(D, Vec<u8>), Error>,
    mut gate: impl FnMut() -> anticheat::Detection,
    deadline: Instant,
    mut completed: impl FnMut(&Module),
) -> Snapshot {
    let mut result = Snapshot::default();
    if !enabled {
        return result;
    }
    if initial.gate_decision() != anticheat::GateDecision::Clear {
        result.gate = Some(initial.clone());
        return result;
    }
    if Instant::now() >= deadline {
        result.failures.push(Error::Timeout);
        return result;
    }
    let fresh = gate();
    if fresh.gate_decision() != anticheat::GateDecision::Clear {
        result.gate = Some(fresh);
        return result;
    }
    let mut client = match open().and_then(|(device, module)| {
        let mut client = Client::query(device, deadline)?;
        let fresh = gate();
        if fresh.gate_decision() != anticheat::GateDecision::Clear {
            result.gate = Some(fresh);
            return Ok(None);
        }
        client.initialize(&module)?;
        Ok(Some(client))
    }) {
        Ok(Some(client)) => client,
        Ok(None) => return result,
        Err(error) => {
            result.failures.push(error);
            return result;
        }
    };
    // One audited i801 controller, eight standard SPD addresses; no arbitrary bus scan.
    for address in 0x50..=0x57 {
        if Instant::now() >= deadline {
            result.failures.push(Error::Timeout);
            break;
        }
        let fresh = gate();
        if fresh.gate_decision() != anticheat::GateDecision::Clear {
            result.gate = Some(fresh);
            break;
        }
        match client.slot(address) {
            Ok(Some(manufacturing)) => {
                if manufacturing.serial.as_ref().is_some_and(|serial| {
                    result
                        .modules
                        .iter()
                        .any(|module| module.manufacturing.serial.as_ref() == Some(serial))
                }) {
                    result.failures.push(Error::Malformed(
                        "manufacturing serial repeated across SPD slots; association unproven",
                    ));
                }
                let module = Module {
                    address,
                    manufacturing,
                };
                completed(&module);
                result.modules.push(module);
            }
            Ok(None) => {}
            Err(error) => {
                let abort = error.aborts_controller();
                result.failures.push(error);
                if abort {
                    break;
                }
            }
        }
    }
    result
}

#[cfg(not(test))]
pub fn collect(initial: &anticheat::Detection) -> Snapshot {
    native::collect(initial.clone())
}

// An ordinary cargo test cannot open PawnIO, even on a machine with it installed.
#[cfg(test)]
pub fn collect(_: &anticheat::Detection) -> Snapshot {
    Snapshot {
        failures: vec![Error::ModuleUnavailable],
        ..Snapshot::default()
    }
}

#[cfg(not(test))]
mod native {
    use super::*;
    use crate::win::{self, OwnedHandle, wide};
    use std::{
        io::Read,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::Duration,
    };
    use windows::{
        Win32::{
            Foundation::{
                ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PATH_NOT_FOUND, GENERIC_READ,
                GENERIC_WRITE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
            },
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_DELETE, FILE_SHARE_READ,
                FILE_SHARE_WRITE, OPEN_EXISTING,
            },
            System::{
                IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED},
                Threading::{CreateEventW, CreateMutexW, ReleaseMutex, WaitForSingleObject},
            },
        },
        core::PCWSTR,
    };

    static BUSY: AtomicBool = AtomicBool::new(false);
    static QUARANTINED: AtomicBool = AtomicBool::new(false);
    struct Busy;
    impl Drop for Busy {
        fn drop(&mut self) {
            BUSY.store(false, Ordering::Release);
        }
    }
    struct BusLock(OwnedHandle);
    impl Drop for BusLock {
        fn drop(&mut self) {
            // SAFETY: This thread acquired this mutex; release precedes handle close.
            if let Err(error) = unsafe { ReleaseMutex(self.0.as_raw()) } {
                win::record(WinError::from_win("PawnIO SMBus ReleaseMutex", error));
            }
        }
    }
    struct NativeDevice {
        handle: Arc<OwnedHandle>,
        _bus: BusLock,
    }
    struct Request {
        _handle: Arc<OwnedHandle>,
        event: OwnedHandle,
        overlapped: OVERLAPPED,
        input: Vec<u8>,
        output: Vec<u8>,
    }
    impl Device for NativeDevice {
        fn ioctl(
            &mut self,
            code: u32,
            input: &[u8],
            output: usize,
            deadline: Instant,
        ) -> Result<Vec<u8>, Error> {
            if QUARANTINED.load(Ordering::Acquire) {
                return Err(Error::CleanupIncomplete);
            }
            // SAFETY: No borrowed security/name pointers; event is uniquely owned.
            let event = unsafe { CreateEventW(None, true, false, None) }
                .map_err(|error| Error::ReadFailed(WinError::from_win("PawnIO event", error)))?;
            // SAFETY: CreateEventW returned a newly owned kernel handle.
            let event = unsafe { OwnedHandle::from_raw(event) }.map_err(Error::ReadFailed)?;
            let mut request = Box::new(Request {
                _handle: Arc::clone(&self.handle),
                overlapped: OVERLAPPED {
                    hEvent: event.as_raw(),
                    ..OVERLAPPED::default()
                },
                event,
                input: input.to_vec(),
                output: vec![0; output],
            });
            let mut returned = 0;
            // SAFETY: Stable boxed storage and the borrowed handle outlive completion/drain.
            let started = unsafe {
                DeviceIoControl(
                    self.handle.as_raw(),
                    code,
                    (!request.input.is_empty()).then_some(request.input.as_ptr().cast()),
                    request.input.len() as u32,
                    (!request.output.is_empty()).then_some(request.output.as_mut_ptr().cast()),
                    request.output.len() as u32,
                    Some(&mut returned),
                    Some(&mut request.overlapped),
                )
            };
            if let Err(error) = started {
                let error = WinError::from_win("PawnIO IOCTL", error);
                if error.code != ERROR_IO_PENDING.0 {
                    return Err(Error::ReadFailed(error));
                }
                let wait = deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(250))
                    .as_millis() as u32;
                // SAFETY: Request owns the live event; wait is bounded.
                let status = unsafe { WaitForSingleObject(request.event.as_raw(), wait) };
                if status != WAIT_OBJECT_0 {
                    // SAFETY: The exact live OVERLAPPED belongs to this device request.
                    let _ = unsafe { CancelIoEx(self.handle.as_raw(), Some(&request.overlapped)) };
                    // SAFETY: Keep all buffers alive through this bounded cancellation drain.
                    if unsafe { WaitForSingleObject(request.event.as_raw(), 250) } != WAIT_OBJECT_0
                    {
                        QUARANTINED.store(true, Ordering::Release);
                        // ponytail: retain one undrained request until reboot; audited cancellation is the upgrade path.
                        Box::leak(request);
                        return Err(Error::CleanupIncomplete);
                    }
                    return Err(if status == WAIT_TIMEOUT {
                        Error::Timeout
                    } else {
                        Error::ReadFailed(WinError::last("PawnIO wait"))
                    });
                }
                // SAFETY: Event signalled completion; no blocking wait or pending buffer use.
                unsafe {
                    GetOverlappedResult(
                        self.handle.as_raw(),
                        &request.overlapped,
                        &mut returned,
                        false,
                    )
                }
                .map_err(|error| {
                    Error::ReadFailed(WinError::from_win("PawnIO completion", error))
                })?;
            }
            if returned as usize != output {
                return Err(Error::Malformed("native reply length"));
            }
            Ok(std::mem::take(&mut request.output))
        }
    }

    fn open() -> Result<(NativeDevice, Vec<u8>), Error> {
        let root = std::env::var_os("ProgramData").ok_or(Error::ModuleUnavailable)?;
        let path = std::path::PathBuf::from(root)
            .join("HWIDChecker/kernel-reader/pawnio/0.2.11/SmbusI801.bin");
        let file = std::fs::File::open(path).map_err(|_| Error::ModuleUnavailable)?;
        let mut module = Vec::new();
        file.take(MODULE_SIZE as u64 + 1)
            .read_to_end(&mut module)
            .map_err(|_| Error::ModuleUnavailable)?;
        if module.len() != MODULE_SIZE
            || !win::hash::sha256_bytes(&module)
                .map_err(Error::ReadFailed)?
                .eq_ignore_ascii_case(MODULE_HASH)
        {
            return Err(Error::Malformed(
                "signed module size or pinned SHA-256 mismatch",
            ));
        }
        let name = wide::to_wide(r"Global\Access_SMBUS.HTP.Method");
        // SAFETY: NUL-terminated name lives through call; no inherited/security handles.
        let mutex = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|e| Error::OpenFailed(WinError::from_win("PawnIO SMBus mutex", e)))?;
        // SAFETY: Newly owned mutex handle is CloseHandle-compatible.
        let mutex = unsafe { OwnedHandle::from_raw(mutex) }.map_err(Error::OpenFailed)?;
        // SAFETY: Borrowed mutex remains live; zero wait avoids competing monitoring tools.
        let acquired = unsafe { WaitForSingleObject(mutex.as_raw(), 0) };
        if acquired == WAIT_ABANDONED {
            drop(BusLock(mutex));
            return Err(Error::CleanupIncomplete);
        }
        if acquired != WAIT_OBJECT_0 {
            return Err(Error::ReadFailed(WinError::msg(
                "PawnIO SMBus mutex",
                "controller busy",
            )));
        }
        let bus = BusLock(mutex);
        let path = wide::to_wide(DEVICE_PATH);
        // SAFETY: Device path lives through call; overlapped handle is owned by this session.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        }
        .map_err(|e| {
            let error = WinError::from_win("PawnIO open", e);
            if [ERROR_FILE_NOT_FOUND.0, ERROR_PATH_NOT_FOUND.0].contains(&error.code) {
                Error::NotInstalled
            } else {
                Error::OpenFailed(error)
            }
        })?;
        // SAFETY: CreateFileW returned the sole owning handle for this file-object/VM.
        let handle = unsafe { OwnedHandle::from_raw(handle) }.map_err(Error::OpenFailed)?;
        Ok((
            NativeDevice {
                handle: Arc::new(handle),
                _bus: bus,
            },
            module,
        ))
    }

    pub(super) fn collect(initial: anticheat::Detection) -> Snapshot {
        if QUARANTINED.load(Ordering::Acquire) {
            return Snapshot {
                failures: vec![Error::CleanupIncomplete],
                ..Snapshot::default()
            };
        }
        if BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Snapshot {
                failures: vec![Error::Timeout],
                ..Snapshot::default()
            };
        }
        let busy = Busy;
        let deadline = Instant::now() + BUDGET;
        let (send, receive) = mpsc::channel();
        enum Event {
            Module(Module),
            Done(Snapshot),
        }
        let spawned = std::thread::Builder::new()
            .name("PawnIO SPD".into())
            .spawn(move || {
                let _busy = busy;
                let result = win::catch_panic(|| {
                    collect_with(
                        true,
                        &initial,
                        open,
                        anticheat::detect,
                        deadline,
                        |module| {
                            let _ = send.send(Event::Module(module.clone()));
                        },
                    )
                })
                .unwrap_or_else(|_| Snapshot {
                    failures: vec![Error::CleanupIncomplete],
                    ..Snapshot::default()
                });
                if result
                    .failures
                    .iter()
                    .any(|failure| matches!(failure, Error::CleanupIncomplete))
                {
                    QUARANTINED.store(true, Ordering::Release);
                }
                let _ = send.send(Event::Done(result));
            });
        if spawned.is_err() {
            return Snapshot {
                failures: vec![Error::OpenFailed(WinError::msg(
                    "PawnIO worker",
                    "cannot spawn worker",
                ))],
                ..Snapshot::default()
            };
        }
        let mut partial = Snapshot::default();
        loop {
            match receive.recv_timeout(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(500),
            ) {
                Ok(Event::Module(module)) => partial.modules.push(module),
                Ok(Event::Done(result)) => return result,
                Err(_) => {
                    // A synchronous dispatch can outlive cancellation. Never announce cleanup/success.
                    QUARANTINED.store(true, Ordering::Release);
                    partial.failures.push(Error::CleanupIncomplete);
                    return partial;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    struct Fake {
        bytes: Vec<u8>,
        page: u8,
        hub_page: u8,
        loaded: bool,
        calls: Rc<Cell<usize>>,
        fail_address: Option<u8>,
        fail_command: Option<u8>,
        duplicate: bool,
        truncated: bool,
        version: u32,
    }
    impl Fake {
        fn new(generation: spd::Generation) -> Self {
            Self {
                bytes: spd::fixture(generation),
                page: 0,
                hub_page: 2,
                loaded: false,
                calls: Rc::new(Cell::new(0)),
                fail_address: None,
                fail_command: None,
                duplicate: false,
                truncated: false,
                version: VERSION,
            }
        }
    }
    impl Device for Fake {
        fn ioctl(
            &mut self,
            code: u32,
            input: &[u8],
            output: usize,
            _: Instant,
        ) -> Result<Vec<u8>, Error> {
            self.calls.set(self.calls.get() + 1);
            match code {
                QUERY_VERSION => {
                    assert!(input.is_empty());
                    assert_eq!(output, 4);
                    Ok(self.version.to_le_bytes()[..if self.truncated { 3 } else { 4 }].to_vec())
                }
                LOAD => {
                    assert_eq!(input, b"signed-fixture");
                    self.loaded = true;
                    Ok(vec![])
                }
                EXECUTE => {
                    assert!(self.loaded);
                    let name = std::str::from_utf8(&input[..32])
                        .unwrap()
                        .trim_end_matches('\0');
                    if name == "ioctl_identity" {
                        assert_eq!(input.len(), 32);
                        return Ok(
                            [u64::from(u32::from_le_bytes(*b"i801")), 0xef80, 0x7a238086]
                                .into_iter()
                                .flat_map(u64::to_le_bytes)
                                .collect(),
                        );
                    }
                    assert_eq!(name, "ioctl_smbus_xfer");
                    let cells: Vec<u64> = input[32..]
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| u64::from_le_bytes(*c))
                        .collect();
                    let [address, direction, command, protocol] = cells[..4] else {
                        panic!("four protocol cells required")
                    };
                    if direction == 0 {
                        assert_eq!(cells.len(), 5);
                        assert_eq!(output, 0);
                        if address == 0x36 || address == 0x37 {
                            assert_eq!((command, protocol, cells[4]), (0, 1, 0));
                            self.page = (address - 0x36) as u8;
                        } else {
                            assert_eq!((address, command, protocol), (0x50, 0x0b, 2));
                            self.hub_page = cells[4] as u8;
                        }
                        return Ok(vec![]);
                    }
                    assert_eq!(direction, 1);
                    assert_eq!(output, 8);
                    if Some(address as u8) == self.fail_address {
                        return Err(Error::Timeout);
                    }
                    if address == 0x50 && Some(command as u8) == self.fail_command {
                        return Err(Error::Timeout);
                    }
                    let value = if address == 0x36 {
                        assert_eq!(protocol, 1);
                        if self.page == 1 {
                            return Err(Error::ReadFailed(WinError {
                                op: "fake",
                                code: 433,
                                detail: "no response".into(),
                            }));
                        }
                        0
                    } else if address != 0x50 && !(self.duplicate && address == 0x51) {
                        assert!((0x50..=0x57).contains(&address));
                        return Err(Error::ReadFailed(WinError {
                            op: "fake",
                            code: 433,
                            detail: "no response".into(),
                        }));
                    } else if self.bytes.len() == 512 {
                        assert_eq!(protocol, 2);
                        self.bytes[self.page as usize * 256 + command as usize]
                    } else {
                        assert_eq!(protocol, 2);
                        match command {
                            0 => 0x51,
                            1 => 0x18,
                            0x0b => self.hub_page,
                            0x80..=0xff => {
                                self.bytes[self.hub_page as usize * 128 + command as usize - 0x80]
                            }
                            _ => panic!("unexpected hub command"),
                        }
                    };
                    Ok(u64::from(value).to_le_bytes().to_vec())
                }
                _ => panic!("unknown IOCTL"),
            }
        }
    }
    fn clear() -> anticheat::Detection {
        anticheat::Detection {
            database_version: Some(1),
            ..Default::default()
        }
    }

    #[test]
    fn documented_protocol_reads_fabricated_dimm_and_restores_page_for_both_generations() {
        for generation in [spd::Generation::Ddr4, spd::Generation::Ddr5] {
            let fake = Fake::new(generation);
            let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
            client.initialize(b"signed-fixture").unwrap();
            let decoded = client.slot(0x50).unwrap().unwrap();
            assert_eq!(decoded.serial.as_deref(), Some("8C31D7A2"));
            assert_eq!(client.device.page, 0);
            assert_eq!(client.device.hub_page, 2);
            assert!(client.slot(0x51).unwrap().is_none());
            assert!(client.slot(0x58).is_err());
        }
    }

    #[test]
    fn option_and_gate_refuse_before_open_and_recheck_before_module_and_slots() {
        let unknown = anticheat::Detection::default();
        for (enabled, snapshot) in [(false, clear()), (true, unknown.clone())] {
            let result = collect_with::<Fake>(
                enabled,
                &snapshot,
                || panic!("must not open"),
                || panic!("no fresh check required"),
                Instant::now() + BUDGET,
                |_| {},
            );
            assert!(result.modules.is_empty());
        }
        let fake = Fake::new(spd::Generation::Ddr4);
        let calls = Rc::clone(&fake.calls);
        let mut gates = [clear(), unknown.clone()].into_iter();
        let result = collect_with(
            true,
            &clear(),
            || Ok((fake, b"signed-fixture".to_vec())),
            || gates.next().unwrap(),
            Instant::now() + BUDGET,
            |_| {},
        );
        assert!(result.gate.is_some());
        assert_eq!(calls.get(), 1, "version only, no module init");
        let fake = Fake::new(spd::Generation::Ddr4);
        let calls = Rc::clone(&fake.calls);
        let mut gates = [clear(), clear(), unknown].into_iter();
        let result = collect_with(
            true,
            &clear(),
            || Ok((fake, b"signed-fixture".to_vec())),
            || gates.next().unwrap(),
            Instant::now() + BUDGET,
            |_| {},
        );
        assert!(result.gate.is_some());
        assert_eq!(calls.get(), 3, "no first hardware read");
    }

    #[test]
    fn failed_reads_restore_pages_and_repeated_serials_stay_separate() {
        for generation in [spd::Generation::Ddr4, spd::Generation::Ddr5] {
            let mut fake = Fake::new(generation);
            fake.fail_command = Some(if generation == spd::Generation::Ddr4 {
                0x20
            } else {
                0xa0
            });
            let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
            client.initialize(b"signed-fixture").unwrap();
            assert!(matches!(client.slot(0x50), Err(Error::Timeout)));
            assert_eq!(client.device.page, 0);
            assert_eq!(client.device.hub_page, 2);
        }
        let mut fake = Fake::new(spd::Generation::Ddr4);
        fake.page = 1;
        let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
        client.initialize(b"signed-fixture").unwrap();
        assert!(client.slot(0x50).unwrap().is_some());
        assert_eq!(client.device.page, 1);
        let mut fake = Fake::new(spd::Generation::Ddr4);
        fake.duplicate = true;
        let result = collect_with(
            true,
            &clear(),
            || Ok((fake, b"signed-fixture".to_vec())),
            clear,
            Instant::now() + BUDGET,
            |_| {},
        );
        assert_eq!(result.modules.len(), 2);
        assert_eq!(result.modules[0].address, 0x50);
        assert_eq!(result.modules[1].address, 0x51);
        assert!(
            result
                .failures
                .iter()
                .any(|e| e.to_string().contains("repeated"))
        );
        let mut fake = Fake::new(spd::Generation::Ddr4);
        fake.truncated = true;
        assert!(matches!(
            Client::query(fake, Instant::now() + BUDGET),
            Err(Error::Malformed(_))
        ));
    }

    #[test]
    fn partial_slots_survive_read_failure_and_errors_are_distinct() {
        let mut fake = Fake::new(spd::Generation::Ddr4);
        fake.fail_address = Some(0x51);
        let result = collect_with(
            true,
            &clear(),
            || Ok((fake, b"signed-fixture".to_vec())),
            clear,
            Instant::now() + BUDGET,
            |_| {},
        );
        assert_eq!(result.modules.len(), 1);
        assert!(matches!(result.failures[0], Error::Timeout));
        let mut fake = Fake::new(spd::Generation::Ddr4);
        fake.version = 0x10000;
        assert!(matches!(
            Client::query(fake, Instant::now() + BUDGET),
            Err(Error::VersionUnsupported(0x10000))
        ));
        assert!(matches!(
            Client::query(Fake::new(spd::Generation::Ddr4), Instant::now()),
            Err(Error::Timeout)
        ));
        for error in [
            Error::NotInstalled,
            Error::ModuleUnavailable,
            Error::OpenFailed(WinError::msg("fake", "denied")),
            Error::VersionUnsupported(1),
            Error::UnsupportedController,
            Error::ReadFailed(WinError::msg("fake", "failed")),
            Error::Malformed("bad size"),
            Error::Timeout,
            Error::CleanupIncomplete,
        ] {
            assert!(!error.to_string().is_empty());
            assert!(!error.diagnostic().detail.contains("8C31D7A2"));
        }
        assert_eq!(LOAD, 0xa1b22084);
        assert_eq!(EXECUTE, 0xa1b22104);
        assert_eq!(QUERY_VERSION, 0xa1b22184);
        assert_eq!(MODULE_HASH.len(), 64);
        assert_eq!(MODULE_SIZE, 55_540);
        assert!(DEVICE_PATH.ends_with("\\Device\\PawnIO"));
    }
}
