//! Optional DLLs are loaded at runtime exclusively from System32.

use super::{Error, Result};
use std::ffi::CStr;
use windows::Win32::Foundation::{FARPROC, HMODULE};

pub struct Library {
    _module: HMODULE,
}
/// Loads a System32 DLL and owns its lifetime; never searches PATH or the exe folder.
pub fn load_system_dll(_name: &str) -> Result<Library> {
    Err(Error::msg("LoadLibraryExW", "not ported yet"))
}
impl Library {
    /// Returns an untyped symbol address valid only while this library stays loaded.
    pub fn proc_address(&self, _name: &CStr) -> FARPROC {
        None
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // The stub loader always returns Err and never acquires a module.
    }
}
