//! Owned by WP-10b: the main hardware window (C# `SectionedViewForm` with `isMainWindow: true`).
//!
//! Load flow (`SectionedViewForm.cs:496-604`): every section starts as `Loading...` with the
//! loading overlay shown; one worker runs all providers; the sections fill only when all are done
//! (OPT-4). Each load has an id, so only the newest load's result is applied (F26, AD-41).

use super::controls::{ButtonSpec, Ctl, EditSpec, LabelSpec};
use super::layout::{Anchor, FlowDir, Kind, Node, Size, Track};
use super::msgbox::{self, Buttons, Icon};
use super::theme::glyph;
use super::window::{self, Event, Form, FormSpec, StartPosition, WindowSize};
use super::{clean_devices, clean_logs, dpi, raw_view, theme, update_progress};
use crate::report::{self, Section};
use crate::win::hash::io_error;
use crate::{hw, settings, update, win};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::SM_CXVSCROLL;

const MAIN_TABLE: u16 = 1;
const SIDEBAR: u16 = 2;
const SIDEBAR_TITLE: u16 = 3;
const SIDEBAR_SUBTITLE: u16 = 4;
const SECTION_TITLE: u16 = 5;
const SECTION_META: u16 = 6;
const CONTENT: u16 = 7;
const FOOTER: u16 = 8;
/// The loading state (fills the content pane while a load runs).
const LOADING: u16 = 9;
const SPINNER: u16 = 10;
const LOADING_TITLE: u16 = 11;
const LOADING_PROGRESS: u16 = 12;
/// The header card and text well (hidden while a load runs).
const CONTENT_TABLE: u16 = 13;
const COPY: u16 = 14;
const LOADING_BOX: u16 = 15;
const SIDEBAR_CELL: u16 = 16;
const TOOLS: u16 = 17;
const TOOLS_DIVIDER: u16 = 18;
const STARTUP_UPDATES: u16 = 26;
const COMPARE_EXPORTS: u16 = 27;
const REFRESH: u16 = 20;
const EXPORT: u16 = 21;
const CLEAN_DEVICES: u16 = 22;
const CLEAN_LOGS: u16 = 23;
const UPDATES: u16 = 24;
const OLD_VIEW: u16 = 25;
const FIRST_SECTION: u16 = 100;
/// Repaints the loading indicator (DESIGN.md 8.9); killed while minimized by the kit.
const SPIN_TIMER: usize = 1;

const OLD_VIEW_TEXT: &str = "Old View";
const OLD_VIEW_LOADING: &str = "Loading...";
const LOADING_TEXT: &str = "Loading hardware information...";
// C# parity: SectionedViewForm.cs:251-271, left to right. The C# emoji prefixes are icon-font
// glyphs here (DESIGN.md 14).
const FOOTER_BUTTONS: [(u16, &str, char); 6] = [
    (REFRESH, "Refresh", glyph::REFRESH),
    (EXPORT, "Export", glyph::SAVE),
    (CLEAN_DEVICES, "Clean Devices", glyph::BROOM),
    (CLEAN_LOGS, "Clean Logs", glyph::DELETE),
    (UPDATES, update_progress::UPDATES_TEXT, glyph::SYNC),
    (OLD_VIEW, OLD_VIEW_TEXT, glyph::HISTORY),
];

/// Runs the main window until it closes.
pub fn run() {
    let (spec, nodes, handler) = parts();
    if let Err(error) = window::run_main(spec, nodes, handler) {
        window::show_error(HWND::default(), &error.to_string(), "HWID Checker");
    }
}

#[derive(Default)]
struct State {
    /// The 14 sections in provider order; bodies are raw provider text (`Loading...` while a
    /// load runs).
    sections: RefCell<Vec<Section>>,
    /// Index of the highlighted sidebar item (C# finds it by its `BackColor`).
    active: Cell<usize>,
    /// Id of the newest load; older results are dropped (F26).
    load: Cell<u64>,
    /// Sections collected so far by the newest load (the loading counter).
    collected: Cell<usize>,
    /// DPI of the last layout the footer bounds belong to (0 = none yet).
    layout_dpi: Cell<u32>,
    settings: RefCell<settings::Settings>,
}

enum Msg {
    /// Posted from `Created` with the first load id, so the first paint happens before any
    /// collection starts (A7).
    Startup(u64),
    /// One provider of load `load` finished (`hw::collect_all`'s `on_done`); only the counter
    /// and the sidebar item move, the sections still fill at the end (OPT-4).
    Progress { load: u64, index: usize },
    Loaded {
        load: u64,
        refresh: bool,
        result: Result<Vec<Section>, String>,
    },
}

type Handler = Box<dyn Fn(&Form, Event) -> bool>;

/// The form spec, the control tree, and the event handler of the main window.
fn parts() -> (FormSpec, Vec<Node>, Handler) {
    // C# parity: SectionedViewForm.cs:62-71.
    let mut spec = FormSpec::new(
        &format!("HWID Checker {}", env!("CARGO_PKG_VERSION")),
        WindowSize::Client(theme::MAIN_CLIENT_SIZE),
    );
    spec.find_keys = true;
    spec.min = Some(theme::MAIN_MIN_SIZE);
    spec.start = StartPosition::CenterScreen;
    spec.maximize_if_too_big = true; // AD-38
    let state = Rc::new(State::default());
    let find = super::find::Find::new(CONTENT);
    let handler = move |form: &Form, event: Event| {
        if let Event::Key(key) = event {
            return on_find_key(form, &find, key);
        }
        if !find.event(form, &event) {
            handle(form, &state, event);
        }
        find.refresh(form);
        true
    };
    (spec, tree(), Box::new(handler))
}

fn handle(form: &Form, state: &State, event: Event) {
    match event {
        Event::Resize { client, .. } => responsive(form, state, client),
        Event::Created => {
            load_settings(form, state);
            // C# parity: SectionedViewForm.cs:53-55 starts the load in the constructor, so the
            // first paint already shows the loading state and the `Loading...` bodies.
            let load = begin_load(form, state);
            responsive(form, state, form.client_size());
            form.relayout();
            post(form, Msg::Startup(load));
        }
        Event::Worker(value) => {
            if let Ok(msg) = value.downcast::<Msg>() {
                on_msg(form, state, *msg);
            }
        }
        Event::Timer(SPIN_TIMER) => form.spin(SPINNER),
        Event::Click(id) => on_click(form, state, id),
        Event::Toggled(STARTUP_UPDATES, checked) => set_startup_updates(form, state, checked),
        _ => {}
    }
}

fn post(form: &Form, msg: Msg) {
    if let Some(poster) = form.poster() {
        // `false` only when the window is already gone; nobody waits for the message then.
        let _ = poster.post(msg);
    }
}

fn on_msg(form: &Form, state: &State, msg: Msg) {
    match msg {
        Msg::Startup(load) => {
            spawn_load(form, state, load, false);
            show_pending_update_error();
            start_update_check(form, state);
        }
        Msg::Progress { load, index } => {
            if load == state.load.get() {
                state.collected.set(state.collected.get() + 1);
                form.set_text(LOADING_PROGRESS, &progress_text(state.collected.get()));
                form.set_pending(section_id(index), false);
            }
        }
        Msg::Loaded {
            load,
            refresh,
            result,
        } => {
            // AD-41: a superseded load is dropped whole, including its Refresh message box.
            if load == state.load.get() {
                finish_load(form, state, refresh, result);
            }
        }
    }
}

fn on_click(form: &Form, state: &State, id: u16) {
    match id {
        REFRESH => {
            // C# parity: SectionedViewForm.cs:684-690; Refresh stays enabled during a load.
            let load = begin_load(form, state);
            spawn_load(form, state, load, true);
        }
        EXPORT => export(form, state),
        COPY => form.edit_copy_all(CONTENT),
        COMPARE_EXPORTS => compare_exports(form),
        // The frozen dialogs report their own errors; a panic in one reaches the kit's handler
        // boundary (AD-42) instead of C#'s `Error opening ...` boxes.
        CLEAN_DEVICES => clean_devices::show(form.hwnd()),
        CLEAN_LOGS => clean_logs::show(form.hwnd()),
        // `check_and_update` owns the button's `⟳ Checking...` state (WP-17).
        UPDATES => on_updates(form, state),
        OLD_VIEW => {
            // C# parity: SectionedViewForm.cs:821-872.
            set_button_text(form, OLD_VIEW, false, OLD_VIEW_LOADING);
            raw_view::show(form.hwnd());
            set_button_text(form, OLD_VIEW, true, OLD_VIEW_TEXT);
        }
        _ => {
            if let Some(index) = id.checked_sub(FIRST_SECTION).map(usize::from)
                && index < hw::PROVIDERS.len()
            {
                // C# parity: SectionedViewForm.cs:435-438.
                show_section(form, state, index);
                highlight(form, state, index);
            }
        }
    }
}

// Step 2d owns this entry point; the scaffold deliberately performs no file operations.
fn compare_exports(_form: &Form) {}

// Step 2c owns these two hooks; the trunk never checks on start.
fn start_update_check(_form: &Form, _state: &State) {}

fn on_updates(form: &Form, _state: &State) {
    update_progress::check_and_update(form.hwnd(), form.control(UPDATES).unwrap_or_default());
}

// Step 2b owns find behavior; unhandled keys keep the existing dialog navigation.
fn on_find_key(form: &Form, find: &super::find::Find, key: window::FindKey) -> bool {
    find.key(form, key)
}

fn load_settings(form: &Form, state: &State) {
    let settings = settings::Settings::load();
    form.set_checked(STARTUP_UPDATES, settings.check_updates_on_start());
    *state.settings.borrow_mut() = settings;
}

fn set_startup_updates(form: &Form, state: &State, checked: bool) {
    let result = state
        .settings
        .borrow_mut()
        .set_check_updates_on_start(checked);
    if let Err(error) = result {
        form.set_checked(
            STARTUP_UPDATES,
            state.settings.borrow().check_updates_on_start(),
        );
        msgbox::show(
            form.hwnd(),
            &format!("Could not save settings: {error}"),
            "Settings Error",
            Buttons::Ok,
            Icon::Error,
        );
    }
}

fn set_button_text(form: &Form, id: u16, enabled: bool, text: &str) {
    // The kit moves the focus off a disabled control and relayouts AutoSize buttons on a text
    // change (DESIGN.md 8.2).
    form.set_enabled(id, enabled);
    form.set_text(id, text);
}

// ---------------------------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------------------------

/// Resets every section to `Loading...`, selects the first one, shows the overlay; returns the
/// new load id.
fn begin_load(form: &Form, state: &State) -> u64 {
    let load = state.load.get() + 1;
    state.load.set(load);
    state.collected.set(0);
    // C# parity: SectionedViewForm.cs:506-527 (placeholders from GetAvailableSections, sidebar
    // rebuilt, first section shown and highlighted).
    *state.sections.borrow_mut() = hw::PROVIDERS
        .iter()
        .map(|p| Section {
            title: p.title,
            body: "Loading...".to_owned(),
            ..Section::default()
        })
        .collect();
    form.set_text(LOADING_PROGRESS, &progress_text(0));
    set_loading(form, true);
    show_section(form, state, 0);
    highlight(form, state, 0);
    load
}

/// `Collected {n} of {total} sections`.
fn progress_text(collected: usize) -> String {
    format!("Collected {collected} of {} sections", hw::PROVIDERS.len())
}

fn spawn_load(form: &Form, state: &State, load: u64, refresh: bool) {
    let Some(poster) = form.poster() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("main load".to_owned())
        .spawn(move || {
            let progress = poster.clone();
            let result = win::catch_panic(|| {
                hw::collect_all(None, &|index, _| {
                    // `false` only when the window is already gone.
                    let _ = progress.post(Msg::Progress { load, index });
                })
            });
            // `false` only when the window is already gone; nobody waits for the result then.
            let _ = poster.post(Msg::Loaded {
                load,
                refresh,
                result,
            });
        });
    if let Err(error) = spawned {
        let error = win::Error::msg("thread::spawn", error.to_string()).to_string();
        finish_load(form, state, refresh, Err(error));
    }
}

fn finish_load(form: &Form, state: &State, refresh: bool, result: Result<Vec<Section>, String>) {
    match result {
        Ok(fresh) => {
            for section in state.sections.borrow_mut().iter_mut() {
                // C# parity: SectionedViewForm.cs:554-567 matches titles OrdinalIgnoreCase; a
                // missing title reads `No data available` (empty body).
                *section = fresh
                    .iter()
                    .find(|f| report::eq_ignore_case(f.title, section.title))
                    .cloned()
                    .unwrap_or(Section {
                        title: section.title,
                        ..Section::default()
                    });
            }
            set_loading(form, false);
            show_section(form, state, state.active.get());
        }
        Err(error) => {
            // C# parity: SectionedViewForm.cs:599-603; the bodies stay `Loading...`.
            set_loading(form, false);
            msgbox::show(
                active_window(),
                &format!("Error loading hardware information: {error}"),
                "Error",
                Buttons::Ok,
                Icon::Error,
            );
        }
    }
    if refresh {
        // C# parity: SectionedViewForm.cs:689 shows this even after the error box.
        msgbox::show(
            active_window(),
            "Hardware data refreshed successfully!",
            "Refresh",
            Buttons::Ok,
            Icon::Information,
        );
    }
}

/// Shows a failed update replacement from the previous run (WP-13 marker, AD-34).
fn show_pending_update_error() {
    let text = match update::take_pending_error() {
        Ok(None) => return,
        Ok(Some(text)) => text,
        // Not silent: a marker that cannot be read or deleted is shown like a failed update.
        Err(error) => error,
    };
    msgbox::show(
        active_window(),
        &text,
        "Update Error",
        Buttons::Ok,
        Icon::Error,
    );
}

/// C# `MessageBox.Show` without an owner uses the active window (`msgbox::active_window`).
fn active_window() -> HWND {
    msgbox::active_window()
}

/// Shows the loading state in place of the header and text well (DESIGN.md 13): the
/// indicator turns on the form's timer, every sidebar item is faint until collected.
fn set_loading(form: &Form, show: bool) {
    // C# parity: SectionedViewForm.cs:606-617 (the overlay); the flow is the same.
    form.set_visible(CONTENT_TABLE, !show);
    form.set_visible(LOADING, show);
    for i in 0..hw::PROVIDERS.len() {
        form.set_pending(section_id(i), show);
    }
    if show && form.spinner_animates(SPINNER) {
        form.set_timer(SPIN_TIMER, theme::SPINNER_STEP_MS);
    } else {
        form.kill_timer(SPIN_TIMER);
    }
}

/// The status color of the section meta line (DESIGN.md 3 status map applied to the body).
fn meta_color(body: &str) -> theme::Color {
    if body == "Loading..." {
        theme::INFO
    } else if body.lines().any(|l| l.starts_with("Error retrieving")) {
        theme::DANGER
    } else if body == "No data available"
        || body.lines().any(|l| {
            l.contains("Unavailable (") || l.starts_with("Error:") || l.starts_with("Error in")
        })
    {
        theme::WARNING
    } else {
        theme::MUTED_TEXT
    }
}

// ---------------------------------------------------------------------------------------------
// Sections and export
// ---------------------------------------------------------------------------------------------

fn show_section(form: &Form, state: &State, index: usize) {
    // C# parity: SectionedViewForm.cs:477-487.
    let (title, content, count) = {
        let sections = state.sections.borrow();
        let Some(section) = sections.get(index) else {
            return;
        };
        (
            section.title,
            report::section_content(&section.body),
            sections.len(),
        )
    };
    form.set_text(SECTION_TITLE, title);
    form.set_label_color(SECTION_META, meta_color(&content));
    form.set_text(SECTION_META, &format!("Section {} of {count}", index + 1));
    form.edit_set_text(CONTENT, &content);
    form.edit_scroll_to_top(CONTENT);
}

fn highlight(form: &Form, state: &State, index: usize) {
    // C# parity: SectionedViewForm.cs:461-475.
    for i in 0..hw::PROVIDERS.len() {
        form.set_active(section_id(i), i == index);
    }
    state.active.set(index);
}

fn section_id(index: usize) -> u16 {
    FIRST_SECTION + index as u16
}

fn export(form: &Form, state: &State) {
    // C# parity: SectionedViewForm.cs:708-744. The `No data to export` branch needs zero
    // sections, which the main window never has.
    let text = report::export_text(&state.sections.borrow());
    match write_export(&text) {
        Ok(path) => msgbox::show(
            form.hwnd(),
            &format!(
                "Export completed successfully!\nSaved to: {}",
                path.display()
            ),
            "Export",
            Buttons::Ok,
            Icon::Information,
        ),
        Err(error) => msgbox::show(
            form.hwnd(),
            &format!("Error exporting file: {error}"),
            "Export Error",
            Buttons::Ok,
            Icon::Error,
        ),
    };
}

/// Writes the export next to the exe (UTF-8 without BOM, same-second files overwritten).
fn write_export(text: &str) -> win::Result<PathBuf> {
    // C# parity: FileExportService.cs:18-31 with AppDomain.BaseDirectory.
    let exe = std::env::current_exe().map_err(|e| io_error("Locate executable folder", e))?;
    let folder = exe
        .parent()
        .ok_or_else(|| win::Error::msg("Locate executable folder", "no parent folder"))?;
    let (date, time) = win::time::export_stamp();
    let path = folder.join(format!("HWID-EXPORT-{date}-{time}.txt"));
    std::fs::write(&path, text).map_err(|e| io_error("Write export file", e))?;
    Ok(path)
}

// ---------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------

/// `GetSectionIcon`: lower-case `Contains`, first match wins (`SectionedViewForm.cs:443-459`);
/// the C# emoji are icon-font glyphs (DESIGN.md 14).
fn section_icon(title: &str) -> char {
    let t = title.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if any(&["cpu", "processor"]) {
        glyph::CPU
    } else if any(&["gpu", "graphics"]) {
        glyph::GAME
    } else if any(&["ram", "memory"]) {
        glyph::RAM
    } else if any(&["motherboard", "board"]) {
        glyph::COMPONENT
    } else if any(&["disk", "drive", "storage"]) {
        glyph::HARD_DRIVE
    } else if any(&["network", "ethernet"]) {
        glyph::ETHERNET
    } else if any(&["chassis"]) {
        glyph::PC
    } else if any(&["system", "computer"]) {
        glyph::INFO
    } else if any(&["bios", "firmware"]) {
        glyph::COMMAND_PROMPT
    } else if any(&["tpm", "security"]) {
        glyph::LOCK
    } else if any(&["bluetooth"]) {
        glyph::BLUETOOTH
    } else if any(&["usb", "device"]) {
        glyph::USB
    } else if any(&["monitor", "display"]) {
        glyph::MONITOR
    } else if any(&["arp", "address"]) {
        glyph::NETWORK
    } else {
        glyph::LIST
    }
}

/// C# `GetSidebarWidth` with the owner-approved DPI-scaled clamp (AD-37).
fn sidebar_width(form: &Form, client_w: i32) -> i32 {
    (client_w * theme::SIDEBAR_WIDTH_PERCENT / 100).clamp(
        form.scale(theme::SIDEBAR_MIN_WIDTH),
        form.scale(theme::SIDEBAR_MAX_WIDTH),
    )
}

/// The sidebar content height of a tier in device pixels (what the flow layout will measure).
fn tier_height(tier: &theme::SidebarTier, padding: i32, dpi: u32) -> i32 {
    let s = |v: i32| dpi::scale(v, dpi);
    let title = s(tier.title) + s(tier.title_gap);
    let subtitle = if tier.subtitle {
        s(theme::SIDEBAR_SUBTITLE_HEIGHT) + s(theme::SIDEBAR_SUBTITLE_MARGIN.b)
    } else {
        0
    };
    padding + title + subtitle + hw::PROVIDERS.len() as i32 * (s(tier.item) + s(tier.gap))
}

/// C# `UpdateResponsiveLayout`, extended by the sidebar tiers (DESIGN.md 11): runs on every
/// resize and DPI change (tree values reset there) and edits the tree only, so the kit's one
/// layout pass per event stays one (8.8). Heights come from the last layout's footer.
fn responsive(form: &Form, state: &State, client: Size) {
    let dpi = form.dpi();
    let s = |v: i32| dpi::scale(v, dpi);
    let sidebar = sidebar_width(form, client.w);
    let bar_w = dpi::metric(SM_CXVSCROLL, dpi);
    // The last layout's bounds are device pixels of that layout's DPI.
    let last_dpi = state.layout_dpi.replace(dpi);
    let rescale = |v: i32| {
        if last_dpi == 0 || last_dpi == dpi {
            v
        } else {
            (i64::from(v) * i64::from(dpi) / i64::from(last_dpi)) as i32
        }
    };
    form.with_tree(|t| {
        if let Some(Kind::Table { cols, .. }) = t.find_mut(MAIN_TABLE).map(|n| &mut n.kind) {
            cols[0] = Track::Absolute(sidebar);
        }
        // Footer: one row on every listed work area; a bottom margin separates wrapped rows
        // (the flow layout has no row gap of its own).
        let footer_h = t.find_mut(FOOTER).map_or(0, |footer| {
            let row_w: i32 = footer
                .children()
                .iter()
                .map(|b| rescale(b.bounds.w) + b.margin.horizontal())
                .sum::<i32>()
                + footer.padding.horizontal();
            let button_h = footer
                .children()
                .iter()
                .map(|b| rescale(b.bounds.h))
                .max()
                .unwrap_or(0)
                .max(s(theme::FOOTER_BUTTON_MIN.h) + 4);
            let wraps = row_w > client.w && footer.bounds.w > 0;
            let gap = if wraps { s(theme::FOOTER_ROW_GAP) } else { 0 };
            for b in footer.children_mut() {
                b.margin.b = gap;
            }
            if wraps && footer.bounds.h > 0 {
                rescale(footer.bounds.h)
            } else {
                footer.padding.vertical() + button_h
            }
        });
        let cell_margin = t.find(SIDEBAR_CELL).map_or(0, |n| n.margin.vertical());
        let cell_margin_w = t.find(SIDEBAR_CELL).map_or(0, |n| n.margin.horizontal());
        let block_h = s(theme::TOOLS_BLOCK_CONTENT) + theme::STROKE;
        if let Some(divider) = t.find_mut(TOOLS_DIVIDER) {
            divider.size.h = theme::STROKE;
        }
        let Some(side) = t.find_mut(SIDEBAR) else {
            return;
        };
        let inner = client.h - footer_h - cell_margin - block_h;
        let padding = side.padding.vertical();
        let (tier, scroll) = theme::SIDEBAR_TIERS
            .iter()
            .find(|tier| tier_height(tier, padding, dpi) <= inner)
            .map_or((&theme::SIDEBAR_TIERS[3], true), |tier| (tier, false));
        // Width: margins, padding, the scroll bar once, and the scaled inset (audit F7).
        let bar = if scroll { bar_w } else { 0 };
        let item_w = (sidebar
            - cell_margin_w
            - side.padding.horizontal()
            - bar
            - s(theme::SIDEBAR_ITEM_INSET))
        .max(s(theme::SIDEBAR_ITEM_MIN_WIDTH));
        for c in side.children_mut() {
            c.size.w = item_w;
            match c.id {
                SIDEBAR_TITLE => {
                    c.size.h = s(tier.title);
                    c.margin.b = s(tier.title_gap);
                }
                SIDEBAR_SUBTITLE => c.visible = tier.subtitle,
                _ => {
                    c.size.h = s(tier.item);
                    c.margin.b = s(tier.gap);
                }
            }
        }
        for id in [STARTUP_UPDATES, COMPARE_EXPORTS] {
            if let Some(row) = t.find_mut(id) {
                row.size.w = item_w;
            }
        }
    });
    // The subtitle's native window follows its node (set_visible would relayout).
    let subtitle_visible = form.with_tree(|t| t.find(SIDEBAR_SUBTITLE).is_some_and(|n| n.visible));
    if let Some(visible) = subtitle_visible {
        form.show_control(SIDEBAR_SUBTITLE, visible);
    }
}

fn label(id: u16, text: &str, font: theme::FontSpec, fore: theme::Color) -> Node {
    Node::leaf(id, Ctl::Label(LabelSpec::new(text, font, fore)))
}

fn sidebar() -> Node {
    // C# parity: SectionedViewForm.cs:366-441; widths start at the C# fallback 240 - inset.
    let item_w = theme::SIDEBAR_MIN_WIDTH - theme::SIDEBAR_ITEM_INSET;
    let mut items = vec![
        label(
            SIDEBAR_TITLE,
            "Hardware Sections",
            theme::SIDEBAR_TITLE_FONT,
            theme::SIDEBAR_HEADER_TEXT,
        )
        .size(Size {
            w: item_w,
            h: theme::SIDEBAR_TITLE_HEIGHT,
        })
        .margin(theme::SIDEBAR_TITLE_MARGIN),
        label(
            SIDEBAR_SUBTITLE,
            &format!("{} sections", hw::PROVIDERS.len()),
            theme::SIDEBAR_SUBTITLE_FONT,
            theme::MUTED_TEXT,
        )
        .size(Size {
            w: item_w,
            h: theme::SIDEBAR_SUBTITLE_HEIGHT,
        })
        .margin(theme::SIDEBAR_SUBTITLE_MARGIN),
    ];
    items.extend(hw::PROVIDERS.iter().enumerate().map(|(i, p)| {
        let spec = ButtonSpec::sidebar(p.title).icon(section_icon(p.title));
        Node::leaf(section_id(i), Ctl::Button(spec))
            .size(Size {
                w: item_w,
                h: theme::SECTION_BUTTON_HEIGHT,
            })
            .padding(theme::SECTION_BUTTON_PADDING)
            .margin(theme::SECTION_BUTTON_MARGIN)
    }));
    let sections = Node::flow(FlowDir::TopDown, false, items)
        .id(SIDEBAR)
        .fill()
        .scroll()
        .padding(theme::SIDEBAR_PADDING)
        .margin(theme::NO_PAD)
        .back(theme::SIDEBAR_BACKGROUND)
        .cell(0, 0);
    Node::table(
        vec![Track::Percent(100.0)],
        vec![Track::Percent(100.0), Track::AutoSize],
        vec![sections, tools_block(item_w).cell(0, 1)],
    )
    .id(SIDEBAR_CELL)
    .fill()
    .back(theme::SIDEBAR_BACKGROUND)
    .cell(0, 0)
}

fn tools_block(item_w: i32) -> Node {
    let row = |id, button| {
        Node::leaf(id, Ctl::Button(button))
            .size(Size {
                w: item_w,
                h: theme::TOOLS_ROW_HEIGHT,
            })
            .padding(theme::TOOLS_ROW_PADDING)
            .margin(theme::TOOLS_ROW_MARGIN)
    };
    let mut toggle = row(
        STARTUP_UPDATES,
        ButtonSpec::sidebar("Startup Update Check")
            .icon(glyph::SYNC)
            .toggle(),
    );
    toggle.margin.b = theme::TOOLS_ROW_GAP;
    // Empty painted panel: stroke height is restored to a device pixel in responsive().
    let divider = Node::panel(vec![])
        .id(TOOLS_DIVIDER)
        .height(theme::STROKE)
        .anchor(Anchor(Anchor::LEFT.0 | Anchor::RIGHT.0))
        .margin(theme::TOOLS_DIVIDER_MARGIN)
        .back(theme::BORDER);
    Node::flow(
        FlowDir::TopDown,
        false,
        vec![
            divider,
            toggle,
            row(
                COMPARE_EXPORTS,
                ButtonSpec::sidebar("Compare Exports").icon(glyph::SWITCH),
            ),
        ],
    )
    .id(TOOLS)
    .fill()
    .auto_size()
    .margin(theme::NO_PAD)
}

fn content() -> Node {
    // C# parity: SectionedViewForm.cs:140-220. The header is a card (DESIGN.md 13) with the
    // `Copy` button at its right edge; the text well follows after a gap.
    let titles = Node::panel(vec![
        Node::leaf(
            SECTION_TITLE,
            Ctl::Label(
                LabelSpec::new(
                    "Section",
                    theme::SECTION_TITLE_FONT,
                    theme::SIDEBAR_HEADER_TEXT,
                )
                .ellipsis(),
            ),
        )
        .top()
        .height(theme::SECTION_TITLE_HEIGHT),
        Node::leaf(
            SECTION_META,
            Ctl::Label(LabelSpec::new("", theme::SECTION_META_FONT, theme::MUTED_TEXT).ellipsis()),
        )
        .top()
        .height(theme::SECTION_META_HEIGHT),
    ])
    .fill()
    .auto_size()
    .margin(theme::NO_PAD)
    .cell(0, 0);
    let copy = Node::leaf(
        COPY,
        Ctl::Button(ButtonSpec::outline("Copy").icon(glyph::COPY)),
    )
    .auto_size()
    .min(theme::COPY_BUTTON_SIZE)
    .anchor(Anchor::NONE)
    .margin(theme::NO_PAD)
    .cell(1, 0);
    let header = Node::table(
        vec![Track::Percent(100.0), Track::AutoSize],
        vec![Track::AutoSize],
        vec![titles, copy],
    )
    .fill()
    .auto_size()
    .card(theme::HEADER_RADIUS)
    .back(theme::CONTENT_BACKGROUND)
    .padding(theme::HEADER_PADDING)
    .margin(theme::HEADER_MARGIN)
    .cell(0, 0);
    // C# parity: SectionedViewForm.cs:214 uses `BorderStyle.FixedSingle`. The kit keeps the
    // default `Fixed3D` edge (DESIGN.md 5): the 1 px `BORDER` ring is painted over the client
    // edge with the scroll bars inside it, like every other well. A `WS_BORDER` multiline edit
    // draws its own light frame inside the client area and leaves the bars outside it.
    let edit = EditSpec::new(
        theme::CONTENT_FONT,
        theme::TEXT_BOX_TEXT,
        theme::TEXT_BOX_BACKGROUND,
    )
    .keep_selection();
    let loaded = Node::table(
        vec![Track::Percent(100.0)],
        vec![Track::AutoSize, Track::AutoSize, Track::Percent(100.0)],
        vec![
            header,
            super::find::bar().cell(0, 1),
            Node::leaf(CONTENT, Ctl::Edit(edit))
                .fill()
                .margin(theme::NO_PAD)
                .cell(0, 2),
        ],
    )
    .id(CONTENT_TABLE)
    .fill()
    .margin(theme::NO_PAD)
    .back(theme::SURFACE_BACKGROUND);
    // The loading state (DESIGN.md 13): indicator, title and counter centered in the pane.
    let loading = Node::table(
        vec![Track::Percent(100.0)],
        vec![Track::Percent(100.0)],
        vec![
            Node::flow(
                FlowDir::TopDown,
                false,
                vec![
                    Node::leaf(SPINNER, Ctl::Spinner)
                        .anchor(Anchor::NONE)
                        .margin(theme::SPINNER_MARGIN),
                    label(
                        LOADING_TITLE,
                        LOADING_TEXT,
                        theme::LOADING_TITLE_FONT,
                        theme::TEXT,
                    )
                    .auto_size()
                    .anchor(Anchor::NONE)
                    .margin(theme::LOADING_TITLE_MARGIN),
                    label(
                        LOADING_PROGRESS,
                        &progress_text(0),
                        theme::LOADING_PROGRESS_FONT,
                        theme::MUTED_TEXT,
                    )
                    .auto_size()
                    .anchor(Anchor::NONE)
                    .margin(theme::NO_PAD),
                ],
            )
            .id(LOADING_BOX)
            .auto_size()
            .anchor(Anchor::NONE)
            .margin(theme::NO_PAD)
            .cell(0, 0),
        ],
    )
    .id(LOADING)
    .fill()
    .margin(theme::NO_PAD)
    .back(theme::SURFACE_BACKGROUND)
    .visible(false);
    Node::panel(vec![loaded, loading])
        .fill()
        .padding(theme::CONTENT_PADDING)
        .back(theme::SURFACE_BACKGROUND)
        .cell(1, 0)
}

fn footer() -> Node {
    // C# parity: SectionedViewForm.cs:223-233, 304-321 (ApplyStyle sets the padding last).
    // DESIGN.md 6: Refresh is the window's one primary button (the main action).
    let buttons = FOOTER_BUTTONS
        .iter()
        .map(|&(id, text, icon)| {
            let spec = if id == REFRESH {
                ButtonSpec::primary(text)
            } else {
                ButtonSpec::outline(text)
            }
            .icon(icon);
            Node::leaf(id, Ctl::Button(spec))
                .auto_size()
                .min(theme::FOOTER_BUTTON_MIN)
                .padding(theme::SHARED_BUTTON_PADDING)
                .margin(theme::FOOTER_BUTTON_MARGIN)
        })
        .collect();
    Node::flow(FlowDir::LeftToRight, true, buttons)
        .id(FOOTER)
        .fill()
        .auto_size()
        .padding(theme::FOOTER_PADDING)
        .margin(theme::NO_PAD)
        .back(theme::BUTTON_PANEL_BACKGROUND)
        .cell(0, 1)
        .span(2)
}

fn tree() -> Vec<Node> {
    // C# parity: SectionedViewForm.cs:111-125, 235-245, 292-299.
    let start = theme::MAIN_CLIENT_SIZE.w * theme::SIDEBAR_WIDTH_PERCENT / 100;
    let sidebar_w = start.clamp(theme::SIDEBAR_MIN_WIDTH, theme::SIDEBAR_MAX_WIDTH);
    vec![
        Node::table(
            vec![Track::Absolute(sidebar_w), Track::Percent(100.0)],
            vec![Track::Percent(100.0), Track::AutoSize],
            vec![sidebar(), content(), footer()],
        )
        .id(MAIN_TABLE)
        .fill()
        .back(theme::MAIN_BACKGROUND),
    ]
}

#[cfg(test)]
mod live {
    //! Real-window check: `cargo test --locked --lib -- --ignored ui::main_window::live --nocapture`.
    //! Collects real (non-admin) data, drives every main-window flow except the three wave B
    //! dialogs, and saves screenshots and private texts under `golden/wp-10b/`.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ui::layout::Rect;
    use crate::win::wide::to_wide;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::Controls::EM_GETSEL;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, GetFocus, IsWindowEnabled};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindow, PostMessageW, SWP_NOMOVE, SWP_NOZORDER,
        SendMessageW, SetForegroundWindow, SetWindowPos, WM_CLOSE, WM_DPICHANGED, WM_KEYDOWN,
    };
    use windows::core::BOOL;

    const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\feat-trunk";
    const TICK: usize = 0x7E57;
    const CLOSE_WHILE_BUSY: usize = 0x7E58;

    #[repr(C)]
    struct ActCtx {
        cb_size: u32,
        flags: u32,
        source: *const u16,
        arch: u16,
        lang: u16,
        dir: *const u16,
        resource: *const u16,
        app: *const u16,
        module: *mut core::ffi::c_void,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateActCtxW(ctx: *const ActCtx) -> *mut core::ffi::c_void;
        fn ActivateActCtx(h: *mut core::ffi::c_void, cookie: *mut usize) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: *mut core::ffi::c_void, hdc: *mut core::ffi::c_void, f: u32) -> i32;
    }

    /// The test exe has no manifest; activate Common Controls 6 like the app manifest does.
    fn activate_comctl6() -> bool {
        let path = Path::new(GOLDEN).join("comctl6.manifest");
        std::fs::write(
            &path,
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
</assembly>"#,
        )
        .unwrap();
        let wide = to_wide(&path.to_string_lossy());
        let ctx = ActCtx {
            cb_size: std::mem::size_of::<ActCtx>() as u32,
            flags: 0,
            source: wide.as_ptr(),
            arch: 0,
            lang: 0,
            dir: std::ptr::null(),
            resource: std::ptr::null(),
            app: std::ptr::null(),
            module: std::ptr::null_mut(),
        };
        // SAFETY: `ctx` and the path buffer live through the calls; the context stays active
        // for the rest of the test thread on purpose.
        unsafe {
            let h = CreateActCtxW(&ctx);
            let mut cookie = 0usize;
            !h.is_null() && h as isize != -1 && ActivateActCtx(h, &mut cookie) != 0
        }
    }

    fn hwnd(raw: isize) -> HWND {
        HWND(raw as *mut core::ffi::c_void)
    }

    fn text_of(h: HWND) -> String {
        // SAFETY: Length query and a read into a buffer sized for the text plus NUL.
        unsafe {
            let len = GetWindowTextLengthW(h);
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(h, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    fn class_of(h: HWND) -> String {
        let mut buf = [0u16; 64];
        // SAFETY: Writable buffer of the given length.
        let n = unsafe { GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n as usize])
    }

    fn rect_of(h: HWND, client: bool) -> RECT {
        let mut r = RECT::default();
        // SAFETY: Writable RECT; a dead handle leaves it zeroed.
        unsafe {
            let _ = if client {
                GetClientRect(h, &mut r)
            } else {
                GetWindowRect(h, &mut r)
            };
        }
        r
    }

    /// The first descendant window (any depth) of `parent` with window class `class`.
    fn descendant_of_class(parent: HWND, class: &str) -> Option<HWND> {
        unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the Vec passed below, alive for the enumeration.
            unsafe { (*(data.0 as *mut Vec<HWND>)).push(h) };
            BOOL(1)
        }
        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: Synchronous enumeration whose callback only pushes into `all`.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::EnumChildWindows(
                Some(parent),
                Some(collect),
                LPARAM(&mut all as *mut Vec<HWND> as isize),
            );
        }
        all.into_iter()
            .find(|h| class_of(*h).eq_ignore_ascii_case(class))
    }

    /// Top-level windows of this process.
    fn own_windows() -> Vec<HWND> {
        unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the Vec passed below, alive for the enumeration.
            unsafe { (*(data.0 as *mut Vec<HWND>)).push(h) };
            BOOL(1)
        }
        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: Synchronous enumeration whose callback only pushes into `all`.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize));
        }
        all.into_iter()
            .filter(|h| {
                let mut pid = 0;
                // SAFETY: Writable pid out-parameter.
                unsafe { GetWindowThreadProcessId(*h, Some(&mut pid)) };
                pid == std::process::id()
            })
            .collect()
    }

    /// PrintWindow capture (works while other apps cover the window) saved as a BMP.
    fn shot(h: HWND, name: &str) {
        shot_dir(h, GOLDEN, name);
    }

    fn shot_dir(h: HWND, dir: &str, name: &str) {
        let r = rect_of(h, false);
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        let mut bits = vec![0u8; (w * hgt * 4) as usize];
        // SAFETY: DC and bitmap are created, used, and released here; the buffer fits a
        // 32-bit top-down DIB of the window size.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, hgt);
            let old = SelectObject(mem, bmp.into());
            PrintWindow(h.0, mem.0, 2);
            SelectObject(mem, old);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -hgt,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            GetDIBits(
                mem,
                bmp,
                0,
                hgt as u32,
                Some(bits.as_mut_ptr().cast()),
                &mut bi,
                DIB_RGB_COLORS,
            );
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
        let mut out = Vec::with_capacity(54 + bits.len());
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(54 + bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&(-hgt).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        out.extend_from_slice(&bits);
        std::fs::write(Path::new(dir).join(format!("{name}.bmp")), out).unwrap();
    }

    fn bmps_to_png() {
        bmps_to_png_in(GOLDEN);
    }

    fn bmps_to_png_in(dir: &str) {
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; \
             Get-ChildItem '{dir}' -Filter *.bmp | ForEach-Object {{ \
               $i = [System.Drawing.Image]::FromFile($_.FullName); \
               $i.Save(($_.FullName -replace '\\.bmp$', '.png'), [System.Drawing.Imaging.ImageFormat]::Png); \
               $i.Dispose(); Remove-Item $_.FullName }}"
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Pumps the UI thread with the kit's loop for `ms`; the form's test timer keeps messages
    /// coming so the deadline is checked.
    fn pump_for(ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        window::pump_until(|| Instant::now() >= end);
    }

    fn pump_while(limit: Duration, busy: impl Fn() -> bool) -> Duration {
        let start = Instant::now();
        window::pump_until(|| !busy() || start.elapsed() >= limit);
        start.elapsed()
    }

    fn loading(form: &Form) -> bool {
        form.with_tree(|t| t.find(LOADING).is_some_and(|n| n.visible)) == Some(true)
    }

    fn bounds(form: &Form, id: u16) -> Rect {
        form.with_tree(|t| t.find(id).map(|n| n.bounds))
            .flatten()
            .unwrap_or_default()
    }

    /// The loading box (indicator, title, counter) is centered in the content pane.
    fn centered(form: &Form) -> bool {
        let (lr, c) = (bounds(form, LOADING_BOX), bounds(form, LOADING));
        (lr.x - (c.w - lr.w) / 2).abs() <= 1 && (lr.y - (c.h - lr.h) / 2).abs() <= 1
    }

    fn dpi_change(form: &Form, new_dpi: u32) {
        let r = rect_of(form.hwnd(), false);
        let f = |v: i32| v * new_dpi as i32 / form.dpi() as i32;
        let suggested = RECT {
            left: r.left,
            top: r.top,
            right: r.left + f(r.right - r.left),
            bottom: r.top + f(r.bottom - r.top),
        };
        // SAFETY: Synchronous message to our own window with a pointer to a live RECT.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM((new_dpi | (new_dpi << 16)) as usize)),
                Some(LPARAM(&suggested as *const RECT as isize)),
            );
        }
        pump_for(300);
    }

    /// Closes message boxes (logging them) and inspects, captures, then closes the Old View.
    fn closer(stop: Arc<AtomicBool>, log: Arc<Mutex<Vec<String>>>, old_view_button: isize) {
        let mut handled = std::collections::HashSet::new();
        while !stop.load(Ordering::SeqCst) {
            // Handle values are reused after a window dies.
            // SAFETY: Read-only handle validity checks.
            handled.retain(|h| unsafe { IsWindow(Some(hwnd(*h))) }.as_bool());
            for h in own_windows() {
                let class = class_of(h);
                if handled.contains(&(h.0 as isize)) {
                    continue;
                }
                if msgbox::testing::is_box(h) {
                    let body = msgbox::testing::text(h);
                    if body.is_empty() {
                        continue; // Still being created.
                    }
                    handled.insert(h.0 as isize);
                    log.lock()
                        .unwrap()
                        .push(format!("box {} | {}", text_of(h), body));
                    // Pressed by HWND: every box of this test is an OK box.
                    msgbox::testing::press(h, windows::Win32::UI::WindowsAndMessaging::IDOK);
                } else if class == window::FORM_CLASS && text_of(h) == raw_view::TITLE {
                    handled.insert(h.0 as isize);
                    std::thread::sleep(Duration::from_millis(700));
                    let button = hwnd(old_view_button);
                    // SAFETY: Read-only state query of the main window's button.
                    let enabled = unsafe { IsWindowEnabled(button) }.as_bool();
                    let edit = descendant_of_class(h, "Edit").unwrap_or_default();
                    let (mut s, mut e) = (0u32, 0u32);
                    // SAFETY: Cross-thread EM_GETSEL into live locals; the UI thread pumps.
                    unsafe {
                        SendMessageW(
                            edit,
                            EM_GETSEL,
                            Some(WPARAM(&mut s as *mut u32 as usize)),
                            Some(LPARAM(&mut e as *mut u32 as isize)),
                        );
                    }
                    let text = text_of(edit);
                    std::fs::write(Path::new(GOLDEN).join("rust-oldview.txt"), &text).unwrap();
                    let (r, c) = (rect_of(h, false), rect_of(h, true));
                    log.lock().unwrap().push(format!(
                        "oldview outer {}x{} client {}x{} selection {s}..{e} of {} \
                         header {:?} button {:?} enabled {enabled}",
                        r.right - r.left,
                        r.bottom - r.top,
                        c.right,
                        c.bottom,
                        text.encode_utf16().count(),
                        text.lines().nth(1).unwrap_or("").trim(),
                        text_of(button),
                    ));
                    shot(h, "oldview");
                    // A second DPI for the design check (WP-18): synthetic change to 96 or 144.
                    let dpi = dpi::window_dpi(h);
                    let other: u32 = if dpi == 96 { 144 } else { 96 };
                    let scale = |v: i32| v * other as i32 / dpi as i32;
                    let suggested = RECT {
                        left: r.left,
                        top: r.top,
                        right: r.left + scale(r.right - r.left),
                        bottom: r.top + scale(r.bottom - r.top),
                    };
                    // SAFETY: Synchronous message with a pointer to a live RECT; the UI thread
                    // pumps while this helper thread waits.
                    unsafe {
                        SendMessageW(
                            h,
                            WM_DPICHANGED,
                            Some(WPARAM((other | (other << 16)) as usize)),
                            Some(LPARAM(&suggested as *const RECT as isize)),
                        );
                    }
                    std::thread::sleep(Duration::from_millis(400));
                    shot(h, &format!("oldview-synthetic-{other}"));
                    // SAFETY: Value-only message to our own window.
                    unsafe {
                        let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn take(log: &Mutex<Vec<String>>) -> Vec<String> {
        std::mem::take(&mut *log.lock().unwrap())
    }

    /// The per-setup fit table of DESIGN.md 11: every listed screen setup as a forced work area
    /// plus a synthetic DPI change; the restored (clamped) window and, where the default does
    /// not fit, the maximized client. Screenshots and `fit-matrix.md` go to `golden/feat-trunk`.
    /// `cargo test --locked --lib -- --ignored --exact ui::main_window::live::fit_matrix --nocapture`
    #[test]
    #[ignore = "opens a real window and collects real hardware data"]
    fn fit_matrix() {
        const OUT: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\feat-trunk\fit";
        std::fs::create_dir_all(OUT).unwrap();
        std::fs::create_dir_all(GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        assert!(activate_comctl6(), "comctl v6 activation context");
        let (spec, nodes, handler) = parts();
        let (style, ex) = window::styles(&spec);
        let form = Form::create(HWND::default(), spec, nodes, handler).unwrap();
        form.show();
        form.set_timer(TICK, 20);
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let button = form.control(OLD_VIEW).unwrap().0 as isize;
        let closer_thread = {
            let (stop, log) = (Arc::clone(&stop), Arc::clone(&log));
            std::thread::spawn(move || closer(stop, log, button))
        };
        pump_while(Duration::from_secs(150), || loading(&form));
        pump_for(300);
        trunk_tools_check(&form);
        let real = dpi::window_dpi(form.hwnd());
        // C8's longest idle caption must still fit before the behavior lands in step 2c.
        form.set_text(UPDATES, "Update available");
        form.set_button_fore(UPDATES, Some(theme::INFO));
        form.set_checked(STARTUP_UPDATES, true);
        // (name, width, height, dpi)
        let setups: [(&str, i32, i32, u32); 10] = [
            ("1920x1080 @100", 1920, 1080, 96),
            ("1920x1080 @125", 1920, 1080, 120),
            ("1920x1080 @150", 1920, 1080, 144),
            ("2560x1440 @100", 2560, 1440, 96),
            ("2560x1440 @125", 2560, 1440, 120),
            ("3840x2160 @150", 3840, 2160, 144),
            ("3840x2160 @175", 3840, 2160, 168),
            ("3840x2160 @200", 3840, 2160, 192),
            ("1366x768 @125", 1366, 768, 120),
            ("1920x1080 custom 137 DPI", 1920, 1080, 137),
        ];
        let mut rows = vec![
            "| Setup | Work area (px) | Default outer | Start | Restored outer | Client | Tier | 14 visible | Scrollbar | Elided | Footer rows |".to_owned(),
            "|---|---|---|---|---|---|---|---|---|---|---|".to_owned(),
        ];
        let mut fit_failures = Vec::new();
        let measure_state = |form: &Form, dpi: u32| -> (String, bool, bool, usize, usize) {
            form.with_tree(|t| {
                let side = t.find(SIDEBAR).unwrap();
                let first = t.find(FIRST_SECTION).unwrap();
                let last = t.find(section_id(hw::PROVIDERS.len() - 1)).unwrap();
                let tier = ["A", "B", "C", "D"]
                    .iter()
                    .zip(theme::SIDEBAR_TIERS.iter())
                    .find(|(_, tier)| dpi::scale(tier.item, dpi) == first.bounds.h)
                    .map_or("?", |(n, _)| n)
                    .to_owned();
                let visible = !side.vscroll && last.bounds.bottom() <= side.bounds.h;
                let footer = t.find(FOOTER).unwrap();
                let mut ys: Vec<i32> = footer.children().iter().map(|b| b.bounds.y).collect();
                ys.sort_unstable();
                ys.dedup();
                (tier, visible, side.vscroll, 0, ys.len())
            })
            .unwrap()
        };
        let elided = |form: &Form| -> usize {
            // Captions whose single-line width exceeds the item's text field get an ellipsis.
            let dpi = form.dpi();
            let font = dpi::Font::new(theme::SECTION_BUTTON_FONT, dpi).unwrap();
            (0..hw::PROVIDERS.len())
                .map(section_id)
                .chain([STARTUP_UPDATES, COMPARE_EXPORTS])
                .filter(|&id| {
                    let (w, spec) = form
                        .with_tree(|t| {
                            let n = t.find(id).unwrap();
                            let Kind::Leaf(Ctl::Button(b)) = &n.kind else {
                                unreachable!()
                            };
                            (n.bounds.w, (b.clone(), n.padding))
                        })
                        .unwrap();
                    let need = crate::ui::controls::measure(
                        &Ctl::Button(spec.0),
                        font.handle(),
                        spec.1,
                        Size::default(),
                        Size::default(),
                        dpi,
                    );
                    need.w > w
                })
                .count()
        };
        for (name, w, h, dpi) in setups {
            let taskbar = dpi::scale(48, dpi);
            let work = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h - taskbar,
            };
            window::force_work_area(Some(work));
            let client = dpi::scale_size(theme::MAIN_CLIENT_SIZE, dpi);
            let outer = dpi::outer_for_client(client, style, ex, dpi).unwrap();
            let fits = outer.w <= w && outer.h <= h - taskbar;
            let suggested = RECT {
                left: 0,
                top: 0,
                right: outer.w,
                bottom: outer.h,
            };
            // SAFETY: Synchronous message to our own window with a pointer to a live RECT.
            unsafe {
                SendMessageW(
                    form.hwnd(),
                    WM_DPICHANGED,
                    Some(WPARAM((dpi | (dpi << 16)) as usize)),
                    Some(LPARAM(&suggested as *const RECT as isize)),
                );
            }
            pump_for(300);
            let r = form.window_rect();
            let c = form.client_size();
            let (tier, visible, scroll, _, rows_n) = measure_state(&form, dpi);
            let el = elided(&form);
            if name != "1366x768 @125" && (!visible || scroll || el != 0 || rows_n != 1) {
                fit_failures.push(format!(
                    "{name}: visible={visible}, scroll={scroll}, elided={el}, footer={rows_n}"
                ));
            }
            let tag = name.replace(' ', "-").replace('@', "at");
            shot_dir(form.hwnd(), OUT, &format!("{tag}-restored"));
            rows.push(format!(
                "| {name} | {}x{} | {}x{} | {} | {}x{} | {}x{} | {tier} | {visible} | {scroll} | {el} | {rows_n} |",
                w,
                h - taskbar,
                outer.w,
                outer.h,
                if fits { "normal" } else { "maximized (AD-38)" },
                r.right - r.left,
                r.bottom - r.top,
                c.w,
                c.h,
            ));
            if !fits {
                // The maximized client: the work area minus the caption (borders hang off
                // screen), per the audit's frame arithmetic.
                let frame = dpi::outer_for_client(Size { w: 0, h: 0 }, style, ex, dpi).unwrap();
                let max_client = Size {
                    w,
                    h: h - taskbar - (frame.h - frame.w),
                };
                // SAFETY: Resizes our own window to the maximized client's outer size.
                unsafe {
                    let _ = SetWindowPos(
                        form.hwnd(),
                        None,
                        0,
                        0,
                        max_client.w + frame.w,
                        max_client.h + frame.h,
                        SWP_NOMOVE | SWP_NOZORDER,
                    );
                }
                pump_for(300);
                let c = form.client_size();
                let (tier, visible, scroll, _, rows_n) = measure_state(&form, dpi);
                let el = elided(&form);
                if name != "1366x768 @125" && (!visible || scroll || el != 0 || rows_n != 1) {
                    fit_failures.push(format!("{name} maximized: visible={visible}, scroll={scroll}, elided={el}, footer={rows_n}"));
                }
                shot_dir(form.hwnd(), OUT, &format!("{tag}-maximized"));
                rows.push(format!(
                    "| {name} (maximized) | {}x{} | - | maximized | - | {}x{} | {tier} | {visible} | {scroll} | {el} | {rows_n} |",
                    w,
                    h - taskbar,
                    c.w,
                    c.h,
                ));
            }
        }
        // The minimum size at 96 DPI on a large work area (the shrink-to-minimum case).
        window::force_work_area(Some(RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1392,
        }));
        let suggested = RECT {
            left: 0,
            top: 0,
            right: 1056,
            bottom: 839,
        };
        // SAFETY: Synchronous message to our own window with a pointer to a live RECT.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM((96 | (96 << 16)) as usize)),
                Some(LPARAM(&suggested as *const RECT as isize)),
            );
            let _ = SetWindowPos(form.hwnd(), None, 0, 0, 100, 100, SWP_NOMOVE | SWP_NOZORDER);
        }
        pump_for(300);
        let r = form.window_rect();
        let c = form.client_size();
        let (tier, visible, scroll, _, rows_n) = measure_state(&form, 96);
        let el = elided(&form);
        if !visible || scroll || el != 0 || rows_n != 1 {
            fit_failures.push(format!(
                "minimum: visible={visible}, scroll={scroll}, elided={el}, footer={rows_n}"
            ));
        }
        shot_dir(form.hwnd(), OUT, "minimum-96");
        rows.push(format!(
            "| minimum @100 (2560x1392) | 2560x1392 | 900x750 | normal | {}x{} | {}x{} | {tier} | {visible} | {scroll} | {el} | {rows_n} |",
            r.right - r.left,
            r.bottom - r.top,
            c.w,
            c.h
        ));
        // F5 evidence on the real main window: one layout pass per resize and per DPI change.
        let before = form.layout_count();
        // SAFETY: Resizes our own window.
        unsafe {
            let _ = SetWindowPos(
                form.hwnd(),
                None,
                0,
                0,
                1000,
                820,
                SWP_NOMOVE | SWP_NOZORDER,
            );
        }
        pump_for(200);
        let resize_passes = form.layout_count() - before;
        let before = form.layout_count();
        let r = form.window_rect();
        let suggested = RECT {
            left: r.left,
            top: r.top,
            right: r.left + (r.right - r.left) * 3 / 2,
            bottom: r.top + (r.bottom - r.top) * 3 / 2,
        };
        // SAFETY: Synchronous message to our own window with a pointer to a live RECT.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM((144 | (144 << 16)) as usize)),
                Some(LPARAM(&suggested as *const RECT as isize)),
            );
        }
        pump_for(300);
        let dpi_passes = form.layout_count() - before;
        rows.push(format!(
            "
Layout passes on the real main window: {resize_passes} per resize, {dpi_passes} per DPI change (DESIGN.md 8.8 wants 1 and 1)."
        ));
        // Back to the real monitor.
        window::force_work_area(None);
        let r = form.window_rect();
        let suggested = RECT {
            left: r.left,
            top: r.top,
            right: r.left + (r.right - r.left) * real as i32 / 96,
            bottom: r.top + (r.bottom - r.top) * real as i32 / 96,
        };
        // SAFETY: Synchronous message to our own window with a pointer to a live RECT.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM((real | (real << 16)) as usize)),
                Some(LPARAM(&suggested as *const RECT as isize)),
            );
        }
        pump_for(200);
        for row in &rows {
            println!("RESULT {row}");
        }
        std::fs::write(
            Path::new(OUT).join("fit-matrix.md"),
            rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
        )
        .unwrap();
        form.destroy();
        stop.store(true, Ordering::SeqCst);
        closer_thread.join().unwrap();
        bmps_to_png_in(OUT);
        assert!(fit_failures.is_empty(), "{fit_failures:?}");
        trunk_primitives_check();
        msgbox::testing::review_keyboard();
    }

    // Extend the existing real-HWND run rather than adding mock layout tests.
    fn trunk_tools_check(form: &Form) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_RETURN, VK_SPACE};
        use windows::Win32::UI::WindowsAndMessaging::{MSG, WM_KEYDOWN, WM_KEYUP};
        let path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("HWIDChecker.settings.json");
        let previous = std::fs::read(&path).ok();
        let initial = form.is_checked(STARTUP_UPDATES);
        let toggle = form.control(STARTUP_UPDATES).unwrap();
        form.focus(STARTUP_UPDATES);
        let enter = MSG {
            hwnd: toggle,
            message: WM_KEYDOWN,
            wParam: WPARAM(VK_RETURN.0 as usize),
            ..Default::default()
        };
        assert!(window::pre_translate(&enter));
        assert_eq!(form.is_checked(STARTUP_UPDATES), !initial);
        assert_eq!(
            text_of(toggle),
            format!(
                "Startup Update Check, {}",
                if initial { "off" } else { "on" }
            )
        );
        assert_eq!(
            settings::Settings::load().check_updates_on_start(),
            !initial
        );
        // SAFETY: Native Space down/up to this test's focused button, no desktop input.
        unsafe {
            SendMessageW(toggle, WM_KEYDOWN, Some(WPARAM(VK_SPACE.0 as usize)), None);
            SendMessageW(toggle, WM_KEYUP, Some(WPARAM(VK_SPACE.0 as usize)), None);
        }
        assert_eq!(form.is_checked(STARTUP_UPDATES), initial);
        assert_eq!(settings::Settings::load().check_updates_on_start(), initial);
        let windows = own_windows().len();
        form.click(COMPARE_EXPORTS);
        assert_eq!(
            own_windows().len(),
            windows,
            "Compare scaffold opens nothing"
        );
        match previous {
            Some(bytes) => std::fs::write(&path, bytes).unwrap(),
            None => std::fs::remove_file(&path).unwrap(),
        }
        println!(
            "RESULT tools: Enter/Space toggle, accessible name, persisted reload, Compare no-op passed"
        );
    }

    fn trunk_primitives_check() {
        use windows::Win32::UI::Controls::{EM_GETLIMITTEXT, EM_REPLACESEL, EM_SETSEL};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SetKeyboardState, VK_CONTROL, VK_ESCAPE, VK_F3, VK_RETURN, VK_SHIFT,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            ES_NOHIDESEL, GWL_STYLE, GetWindowLongW, MSG, WM_KEYDOWN,
        };
        let changes = Rc::new(Cell::new(0));
        let keys = Rc::new(RefCell::new(Vec::new()));
        let (changed, received) = (Rc::clone(&changes), Rc::clone(&keys));
        let mut spec = FormSpec::new(
            "Trunk primitives",
            WindowSize::Client(Size { w: 500, h: 220 }),
        );
        spec.find_keys = true;
        let nodes = vec![
            Node::flow(
                FlowDir::TopDown,
                false,
                vec![
                    Node::leaf(
                        1,
                        Ctl::Button(ButtonSpec::outline("Mask IDs").icon(glyph::HIDE).toggle()),
                    )
                    .min(theme::MASK_BUTTON_SIZE)
                    .auto_size(),
                    Node::leaf(
                        2,
                        Ctl::Edit(
                            EditSpec::new(theme::BODY_FONT, theme::TEXT, theme::CARD)
                                .single_line()
                                .cue("Find"),
                        ),
                    )
                    .size(Size {
                        w: 320,
                        h: theme::FIND_BAR_HEIGHT,
                    }),
                    Node::leaf(
                        3,
                        Ctl::Button(
                            ButtonSpec::outline("Previous match")
                                .icon(glyph::CHEVRON_UP)
                                .icon_only(),
                        ),
                    )
                    .size(theme::FIND_BUTTON_SIZE),
                    Node::leaf(
                        4,
                        Ctl::Edit(
                            EditSpec::new(theme::CONTENT_FONT, theme::TEXT, theme::CARD)
                                .keep_selection(),
                        ),
                    )
                    .size(Size { w: 400, h: 60 }),
                ],
            )
            .fill()
            .padding(theme::OUTPUT_PANEL_PADDING),
        ];
        let form = Form::create(HWND::default(), spec, nodes, move |_, event| {
            match event {
                Event::TextChanged(2) => changed.set(changed.get() + 1),
                Event::Key(key) => received.borrow_mut().push(key),
                _ => {}
            }
            true
        })
        .unwrap();
        form.show();
        form.set_timer(TICK, 20);
        form.click(1);
        assert!(form.is_checked(1));
        assert_eq!(form.text(1), "Mask IDs, on");
        assert_eq!(form.text(3), "Previous match");
        let edit = form.control(2).unwrap();
        let well = form.control(4).unwrap();
        form.edit_set_text(4, "Selected match remains visible");
        // SAFETY: Messages and style queries target this test's live HWNDs.
        unsafe {
            assert_eq!(SendMessageW(edit, EM_GETLIMITTEXT, None, None).0, 256);
            let text = to_wide(&"A".repeat(300));
            SendMessageW(
                edit,
                EM_REPLACESEL,
                None,
                Some(LPARAM(text.as_ptr() as isize)),
            );
            assert_eq!(GetWindowLongW(well, GWL_STYLE) & ES_NOHIDESEL, ES_NOHIDESEL);
            SendMessageW(well, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(14)));
        }
        assert_eq!(form.text(2).len(), 256);
        assert!(changes.get() > 0, "real EN_CHANGE reaches form");
        form.focus(3);
        let frame = form.with_tree(|t| t.find(2).unwrap().bounds).unwrap();
        // SAFETY: Click the top padding of this test's input, outside its native text area.
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
            use windows::Win32::UI::WindowsAndMessaging::{GetParent, WM_LBUTTONDOWN};
            SendMessageW(
                GetParent(edit).unwrap(),
                WM_LBUTTONDOWN,
                None,
                Some(LPARAM(((frame.y + 1) << 16 | (frame.x + 1)) as isize)),
            );
            assert_eq!(GetFocus(), edit);
        }
        for (vk, modifiers, expected) in [
            (u16::from(b'F'), vec![VK_CONTROL.0], window::FindKey::Open),
            (
                VK_F3.0,
                vec![VK_SHIFT.0],
                window::FindKey::Step { backwards: true },
            ),
            (
                VK_RETURN.0,
                vec![VK_SHIFT.0],
                window::FindKey::Enter {
                    id: 2,
                    backwards: true,
                },
            ),
            (VK_ESCAPE.0, vec![], window::FindKey::Escape { id: Some(2) }),
        ] {
            let mut keyboard = [0u8; 256];
            for modifier in modifiers {
                keyboard[modifier as usize] = 0x80;
            }
            // SAFETY: Changes only this UI thread's keyboard snapshot, restored immediately.
            unsafe {
                SetKeyboardState(&keyboard).unwrap();
            }
            assert!(window::pre_translate(&MSG {
                hwnd: edit,
                message: WM_KEYDOWN,
                wParam: WPARAM(vk as usize),
                ..Default::default()
            }));
            // SAFETY: Restores this test thread's neutral modifier state.
            unsafe {
                SetKeyboardState(&[0; 256]).unwrap();
            }
            assert_eq!(keys.borrow().last(), Some(&expected));
        }
        form.edit_set_text(2, "");
        // SAFETY: Enables keyboard focus cues only in this test form.
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                UIS_CLEAR, UISF_HIDEFOCUS, WM_UPDATEUISTATE,
            };
            SendMessageW(
                form.hwnd(),
                WM_UPDATEUISTATE,
                Some(WPARAM((UIS_CLEAR | (UISF_HIDEFOCUS << 16)) as usize)),
                None,
            );
        }
        dpi_change(&form, 96);
        pump_for(100);
        shot(form.hwnd(), "primitives-96");
        dpi_change(&form, 144);
        shot(form.hwnd(), "primitives-144");
        form.destroy();
        // Open the real native picker with both filters, then cancel its own HWND.
        let closer = std::thread::spawn(|| {
            for _ in 0..100 {
                if let Some(dialog) = own_windows()
                    .into_iter()
                    .find(|h| text_of(*h) == "Trunk picker check")
                {
                    // SAFETY: Value-only close message to this process's test picker.
                    unsafe {
                        PostMessageW(Some(dialog), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                    }
                    return true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            false
        });
        assert!(
            win::dialog::open_file(
                HWND::default(),
                "Trunk picker check",
                &[
                    ("HWID exports (*.txt;*.json)", "*.txt;*.json"),
                    ("All files (*.*)", "*.*")
                ],
                Path::new(GOLDEN)
            )
            .is_none()
        );
        assert!(
            closer.join().unwrap(),
            "picker actually opened before cancellation"
        );
        bmps_to_png();
        println!(
            "RESULT primitives: toggle, icon name, input limit/change, keep-selection style, find key routing, native picker cancel passed"
        );
    }

    #[test]
    #[ignore = "opens real windows and collects real hardware data"]
    fn live() {
        std::fs::create_dir_all(GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        assert!(activate_comctl6(), "comctl v6 activation context");
        let mut results: Vec<String> = Vec::new();
        let mut record = |line: String| {
            println!("RESULT {line}");
            results.push(line);
        };

        let (spec, nodes, handler) = parts();
        let started = Instant::now();
        let form = Form::create(HWND::default(), spec, nodes, move |form, event| {
            if let Event::Timer(CLOSE_WHILE_BUSY) = event {
                form.kill_timer(CLOSE_WHILE_BUSY);
                form.close();
                true
            } else {
                handler(form, event)
            }
        })
        .unwrap();
        form.show();
        form.set_timer(TICK, 20);
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let button = form.control(OLD_VIEW).unwrap().0 as isize;
        let closer_thread = {
            let (stop, log) = (Arc::clone(&stop), Arc::clone(&log));
            std::thread::spawn(move || closer(stop, log, button))
        };
        pump_for(300);
        let dpi_now = form.dpi();
        record(format!(
            "start: dpi {dpi_now}, client {:?}, loading {}, title {:?}, meta {:?}, body {:?}, \
             overlay {:?} centered {}",
            form.client_size(),
            loading(&form),
            form.text(SECTION_TITLE),
            form.text(SECTION_META),
            form.text(CONTENT),
            bounds(&form, LOADING),
            centered(&form)
        ));
        shot(form.hwnd(), &format!("main-{dpi_now}-loading"));
        assert!(loading(&form));
        assert_eq!(form.text(CONTENT), "Loading...");

        let took = pump_while(Duration::from_secs(150), || loading(&form));
        record(format!(
            "first load done after {} ms (from create {} ms)",
            took.as_millis(),
            started.elapsed().as_millis()
        ));
        assert!(!loading(&form), "load finished");
        pump_for(300);
        record(format!("startup boxes {:?}", take(&log)));
        shot(form.hwnd(), &format!("main-{dpi_now}-loaded"));
        record(format!(
            "sidebar {:?} footer {:?} edit {:?} section button 0 {:?}",
            bounds(&form, SIDEBAR),
            bounds(&form, FOOTER),
            bounds(&form, CONTENT),
            bounds(&form, FIRST_SECTION)
        ));
        for (id, _, _) in FOOTER_BUTTONS {
            record(format!("footer button {id} {:?}", bounds(&form, id)));
        }

        // Every section: title, meta, sidebar text, body; bodies stay private.
        let mut shown = Vec::new();
        let mut private = String::new();
        for (i, p) in hw::PROVIDERS.iter().enumerate() {
            form.click(section_id(i));
            pump_for(30);
            assert_eq!(form.text(SECTION_TITLE), p.title);
            assert_eq!(form.text(SECTION_META), format!("Section {} of 14", i + 1));
            assert_eq!(form.text(section_id(i)), p.title);
            let body = form.text(CONTENT);
            private.push_str(&format!("===== {} =====\r\n{body}\r\n\r\n", p.title));
            let errors = body
                .lines()
                .filter(|l| l.starts_with("Error retrieving"))
                .count();
            if errors > 0 {
                record(format!("{} shows {errors} provider error line(s)", p.title));
            }
            shown.push(body);
        }
        std::fs::write(Path::new(GOLDEN).join("rust-sections.txt"), private).unwrap();
        form.click(section_id(2));
        pump_for(200);
        shot(form.hwnd(), &format!("main-{dpi_now}-section3"));
        assert!(form.is_active(section_id(2)));
        assert!(!form.is_active(section_id(0)));

        // Export through the real button; the box names the file.
        form.click(EXPORT);
        pump_for(300);
        let boxes = take(&log);
        record(format!("export boxes {boxes:?}"));
        let path = boxes
            .iter()
            .find_map(|b| b.split("Saved to: ").nth(1))
            .map(PathBuf::from)
            .unwrap();
        assert!(boxes[0].starts_with("box Export | Export completed successfully!\nSaved to: "));
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(Path::new(GOLDEN).join("rust-export.txt"), &bytes).unwrap();
        std::fs::remove_file(&path).unwrap();
        let expected: String = hw::PROVIDERS
            .iter()
            .zip(&shown)
            .map(|(p, body)| format!("===== {} =====\r\n{body}\r\n\r\n", p.title))
            .collect();
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "no BOM");
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            expected,
            "export = shown bodies"
        );
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let n = name.as_bytes();
        record(format!(
            "export name {name} next to the exe: {}",
            path.parent() == std::env::current_exe().unwrap().parent()
        ));
        assert!(name.len() == 35 && name.starts_with("HWID-EXPORT-") && n[14] == b'.');
        assert!(n[25] == b';' && n[28] == b';' && name.ends_with(".txt"));

        // Two quick Refresh clicks: only the newest load lands, one success box (AD-41).
        form.click(REFRESH);
        pump_for(50);
        form.click(REFRESH);
        record(format!(
            "after refresh: loading {} meta {:?} body {:?} first item active {}",
            loading(&form),
            form.text(SECTION_META),
            form.text(CONTENT),
            form.is_active(section_id(0))
        ));
        let took = pump_while(Duration::from_secs(150), || loading(&form));
        pump_for(3000);
        let boxes = take(&log);
        record(format!(
            "refresh x2 done after {} ms, boxes {boxes:?}",
            took.as_millis()
        ));
        assert_eq!(
            boxes,
            vec!["box Refresh | Hardware data refreshed successfully!".to_owned()]
        );

        // Old View: the closer inspects, captures, and closes it.
        let t = Instant::now();
        form.focus(OLD_VIEW); // as after a mouse click
        // SAFETY: Reads this thread's focus and active windows.
        let (before, active) = unsafe { (GetFocus(), GetActiveWindow()) };
        record(format!(
            "before old view: focus on id {:?}, main active {}",
            (0..200u16).find(|&id| form.control(id) == Some(before)),
            active == form.hwnd()
        ));
        form.click(OLD_VIEW);
        pump_for(200);
        // SAFETY: Reads this thread's focus and active windows.
        let (after, active) = unsafe { (GetFocus(), GetActiveWindow()) };
        let focus_id = (0..200u16).find(|&id| form.control(id) == Some(after));
        let active = active == form.hwnd();
        record(format!(
            "old view round trip {} ms, log {:?}, button after {:?} enabled {}, main active {}, \
             focus on id {:?} (C#: next tab stop = first section)",
            t.elapsed().as_millis(),
            take(&log),
            form.text(OLD_VIEW),
            form.is_enabled(OLD_VIEW),
            active,
            focus_id,
        ));
        assert_eq!(form.text(OLD_VIEW), OLD_VIEW_TEXT);
        assert!(form.is_enabled(OLD_VIEW));
        // Another app may take the foreground while the test runs; then no focus is expected.
        if active {
            assert_eq!(focus_id, Some(FIRST_SECTION));
        }
        // Check the restored tab stop even when another app took the foreground at close.
        // SAFETY: Activates our own test window; the kit restores its saved focus on activation.
        unsafe {
            let _ = SetForegroundWindow(form.hwnd());
        }
        pump_for(200);
        // SAFETY: Read-only query of this UI thread's focus.
        let restored = unsafe { GetFocus() };
        let restored_id = (0..200u16).find(|&id| form.control(id) == Some(restored));
        record(format!("old view reactivated: focus on id {restored_id:?}"));
        assert_eq!(restored_id, Some(FIRST_SECTION));

        // Native dialog keys on this form, which declares neither AcceptButton nor CancelButton.
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN, VK_TAB};
        form.focus(OLD_VIEW);
        // SAFETY: Value-only keyboard message to our own focused test control.
        unsafe {
            PostMessageW(
                form.control(OLD_VIEW),
                WM_KEYDOWN,
                WPARAM(VK_TAB.0 as usize),
                LPARAM(0),
            )
            .unwrap();
        }
        pump_for(100);
        // SAFETY: Reads the calling UI thread's focus.
        assert_eq!(unsafe { GetFocus() }, form.control(FIRST_SECTION).unwrap());
        form.focus(section_id(2));
        // SAFETY: Value-only keyboard message to our own focused test control.
        unsafe {
            PostMessageW(
                form.control(section_id(2)),
                WM_KEYDOWN,
                WPARAM(VK_RETURN.0 as usize),
                LPARAM(0),
            )
            .unwrap();
        }
        pump_for(100);
        assert_eq!(form.text(SECTION_TITLE), hw::PROVIDERS[2].title);
        form.focus(CONTENT);
        for key in [VK_RETURN, VK_ESCAPE] {
            // SAFETY: Value-only keyboard message to our own focused test edit.
            unsafe {
                PostMessageW(
                    form.control(CONTENT),
                    WM_KEYDOWN,
                    WPARAM(key.0 as usize),
                    LPARAM(0),
                )
                .unwrap();
            }
            pump_for(100);
            assert!(form.is_alive());
            assert_eq!(form.text(SECTION_TITLE), hw::PROVIDERS[2].title);
        }
        record("keyboard: Tab wraps from Old View to first section; focused-button Enter clicks; edit Enter and Esc leave form open".to_owned());

        // A main load completes inside a modal loop. Its Refresh box must own the active
        // child, so closing the box cannot re-enable the still-modal main window.
        form.click(REFRESH);
        let parent = form;
        let modal_started = Instant::now();
        let modal_owner_disabled = Rc::new(Cell::new(false));
        let owner_check = Rc::clone(&modal_owner_disabled);
        window::run_modal(
            form.hwnd(),
            FormSpec::new(
                "Review load ownership",
                WindowSize::Client(theme::OLD_VIEW_SIZE),
            ),
            vec![],
            move |child, event| {
                match event {
                    Event::Created => child.set_timer(TICK, 20),
                    Event::Timer(TICK) => {
                        // SAFETY: Read-only enabled-state queries on this thread's own windows.
                        let child_enabled = unsafe { IsWindowEnabled(child.hwnd()) }.as_bool();
                        if !loading(&parent) && child_enabled {
                            // SAFETY: Read-only query of the still-modal owner.
                            owner_check.set(!unsafe { IsWindowEnabled(parent.hwnd()) }.as_bool());
                            child.close();
                        } else if modal_started.elapsed() > Duration::from_secs(75) {
                            child.close();
                        }
                    }
                    _ => {}
                }
                true
            },
        )
        .unwrap();
        assert!(
            modal_started.elapsed() < Duration::from_secs(75),
            "modal load finished before its deadline"
        );
        assert!(
            modal_owner_disabled.get(),
            "Refresh box preserved the disabled owner"
        );
        assert!(!loading(&form));
        assert_eq!(
            take(&log),
            vec!["box Refresh | Hardware data refreshed successfully!".to_owned()]
        );
        record("modal load: Refresh box closes without enabling the main owner; child then closes normally".to_owned());

        // Minimum size: the footer wraps.
        // SAFETY: Resizes our own window below its minimum; WM_GETMINMAXINFO clamps it.
        unsafe {
            let _ = SetWindowPos(form.hwnd(), None, 0, 0, 100, 100, SWP_NOMOVE | SWP_NOZORDER);
        }
        pump_for(300);
        let r = rect_of(form.hwnd(), false);
        record(format!(
            "minimum outer {}x{} footer {:?} sidebar {:?} sidebar scroll {}",
            r.right - r.left,
            r.bottom - r.top,
            bounds(&form, FOOTER),
            bounds(&form, SIDEBAR),
            form.vscroll_visible(SIDEBAR)
        ));
        shot(form.hwnd(), &format!("main-{dpi_now}-minimum"));

        // Cross the height at which the sidebar scrollbar appears, without changing its width.
        for height in [1000, 750] {
            // SAFETY: Resizes our own test form on its UI thread.
            unsafe {
                let _ = SetWindowPos(
                    form.hwnd(),
                    None,
                    0,
                    0,
                    form.scale(900),
                    form.scale(height),
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
            pump_for(200);
            let before = bounds(&form, FIRST_SECTION);
            let scroll = form.vscroll_visible(SIDEBAR);
            responsive(&form, &State::default(), form.client_size());
            form.relayout();
            let after = bounds(&form, FIRST_SECTION);
            record(format!(
                "sidebar outer height {height}: scroll {scroll}, first item before {before:?} after {after:?}"
            ));
            assert_eq!(
                before, after,
                "sidebar widths already reflect the current scrollbar"
            );
        }

        // Synthetic DPI changes while the overlay shows (it must stay centered).
        let real = dpi::window_dpi(form.hwnd());
        for new_dpi in [96u32, 144, 192, real] {
            form.click(REFRESH);
            dpi_change(&form, new_dpi);
            record(format!(
                "dpi {new_dpi}: client {:?} sidebar {:?} footer {:?} overlay {:?} centered {}",
                form.client_size(),
                bounds(&form, SIDEBAR),
                bounds(&form, FOOTER),
                bounds(&form, LOADING),
                centered(&form)
            ));
            shot(form.hwnd(), &format!("main-dpi{new_dpi}-loading"));
            pump_while(Duration::from_secs(150), || loading(&form));
            pump_for(500);
            take(&log);
        }

        // Title-bar close remains available while both fresh collections are running.
        form.click(REFRESH);
        form.set_timer(CLOSE_WHILE_BUSY, 100);
        let close_started = Instant::now();
        form.click(OLD_VIEW);
        record(format!(
            "close while collecting: owner alive {}, returned after {} ms",
            form.is_alive(),
            close_started.elapsed().as_millis()
        ));
        assert!(!form.is_alive());
        assert!(close_started.elapsed() < Duration::from_secs(2));
        stop.store(true, Ordering::SeqCst);
        closer_thread.join().unwrap();
        bmps_to_png();
        let report: String = results.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(Path::new(GOLDEN).join("live-results.txt"), report).unwrap();
    }
}
