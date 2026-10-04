//! Native LISTBOX plumbing and GDI rendering for C2c; no matching logic here.

mod paint;

use super::*;
use crate::report::compare::Kind as Change;
use crate::ui::compare::table::{Row, RowId, Table, legend};
use windows::Win32::UI::WindowsAndMessaging::{
    GetScrollPos, LB_GETTOPINDEX, LB_ITEMFROMPOINT, LB_SETCURSEL, LB_SETHORIZONTALEXTENT,
    LB_SETTOPINDEX, SB_HORZ,
};

pub(super) struct TableData {
    model: Rc<RefCell<Table>>,
    pub(super) brush: Brush,
    fonts: RefCell<[dpi::Font; 5]>,
    frame: Cell<Rect>,
    columns: Cell<[i32; 5]>,
    value_widths: Cell<[i32; 2]>,
    resizing: Cell<bool>,
    keys: RefCell<Vec<RowId>>,
}

fn fonts(dpi: u32) -> win::Result<[dpi::Font; 5]> {
    Ok([
        dpi::Font::new(theme::SECTION_META_FONT, dpi)?,
        dpi::Font::new(theme::SMALL_FONT, dpi)?,
        dpi::Font::new(theme::BUTTON_FONT, dpi)?,
        dpi::Font::new(theme::CONTENT_FONT, dpi)?,
        dpi::Font::new(theme::icon_font(theme::ICON_PX), dpi)?,
    ])
}

impl TableData {
    pub(super) fn new(model: Rc<RefCell<Table>>, dpi: u32) -> win::Result<Self> {
        let data = Self {
            model,
            brush: Brush::new(theme::CARD),
            fonts: RefCell::new(fonts(dpi)?),
            frame: Cell::new(Rect::default()),
            columns: Cell::new([0; 5]),
            value_widths: Cell::new([0; 2]),
            resizing: Cell::new(false),
            keys: RefCell::new(Vec::new()),
        };
        data.measure_values(dpi);
        Ok(data)
    }

    pub(super) fn apply_dpi(&self, dpi: u32) {
        match fonts(dpi) {
            Ok(fonts) => *self.fonts.borrow_mut() = fonts,
            Err(error) => win::record(error),
        }
        self.measure_values(dpi);
    }

    fn measure_values(&self, dpi: u32) {
        let dc = ScreenDc::new();
        let fonts = self.fonts.borrow();
        let padding = 2 * dpi::scale(theme::COMPARE_CELL_PADDING, dpi) + theme::COMPARE_VERDICT_BAR;
        let mut widths = [dpi::scale(theme::COMPARE_VALUE_MIN_WIDTH, dpi); 2];
        for values in self.model.borrow().values() {
            for i in 0..2 {
                widths[i] =
                    widths[i].max(paint::width(dc.0, values[i], fonts[3].handle()) + padding);
            }
        }
        self.value_widths.set(widths);
    }

    fn scroll_offset(&self, hwnd: HWND) -> i32 {
        // SAFETY: Reads the native horizontal position of this live list.
        unsafe { GetScrollPos(hwnd, SB_HORZ) }
    }

    pub(super) fn bounds(&self, st: &CtlState, bounds: Rect) -> Rect {
        self.frame.set(bounds);
        let strip = dpi::scale(theme::COMPARE_HEADER_HEIGHT, st.dpi.get());
        bounds.deflate(Pad {
            l: theme::STROKE,
            t: strip + theme::STROKE,
            r: theme::STROKE,
            b: theme::STROKE,
        })
    }

    pub(super) fn resize(&self, st: &CtlState) {
        // Changing the native extent can synchronously send WM_SIZE back to this list.
        if self.resizing.replace(true) {
            return;
        }
        let [before, after] = self.value_widths.get();
        let scale = |n| dpi::scale(n, st.dpi.get());
        let status = scale(theme::COMPARE_STATUS_WIDTH);
        // Let Windows settle both scrollbars before reading the resulting viewport width.
        let minimum = scale(theme::COMPARE_FIELD_MIN_WIDTH) + before + after + status;
        send(st.hwnd, LB_SETHORIZONTALEXTENT, minimum as usize, 0);
        let mut client = RECT::default();
        // SAFETY: Live list window and writable RECT on its owning thread.
        unsafe {
            if let Err(error) = GetClientRect(st.hwnd, &mut client) {
                win::record(win::Error::from_win("Compare table size", error));
                self.resizing.set(false);
                return;
            }
        }
        let width = client.right;
        let field = (width - status - before - after).clamp(
            scale(theme::COMPARE_FIELD_MIN_WIDTH),
            scale(theme::COMPARE_FIELD_WIDTH),
        );
        self.columns.set([
            0,
            field,
            field + before,
            field + before + after,
            field + before + after + status,
        ]);
        self.resizing.set(false);
        // The strip is painted by the parent and must follow native scrollbar width changes.
        // SAFETY: Queries this live list's parent; invalidate accepts a live HWND.
        if let Ok(parent) = unsafe { GetParent(st.hwnd) } {
            invalidate(parent);
        }
    }

    fn selected(&self, hwnd: HWND) -> Option<RowId> {
        let selected = send(hwnd, LB_GETCURSEL, 0, 0).0;
        usize::try_from(selected)
            .ok()
            .and_then(|i| self.keys.borrow().get(i).copied())
    }

    fn replace(&self, st: &CtlState, selected: Option<RowId>) {
        let top = send(st.hwnd, LB_GETTOPINDEX, 0, 0).0.max(0);
        send(st.hwnd, WM_SETREDRAW, 0, 0);
        send(st.hwnd, LB_RESETCONTENT, 0, 0);
        let model = self.model.borrow();
        let mut keys = self.keys.borrow_mut();
        keys.clear();
        for row in &model.rows {
            let text = to_wide(&row.line);
            let result = send(st.hwnd, LB_ADDSTRING, 0, text.as_ptr() as isize).0;
            if result < 0 {
                win::record(win::Error::msg(
                    "Compare row",
                    "LISTBOX could not add a row",
                ));
                break;
            }
            keys.push(row.id);
        }
        let mut target = selected;
        let selected = loop {
            let Some(id) = target else { break None };
            if let Some(index) = keys.iter().position(|key| *key == id) {
                break Some(index);
            }
            target = model.parent(id);
        };
        send(st.hwnd, LB_SETCURSEL, selected.unwrap_or(0), 0);
        send(st.hwnd, LB_SETTOPINDEX, top as usize, 0);
        drop(keys);
        drop(model);
        send(st.hwnd, WM_SETREDRAW, 1, 0);
        self.resize(st);
        invalidate(st.hwnd);
    }

    fn activate(&self, st: &CtlState, expand: Option<bool>) {
        if let Some(id) = self.selected(st.hwnd) {
            if id == RowId::Notice || (matches!(id, RowId::Field(_, _)) && expand != Some(false)) {
                return;
            }
            let selected = self.model.borrow_mut().activate(id, expand);
            self.replace(st, Some(selected));
            if selected != id
                && let Some(index) = self.keys.borrow().iter().position(|key| *key == selected)
            {
                // Left/More are navigation too: bring the new selection into view.
                send(st.hwnd, LB_SETCURSEL, index, 0);
            }
        }
    }

    pub(super) fn click(&self, st: &CtlState, point: LPARAM) {
        let hit = send(st.hwnd, LB_ITEMFROMPOINT, 0, point.0).0 as u32;
        if hit >> 16 == 0 && (hit & 0xffff) < self.keys.borrow().len() as u32 {
            self.activate(st, None);
        }
    }
}

/// The Compare client width, including its panel, frame and possible vertical scrollbar.
pub(crate) fn table_window_width(hwnd: HWND) -> Option<i32> {
    let st = state_of(hwnd)?;
    let Data::Table(t) = &st.data else {
        return None;
    };
    let [before, after] = t.value_widths.get();
    Some(
        before
            + after
            + dpi::scale(
                theme::COMPARE_FIELD_WIDTH
                    + theme::COMPARE_STATUS_WIDTH
                    + theme::CONTENT_PADDING.horizontal(),
                st.dpi.get(),
            )
            + 2 * theme::STROKE
            + dpi::metric(SM_CXVSCROLL, st.dpi.get()),
    )
}

pub(crate) fn table_refresh(hwnd: HWND) {
    if let Some(st) = state_of(hwnd)
        && let Data::Table(t) = &st.data
    {
        let selected = t.selected(hwnd);
        t.model.borrow_mut().rebuild();
        t.replace(&st, selected);
    }
}

pub(crate) fn table_key(hwnd: HWND, vk: u16) -> bool {
    let Some(st) = state_of(hwnd) else {
        return false;
    };
    let Data::Table(t) = &st.data else {
        return false;
    };
    if vk == u16::from(b'C') && ctrl_down() {
        let selected = send(hwnd, LB_GETCURSEL, 0, 0).0;
        if selected >= 0 {
            let len = send(hwnd, LB_GETTEXTLEN, selected as usize, 0).0;
            if len >= 0 {
                let mut text = vec![0u16; len as usize + 1];
                send(
                    hwnd,
                    LB_GETTEXT,
                    selected as usize,
                    text.as_mut_ptr() as isize,
                );
                copy_text(hwnd, &from_wide(&text));
            }
        }
        return true;
    }
    if ctrl_down() {
        return false;
    }
    match vk {
        13 | 32 => t.activate(&st, None),
        key if key == VK_RIGHT.0 => t.activate(&st, Some(true)),
        key if key == VK_LEFT.0 => t.activate(&st, Some(false)),
        _ => return false,
    }
    true
}
