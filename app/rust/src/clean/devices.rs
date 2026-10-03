//! Owned by WP-11: ghost scanning and guarded device removal.

use crate::win::process::Cancel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Absent,
    Unclear,
}
#[derive(Clone, Debug)]
pub struct GhostDevice {
    pub name: String,
    pub description: String,
    pub hardware_id: String,
    pub class: String,
    pub instance_id: String,
    pub presence: Presence,
}
pub struct Scan {
    devices: Vec<GhostDevice>,
    _set: crate::win::setupapi::DevInfoSet,
}

/// Scans ghost devices while retaining the native snapshot needed for removal.
pub fn scan() -> Result<Scan, String> {
    Err("Device scan not ported yet".to_owned())
}
impl Scan {
    /// Borrows the scanned ghosts and devices with unclear presence.
    pub fn devices(&self) -> &[GhostDevice] {
        &self.devices
    }
    /// Removes selected eligible devices, checking cancellation before each one.
    pub fn remove(self, _selected: &[usize], _cancel: &Cancel, status: &dyn Fn(&str)) {
        status("Device removal not ported yet");
    }
}
