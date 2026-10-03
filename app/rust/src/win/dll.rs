//! Optional DLLs are loaded at runtime exclusively from System32.

use super::{Error, Result, wide};
use std::ffi::CStr;
use windows::Win32::Foundation::{FARPROC, FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::core::{PCSTR, PCWSTR};

pub struct Library {
    module: HMODULE,
}
/// Loads a System32 DLL and owns its lifetime; never searches PATH or the exe folder.
pub fn load_system_dll(name: &str) -> Result<Library> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['\\', '/', ':', '\0']) {
        return Err(Error::msg(
            "LoadLibraryExW",
            "expected a System32 DLL filename without a path",
        ));
    }
    let mut directory = vec![0_u16; 260];
    loop {
        // SAFETY: The SDK receives the actual writable slice length.
        let length = unsafe { GetSystemDirectoryW(Some(&mut directory)) } as usize;
        if length == 0 {
            return Err(Error::last("GetSystemDirectoryW"));
        }
        if length < directory.len() {
            directory.truncate(length);
            break;
        }
        if length > 32768 {
            return Err(Error::msg(
                "GetSystemDirectoryW",
                "System32 path is too long",
            ));
        }
        directory.resize(length + 1, 0);
    }
    directory.push(u16::from(b'\\'));
    directory.extend(wide::to_wide(name));
    // SAFETY: The absolute, terminated System32 path is live; the loader searches only System32.
    let module = unsafe {
        LoadLibraryExW(
            PCWSTR(directory.as_ptr()),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|e| Error::from_win("LoadLibraryExW", e))?;
    Ok(Library { module })
}
impl Library {
    /// Returns an untyped symbol address valid only while this library stays loaded.
    pub fn proc_address(&self, name: &CStr) -> FARPROC {
        // SAFETY: The module is held by self and name is a terminated C string.
        let address = unsafe { GetProcAddress(self.module, PCSTR(name.as_ptr().cast())) };
        if address.is_none() {
            eprintln!("{}", Error::last("GetProcAddress"));
        }
        address
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: The module is the uniquely owned reference acquired by LoadLibraryExW.
        if let Err(error) = unsafe { FreeLibrary(self.module) } {
            eprintln!("{}", Error::from_win("FreeLibrary", error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_kernel32_and_resolves_optional_symbols() {
        let library = load_system_dll("kernel32.dll").expect("Windows check should succeed");
        assert!(library.proc_address(c"GetCurrentProcessId").is_some());
        assert!(
            library
                .proc_address(c"HWIDCheckerPhase1MissingFunction")
                .is_none()
        );
    }

    #[test]
    fn missing_dll_and_non_system_paths_are_errors() {
        assert!(load_system_dll("hwidchecker-phase1-missing-6f28e90c.dll").is_err());
        for name in [
            "",
            "..",
            "..\\kernel32.dll",
            "C:\\kernel32.dll",
            "kernel32.dll\0ignored",
        ] {
            assert!(load_system_dll(name).is_err());
        }
    }
}
