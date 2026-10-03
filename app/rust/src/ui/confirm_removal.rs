//! Owned by WP-14: device removal confirmation dialog.

use windows::Win32::Foundation::HWND;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmResult {
    YesAutoClose,
    Yes,
    No,
}
/// Confirms the device count and optional automatic close behavior.
pub fn show(owner: HWND, _count: usize) -> ConfirmResult {
    super::window::show_info(
        owner,
        "Removal confirmation UI not ported yet",
        "HWID Checker",
    );
    ConfirmResult::No
}
