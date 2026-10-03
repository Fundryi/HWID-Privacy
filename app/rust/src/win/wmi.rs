//! WMI queries and method calls; the fill-in agent owns all COM/WMI work here.

use super::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Namespace {
    Cimv2,
    Wmi,
    MicrosoftTpm,
    Storage,
}

#[derive(Debug, Default)]
pub struct Row {
    _values: std::collections::HashMap<String, ::wmi::Variant>,
}

/// Queries a namespace with WQL using a connection local to the calling thread.
pub fn query(_ns: Namespace, _wql: &str) -> Result<Vec<Row>> {
    Err(Error::msg("WMI query", "not ported yet"))
}

/// Calls a no-input WMI method and rejects a nonzero ReturnValue.
pub fn call_method(_ns: Namespace, _object_path: &str, _method: &str) -> Result<Row> {
    Err(Error::msg("WMI method", "not ported yet"))
}

impl Row {
    /// Returns .NET ToString-compatible property text, or None for null/empty.
    pub fn str(&self, _name: &str) -> Option<String> {
        None
    }
    /// Reads an unsigned 32-bit property without truncation.
    pub fn u32(&self, _name: &str) -> Option<u32> {
        None
    }
    /// Reads an unsigned 64-bit property without truncation.
    pub fn u64(&self, _name: &str) -> Option<u64> {
        None
    }
    /// Reads a boolean property.
    pub fn bool(&self, _name: &str) -> Option<bool> {
        None
    }
    /// Reads a UInt16 array, including WmiMonitorID text arrays.
    pub fn u16_array(&self, _name: &str) -> Option<Vec<u16>> {
        None
    }
}
