//! C2: two native pickers, background parsing and a modal comparison well.

use super::controls::{ButtonSpec, Ctl, EditSpec, LabelSpec};
use super::layout::{Anchor, Node, Track};
use super::msgbox::{self, Buttons, Icon};
use super::theme;
use super::window::{self, Event, FindKey, Form, FormSpec, WindowSize};
use crate::report::compare::{self, Comparison, ReadError};
use crate::win;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use windows::Win32::Foundation::HWND;

const COPY: u16 = 1;
const TEXT: u16 = 2;
const BEFORE_LABEL: u16 = 3;
const BEFORE_NAME: u16 = 4;
const AFTER_LABEL: u16 = 5;
const AFTER_NAME: u16 = 6;
const SUMMARY: u16 = 7;
const TITLE: &str = "Compare Exports";

/// Selects exports, keeps the main window usable while parsing, then shows the comparison.
pub fn show(form: &Form, button: u16) {
    if !form.is_enabled(button) {
        return;
    }
    let folder = match std::env::current_exe() {
        Ok(exe) => exe.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Err(error) => {
            window::show_error(form.hwnd(), &error.to_string(), "Compare Error");
            return;
        }
    };
    let filters = [
        ("HWID exports (*.txt;*.json)", "*.txt;*.json"),
        ("All files (*.*)", "*.*"),
    ];
    let Some(before) =
        win::dialog::open_file(form.hwnd(), "Select the BEFORE export", &filters, &folder)
    else {
        return;
    };
    let Some(after) = win::dialog::open_file(
        form.hwnd(),
        "Select the AFTER export",
        &filters,
        before.parent().unwrap_or(&folder),
    ) else {
        return;
    };
    form.set_text(button, "Comparing...");
    form.set_enabled(button, false);
    let result = collect(form, before.clone(), after.clone());
    if !form.is_alive() {
        return;
    }
    form.set_text(button, TITLE);
    form.set_enabled(button, true);
    match result {
        Some(Ok(result)) => {
            if let Err(error) = show_result(form.hwnd(), &before, &after, result) {
                window::show_error(msgbox::active_window(), &error.to_string(), "Compare Error");
            }
        }
        Some(Err(error)) => {
            msgbox::show(
                msgbox::active_window(),
                error.message(),
                "Compare Error",
                Buttons::Ok,
                if matches!(error, ReadError::Empty(_)) {
                    Icon::Warning
                } else {
                    Icon::Error
                },
            );
        }
        // WM_QUIT or owner destruction ends the nested pump without displaying stale data.
        None => {}
    }
}

fn collect(form: &Form, before: PathBuf, after: PathBuf) -> Option<Result<Comparison, ReadError>> {
    let poster = form.poster()?;
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("compare exports".to_owned())
        .spawn(move || {
            let result = win::catch_panic(|| {
                let left = compare::read(&before)?;
                let right = compare::read(&after)?;
                Ok(compare::compare(&left, &right, &before, &after))
            })
            .unwrap_or_else(|error| Err(ReadError::Read(error)));
            if tx.send(result).is_ok() {
                // Value-only wakeup: the main handler ignores its type. Poster checks its window
                // generation, so a closed/reused HWND cannot receive a stale result.
                let _ = poster.post(());
            }
        });
    if let Err(error) = spawned {
        return Some(Err(ReadError::Read(
            win::Error::msg("thread::spawn", error.to_string()).to_string(),
        )));
    }
    // Like Old View's collection pump, but without hardware work or UI-thread file reads.
    let result = RefCell::new(None);
    window::pump_until(|| {
        match rx.try_recv() {
            Ok(value) => *result.borrow_mut() = Some(value),
            Err(TryRecvError::Disconnected) if result.borrow().is_none() => {
                *result.borrow_mut() = Some(Err(ReadError::Read(
                    win::Error::msg("Compare exports", "ended without a result").to_string(),
                )));
            }
            Err(TryRecvError::Disconnected | TryRecvError::Empty) => {}
        }
        result.borrow().is_some() || !form.is_alive()
    });
    result.into_inner()
}

fn show_result(owner: HWND, before: &Path, after: &Path, result: Comparison) -> win::Result<()> {
    let (spec, nodes) = parts(before, after, &result.summary);
    window::run_modal(owner, spec, nodes, move |form, event| {
        handle(form, event, &result.text)
    })
}

fn handle(form: &Form, event: Event, text: &str) -> bool {
    match event {
        Event::Created => {
            form.edit_set_text(TEXT, text);
            form.edit_scroll_to_top(TEXT);
        }
        Event::Click(COPY) => form.edit_copy_all(TEXT),
        Event::Key(FindKey::Escape { .. }) => form.close(),
        Event::Key(_) => return false,
        _ => {}
    }
    true
}

fn parts(before: &Path, after: &Path, summary: &str) -> (FormSpec, Vec<Node>) {
    let mut spec = FormSpec::new(
        TITLE,
        WindowSize::Outer {
            size: theme::COMPARE_SIZE,
            scaled: true,
        },
    );
    spec.min = Some(theme::COMPARE_MIN_SIZE);
    // Use the kit's Escape event directly: no hidden/focusable Close control or find bar.
    spec.find_keys = true;
    let mut rows = Vec::new();
    for (row, caption, path, label_id, name_id) in [
        (0, "Before", before, BEFORE_LABEL, BEFORE_NAME),
        (1, "After", after, AFTER_LABEL, AFTER_NAME),
    ] {
        rows.push(
            Node::leaf(
                label_id,
                Ctl::Label(LabelSpec::new(
                    caption,
                    theme::SECTION_META_FONT,
                    theme::SECONDARY,
                )),
            )
            .fill()
            .margin(theme::NO_PAD)
            .cell(0, row),
        );
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        rows.push(
            Node::leaf(
                name_id,
                Ctl::Label(LabelSpec::new(&name, theme::BODY_FONT, theme::TEXT).ellipsis()),
            )
            .fill()
            .margin(theme::NO_PAD)
            .cell(1, row),
        );
    }
    rows.push(
        Node::leaf(
            SUMMARY,
            Ctl::Label(LabelSpec::new(summary, theme::SECTION_META_FONT, theme::FAINT).ellipsis()),
        )
        .fill()
        .margin(theme::NO_PAD)
        .cell(0, 2)
        .span(2),
    );
    let names = Node::table(
        vec![
            Track::Absolute(theme::COMPARE_LABEL_WIDTH),
            Track::Percent(100.0),
        ],
        vec![
            Track::Absolute(theme::COMPARE_FILE_HEIGHT),
            Track::Absolute(theme::COMPARE_FILE_HEIGHT),
            Track::Absolute(theme::SECTION_META_HEIGHT),
        ],
        rows,
    )
    .fill()
    .auto_size()
    .margin(theme::NO_PAD)
    .cell(0, 0);
    let copy = Node::leaf(
        COPY,
        Ctl::Button(ButtonSpec::outline("Copy").icon(theme::glyph::COPY)),
    )
    // As in the main header, 72 is a minimum: Inter + icon wraps at exactly 72 at 96 DPI.
    .auto_size()
    .min(theme::COPY_BUTTON_SIZE)
    .anchor(Anchor::NONE)
    .margin(theme::NO_PAD)
    .cell(1, 0);
    let header = Node::table(
        vec![Track::Percent(100.0), Track::AutoSize],
        vec![Track::AutoSize],
        vec![names, copy],
    )
    .fill()
    .auto_size()
    .card(theme::HEADER_RADIUS)
    .back(theme::CARD)
    .padding(theme::HEADER_PADDING)
    .margin(theme::HEADER_MARGIN)
    .cell(0, 0);
    let well = Node::leaf(
        TEXT,
        Ctl::Edit(EditSpec::new(theme::CONTENT_FONT, theme::TEXT, theme::CARD)),
    )
    .fill()
    .margin(theme::NO_PAD)
    .cell(0, 1);
    let panel = Node::table(
        vec![Track::Percent(100.0)],
        vec![Track::AutoSize, Track::Percent(100.0)],
        vec![header, well],
    )
    .fill()
    .padding(theme::CONTENT_PADDING)
    .margin(theme::NO_PAD);
    (spec, vec![panel])
}
