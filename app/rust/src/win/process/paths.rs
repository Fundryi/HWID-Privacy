//! System-directory executable resolution without search-path fallback.

use crate::win::{Error, record};
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Component, Path, PathBuf},
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

/// Resolves a System32 child executable without searching PATH or the executable folder.
pub fn system32(name: &str) -> PathBuf {
    match system_path(name) {
        Ok(path) => path,
        Err(error) => {
            // The frozen infallible API must record failures and fail closed: run
            // rejects this empty path, rather than searching PATH or a fallback directory.
            record(error);
            PathBuf::new()
        }
    }
}

/// Resolves the System32 Windows PowerShell 5.1 executable without searching PATH.
pub fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
}

fn system_path(name: &str) -> crate::win::Result<PathBuf> {
    let relative = Path::new(name);
    if name.is_empty()
        || name.contains([':', '\0'])
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::msg(
            "system32",
            "a relative executable path without parent components, a colon or NUL is required",
        ));
    }
    let mut buffer = vec![0_u16; 260];
    loop {
        // SAFETY: The slice is writable for its declared capacity.
        let count = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
        if count == 0 {
            return Err(Error::last("GetSystemDirectoryW"));
        }
        if count >= buffer.len() {
            if count > 32768 {
                return Err(Error::msg(
                    "GetSystemDirectoryW",
                    "system directory is too long",
                ));
            }
            buffer.resize(count + 1, 0);
            continue;
        }
        let path = PathBuf::from(OsString::from_wide(&buffer[..count]));
        if !path.is_absolute() {
            return Err(Error::msg(
                "GetSystemDirectoryW",
                "system directory is not absolute",
            ));
        }
        return Ok(path.join(name));
    }
}
