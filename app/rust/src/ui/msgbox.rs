//! Owned by WP-10a: `MessageBox.Show` with the C# button and icon sets.

use crate::win::wide::to_wide;
use windows::{
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            WindowsAndMessaging::{
                IDNO, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION,
                MB_ICONWARNING, MB_OK, MB_YESNO, MESSAGEBOX_STYLE, MessageBoxW,
            },
        },
    },
    core::PCWSTR,
};

/// The owner for a box raised after async work (DESIGN.md 8.5): C# `MessageBox.Show` without an
/// owner uses the UI thread's active window, including a modal child. Never pass a form that a
/// modal child disabled: the box would enable it again when it closes.
pub fn active_window() -> HWND {
    // SAFETY: Read-only query of this UI thread's active window; a null result is valid.
    unsafe { GetActiveWindow() }
}

/// `MessageBoxButtons` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Buttons {
    /// `MessageBoxButtons.OK`.
    Ok,
    /// `MessageBoxButtons.YesNo`.
    YesNo,
}

/// `MessageBoxIcon` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Icon {
    /// `MessageBoxIcon.None`.
    None,
    /// `MessageBoxIcon.Information`.
    Information,
    /// `MessageBoxIcon.Warning`.
    Warning,
    /// `MessageBoxIcon.Error`.
    Error,
    /// `MessageBoxIcon.Question`.
    Question,
}

/// `DialogResult` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Answer {
    /// OK.
    Ok,
    /// Yes.
    Yes,
    /// No (also returned when the box could not be shown).
    No,
}

/// Shows a modal message box owned by `owner` (pass the form window, like C# does implicitly).
pub fn show(owner: HWND, text: &str, title: &str, buttons: Buttons, icon: Icon) -> Answer {
    let mut style = match buttons {
        Buttons::Ok => MB_OK,
        Buttons::YesNo => MB_YESNO,
    };
    style |= match icon {
        Icon::None => MESSAGEBOX_STYLE(0),
        Icon::Information => MB_ICONINFORMATION,
        Icon::Warning => MB_ICONWARNING,
        Icon::Error => MB_ICONERROR,
        Icon::Question => MB_ICONQUESTION,
    };
    let text = to_wide(text);
    let title = to_wide(title);
    let owner = if owner.is_invalid() {
        None
    } else {
        Some(owner)
    };
    // SAFETY: The terminated buffers live through the modal call.
    let r = unsafe { MessageBoxW(owner, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), style) };
    match r {
        IDYES => Answer::Yes,
        IDOK => Answer::Ok,
        IDNO => Answer::No,
        _ => Answer::No,
    }
}
