//! Owned by WP-14: device cleaning window.

use windows::Win32::Foundation::HWND;
/// Shows the modal device cleaning window.
pub fn show(owner: HWND) {
    super::window::show_info(owner, "Device cleaning UI not ported yet", "HWID Checker");
}
