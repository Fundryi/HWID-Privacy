//! HKLM registry reads always use the 64-bit view.

use super::{Error, Result};

/// Reads a REG_SZ or REG_EXPAND_SZ value from HKLM's 64-bit view.
pub fn read_string(_path: &str, _name: &str) -> Result<String> {
    Err(Error::msg("registry string", "not ported yet"))
}
/// Reads a REG_DWORD value from HKLM's 64-bit view.
pub fn read_dword(_path: &str, _name: &str) -> Result<u32> {
    Err(Error::msg("registry DWORD", "not ported yet"))
}
/// Reads a REG_QWORD value from HKLM's 64-bit view.
pub fn read_qword(_path: &str, _name: &str) -> Result<u64> {
    Err(Error::msg("registry QWORD", "not ported yet"))
}
/// Reads a REG_BINARY value from HKLM's 64-bit view.
pub fn read_binary(_path: &str, _name: &str) -> Result<Vec<u8>> {
    Err(Error::msg("registry binary", "not ported yet"))
}
/// Lists immediate subkey names under a key in HKLM's 64-bit view.
pub fn subkeys(_path: &str) -> Result<Vec<String>> {
    Err(Error::msg("registry subkeys", "not ported yet"))
}
