//! C2/C2b flows with the C2c modal compare table.

pub(crate) mod table;
pub(crate) mod text;

use super::controls::{self, ButtonSpec, Ctl, LabelSpec};
use super::layout::{Anchor, FlowDir, Node, Pad, Size, Track};
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
const CHANGED: u16 = 8;
const ALL: u16 = 9;
const UNCHANGED: u16 = 10;
const SAFE: u16 = 11;
const DEVICES: u16 = 12;
const SAVE: u16 = 13;
const LEGEND: u16 = 20;
const FILTERS: [u16; 4] = [CHANGED, UNCHANGED, SAFE, DEVICES];
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
    let (mut spec, nodes) = parts(before, after, table.clone());
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
            sync_filters(form, &table.borrow());
            form.table_refresh(TEXT);
        }
        Event::Toggled(id, on) if FILTERS.contains(&id) || id == ALL => {
            {
                let mut model = table.borrow_mut();
                if id == ALL {
                    model.all = on;
                    model.filters = [on; 4];
                } else if let Some(index) = FILTERS.iter().position(|filter| *filter == id) {
                    model.filters[index] = on;
                    if !on {
                        model.all = false;
                    }
                }
                sync_filters(form, &model);
            }
            form.table_refresh(TEXT);
        }
        Event::Click(COPY) => controls::copy_text(form.hwnd(), text),
        Event::Click(SAVE) => {
            if let Err(error) = save(form.hwnd(), text) {
                window::show_error(form.hwnd(), &error.to_string(), "Save Compare Error");
            }
        }
        Event::Key(FindKey::Escape { .. }) => form.close(),
        Event::Key(_) => return false,
        _ => {}
    }
    true
}

fn sync_filters(form: &Form, table: &table::Table) {
    for (id, on) in FILTERS
        .into_iter()
        .zip(table.filters)
        .chain([(ALL, table.all)])
    {
        form.set_checked(id, on);
        form.set_button_fore(id, Some(if on { theme::TEXT } else { theme::SECONDARY }));
    }
}

fn save(owner: HWND, text: &str) -> win::Result<()> {
    let exe = std::env::current_exe()
        .map_err(|e| win::Error::msg("Locate executable folder", e.to_string()))?;
    let folder = exe
        .parent()
        .ok_or_else(|| win::Error::msg("Locate executable folder", "no parent"))?;
    if let Some(path) = win::dialog::save_compare(owner, folder)? {
        std::fs::write(path, text)
            .map_err(|e| win::Error::msg("Save comparison", e.to_string()))?;
    }
    Ok(())
}

fn parts(before: &Path, after: &Path, table: Rc<RefCell<table::Table>>) -> (FormSpec, Vec<Node>) {
    let mut spec = FormSpec::new(
        TITLE,
        WindowSize::Outer {
            size: theme::COMPARE_SIZE,
            scaled: true,
        },
    );
    spec.min = Some(theme::COMPARE_MIN_SIZE);
    spec.find_keys = true;
    let mut names = Vec::new();
    for (row, caption, path, label, name) in [
        (0, "Before", before, BEFORE_LABEL, BEFORE_NAME),
        (1, "After", after, AFTER_LABEL, AFTER_NAME),
    ] {
        names.push(
            Node::leaf(
                label,
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
        names.push(
            Node::leaf(
                name,
                Ctl::Label(
                    LabelSpec::new(
                        &path
                            .file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy(),
                        theme::BODY_FONT,
                        theme::TEXT,
                    )
                    .ellipsis(),
                ),
            )
            .fill()
            .margin(theme::NO_PAD)
            .cell(1, row),
        );
    }
    let names = Node::table(
        vec![
            Track::Absolute(theme::COMPARE_LABEL_WIDTH),
            Track::Percent(100.0),
        ],
        vec![Track::Absolute(theme::COMPARE_FILE_HEIGHT); 2],
        names,
    )
    .fill()
    .margin(theme::NO_PAD)
    .cell(0, 0);
    let actions = [
        (COPY, "Copy", theme::glyph::COPY),
        (SAVE, "Save as .txt", theme::glyph::SAVE),
    ]
    .into_iter()
    .map(|(id, text, icon)| {
        let mut button = ButtonSpec::outline(text).icon(icon);
        button.single_line = true;
        Node::leaf(id, Ctl::Button(button))
            .auto_size()
            .min(theme::COPY_BUTTON_SIZE)
            .padding(Pad {
                l: theme::COMPARE_TEXT_GAP,
                r: theme::COMPARE_TEXT_GAP,
                ..theme::NO_PAD
            })
            .margin(Pad {
                l: theme::COMPARE_GAP,
                ..theme::NO_PAD
            })
    })
    .collect();
    let actions = Node::flow(FlowDir::LeftToRight, false, actions)
        .auto_size()
        .anchor(Anchor::NONE)
        .margin(theme::NO_PAD)
        .cell(1, 0);
    let files = Node::table(
        vec![Track::Percent(100.0), Track::AutoSize],
        vec![Track::Absolute(2 * theme::COMPARE_FILE_HEIGHT)],
        vec![names, actions],
    )
    .fill()
    .margin(theme::NO_PAD)
    .cell(0, 0);
    let model = table.borrow();
    let counts = model.counts;
    let extras = model.extras;
    let verdict_text = if !model.verdicts() {
        "Masked exports: spoof verdict unavailable".to_owned()
    } else if counts == [0; 2] {
        "No identifiers found".to_owned()
    } else {
        format!(
            "Spoof check: {} of {} unique IDs changed · {} unchanged",
            counts[1],
            counts[0] + counts[1],
            counts[0]
        )
    };
    let mut verdict = LabelSpec::new(&verdict_text, theme::BUTTON_FONT, theme::TEXT);
    verdict.compare = Some(controls::CompareLabel::Verdict(counts, model.verdicts()));
    let verdict = Node::leaf(SUMMARY, Ctl::Label(verdict))
        .fill()
        .margin(theme::NO_PAD)
        .cell(0, 2);
    let captions = [
        (
            CHANGED,
            format!("Changed {}", counts[1]),
            Some((theme::SUCCESS, false)),
        ),
        (
            UNCHANGED,
            format!("Unchanged {}", counts[0]),
            Some((theme::DANGER, false)),
        ),
        (
            SAFE,
            format!("Safe (not unique) {}", extras[0]),
            Some((theme::COMPARE_SAFE, true)),
        ),
        (
            DEVICES,
            format!("Devices +{} −{} ↔{}", extras[1], extras[2], extras[3]),
            Some((theme::INFO, false)),
        ),
        (ALL, "All".to_owned(), None),
    ];
    drop(model);
    let chips = captions
        .into_iter()
        .map(|(id, text, dot)| {
            let mut button = ButtonSpec::outline(&text).toggle();
            button.single_line = true;
            button.compare_chip = Some(dot);
            Node::leaf(id, Ctl::Button(button))
                .auto_size()
                .min(Size {
                    w: 0,
                    h: theme::COMPARE_FILTER_HEIGHT,
                })
                .padding(Pad {
                    l: theme::COMPARE_TEXT_GAP,
                    r: theme::COMPARE_TEXT_GAP,
                    ..theme::NO_PAD
                })
                .margin(Pad {
                    r: theme::COMPARE_GAP,
                    b: theme::COMPARE_GAP,
                    ..theme::NO_PAD
                })
        })
        .collect();
    let chips = Node::flow(FlowDir::LeftToRight, true, chips)
        .fill()
        .auto_size()
        .margin(theme::NO_PAD)
        .cell(0, 4);
    // Flow's bottom margin supplies the card's bottom 8 px, including wrapped rows.
    let header = Node::table(
        vec![Track::Percent(100.0)],
        vec![
            Track::Absolute(2 * theme::COMPARE_FILE_HEIGHT),
            Track::Absolute(theme::COMPARE_GAP),
            Track::Absolute(theme::COMPARE_FILE_HEIGHT),
            Track::Absolute(theme::COMPARE_GAP),
            Track::AutoSize,
        ],
        vec![files, verdict, chips],
    )
    .fill()
    .auto_size()
    .card(theme::HEADER_RADIUS)
    .back(theme::CARD)
    .padding(Pad {
        b: 0,
        ..theme::HEADER_PADDING
    })
    .margin(theme::HEADER_MARGIN)
    .cell(0, 0);
    let well = Node::leaf(TEXT, Ctl::Table(table))
        .fill()
        .margin(theme::NO_PAD)
        .cell(0, 1);
    let legend = [
        ("changed ID: spoof worked", theme::SUCCESS, false),
        ("unchanged unique ID: still the same", theme::DANGER, false),
        (
            "safe: not unique, fine if it stays",
            theme::COMPARE_SAFE,
            true,
        ),
        ("added / removed / moved device", theme::INFO, false),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (text, color, hollow))| {
        let mut label = LabelSpec::new(text, theme::SMALL_FONT, theme::FAINT);
        label.compare = Some(controls::CompareLabel::Legend(color, hollow));
        Node::leaf(LEGEND + i as u16, Ctl::Label(label))
            .auto_size()
            .min(Size {
                w: 0,
                h: theme::COMPARE_ROW_HEIGHT,
            })
            .margin(Pad {
                r: theme::COMPARE_LEGEND_GAP,
                ..theme::NO_PAD
            })
    })
    .collect();
    let legend = Node::flow(FlowDir::LeftToRight, true, legend)
        .fill()
        .auto_size()
        .margin(Pad {
            t: theme::COMPARE_GAP,
            r: -theme::COMPARE_LEGEND_GAP,
            ..theme::NO_PAD
        })
        .cell(0, 2);
    let panel = Node::table(
        vec![Track::Percent(100.0)],
        vec![Track::AutoSize, Track::Percent(100.0), Track::AutoSize],
        vec![header, well, legend],
    )
    .fill()
    .padding(theme::CONTENT_PADDING)
    .margin(theme::NO_PAD);
    (spec, vec![panel])
}
