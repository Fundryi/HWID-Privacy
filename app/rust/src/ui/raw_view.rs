//! Owned by WP-10b: modal legacy raw report view (C# `SectionedViewForm.DebugButton_Click`).

use super::controls::{Ctl, EditSpec};
use super::layout::Node;
use super::msgbox::{self, Buttons, Icon};
use super::theme;
use super::window::{self, Event, FormSpec, WindowSize};
use crate::{hw, win};
use std::cell::RefCell;
use std::sync::mpsc::{self, TryRecvError};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Controls::EM_SETSEL;
use windows::Win32::UI::WindowsAndMessaging::{
    IsWindow, PostThreadMessageW, SendMessageW, WM_NULL,
};

/// Window title of the Old View form.
pub const TITLE: &str = "Old View - Raw Hardware Data";
const TEXT: u16 = 1;

/// Shows a fresh raw report in the modal Old View window.
pub fn show(owner: HWND) {
    // C# parity: SectionedViewForm.cs:833 collects again instead of showing the main snapshot.
    let report = match collect(owner) {
        Some(Ok(report)) => report,
        Some(Err(error)) => return debug_error(owner, &error),
        // The owner closed (or the app quit) while collecting: nothing is left to show on.
        None => return,
    };
    if let Err(error) = show_report(owner, report) {
        debug_error(owner, &error.to_string());
    }
}

fn debug_error(owner: HWND, error: &str) {
    // C# parity: SectionedViewForm.cs:862.
    msgbox::show(
        owner,
        &format!("Debug error: {error}"),
        "Debug Error",
        Buttons::Ok,
        Icon::Error,
    );
}

/// Collects on a worker while the UI keeps running (C# `await`); `None` when the owner is gone.
/// The wait is bounded by the 60 s per-provider deadline of `hw::collect_all`.
fn collect(owner: HWND) -> Option<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    // SAFETY: Reads the calling (UI) thread id; no pointers.
    let ui_thread = unsafe { GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("old view".to_owned())
        .spawn(move || {
            let report = win::catch_panic(|| hw::full_report(&hw::collect_all(None, &|_, _| {})));
            if tx.send(report).is_ok() {
                // SAFETY: Posts a value-only message to the UI thread so its loop re-checks.
                let posted =
                    unsafe { PostThreadMessageW(ui_thread, WM_NULL, WPARAM(0), LPARAM(0)) };
                if let Err(error) = posted {
                    // Not fatal: the loop also re-checks after the next message of any kind.
                    win::record(win::Error::from_win("PostThreadMessageW", error));
                }
            }
        });
    if let Err(error) = spawned {
        return Some(Err(
            win::Error::msg("thread::spawn", error.to_string()).to_string()
        ));
    }
    let result = RefCell::new(None);
    window::pump_until(|| {
        match rx.try_recv() {
            Ok(report) => *result.borrow_mut() = Some(report),
            Err(TryRecvError::Disconnected) => {
                *result.borrow_mut() =
                    Some(Err("Old View collection ended without a result".to_owned()))
            }
            Err(TryRecvError::Empty) => {}
        }
        // SAFETY: Read-only window handle validity check.
        let owner_gone = !owner.is_invalid() && !unsafe { IsWindow(Some(owner)) }.as_bool();
        result.borrow().is_some() || owner_gone
    });
    result.into_inner()
}

/// Shows `report` in the modal Old View form (`SectionedViewForm.cs:836-858`).
fn show_report(owner: HWND, report: String) -> win::Result<()> {
    // AD-39: C# keeps the outer 1000x700 at every DPI (.NET 10 probe, WP-10b report); scaled here.
    let mut spec = FormSpec::new(
        TITLE,
        WindowSize::Outer {
            size: theme::OLD_VIEW_SIZE,
            scaled: true,
        },
    );
    spec.back = theme::OLD_VIEW_BACKGROUND;
    // C# parity: WordWrap is left at its default (true), so there is no horizontal scroll bar;
    // BorderStyle stays the Fixed3D default.
    let edit = Ctl::Edit(
        EditSpec::new(
            theme::OLD_VIEW_FONT,
            theme::OLD_VIEW_TEXT,
            theme::OLD_VIEW_TEXT_BACKGROUND,
        )
        .word_wrap(),
    );
    window::run_modal(
        owner,
        spec,
        vec![Node::leaf(TEXT, edit).fill()],
        move |form, event| {
            if let Event::Created = event {
                form.edit_set_text(TEXT, &report);
                if let Some(edit) = form.control(TEXT) {
                    // C# parity: TextBox.OnGotFocus selects all text on the first focus when no
                    // selection was set (.NET 10 probe: whole text selected, view at the top).
                    // SAFETY: Value-only message to our own edit; EM_SETSEL does not scroll.
                    unsafe {
                        SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                    }
                }
            }
            true
        },
    )
}
