//! Owned by WP-10a: window lifetime, dispatch, and safe message-box boundary.

use crate::win::wide::to_wide;
use windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW},
    },
    core::PCWSTR,
};

/// Shows a blocking error message with the supplied owner and title.
pub fn show_error(owner: HWND, text: &str, title: &str) {
    let text = to_wide(text);
    let title = to_wide(title);
    // SAFETY: The terminated buffers live through the modal call; a null owner is valid.
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// Shows a blocking informational message with the supplied owner and title.
pub fn show_info(owner: HWND, text: &str, title: &str) {
    let text = to_wide(text);
    let title = to_wide(title);
    // SAFETY: The terminated buffers live through the modal call; a null owner is valid.
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        )
    };
}
