//! Windows helpers and their frozen cross-work-package contracts.

pub mod bluetooth;
pub mod dialog;
pub mod dll;
pub mod edid;
pub mod evt;
pub mod firmware;
pub mod hash;
pub mod http;
pub mod ioctl;
pub mod iphlp;
pub mod nvidia;
mod panic_guard;
pub use panic_guard::{catch_panic, is_guarded};
pub mod process;
pub mod registry;
pub mod security;
pub mod setupapi;
pub mod storage;
pub mod time;
pub mod tpm;
pub mod wide;
pub mod wmi;

// Type-only coverage checks every enabled SDK feature without importing optional DLLs.
const _: () = {
    let _ = std::mem::size_of::<windows::Win32::Foundation::HANDLE>();
    let _ = std::mem::size_of::<windows::Win32::Devices::Bluetooth::BLUETOOTH_RADIO_INFO>();
    let _ = std::mem::size_of::<
        windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    >();
    let _ = std::mem::size_of::<windows::Win32::Devices::Properties::DEVPROPERTY>();
    let _ = std::mem::size_of::<windows::Win32::Graphics::Dwm::DWM_THUMBNAIL_PROPERTIES>();
    let _ = std::mem::size_of::<windows::Win32::Graphics::Gdi::HDC>();
    let _ = std::mem::size_of::<windows::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2>();
    let _ = std::mem::size_of::<windows::Win32::NetworkManagement::Ndis::NET_LUID_LH>();
    let _ = std::mem::size_of::<windows::Win32::Networking::WinHttp::URL_COMPONENTS>();
    let _ = std::mem::size_of::<windows::Win32::Networking::WinSock::SOCKADDR_INET>();
    let _ = std::mem::size_of::<windows::Win32::Security::TOKEN_PRIVILEGES>();
    let _ = std::mem::size_of::<windows::Win32::Security::Cryptography::BCRYPT_ALG_HANDLE>();
    let _ = std::mem::size_of::<
        windows::Win32::Security::Cryptography::Certificates::IX509EndorsementKey,
    >();
    let _ = std::mem::size_of::<windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS>();
    let _ = std::mem::size_of::<windows::Win32::System::Com::COINIT>();
    let _ =
        std::mem::size_of::<windows::Win32::System::Diagnostics::Debug::FORMAT_MESSAGE_OPTIONS>();
    let _ = std::mem::size_of::<windows::Win32::System::EventLog::EVT_VARIANT>();
    let _ = std::mem::size_of::<windows::Win32::System::IO::OVERLAPPED>();
    let _ = std::mem::size_of::<windows::Win32::System::Ioctl::STORAGE_PROPERTY_QUERY>();
    let _ = std::mem::size_of::<
        windows::Win32::System::JobObjects::JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    >();
    let _ = std::mem::size_of::<windows::Win32::System::LibraryLoader::LOAD_LIBRARY_FLAGS>();
    let _ = std::mem::size_of::<windows::Win32::System::Memory::MEMORY_BASIC_INFORMATION>();
    let _ = std::mem::size_of::<windows::Win32::System::Ole::ACTIVATEFLAGS>();
    let _ = std::mem::size_of::<windows::Win32::System::Pipes::NAMED_PIPE_MODE>();
    let _ = std::mem::size_of::<windows::Win32::System::Registry::REG_VALUE_TYPE>();
    let _ = std::mem::size_of::<windows::Win32::System::SystemInformation::SYSTEM_INFO>();
    let _ = std::mem::size_of::<windows::Win32::System::Threading::PROCESS_INFORMATION>();
    let _ = std::mem::size_of::<windows::Win32::System::Time::TIME_ZONE_INFORMATION>();
    let _ = std::mem::size_of::<windows::Win32::System::TpmBaseServices::TBS_CONTEXT_PARAMS>();
    let _ = std::mem::size_of::<windows::Win32::System::Variant::VARIANT>();
    let _ = std::mem::size_of::<windows::Win32::System::Wmi::IWbemClassObject>();
    let _ = std::mem::size_of::<windows::Win32::UI::Controls::INITCOMMONCONTROLSEX>();
    let _ = std::mem::size_of::<windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT>();
    let _ = std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY>();
    let _ = std::mem::size_of::<windows::Win32::UI::Shell::SHELLEXECUTEINFOW>();
    let _ = std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::MSG>();
};

use std::{fmt, marker::PhantomData, rc::Rc, sync::Mutex};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, RPC_E_TOO_LATE};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoInitializeSecurity, CoUninitialize, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
};
use windows::core::PWSTR;

#[derive(Clone, Debug)]
pub struct Error {
    pub op: &'static str,
    pub code: u32,
    pub detail: String,
}

impl Error {
    /// Captures the calling thread's last Win32 error and system description.
    pub fn last(op: &'static str) -> Self {
        // SAFETY: GetLastError takes no pointers and reads this thread's error.
        let code = unsafe { GetLastError() }.0;
        let mut buffer = [0_u16; 2048];
        // SAFETY: buffer is writable for the advertised number of UTF-16 units.
        let count = unsafe {
            FormatMessageW(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                None,
                code,
                0,
                PWSTR(buffer.as_mut_ptr()),
                buffer.len() as u32,
                None,
            )
        };
        let detail = if count == 0 {
            "No system error description available.".to_owned()
        } else {
            wide::from_wide(&buffer[..count as usize])
                .trim_end()
                .to_owned()
        };
        Self { op, code, detail }
    }

    /// Normalizes HRESULT_FROM_WIN32 to a Win32 code; preserves other HRESULTs.
    pub fn from_win(op: &'static str, error: windows::core::Error) -> Self {
        let code = error.code().0 as u32;
        Self {
            op,
            code: if code & 0xFFFF0000 == 0x80070000 {
                code & 0xFFFF
            } else {
                code
            },
            detail: error.message(),
        }
    }

    /// Describes a validation or porting failure without a Win32 error code.
    pub fn msg(op: &'static str, detail: impl Into<String>) -> Self {
        Self {
            op,
            code: 0,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: 0x{:08X} {}", self.op, self.code, self.detail)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

static RECORDED: Mutex<Vec<Error>> = Mutex::new(Vec::new());

/// Records a helper failure process-wide, recovering the entries even after poison.
pub fn record(err: Error) {
    RECORDED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(err);
}

/// Drains the process-wide helper diagnostics, recovering entries even after poison.
pub fn take_recorded() -> Vec<Error> {
    std::mem::take(&mut *RECORDED.lock().unwrap_or_else(|poison| poison.into_inner()))
}

#[derive(Debug)]
pub struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes sole ownership of a kernel handle that must be closed with CloseHandle.
    ///
    /// # Safety
    /// The handle must be owned, non-pseudo, and have no other closing owner.
    pub unsafe fn from_raw(handle: HANDLE) -> Result<Self> {
        if handle.is_invalid() {
            Err(Error::msg("OwnedHandle", "invalid kernel handle"))
        } else {
            Ok(Self(handle))
        }
    }

    /// Borrows the handle without transferring its closing responsibility.
    pub fn as_raw(&self) -> HANDLE {
        self.0
    }

    /// Transfers closing responsibility to the caller.
    pub fn into_raw(self) -> HANDLE {
        let handle = self.0;
        std::mem::forget(self);
        handle
    }
}

// SAFETY: Kernel handles may be used from other threads; ownership remains unique.
unsafe impl Send for OwnedHandle {}
// SAFETY: Shared borrows do not close the handle, and kernel APIs synchronize access.
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: from_raw accepts only a uniquely owned, CloseHandle-compatible handle.
        if let Err(error) = unsafe { CloseHandle(self.0) } {
            record(Error::from_win("CloseHandle", error));
        }
    }
}

pub struct ComApartment(PhantomData<Rc<()>>);

/// Initializes the UI STA before process-wide COM security and balances it on drop.
pub fn initialize_com() -> Result<ComApartment> {
    // SAFETY: No reserved pointer is passed; the guard balances successful initialization.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|e| Error::from_win("CoInitializeEx", e))?;
    let apartment = ComApartment(PhantomData);
    // SAFETY: COM is initialized; all reserved pointers are null and default services apply.
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
        // Security already configured by the host is a documented successful condition.
        if error.code() != RPC_E_TOO_LATE {
            return Err(Error::from_win("CoInitializeSecurity", error));
        }
    }
    Ok(apartment)
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: The !Send guard drops on the same thread after a successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::HRESULT;

    #[test]
    fn hresult_codes_normalize_win32_and_preserve_other_facilities() {
        for (hresult, code) in [
            (0x80070005_u32, 5),
            (0x8007007A, 122),
            (0x80041010, 0x80041010),
        ] {
            let original = windows::core::Error::from_hresult(HRESULT(hresult as i32));
            let detail = original.message();
            let error = Error::from_win("query", original);
            assert_eq!(error.code, code);
            assert_eq!(error.detail, detail);
            assert_eq!(
                error.to_string(),
                format!("query failed: 0x{code:08X} {detail}")
            );
        }
    }

    #[test]
    fn diagnostics_recover_entries_after_poison_and_drain_them() {
        const OP: &str = "diagnostics poison test";
        catch_panic(|| {
            let mut recorded = RECORDED.lock().unwrap_or_else(|poison| poison.into_inner());
            recorded.push(Error::msg(OP, "before poison"));
            panic!("poison diagnostics lock");
        })
        .expect_err("lock must be poisoned by the caught panic");
        assert!(RECORDED.is_poisoned());
        record(Error::msg(OP, "after poison"));
        let entries: Vec<_> = take_recorded()
            .into_iter()
            .filter(|error| error.op == OP)
            .map(|error| error.detail)
            .collect();
        assert_eq!(entries, ["before poison", "after poison"]);
        assert!(!take_recorded().iter().any(|error| error.op == OP));
    }
}
