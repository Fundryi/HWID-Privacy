//! Raw firmware tables and bounds-checked SMBIOS access.

use super::{Error, Result};

#[derive(Clone, Debug)]
pub struct Smbios {
    pub major: u8,
    pub minor: u8,
    pub structures: Vec<Structure>,
}
#[derive(Clone, Debug)]
pub struct Structure {
    pub kind: u8,
    pub handle: u16,
    pub formatted: Vec<u8>,
    pub strings: Vec<String>,
}

/// Reads a firmware table using the numeric provider signature and table ID.
pub fn raw_table(_provider: u32, _id: u32) -> Result<Vec<u8>> {
    Err(Error::msg("GetSystemFirmwareTable", "not ported yet"))
}
/// Parses a complete RawSMBIOSData buffer including its eight-byte header.
pub fn parse_smbios(_raw: &[u8]) -> Result<Smbios> {
    Err(Error::msg("SMBIOS parse", "not ported yet"))
}
/// Reads and parses the RSMB table.
pub fn smbios() -> Result<Smbios> {
    Err(Error::msg("SMBIOS", "not ported yet"))
}

impl Structure {
    /// Reads a one-based SMBIOS string index; zero or missing returns an empty string.
    pub fn string(&self, _idx: u8) -> &str {
        ""
    }
    /// Reads a byte at an absolute formatted-structure offset, or None if truncated.
    pub fn byte(&self, _offset: usize) -> Option<u8> {
        None
    }
    /// Reads a little-endian word at an absolute formatted-structure offset.
    pub fn word(&self, _offset: usize) -> Option<u16> {
        None
    }
    /// Reads a little-endian dword at an absolute formatted-structure offset.
    pub fn dword(&self, _offset: usize) -> Option<u32> {
        None
    }
    /// Reads a little-endian qword at an absolute formatted-structure offset.
    pub fn qword(&self, _offset: usize) -> Option<u64> {
        None
    }
}
