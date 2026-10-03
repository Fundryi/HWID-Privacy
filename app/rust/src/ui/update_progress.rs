//! Owned by WP-17: update check and download progress window.

use windows::Win32::Foundation::HWND;
/// Checks for updates and drives the modal update flow for its originating button.
pub fn check_and_update(owner: HWND, _button: HWND) {
    super::window::show_info(owner, "Update UI not ported yet", "HWID Checker");
}
