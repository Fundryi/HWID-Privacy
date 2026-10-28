//! Opt-in PawnIO 2.2.0 / official SmbusI801 0.2.11 transport, both embedded in the exe.
//! Uses an existing PawnIO install, or installs the embedded signed setup for one
//! session and uninstalls it afterwards. No download or EEPROM-programming operations.

use super::{Error as WinError, anticheat, spd};
use std::{fmt, time::Instant};

pub const DEVICE_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";
const VERSION: u32 = 0x0002_0200;
// pawnio_um.h: CTL_CODE(41394, function, METHOD_BUFFERED, FILE_ANY_ACCESS).
const LOAD: u32 = (41394 << 16) | (0x821 << 2);
const EXECUTE: u32 = (41394 << 16) | (0x841 << 2);
const QUERY_VERSION: u32 = (41394 << 16) | (0x861 << 2);
/// Official signed module, PawnIO.Modules release 0.2.11.
const MODULE: &[u8] = include_bytes!("../../assets/pawnio/SmbusI801.bin");
const MODULE_HASH: &str = "a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c";
const MODULE_SIZE: usize = 55_540;
/// Official signed installer, PawnIO.Setup release 2.2.0.
const SETUP: &[u8] = include_bytes!("../../assets/pawnio/PawnIO_setup.exe");
const SETUP_HASH: &str = "1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032";
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
    InstallFailed(WinError),
    /// MR11 did not read back the selected SPD5118 page; no data is read then.
    PageSelectIgnored,
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
            Self::InstallFailed(error) => write!(f, "PawnIO setup failed ({}: {})", error.op, error.detail),
            Self::PageSelectIgnored => f.write_str("DDR5 SPD page select was not honored (for example, the SMBus controller is in I2C mode)"),
        }
    }
}
impl std::error::Error for Error {}

impl Error {
    pub fn diagnostic(&self) -> WinError {
        let mut error = WinError::msg("PawnIO SPD", self.to_string());
        if let Self::OpenFailed(inner) | Self::ReadFailed(inner) | Self::InstallFailed(inner) = self
        {
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
            Self::ReadFailed(_)
                | Self::Timeout
                | Self::CleanupIncomplete
                // Board-wide: every other DDR5 slot would fail the same way.
                | Self::PageSelectIgnored
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
    /// Intel SPD Write Disable (SMBHSTCFG bit 4): the PCH refuses writes to 0x50..0x57.
    write_protected: bool,
}

impl<D: Device> Client<D> {
    fn query(device: D, deadline: Instant) -> Result<Self, Error> {
        let mut client = Self {
            device,
            deadline,
            write_protected: false,
        };
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
        self.write_protected = self.execute("ioctl_write_protection", &[], 1)?[0] != 0;
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
        if self.write_protected {
            // The PCH blocks write-flagged transactions to 0x50..0x57. A Process Call with
            // the read bit passes and still writes MR11 = page; the high byte (0) lands in
            // set-only MR12. Memtest86+ PR #441 and RAMSPDToolkit 1.2.0 do the same; see
            // docs/research/2026-10-28-ddr5-spd-intel-write-protect.md.
            self.execute(
                "ioctl_smbus_xfer",
                &[u64::from(address), 1, 0x0b, 4, u64::from(value)],
                1,
            )?;
        } else {
            self.execute(
                "ioctl_smbus_xfer",
                &[u64::from(address), 0, 0x0b, 2, u64::from(value)],
                0,
            )?;
        }
        if self.read_byte(address, 0x0b)? & 0x07 != value & 0x07 {
            return Err(Error::PageSelectIgnored);
        }
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
            if self.write_protected {
                super::record(WinError::msg(
                    "PawnIO SPD",
                    format!(
                        "SPD5118 at 0x{address:02X}: MR11 0x{original:02X}, SPD write protection on"
                    ),
                ));
            }
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
            let restored = self.restore(|client| client.hub_page(address, original));
            return Self::settle(read, restored).map(Some);
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
        let restored = self.restore(|client| client.ddr4_page(original_page));
        Self::settle(read, restored).map(Some)
    }
    fn restore(&mut self, work: impl FnOnce(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        let deadline = self.deadline;
        self.deadline = self
            .deadline
            .max(Instant::now() + std::time::Duration::from_millis(500));
        let result = work(self);
        self.deadline = deadline;
        result
    }
    /// The read error wins; a restore failure is then only recorded. Neither becomes
    /// CleanupIncomplete, which is reserved for undrained requests.
    fn settle<T>(read: Result<T, Error>, restored: Result<(), Error>) -> Result<T, Error> {
        match (read, restored) {
            (Err(error), Err(restore)) => {
                super::record(WinError::msg(
                    "PawnIO SPD page restore",
                    restore.to_string(),
                ));
                Err(error)
            }
            (read, Ok(())) => read,
            (Ok(_), Err(restore)) => Err(restore),
        }
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
                // A blank serial (all 00 or FF) repeats on every unprogrammed module of a kit.
                if manufacturing.serial.as_ref().is_some_and(|serial| {
                    serial != "00000000"
                        && serial != "FFFFFFFF"
                        && result
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

/// Uninstalls a PawnIO this app installed in an earlier session that did not finish
/// cleanup. A PawnIO without this app's ownership marker is never touched.
#[cfg(not(test))]
pub fn recover() {
    native::recover();
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
    use crate::win::{self, OwnedHandle, hash, process, wide};
    use std::{
        fs::{self, File},
        io::{self, Write},
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
        path::{Path, PathBuf},
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
                GENERIC_WRITE, HWND, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
            },
            Security::WinTrust::{
                WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
                WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
                WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrust,
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

    const MARKER: &str = "installed-by-hwidchecker";
    const SETUP_TIMEOUT: Duration = Duration::from_secs(60);
    /// How long the PnP device may take to appear after a successful setup run.
    const DEVICE_WAIT: Duration = Duration::from_secs(10);

    fn install_error(op: &'static str, detail: impl Into<String>) -> Error {
        Error::InstallFailed(WinError::msg(op, detail))
    }

    fn folder() -> Result<PathBuf, Error> {
        let root = std::env::var_os("ProgramData")
            .ok_or_else(|| install_error("PawnIO folder", "ProgramData is not set"))?;
        Ok(PathBuf::from(root).join(r"HWIDChecker\pawnio"))
    }

    fn trusted(path: &Path) -> bool {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(path.as_ptr()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            // The SHA-256 pin already fixes the bytes; skip online revocation lookups.
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        // SAFETY: data, file and the NUL-terminated path outlive both calls; no UI window.
        let status =
            unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        // SAFETY: Releases the state the verify call above stored in data.
        unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
        status == 0
    }

    /// Writes the embedded setup, verifies it on disk and keeps a handle that blocks
    /// writes and deletes, so the verified bytes are the bytes that run.
    // shortcut: the folder inherits the ProgramData ACL; add an admin-only ACL if local non-admin tampering matters.
    fn stage(folder: &Path) -> Result<(PathBuf, File), Error> {
        let io = |op| move |error: io::Error| Error::InstallFailed(hash::io_error(op, error));
        let path = folder.join("PawnIO_setup.exe");
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Err(io("Remove stale PawnIO setup")(error));
            }
            _ => {}
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(SETUP))
            .map_err(io("Write PawnIO setup"))?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&path)
            .map_err(io("Open PawnIO setup"))?;
        let digest = hash::sha256_file(&file).map_err(Error::InstallFailed)?;
        if !digest.eq_ignore_ascii_case(SETUP_HASH) {
            return Err(install_error("PawnIO setup", "pinned SHA-256 mismatch"));
        }
        if !trusted(&path) {
            return Err(install_error(
                "PawnIO setup",
                "Authenticode signature invalid",
            ));
        }
        Ok((path, file))
    }

    /// Runs the staged setup silently and returns its exit code. Never `-unrestricted`.
    fn setup(folder: &Path, action: &'static str) -> Result<i32, Error> {
        let (path, locked) = stage(folder)?;
        let output = process::run(
            &path,
            &[action, "-silent"],
            SETUP_TIMEOUT,
            &process::Cancel::new(),
        )
        .map_err(|error| install_error("PawnIO setup", error));
        drop(locked);
        if let Err(error) = fs::remove_file(&path) {
            win::record(hash::io_error("Remove PawnIO setup", error));
        }
        Ok(output?.code)
    }

    /// Ownership of a PawnIO this session installed. Dropping it uninstalls PawnIO.
    struct Installed(PathBuf);

    impl Drop for Installed {
        fn drop(&mut self) {
            if QUARANTINED.load(Ordering::Acquire) {
                win::record(WinError::msg(
                    "PawnIO uninstall",
                    "skipped while a request may still hold the device; retried at next start",
                ));
                return;
            }
            uninstall(&self.0);
        }
    }

    fn uninstall(folder: &Path) {
        let code = match setup(folder, "-uninstall") {
            Ok(code) => code,
            Err(error) => return win::record(error.diagnostic()),
        };
        if code != 0 {
            win::record(WinError::msg(
                "PawnIO uninstall",
                format!("setup exited with code {code}"),
            ));
        }
        // Device absence is the truth, as for install; otherwise retry at next start.
        if (code == 0 || matches!(open_device(), Err(Error::NotInstalled)))
            && let Err(error) = fs::remove_file(folder.join(MARKER))
        {
            win::record(hash::io_error("Remove PawnIO marker", error));
        }
    }

    // shortcut: a second app instance starting mid-session uninstalls the first one's PawnIO; add a named mutex if that happens.
    pub(super) fn recover() {
        if let Ok(folder) = folder()
            && folder.join(MARKER).exists()
        {
            uninstall(&folder);
        }
    }

    // shortcut: no separate crash-guard process; the marker plus recover() at start covers crashes.
    fn install() -> Result<Installed, Error> {
        let folder = folder()?;
        let io = |op| move |error: io::Error| Error::InstallFailed(hash::io_error(op, error));
        fs::create_dir_all(&folder).map_err(io("Create PawnIO folder"))?;
        // The marker exists before setup runs, so a crash at any later point is recovered.
        fs::write(
            folder.join(MARKER),
            "PawnIO was installed by HWIDChecker and is uninstalled after reading.\r\n",
        )
        .map_err(io("Write PawnIO marker"))?;
        let installed = Installed(folder);
        let code = setup(&installed.0, "-install")?;
        let until = Instant::now() + DEVICE_WAIT;
        loop {
            match open_device() {
                Err(Error::NotInstalled) if Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(Error::NotInstalled) => {
                    return Err(install_error(
                        "PawnIO setup",
                        format!("device missing after install (setup exit code {code})"),
                    ));
                }
                // The device exists; open() reports any other failure.
                _ => return Ok(installed),
            }
        }
    }

    fn open_device() -> Result<OwnedHandle, Error> {
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
        unsafe { OwnedHandle::from_raw(handle) }.map_err(Error::OpenFailed)
    }

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
                        // shortcut: retain one undrained request until reboot; audited cancellation is the upgrade path.
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
        if MODULE.len() != MODULE_SIZE
            || !win::hash::sha256_bytes(MODULE)
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
        let handle = open_device()?;
        Ok((
            NativeDevice {
                handle: Arc::new(handle),
                _bus: bus,
            },
            MODULE.to_vec(),
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
        // A PawnIO installed by someone else is used as is and never uninstalled.
        // Declared before the worker so it drops (uninstalls) after the read finishes.
        let _installed = match open_device() {
            Err(Error::NotInstalled) => {
                let fresh = anticheat::detect();
                if fresh.gate_decision() != anticheat::GateDecision::Clear {
                    return Snapshot {
                        gate: Some(fresh),
                        ..Snapshot::default()
                    };
                }
                match install() {
                    Ok(installed) => Some(installed),
                    Err(error) => {
                        return Snapshot {
                            failures: vec![error],
                            ..Snapshot::default()
                        };
                    }
                }
            }
            _ => None,
        };
        // The read budget starts after setup, which has its own bounds.
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
                .unwrap_or_else(|_| {
                    // A panic may leave a request or page selection in an unknown state.
                    QUARANTINED.store(true, Ordering::Release);
                    Snapshot {
                        failures: vec![Error::CleanupIncomplete],
                        ..Snapshot::default()
                    }
                });
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
        write_protected: bool,
        writes: usize,
        /// Refuse SMBus writes after this many, the way a write-disabled PCH does.
        refuse_writes_after: Option<usize>,
        proc_calls: usize,
        /// The hub accepts the transaction but keeps its page (for example, I2C mode).
        ignore_select: bool,
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
                write_protected: false,
                writes: 0,
                refuse_writes_after: None,
                proc_calls: 0,
                ignore_select: false,
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
                    if name == "ioctl_write_protection" {
                        assert_eq!((input.len(), output), (32, 8));
                        return Ok(u64::from(self.write_protected).to_le_bytes().to_vec());
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
                    if protocol == 4 {
                        // Process Call page select: read bit set, MR11, one reply cell.
                        assert_eq!((address, direction, command, output), (0x50, 1, 0x0b, 8));
                        assert_eq!(cells.len(), 5);
                        self.proc_calls += 1;
                        if !self.ignore_select {
                            self.hub_page = cells[4] as u8;
                        }
                        return Ok(0_u64.to_le_bytes().to_vec());
                    }
                    if direction == 0 {
                        assert_eq!(cells.len(), 5);
                        assert_eq!(output, 0);
                        assert!(
                            !(self.write_protected && (0x50..=0x57).contains(&address)),
                            "write-flagged SPD transaction under write protection"
                        );
                        self.writes += 1;
                        if self.refuse_writes_after.is_some_and(|n| self.writes > n) {
                            return Err(Error::ReadFailed(WinError {
                                op: "fake",
                                code: 433,
                                detail: "write refused".into(),
                            }));
                        }
                        if address == 0x36 || address == 0x37 {
                            assert_eq!((command, protocol, cells[4]), (0, 1, 0));
                            self.page = (address - 0x36) as u8;
                        } else {
                            assert_eq!((address, command, protocol), (0x50, 0x0b, 2));
                            if !self.ignore_select {
                                self.hub_page = cells[4] as u8;
                            }
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
        assert_eq!(calls.get(), 4, "no first hardware read");
    }

    #[test]
    fn restore_failures_return_the_real_error_and_never_cleanup_incomplete() {
        // 1: the read itself fails on page 1, then the restore is refused too.
        // 8: all eight page reads succeed, only the restore is refused.
        for refuse_after in [1, 8] {
            let mut fake = Fake::new(spd::Generation::Ddr5);
            fake.refuse_writes_after = Some(refuse_after);
            let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
            client.initialize(b"signed-fixture").unwrap();
            let result = client.slot(0x50);
            assert!(
                matches!(&result, Err(Error::ReadFailed(e)) if e.code == 433),
                "refuse after {refuse_after}: {result:?}"
            );
        }
        let mut fake = Fake::new(spd::Generation::Ddr5);
        fake.refuse_writes_after = Some(1);
        let result = collect_with(
            true,
            &clear(),
            || Ok((fake, b"signed-fixture".to_vec())),
            clear,
            Instant::now() + BUDGET,
            |_| {},
        );
        assert!(result.modules.is_empty());
        assert!(
            !result.failures.is_empty(),
            "present DIMM is not an empty slot"
        );
        assert!(
            !result
                .failures
                .iter()
                .any(|e| matches!(e, Error::CleanupIncomplete))
        );
    }

    #[test]
    fn write_protected_ddr5_selects_pages_by_read_flagged_process_call() {
        let mut fake = Fake::new(spd::Generation::Ddr5);
        fake.write_protected = true;
        let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
        client.initialize(b"signed-fixture").unwrap();
        let decoded = client.slot(0x50).unwrap().unwrap();
        assert_eq!(decoded.serial.as_deref(), Some("8C31D7A2"));
        // Eight page selects plus the restore, all Process Calls; no write-flagged transfer.
        assert_eq!(client.device.proc_calls, 9);
        assert_eq!(client.device.writes, 0);
        assert_eq!(client.device.hub_page, 2);
    }

    #[test]
    fn ignored_page_select_reads_no_data() {
        for protected in [false, true] {
            let mut fake = Fake::new(spd::Generation::Ddr5);
            fake.write_protected = protected;
            fake.ignore_select = true;
            // A data read (command 0x80) would surface as Timeout instead.
            fake.fail_command = Some(0x80);
            let mut client = Client::query(fake, Instant::now() + BUDGET).unwrap();
            client.initialize(b"signed-fixture").unwrap();
            let result = client.slot(0x50);
            assert!(
                matches!(result, Err(Error::PageSelectIgnored)),
                "protected {protected}: {result:?}"
            );
        }
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
            Error::InstallFailed(WinError::msg("fake", "setup")),
            Error::PageSelectIgnored,
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

    #[test]
    fn embedded_artifacts_match_their_pins() {
        assert_eq!(MODULE.len(), MODULE_SIZE);
        assert_eq!(win_hash(MODULE), MODULE_HASH);
        assert_eq!(win_hash(SETUP), SETUP_HASH);
    }

    fn win_hash(bytes: &[u8]) -> String {
        super::super::hash::sha256_bytes(bytes).expect("CNG SHA-256")
    }
}
