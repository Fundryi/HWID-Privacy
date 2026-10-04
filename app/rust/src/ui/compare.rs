//! C2/C2b flows with the C2c modal compare table.

pub(crate) mod table;
pub(crate) mod text;

use super::controls::{self, ButtonSpec, Ctl, LabelSpec};
use super::layout::{Anchor, Node, Pad, Size, Track};
use super::msgbox::{self, Buttons, Icon};
use super::theme;
use super::window::{self, Event, FindKey, Form, FormSpec, WindowSize};
use crate::report::compare::{self, Comparison, ReadError};
use crate::report::{self, Section};
use crate::win;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use windows::Win32::Foundation::HWND;

const COPY: u16 = 1;
const TEXT: u16 = 2;
const BEFORE_LABEL: u16 = 3;
const BEFORE_NAME: u16 = 4;
const AFTER_LABEL: u16 = 5;
const AFTER_NAME: u16 = 6;
const SUMMARY: u16 = 7;
const FILTER: u16 = 8;
const ALL: u16 = 9;
const LEGEND: u16 = 10;
const TITLE: &str = "Compare Exports";

struct Collected {
    comparison: Comparison,
    ids: Vec<String>,
    table: table::Table,
}

/// Selects exports, keeps the main window usable while parsing, then shows the comparison.
pub fn show(form: &Form, button: u16) {
    if !form.is_enabled(button) {
        return;
    }
    show_with(
        form,
        None,
        |busy| {
            form.set_text(
                button,
                if busy {
                    "Comparing..."
                } else {
                    "Compare files"
                },
            );
            form.set_enabled(button, !busy);
        },
        || false,
    );
}

/// Shares the picker/worker flow while the caller owns the pair's loading and busy states.
pub(crate) fn show_with(
    form: &Form,
    current: Option<Vec<Section>>,
    busy: impl Fn(bool),
    masked: impl Fn() -> bool,
) {
    let live = current.is_some();
    let current = current.map(|sections| {
        let mut export = compare::from_sections(&sections, false);
        // A live snapshot always contains the original provider values, even X-like IDs.
        export.masked = false;
        export
    });
    let folder = match std::env::current_exe() {
        Ok(exe) => exe.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Err(error) => {
            window::show_error(form.hwnd(), &error.to_string(), "Compare Error");
            return;
        }
    };
    let filters = [
        ("Text exports (*.txt)", "*.txt"),
        ("Older JSON exports (*.json)", "*.json"),
        ("All files (*.*)", "*.*"),
    ];
    let Some(before) = win::dialog::open_file(
        form.hwnd(),
        if live {
            "Select the export to compare with the current system"
        } else {
            "Select the BEFORE export"
        },
        &filters,
        &folder,
    ) else {
        return;
    };
    let after = if live {
        PathBuf::from("Current system")
    } else {
        let Some(after) = win::dialog::open_file(
            form.hwnd(),
            "Select the AFTER export",
            &filters,
            before.parent().unwrap_or(&folder),
        ) else {
            return;
        };
        after
    };
    busy(true);
    let result = collect(form, before.clone(), after.clone(), current);
    if !form.is_alive() {
        return;
    }
    busy(false);
    match result {
        Some(Ok(Collected {
            comparison: mut result,
            ids,
            mut table,
        })) => {
            if masked() {
                table.mask(&ids);
                // ponytail: text exports carry no IDs, so text-only before-values stay visible.
                result.text = report::masked(&Section {
                    body: result.text,
                    ids,
                    ..Section::default()
                })
                .body;
            }
            if let Err(error) = show_result(form.hwnd(), &before, &after, result, table, live) {
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

fn collect(
    form: &Form,
    before: PathBuf,
    after: PathBuf,
    current: Option<compare::Export>,
) -> Option<Result<Collected, ReadError>> {
    let poster = form.poster()?;
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("compare exports".to_owned())
        .spawn(move || {
            let result = win::catch_panic(|| {
                let left = compare::read(&before)?;
                let live = current.is_some();
                let right = match current {
                    Some(export) => export,
                    None => compare::read(&after)?,
                };
                if right.is_empty() {
                    return Err(ReadError::Empty(format!(
                        "No hardware values found in: {}",
                        after.display()
                    )));
                }
                if left.masked != right.masked {
                    let path = if left.masked { &before } else { &after };
                    let name = path
                        .file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy();
                    let reason = if live {
                        "It cannot be compared with the current system."
                    } else {
                        "A masked export can only be compared with another masked export."
                    };
                    return Err(ReadError::Empty(format!(
                        "{name} is masked (Mask IDs was on when it was exported). {reason}"
                    )));
                }
                let ids = left.ids().iter().chain(right.ids()).cloned().collect();
                let mut result = compare::compare(&left, &right, &before, &after);
                text::format(&mut result, &left, &right, &before, &after);
                let table = table::Table::new(&result, &left, &right);
                Ok(Collected {
                    comparison: result,
                    ids,
                    table,
                })
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

fn show_result(
    owner: HWND,
    before: &Path,
    after: &Path,
    result: Comparison,
    table: table::Table,
    live: bool,
) -> win::Result<()> {
    let table = Rc::new(RefCell::new(table));
    let (mut spec, nodes) = parts(before, after, &result.summary, table.clone());
    if live {
        spec.title = "Compare with Current".to_owned();
    }
    window::run_modal(owner, spec, nodes, move |form, event| {
        handle(form, event, &result.text, &table)
    })
}

fn handle(form: &Form, event: Event, text: &str, table: &RefCell<table::Table>) -> bool {
    match event {
        Event::Created => {
            form.set_checked(FILTER, true);
            form.set_checked(ALL, false);
            form.set_button_fore(ALL, Some(theme::SECONDARY));
            form.table_refresh(TEXT);
        }
        Event::Toggled(id, _) if id == FILTER || id == ALL => {
            table.borrow_mut().all = id == ALL;
            form.set_checked(FILTER, id == FILTER);
            form.set_checked(ALL, id == ALL);
            form.set_button_fore(
                FILTER,
                Some(if id == FILTER {
                    theme::TEXT
                } else {
                    theme::SECONDARY
                }),
            );
            form.set_button_fore(
                ALL,
                Some(if id == ALL {
                    theme::TEXT
                } else {
                    theme::SECONDARY
                }),
            );
            form.table_refresh(TEXT);
        }
        Event::Click(COPY) => controls::copy_text(form.hwnd(), text),
        Event::Key(FindKey::Escape { .. }) => form.close(),
        Event::Key(_) => return false,
        _ => {}
    }
    true
}

fn parts(
    before: &Path,
    after: &Path,
    summary: &str,
    table: Rc<RefCell<table::Table>>,
) -> (FormSpec, Vec<Node>) {
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
    let mut filters = Vec::new();
    for (col, id, caption) in [(0, FILTER, "IDs & changes"), (2, ALL, "All")] {
        let mut button = ButtonSpec::outline(caption).toggle();
        button.single_line = true;
        filters.push(
            Node::leaf(id, Ctl::Button(button))
                .auto_size()
                .fill()
                .min(Size {
                    w: 96,
                    h: theme::COMPARE_FILTER_HEIGHT,
                })
                .padding(Pad {
                    l: theme::COMPARE_CELL_PADDING,
                    r: theme::COMPARE_CELL_PADDING,
                    ..theme::NO_PAD
                })
                .margin(theme::NO_PAD)
                .cell(col, 0),
        );
    }
    let counts = table.borrow().counts;
    for (i, (text, color)) in table::legend(counts)
        .into_iter()
        .zip([
            if counts == [0; 2] {
                theme::FAINT
            } else {
                theme::DANGER
            },
            theme::FAINT,
            theme::SUCCESS,
        ])
        .enumerate()
    {
        filters.push(
            Node::leaf(
                LEGEND + i as u16,
                Ctl::Label(LabelSpec::new(&text, theme::SECTION_META_FONT, color)),
            )
            .auto_size()
            .fill()
            .margin(theme::NO_PAD)
            .cell(4 + i, 0),
        );
    }
    filters.push(
        Node::leaf(
            SUMMARY,
            Ctl::Label(LabelSpec::new(summary, theme::SECTION_META_FONT, theme::FAINT).ellipsis()),
        )
        .fill()
        .margin(theme::NO_PAD)
        .cell(8, 0),
    );
    rows.push(
        Node::table(
            vec![
                Track::AutoSize,
                Track::Absolute(4),
                Track::AutoSize,
                Track::Absolute(12),
                Track::AutoSize,
                Track::AutoSize,
                Track::AutoSize,
                Track::Absolute(12),
                Track::Percent(100.0),
            ],
            vec![Track::Absolute(theme::COMPARE_FILTER_HEIGHT)],
            filters,
        )
        .fill()
        .auto_size()
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
            Track::Absolute(theme::COMPARE_FILTER_HEIGHT),
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
    let well = Node::leaf(TEXT, Ctl::Table(table))
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
