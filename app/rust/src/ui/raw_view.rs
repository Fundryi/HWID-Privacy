//! Owned by WP-10b: modal legacy raw report view.

use windows::Win32::Foundation::HWND;
/// Shows a fresh raw report in the modal Old View window.
pub fn show(owner: HWND) {
    super::window::show_info(owner, "Old View not ported yet", "HWID Checker");
}
