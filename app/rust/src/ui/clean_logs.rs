//! Owned by WP-16: log cleaning window.

use windows::Win32::Foundation::HWND;
/// Shows the modal event log cleaning window.
pub fn show(owner: HWND) {
    super::window::show_info(owner, "Log cleaning UI not ported yet", "HWID Checker");
}
