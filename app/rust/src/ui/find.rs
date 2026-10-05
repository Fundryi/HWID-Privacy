//! C4: shared, view-local find bar. Searches the displayed text without changing it.

use super::controls::{Align, ButtonSpec, Ctl, EditSpec, LabelSpec};
use super::layout::{Node, Pad, Size, Track};
use super::theme::{self, glyph};
use super::window::{Event, FindKey, Form};
use std::cell::{Cell, RefCell};
use std::ops::Range;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Controls::{EM_GETSEL, EM_SCROLLCARET, EM_SETSEL};
use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SendMessageW};

// A reserved range, separate from section ids (100..114) and each form's own controls.
const BAR: u16 = 200;
const QUERY: u16 = 201;
const COUNT: u16 = 202;
const PREVIOUS: u16 = 203;
const NEXT: u16 = 204;
const CLOSE: u16 = 205;
pub(super) const ALL_SECTIONS: u16 = 206;

/// The same hidden row in the main content table and the Old View panel.
pub(super) fn bar() -> Node {
    make_bar(false)
}

pub(super) fn all_sections_bar() -> Node {
    make_bar(true)
}

fn make_bar(all_sections: bool) -> Node {
    let input = Node::leaf(
        QUERY,
        Ctl::Edit(
            EditSpec::new(theme::FIND_EDIT_FONT, theme::TEXT, theme::CARD)
                .single_line()
                .cue("Find"),
        ),
    )
    .fill()
    .min(Size {
        w: theme::FIND_EDIT_MIN_WIDTH,
        h: theme::FIND_BAR_HEIGHT,
    })
    .margin(Pad {
        r: theme::FIND_GAP,
        ..theme::NO_PAD
    })
    .cell(0, 0);
    // Right-aligned: the count hugs the buttons it describes; the minimum width keeps the
    // input from resizing on every keystroke.
    let count = Node::leaf(
        COUNT,
        Ctl::Label(
            LabelSpec::new("", theme::FIND_COUNT_FONT, theme::SECONDARY).align(Align::MiddleRight),
        ),
    )
    .fill()
    .auto_size()
    .min(Size {
        w: theme::FIND_COUNT_MIN_WIDTH,
        h: theme::FIND_BAR_HEIGHT,
    })
    .margin(Pad {
        r: theme::FIND_GAP,
        ..theme::NO_PAD
    })
    .cell(1, 0);
    let button = |id, caption, icon, column, gap| {
        Node::leaf(
            id,
            Ctl::Button(ButtonSpec::outline(caption).icon(icon).icon_only()),
        )
        .size(theme::FIND_BUTTON_SIZE)
        .margin(Pad {
            r: gap,
            ..theme::NO_PAD
        })
        .cell(column, 0)
    };
    let mut bar = Node::table(
        vec![
            Track::Percent(100.0),
            Track::AutoSize,
            Track::AutoSize,
            Track::AutoSize,
            Track::AutoSize,
        ],
        vec![Track::Absolute(theme::FIND_BAR_HEIGHT)],
        vec![
            input,
            count,
            button(
                PREVIOUS,
                "Previous match",
                glyph::CHEVRON_UP,
                2,
                theme::FIND_PAIR_GAP,
            ),
            button(NEXT, "Next match", glyph::CHEVRON_DOWN, 3, theme::FIND_GAP),
            button(CLOSE, "Close find", glyph::CANCEL, 4, 0),
        ],
    )
    .id(BAR)
    .fill()
    .auto_size()
    // The bar is its own window, which clips the focus rings of its children: pad by the ring
    // room and overhang the row by the same, so the input stays flush with the well.
    .padding(theme::FIND_BAR_PADDING)
    .margin(theme::FIND_BAR_MARGIN)
    .visible(false);
    if all_sections {
        // Keep the count beside its navigation buttons, in visual/tab order.
        if let super::layout::Kind::Table { cols, cells, .. } = &mut bar.kind {
            cols.insert(1, Track::AutoSize);
            for child in cells.iter_mut().skip(1) {
                child.col += 1;
            }
            cells.insert(
                1,
                Node::leaf(
                    ALL_SECTIONS,
                    Ctl::Button(
                        ButtonSpec::outline("All sections")
                            .icon(glyph::LIST)
                            .toggle(),
                    ),
                )
                .auto_size()
                .min(Size {
                    w: 0,
                    h: theme::FIND_BAR_HEIGHT,
                })
                .max(Size {
                    w: 0,
                    h: theme::FIND_BAR_HEIGHT,
                })
                .margin(Pad {
                    r: theme::FIND_GAP,
                    ..theme::NO_PAD
                })
                .cell(1, 0),
            );
        }
    }
    bar
}

type SectionSource = Box<dyn Fn() -> (usize, Vec<String>)>;
type ShowSection = Box<dyn Fn(&Form, usize)>;

/// One component per form; the snapshot detects section/reload/mask changes after host events.
pub(super) struct Find {
    well: u16,
    text: RefCell<String>,
    sections: Option<(SectionSource, ShowSection)>,
    snapshot: RefCell<(usize, Vec<String>)>,
    searching: Cell<bool>,
}

impl Find {
    /// Binds this bar to the form's existing text well.
    pub(super) fn new(well: u16) -> Self {
        Self {
            well,
            text: RefCell::new(String::new()),
            sections: None,
            snapshot: RefCell::new((0, Vec::new())),
            searching: Cell::new(false),
        }
    }

    pub(super) fn with_sections(
        well: u16,
        source: impl Fn() -> (usize, Vec<String>) + 'static,
        show: impl Fn(&Form, usize) + 'static,
    ) -> Self {
        Self {
            sections: Some((Box::new(source), Box::new(show))),
            ..Self::new(well)
        }
    }

    fn all_sections(&self, form: &Form) -> bool {
        self.sections.is_some() && form.is_checked(ALL_SECTIONS)
    }

    pub(super) fn section_changed(&self, form: &Form) {
        if self.shown(form) && self.available(form) && self.all_sections(form) {
            self.search(form, Direction::Current);
        }
    }

    fn shown(&self, form: &Form) -> bool {
        form.with_tree(|tree| tree.find(BAR).is_some_and(|node| node.visible))
            .unwrap_or(false)
    }

    fn available(&self, form: &Form) -> bool {
        // SAFETY: Read-only query of this form's live child, including ancestor visibility.
        form.control(self.well)
            .is_some_and(|well| unsafe { IsWindowVisible(well) }.as_bool())
    }

    /// Handles the bar's notifications; other controls retain the host's behavior.
    pub(super) fn event(&self, form: &Form, event: &Event) -> bool {
        match *event {
            Event::Resize { .. } => {
                // Scale each distance before subtracting: at custom DPI, scaling the logical
                // sums drifts by a pixel (for example the Old View's -1 margin at 137 DPI).
                // The kit lays out after this event, so this adds no layout pass.
                let gap = form.scale(theme::FIND_GAP);
                form.with_tree(|tree| {
                    let Some(well_margin) = tree.find(self.well).map(|well| well.margin) else {
                        return;
                    };
                    if let Some(bar) = tree.find_mut(BAR) {
                        bar.margin = Pad {
                            l: well_margin.l - bar.padding.l,
                            t: well_margin.t - bar.padding.t,
                            r: well_margin.r - bar.padding.r,
                            b: gap - bar.padding.b - well_margin.t,
                        };
                    }
                });
                return false;
            }
            Event::TextChanged(QUERY) if self.shown(form) => self.search(form, Direction::Current),
            Event::Click(PREVIOUS) => self.search(form, Direction::Previous),
            Event::Click(NEXT) => self.search(form, Direction::Next),
            Event::Click(CLOSE) => self.close(form),
            Event::Toggled(ALL_SECTIONS, _) => self.search(form, Direction::Current),
            _ => return false,
        }
        true
    }

    /// Implements the kit's key intents; false leaves unrelated keys with the dialog router.
    pub(super) fn key(&self, form: &Form, key: FindKey) -> bool {
        if !self.available(form) {
            return false;
        }
        match key {
            FindKey::Open => {
                let text = form.text(self.well);
                let selection = selection(form, self.well);
                let wide: Vec<_> = text.encode_utf16().collect();
                let selected = wide.get(selection).and_then(|s| String::from_utf16(s).ok());
                // Publish the snapshot before showing controls: layout/text notifications reenter.
                *self.text.borrow_mut() = text;
                form.set_visible(BAR, true);
                if let Some(selected) = selected
                    && !selected.is_empty()
                    && !selected.contains(['\r', '\n'])
                    && selected.encode_utf16().count() <= theme::FIND_QUERY_MAX
                {
                    form.set_text(QUERY, &selected);
                }
                self.search(form, Direction::Current);
                form.focus(QUERY);
                select(
                    form,
                    QUERY,
                    0..form.text(QUERY).encode_utf16().count(),
                    false,
                );
            }
            FindKey::Step { backwards } if self.shown(form) => self.step(form, backwards),
            FindKey::Enter {
                id: QUERY,
                backwards,
            } if self.shown(form) => self.step(form, backwards),
            FindKey::Escape {
                id: Some(QUERY | PREVIOUS | NEXT | CLOSE | ALL_SECTIONS),
            } if self.shown(form) => self.close(form),
            _ => return false,
        }
        true
    }

    /// Re-searches replaced content from the top, after the host finishes updating its well.
    pub(super) fn refresh(&self, form: &Form) {
        if self.searching.get() {
            return;
        }
        if !self.available(form) {
            *self.snapshot.borrow_mut() = (0, Vec::new());
            return;
        }
        if self.shown(form)
            && self.all_sections(form)
            && let Some((source, _)) = &self.sections
        {
            let current = source();
            if *self.snapshot.borrow() != current {
                // Manual section changes start there; mask/reload changes restart globally.
                let direction = if self.snapshot.borrow().1 == current.1 {
                    Direction::Current
                } else {
                    Direction::Top
                };
                self.search(form, direction);
            }
        } else if self.shown(form) {
            let text = form.text(self.well);
            if *self.text.borrow() != text {
                *self.text.borrow_mut() = text;
                self.search(form, Direction::Top);
            }
        }
    }

    fn close(&self, form: &Form) {
        form.set_visible(BAR, false);
        form.focus(self.well);
    }

    fn step(&self, form: &Form, backwards: bool) {
        self.search(
            form,
            if backwards {
                Direction::Previous
            } else {
                Direction::Next
            },
        );
    }

    fn search(&self, form: &Form, direction: Direction) {
        if self.searching.replace(true) {
            return;
        }
        let query = form.text(QUERY);
        let scope = self.sections.as_ref().filter(|_| self.all_sections(form));
        let all = scope.is_some();
        let (active, texts) = if let Some((source, _)) = scope {
            source()
        } else {
            (0, vec![form.text(self.well)])
        };
        let matches: Vec<_> = texts
            .iter()
            .enumerate()
            .flat_map(|(section, text)| {
                matches(text, &query)
                    .into_iter()
                    .map(move |range| (section, range))
            })
            .collect();
        let selected = selection(form, self.well);
        let index = match direction {
            Direction::Top => 0,
            Direction::Current => matches
                .iter()
                .position(|(section, m)| (*section, m.start) >= (active, selected.start))
                .unwrap_or(0),
            Direction::Next => matches
                .iter()
                .position(|(section, m)| (*section, m.start) >= (active, selected.end))
                .unwrap_or(0),
            Direction::Previous => matches
                .iter()
                .rposition(|(section, m)| (*section, m.start) < (active, selected.start))
                .unwrap_or(matches.len().saturating_sub(1)),
        };
        let count = if let Some((section, found)) = matches.get(index) {
            if all {
                *self.snapshot.borrow_mut() = (*section, texts);
                if *section != active
                    && let Some((_, show)) = scope
                {
                    show(form, *section);
                }
            }
            select(form, self.well, found.clone(), true);
            format!("{} of {}", index + 1, matches.len())
        } else {
            if all {
                *self.snapshot.borrow_mut() = (active, texts);
            }
            select(form, self.well, selected.start..selected.start, false);
            if query.is_empty() {
                String::new()
            } else {
                "No matches".to_owned()
            }
        };
        *self.text.borrow_mut() = form.text(self.well);
        form.set_enabled(PREVIOUS, !matches.is_empty());
        form.set_enabled(NEXT, !matches.is_empty());
        if form.text(COUNT) != count {
            form.set_text(COUNT, &count);
        }
        self.searching.set(false);
    }
}

enum Direction {
    Current,
    Next,
    Previous,
    Top,
}

/// Maps lowercase UTF-8 matches back to the original EDIT's UTF-16 offsets, including
/// non-BMP text and lowercase expansions (for example dotted capital I).
fn matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let folded = text.to_lowercase();
    let mut offsets = Vec::with_capacity(folded.len());
    let mut utf16 = 0;
    for c in text.chars() {
        let end = utf16 + c.len_utf16();
        for _ in 0..c.to_lowercase().map(char::len_utf8).sum::<usize>() {
            offsets.push(utf16..end);
        }
        utf16 = end;
    }
    let query = query.to_lowercase();
    folded
        .match_indices(&query)
        .filter_map(|(start, found)| {
            Some(offsets.get(start)?.start..offsets.get(start + found.len() - 1)?.end)
        })
        .collect()
}

// The trunk exposes text/focus but not selection. Keep the two synchronous EDIT operations
// local to this component so step 2b does not modify the shared window/controls kit.
fn selection(form: &Form, id: u16) -> Range<usize> {
    let (mut start, mut end) = (0u32, 0u32);
    if let Some(edit) = form.control(id) {
        // SAFETY: EM_GETSEL writes two DWORDs; both stack addresses live through SendMessageW.
        unsafe {
            SendMessageW(
                edit,
                EM_GETSEL,
                Some(WPARAM(&mut start as *mut u32 as usize)),
                Some(LPARAM(&mut end as *mut u32 as isize)),
            );
        }
    }
    start as usize..end as usize
}

fn select(form: &Form, id: u16, range: Range<usize>, scroll: bool) {
    if let Some(edit) = form.control(id) {
        // SAFETY: Value-only messages to the form's EDIT; indices come from its own text.
        unsafe {
            SendMessageW(
                edit,
                EM_SETSEL,
                Some(WPARAM(range.start)),
                Some(LPARAM(range.end as isize)),
            );
            if scroll {
                SendMessageW(edit, EM_SCROLLCARET, None, None);
            }
        }
    }
}
