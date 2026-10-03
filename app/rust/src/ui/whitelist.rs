//! Owned by WP-14: whitelist editing window.

use crate::clean::devices::GhostDevice;
use windows::Win32::Foundation::HWND;
/// Shows the whitelist editor and returns true only after a successful save.
pub fn show(owner: HWND, _devices: &[GhostDevice]) -> bool {
    super::window::show_info(owner, "Whitelist UI not ported yet", "HWID Checker");
    false
}
