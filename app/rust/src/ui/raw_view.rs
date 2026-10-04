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
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostThreadMessageW, WM_NULL};

/// Window title of the Old View form.
pub const TITLE: &str = "Old View - Raw Hardware Data";
const TEXT: u16 = 1;

/// Shows a fresh raw report in the modal Old View window.
pub fn show(owner: HWND, mask: bool) {
    // C# parity: SectionedViewForm.cs:833 collects again instead of showing the main snapshot.
    let report = match collect(owner, mask) {
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
fn collect(owner: HWND, mask: bool) -> Option<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    // SAFETY: Reads the calling (UI) thread id; no pointers.
    let ui_thread = unsafe { GetCurrentThreadId() };
    let spawned = std::thread::Builder::new()
        .name("old view".to_owned())
        .spawn(move || {
            let report = win::catch_panic(|| {
                let mut sections = hw::collect_all(None, &|_, _| {});
                if mask {
                    sections = sections.iter().map(crate::report::masked).collect();
                }
                hw::full_report(&sections)
            });
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
                // AD-01: the text carries the `{op} failed: 0x{code:08X}` shape.
                *result.borrow_mut() = Some(Err(win::Error::msg(
                    "Old View collection",
                    "ended without a result",
                )
                .to_string()))
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
    // DESIGN.md 11.5: a minimum (C# has none) and the work-area clamp of every form.
    spec.min = Some(theme::OLD_VIEW_MIN_SIZE);
    spec.find_keys = true;
    // C# parity: WordWrap is left at its default (true), so there is no horizontal scroll bar;
    // BorderStyle stays the Fixed3D default. The well sits inside the window padding and
    // opens with nothing selected (DESIGN.md 4; C# selected the whole report).
    let edit = Ctl::Edit(
        EditSpec::new(
            theme::OLD_VIEW_FONT,
            theme::OLD_VIEW_TEXT,
            theme::OLD_VIEW_TEXT_BACKGROUND,
        )
        .word_wrap()
        .keep_selection(),
    );
    let find = super::find::Find::new(TEXT);
    let well = Node::leaf(TEXT, edit).fill().cell(0, 1);
    let mut bar = super::find::bar().cell(0, 0);
    // Keep the existing well margins, but align the bar with its frame. Move the well's
    // top inset above the bar so the visible gap stays 8 and the row still costs 36 px.
    // The bar's own margins are its ring-room overhang (-4 on three sides, 4 below), so the
    // sums are l/r -1, t -1, b 1: the overhang lands inside the panel's 12 px padding.
    bar.margin.l += well.margin.l;
    bar.margin.r += well.margin.r;
    bar.margin.t += well.margin.t;
    bar.margin.b -= well.margin.t;
    window::run_modal(
        owner,
        spec,
        vec![
            Node::table(
                vec![super::layout::Track::Percent(100.0)],
                vec![
                    super::layout::Track::AutoSize,
                    super::layout::Track::Percent(100.0),
                ],
                vec![bar, well],
            )
            .fill()
            .padding(theme::OUTPUT_PANEL_PADDING),
        ],
        move |form, event| {
            if let Event::Key(key) = event {
                return on_find_key(form, &find, key);
            }
            if find.event(form, &event) {
                return true;
            }
            if let Event::Created = event {
                form.edit_set_text(TEXT, &report);
                form.edit_scroll_to_top(TEXT);
            }
            find.refresh(form);
            true
        },
    )
}

// Step 2b installs the shared find component here.
fn on_find_key(form: &window::Form, find: &super::find::Find, key: window::FindKey) -> bool {
    find.key(form, key)
}
