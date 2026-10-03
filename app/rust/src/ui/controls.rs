//! Owned by WP-10a: native controls with WinForms look and behavior.
//!
//! Flat button (owner-drawn BUTTON), label (painted STATIC), read-only multiline EDIT, checked
//! list (owner-drawn LISTBOX), progress bar. Each control keeps its own state in its subclass;
//! containers reflect `WM_DRAWITEM`, `WM_CTLCOLOR*`, `WM_COMMAND`, and `WM_VKEYTOITEM` back to it
//! through [`reflect`]. Text goes through GDI `DrawTextExW` with the flags and margins of
//! WinForms `TextRenderer`, so pixels can match the C# app.
//!
//! Portions ported from dotnet/winforms `ButtonBase.cs`, `Button.cs`,
//! `ButtonInternal/ButtonBaseAdapter*.cs`, `ButtonInternal/ButtonFlatAdapter.cs`,
//! `ControlPaint.HLSColor.cs`, `Rendering/TextExtensions.cs`, `Label.cs`, `CheckedListBox.cs`:
//!
//! The MIT License (MIT)
//!
//! Copyright (c) .NET Foundation and Contributors
//!
//! All rights reserved.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use super::dpi;
use super::layout::{Kind, Node, Pad, Rect, Size};
use super::theme::{self, Color, FontSpec};
use crate::win::{
    self,
    wide::{from_wide, to_wide},
};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateSolidBrush, DFC_BUTTON, DFCS_BUTTONCHECK, DFCS_CHECKED, DFCS_FLAT,
            DRAW_TEXT_FORMAT, DRAWTEXTPARAMS, DT_BOTTOM, DT_CALCRECT, DT_CENTER, DT_EDITCONTROL,
            DT_END_ELLIPSIS, DT_HIDEPREFIX, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
            DT_WORDBREAK, DeleteObject, DrawFocusRect, DrawFrameControl, DrawTextExW, EndPaint,
            FillRect, GetDC, GetTextMetricsW, HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect,
            PAINTSTRUCT, ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor, TEXTMETRICW,
            TRANSPARENT,
        },
        UI::{
            Controls::{
                BPBF_COMPATIBLEBITMAP, BeginBufferedPaint, BufferedPaintInit, BufferedPaintUnInit,
                CloseThemeData, DRAWITEMSTRUCT, DrawThemeBackground, EM_REPLACESEL, EM_SCROLLCARET,
                EM_SETLIMITTEXT, EM_SETSEL, EndBufferedPaint, HTHEME, ODS_FOCUS, ODS_NOFOCUSRECT,
                ODS_SELECTED, OpenThemeData, PBM_SETMARQUEE, PBM_SETPOS, PBM_SETRANGE32,
                PBS_MARQUEE, PBS_SMOOTH, PROGRESS_CLASSW, WC_BUTTONW, WC_EDITW, WC_LISTBOXW,
                WC_STATICW, WM_MOUSELEAVE,
            },
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, GetKeyState, IsWindowEnabled, TME_LEAVE, TRACKMOUSEEVENT,
                TrackMouseEvent, VK_CONTROL, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR,
                VK_RIGHT, VK_UP,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                BM_GETSTATE, BN_CLICKED, BN_DBLCLK, BS_OWNERDRAW, CreateWindowExW,
                DLGC_WANTALLKEYS, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY,
                GWL_STYLE, GetClientRect, GetPropW, GetWindowLongPtrW, GetWindowTextLengthW,
                GetWindowTextW, HMENU, HTCLIENT, IDC_HAND, LB_ADDSTRING, LB_ERR, LB_GETCURSEL,
                LB_GETTEXT, LB_GETTEXTLEN, LB_RESETCONTENT, LB_SETITEMHEIGHT, LBN_DBLCLK,
                LBN_SELCHANGE, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY,
                LBS_OWNERDRAWFIXED, LBS_WANTKEYBOARDINPUT, LoadCursorW, RemovePropW, SendMessageW,
                SetCursor, SetPropW, SetWindowLongPtrW, SetWindowTextW, UISF_HIDEACCEL,
                UISF_HIDEFOCUS, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CTLCOLOREDIT,
                WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_ERASEBKGND, WM_GETDLGCODE,
                WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_NCDESTROY, WM_PAINT, WM_QUERYUISTATE, WM_SETCURSOR, WM_SETFOCUS, WM_SETFONT,
                WM_SETREDRAW, WM_UPDATEUISTATE, WM_VKEYTOITEM, WS_BORDER, WS_CHILD,
                WS_CLIPSIBLINGS, WS_EX_CLIENTEDGE, WS_HSCROLL, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
    core::{PCWSTR, w},
};

const SUBCLASS_ID: usize = 0x4857_4944; // "HWID"
const STATE_PROP: PCWSTR = w!("HWIDChecker.Ctl");
const UNBOUNDED: i32 = i32::MAX;
const BST_PUSHED: isize = 0x0004;
const BP_CHECKBOX: i32 = 3;
const CBS_UNCHECKEDNORMAL: i32 = 1;
const CBS_CHECKEDNORMAL: i32 = 5;
/// `ButtonBaseAdapter.LayoutOptions.TextImageInset` (not DPI scaled in WinForms).
const TEXT_IMAGE_INSET: i32 = 2;

// ---------------------------------------------------------------------------------------------
// Specs (what a form declares)
// ---------------------------------------------------------------------------------------------

/// `ContentAlignment` subset used by the forms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Align {
    /// `ContentAlignment.MiddleLeft`.
    MiddleLeft,
    /// `ContentAlignment.MiddleCenter`.
    MiddleCenter,
}

/// How a button changes its `BackColor` on mouse events (the C# event handlers).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hover {
    /// No handlers (sidebar buttons; `FlatAppearance` colors only).
    None,
    /// `Buttons.ApplyStyle`: enter/leave/down/up when enabled, and disabled colors.
    Shared {
        /// Normal back color.
        normal: Color,
        /// Hover back color.
        hover: Color,
        /// Pressed back color.
        pressed: Color,
    },
    /// `DeviceRemovalConfirmationForm.AddHoverEffects`: enter and leave only.
    EnterLeave {
        /// Normal back color.
        normal: Color,
        /// Hover back color.
        hover: Color,
    },
}

/// `Buttons.ButtonVariant`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Variant {
    /// Blue.
    Primary,
    /// Dark grey.
    Secondary,
    /// Red.
    Danger,
}

/// A WinForms `FlatStyle.Flat` button.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonSpec {
    /// Text; `&` marks a mnemonic like WinForms `UseMnemonic`.
    pub text: String,
    /// Font.
    pub font: FontSpec,
    /// Initial `BackColor`.
    pub back: Color,
    /// `ForeColor`.
    pub fore: Color,
    /// `FlatAppearance.BorderColor`.
    pub border: Color,
    /// `FlatAppearance.BorderSize` (device pixels, not DPI scaled, like WinForms).
    pub border_size: i32,
    /// `FlatAppearance.MouseOverBackColor` (`None` = WinForms computed color).
    pub over_back: Option<Color>,
    /// `FlatAppearance.MouseDownBackColor` (`None` = WinForms computed color).
    pub down_back: Option<Color>,
    /// `TextAlign`.
    pub align: Align,
    /// Event handlers that change `BackColor`.
    pub hover: Hover,
}

impl ButtonSpec {
    /// `Buttons.ApplyStyle(button, variant)`.
    pub fn shared(text: &str, variant: Variant) -> Self {
        let (normal, hover, pressed) = match variant {
            Variant::Primary => (
                theme::PRIMARY_BUTTON,
                theme::PRIMARY_BUTTON_HOVER,
                theme::PRIMARY_BUTTON_PRESSED,
            ),
            Variant::Secondary => (
                theme::BUTTON_BACKGROUND,
                theme::BUTTON_HOVER,
                theme::BUTTON_BACKGROUND,
            ),
            Variant::Danger => (
                theme::DANGER_BUTTON,
                theme::DANGER_BUTTON_HOVER,
                theme::DANGER_BUTTON,
            ),
        };
        Self {
            text: text.to_owned(),
            font: theme::BUTTON_FONT,
            back: normal,
            fore: theme::PRIMARY_TEXT,
            border: theme::BUTTON_BORDER,
            border_size: theme::BUTTON_BORDER_SIZE,
            over_back: None,
            down_back: None,
            align: Align::MiddleCenter,
            hover: Hover::Shared {
                normal,
                hover,
                pressed,
            },
        }
    }

    /// `Buttons.ApplyStyle(button, ButtonVariant.Secondary)`.
    pub fn secondary(text: &str) -> Self {
        Self::shared(text, Variant::Secondary)
    }

    /// `Buttons.ApplyStyle(button, ButtonVariant.Primary)`.
    pub fn primary(text: &str) -> Self {
        Self::shared(text, Variant::Primary)
    }
}

/// A WinForms `Label` (transparent back, `UseMnemonic` text drawn without prefix processing).
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSpec {
    /// Text (data text: `&` is shown as is).
    pub text: String,
    /// Font.
    pub font: FontSpec,
    /// `ForeColor`.
    pub fore: Color,
    /// `TextAlign`.
    pub align: Align,
    /// `AutoEllipsis`.
    pub ellipsis: bool,
}

impl LabelSpec {
    /// A `MiddleLeft` label without ellipsis. (C# `TopLeft` labels are all `AutoSize`, where the
    /// alignment does not move the text, so `MiddleLeft` covers them too.)
    pub fn new(text: &str, font: FontSpec, fore: Color) -> Self {
        Self {
            text: text.to_owned(),
            font,
            fore,
            align: Align::MiddleLeft,
            ellipsis: false,
        }
    }

    /// Sets `TextAlign`.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Sets `AutoEllipsis = true`.
    pub fn ellipsis(mut self) -> Self {
        self.ellipsis = true;
        self
    }
}

/// `TextBox.BorderStyle`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditBorder {
    /// `BorderStyle.Fixed3D` (WinForms default, `WS_EX_CLIENTEDGE`).
    Fixed3D,
    /// `BorderStyle.FixedSingle` (`WS_BORDER`).
    FixedSingle,
}

/// A read-only multiline `TextBox` with both scroll bars.
#[derive(Clone, Debug, PartialEq)]
pub struct EditSpec {
    /// Font.
    pub font: FontSpec,
    /// `ForeColor`.
    pub fore: Color,
    /// `BackColor`.
    pub back: Color,
    /// `BorderStyle`.
    pub border: EditBorder,
    /// `WordWrap` (true hides the horizontal scroll bar, like WinForms).
    pub word_wrap: bool,
}

impl EditSpec {
    /// Read-only, `ScrollBars.Both`, `WordWrap = false`, `BorderStyle.Fixed3D`.
    pub fn new(font: FontSpec, fore: Color, back: Color) -> Self {
        Self {
            font,
            fore,
            back,
            border: EditBorder::Fixed3D,
            word_wrap: false,
        }
    }

    /// Sets `BorderStyle`.
    pub fn border(mut self, border: EditBorder) -> Self {
        self.border = border;
        self
    }

    /// Sets `WordWrap = true`.
    pub fn word_wrap(mut self) -> Self {
        self.word_wrap = true;
        self
    }
}

/// A `CheckedListBox` with `CheckOnClick = true`, `IntegralHeight = false`.
#[derive(Clone, Debug, PartialEq)]
pub struct ListSpec {
    /// Font.
    pub font: FontSpec,
    /// Item text color.
    pub fore: Color,
    /// List back color.
    pub back: Color,
    /// Selected item back color (WinForms: `SystemColors.Highlight`).
    pub selected_back: Color,
    /// Selected item text color (WinForms: `SystemColors.HighlightText`).
    pub selected_fore: Color,
}

/// One native control.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    /// Flat owner-drawn button.
    Button(ButtonSpec),
    /// Painted label.
    Label(LabelSpec),
    /// Read-only multiline text box.
    Edit(EditSpec),
    /// Checked list box.
    CheckedList(ListSpec),
    /// `ProgressBar` with `ProgressBarStyle.Continuous`, range 0 to 100.
    Progress,
}

impl Ctl {
    /// The font this control uses.
    pub fn font(&self) -> FontSpec {
        match self {
            Ctl::Button(b) => b.font,
            Ctl::Label(l) => l.font,
            Ctl::Edit(e) => e.font,
            Ctl::CheckedList(l) => l.font,
            Ctl::Progress => theme::DEFAULT_FONT,
        }
    }

    /// WinForms default margin and size of this control kind.
    pub(crate) fn defaults(&self) -> (Pad, Size) {
        match self {
            Ctl::Button(_) => (theme::DEFAULT_MARGIN, theme::BUTTON_DEFAULT_SIZE),
            Ctl::Label(_) => (theme::LABEL_DEFAULT_MARGIN, theme::LABEL_DEFAULT_SIZE),
            Ctl::Edit(_) => (theme::DEFAULT_MARGIN, theme::TEXT_BOX_DEFAULT_SIZE),
            Ctl::CheckedList(_) => (theme::DEFAULT_MARGIN, theme::LIST_BOX_DEFAULT_SIZE),
            Ctl::Progress => (theme::DEFAULT_MARGIN, theme::PROGRESS_BAR_DEFAULT_SIZE),
        }
    }
}

/// What a control reports to its form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CtlEvent {
    /// Button click (mouse, Space, or a double click counted as a click).
    Click,
    /// Check box toggled: item index and new state.
    ItemCheck(usize, bool),
}

// ---------------------------------------------------------------------------------------------
// GDI helpers
// ---------------------------------------------------------------------------------------------

/// An owned solid brush.
#[derive(Debug)]
pub(crate) struct Brush(HBRUSH);

impl Brush {
    pub(crate) fn new(color: Color) -> Self {
        // SAFETY: Creates a new GDI object owned by the returned value.
        Self(unsafe { CreateSolidBrush(color.colorref()) })
    }

    pub(crate) fn handle(&self) -> HBRUSH {
        self.0
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        // SAFETY: The brush was created by CreateSolidBrush and is not selected into any DC.
        // A failed delete cannot be reported from Drop; it only leaks one GDI object.
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

/// The screen DC, released on drop (WinForms measures text on it too).
struct ScreenDc(HDC);

impl ScreenDc {
    fn new() -> Self {
        // SAFETY: GetDC(None) returns the screen DC; released in Drop.
        Self(unsafe { GetDC(None) })
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: Releases the DC obtained in `new`.
        unsafe {
            ReleaseDC(None, self.0);
        }
    }
}

/// Selects a font into a DC and restores the previous one on drop.
struct Select {
    hdc: HDC,
    old: HGDIOBJ,
}

impl Select {
    fn new(hdc: HDC, font: HFONT) -> Self {
        // SAFETY: Both handles are valid for the scope of this guard.
        let old = unsafe { SelectObject(hdc, font.into()) };
        Self { hdc, old }
    }
}

impl Drop for Select {
    fn drop(&mut self) {
        // SAFETY: Restores the object that was selected before `new`.
        unsafe {
            SelectObject(self.hdc, self.old);
        }
    }
}

/// An open visual-style theme, closed on drop.
struct Theme(HTHEME);

impl Drop for Theme {
    fn drop(&mut self) {
        // SAFETY: The handle came from OpenThemeData and is closed once.
        unsafe {
            let _ = CloseThemeData(self.0);
        }
    }
}

/// A paint DC whose update region is validated even if painting panics.
pub(crate) struct Paint {
    hwnd: HWND,
    ps: PAINTSTRUCT,
    hdc: HDC,
}

impl Paint {
    /// Begins painting inside this window's WM_PAINT handler.
    pub(crate) fn begin(hwnd: HWND) -> Self {
        let mut ps = PAINTSTRUCT::default();
        // SAFETY: Writable PAINTSTRUCT; the guard balances BeginPaint on this UI thread.
        let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
        Self { hwnd, ps, hdc }
    }

    /// Borrows the DC until the guard drops.
    pub(crate) fn hdc(&self) -> HDC {
        self.hdc
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        // SAFETY: Ends this guard's BeginPaint, also while unwinding a caught paint panic.
        unsafe {
            let _ = EndPaint(self.hwnd, &self.ps);
        }
    }
}

struct BufferedPaintThread(bool);

impl Drop for BufferedPaintThread {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Balances this thread's successful BufferedPaintInit once at thread exit.
            unsafe {
                let _ = BufferedPaintUnInit();
            }
        }
    }
}

thread_local! {
    static BUFFERED_PAINT: BufferedPaintThread = {
        // SAFETY: Initializes buffered painting only for the calling UI thread.
        BufferedPaintThread(unsafe { BufferedPaintInit() }.is_ok())
    };
}

struct PaintBuffer {
    handle: isize,
    update: bool,
}

impl Drop for PaintBuffer {
    fn drop(&mut self) {
        if self.handle != 0 {
            // SAFETY: Ends the buffer acquired by BeginBufferedPaint exactly once; a panic
            // discards the partial drawing and still releases its bitmap and DC.
            unsafe {
                let _ = EndBufferedPaint(self.handle, self.update);
            }
        }
    }
}

fn rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

fn fill(hdc: HDC, r: Rect, color: Color) {
    let brush = Brush::new(color);
    let rc = rect(r);
    // SAFETY: Valid DC, rectangle, and brush for the duration of the call.
    unsafe { FillRect(hdc, &rc, brush.handle()) };
}

/// `ControlPaint.DrawBorderSimple` / `HDC.DrawRectangle`: a 1-pixel frame inside `r`.
fn frame(hdc: HDC, r: Rect, color: Color) {
    border_with_size(hdc, r, color, 1);
}

/// `ButtonBaseAdapter.DrawFlatBorderWithSize`.
fn border_with_size(hdc: HDC, b: Rect, color: Color, size: i32) {
    let size = size.min(b.w.min(b.h));
    if size <= 0 {
        return;
    }
    fill(hdc, Rect { w: size, ..b }, color);
    fill(
        hdc,
        Rect {
            x: b.x + b.w - size,
            w: size,
            ..b
        },
        color,
    );
    fill(
        hdc,
        Rect {
            x: b.x + size,
            w: b.w - size * 2,
            h: size,
            ..b
        },
        color,
    );
    fill(
        hdc,
        Rect {
            x: b.x + size,
            y: b.y + b.h - size,
            w: b.w - size * 2,
            h: size,
        },
        color,
    );
}

fn text_height(hdc: HDC, font: HFONT) -> i32 {
    let _sel = Select::new(hdc, font);
    let mut tm = TEXTMETRICW::default();
    // SAFETY: `tm` is writable; the DC has the font selected.
    // A failure leaves tmHeight 0, which only removes the text margins.
    let _ = unsafe { GetTextMetricsW(hdc, &mut tm) };
    tm.tmHeight
}

/// `TextExtensions.GetTextMargins(GlyphOverhangPadding)`.
fn text_margins(hdc: HDC, font: HFONT) -> DRAWTEXTPARAMS {
    let overhang = text_height(hdc, font) as f32 / 6.0;
    DRAWTEXTPARAMS {
        cbSize: std::mem::size_of::<DRAWTEXTPARAMS>() as u32,
        iLeftMargin: overhang.ceil() as i32,
        iRightMargin: (overhang * 1.5).ceil() as i32,
        ..Default::default()
    }
}

/// `TextRenderer.MeasureText`.
fn measure_text(
    hdc: HDC,
    text: &str,
    font: HFONT,
    proposed: Size,
    flags: DRAW_TEXT_FORMAT,
) -> Size {
    if text.is_empty() {
        return Size::default();
    }
    let params = text_margins(hdc, font);
    let mut proposed = proposed;
    let min_width = 1 + params.iLeftMargin + params.iRightMargin;
    if proposed.w <= min_width {
        proposed.w = min_width;
    }
    if proposed.h <= 0 {
        proposed.h = 1;
    }
    let mut flags = flags;
    if proposed.h == UNBOUNDED && flags.contains(DT_SINGLELINE) {
        flags &= !(DT_BOTTOM | DT_VCENTER);
    }
    if proposed.w == UNBOUNDED {
        flags &= !DT_WORDBREAK;
    }
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: proposed.w,
        bottom: proposed.h,
    };
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let _sel = Select::new(hdc, font);
    // SAFETY: `buf` and `rc` are valid for the call; DT_MODIFYSTRING is never passed.
    unsafe { DrawTextExW(hdc, &mut buf, &mut rc, flags | DT_CALCRECT, Some(&params)) };
    Size {
        w: rc.right - rc.left,
        h: rc.bottom - rc.top,
    }
}

/// `TextRenderer.DrawText` (transparent background).
fn draw_text(
    hdc: HDC,
    text: &str,
    font: HFONT,
    bounds: Rect,
    color: Color,
    flags: DRAW_TEXT_FORMAT,
) {
    if text.is_empty() {
        return;
    }
    let params = text_margins(hdc, font);
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let _sel = Select::new(hdc, font);
    let mut bounds = bounds;
    // TextExtensions.AdjustForVerticalAlignment: GDI does not center multi-line layouts.
    if (flags.contains(DT_VCENTER) || flags.contains(DT_BOTTOM)) && !flags.contains(DT_SINGLELINE) {
        let mut rc = rect(bounds);
        // SAFETY: `buf` and `rc` are valid for the call.
        let text_h =
            unsafe { DrawTextExW(hdc, &mut buf, &mut rc, flags | DT_CALCRECT, Some(&params)) };
        if text_h <= bounds.h {
            bounds.y = if flags.contains(DT_VCENTER) {
                bounds.y + bounds.h / 2 - text_h / 2
            } else {
                bounds.bottom() - text_h
            };
        }
    }
    let mut rc = rect(bounds);
    // SAFETY: Plain DC state changes and a draw call with valid buffers.
    unsafe {
        SetTextColor(hdc, color.colorref());
        SetBkMode(hdc, TRANSPARENT);
        DrawTextExW(hdc, &mut buf, &mut rc, flags, Some(&params));
    }
}

/// Paints into an off-screen buffer and copies it to `hdc` in one step (no flicker).
fn buffered(hdc: HDC, area: Rect, paint: impl FnOnce(HDC)) {
    BUFFERED_PAINT.with(|_| {});
    let rc = rect(area);
    let mut mem = HDC::default();
    // SAFETY: `rc` and `mem` are valid; on failure the handle is 0 and we paint directly.
    let mut pb = PaintBuffer {
        // SAFETY: `rc` and `mem` live through the call; the guard owns any returned buffer.
        handle: unsafe { BeginBufferedPaint(hdc, &rc, BPBF_COMPATIBLEBITMAP, None, &mut mem) },
        update: false,
    };
    if pb.handle == 0 || mem.is_invalid() {
        paint(hdc);
        return;
    }
    paint(mem);
    pb.update = true;
}

fn client(hwnd: HWND) -> Rect {
    let mut rc = RECT::default();
    // SAFETY: `rc` is writable; an invalid window leaves it empty.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    Rect {
        x: 0,
        y: 0,
        w: rc.right - rc.left,
        h: rc.bottom - rc.top,
    }
}

fn invalidate(hwnd: HWND) {
    // SAFETY: Invalidating a (possibly destroyed) window has no memory effects.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn send(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> LRESULT {
    // SAFETY: Callers pass messages whose parameters are plain values or point to live data.
    unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

fn align_flags(align: Align) -> DRAW_TEXT_FORMAT {
    match align {
        Align::MiddleLeft => DT_VCENTER | DT_LEFT,
        Align::MiddleCenter => DT_VCENTER | DT_CENTER,
    }
}

/// `LayoutUtils.Align(Size, Rectangle, ContentAlignment)`.
fn align_in(size: Size, within: Rect, align: Align) -> Rect {
    let x = match align {
        Align::MiddleLeft => within.x,
        Align::MiddleCenter => within.x + (within.w - size.w) / 2,
    };
    Rect {
        x,
        y: within.y + (within.h - size.h) / 2,
        w: size.w,
        h: size.h,
    }
}

// ---------------------------------------------------------------------------------------------
// WinForms color math (ControlPaint.HLSColor, ButtonBaseAdapter.ColorOptions)
// ---------------------------------------------------------------------------------------------

const HLS_MAX: i32 = 240;
const RGB_MAX: i32 = 255;
const UNDEFINED_HUE: i32 = HLS_MAX * 2 / 3;

struct Hls {
    hue: i32,
    lum: i32,
    sat: i32,
}

fn hls(c: Color) -> Hls {
    let (r, g, b) = (i32::from(c.r), i32::from(c.g), i32::from(c.b));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let sum = max + min;
    let lum = (sum * HLS_MAX + RGB_MAX) / (2 * RGB_MAX);
    let dif = max - min;
    if dif == 0 {
        return Hls {
            hue: UNDEFINED_HUE,
            lum,
            sat: 0,
        };
    }
    let sat = if lum <= HLS_MAX / 2 {
        (dif * HLS_MAX + sum / 2) / sum
    } else {
        (dif * HLS_MAX + (2 * RGB_MAX - sum) / 2) / (2 * RGB_MAX - sum)
    };
    let delta = |v: i32| ((max - v) * (HLS_MAX / 6) + dif / 2) / dif;
    let (rd, gd, bd) = (delta(r), delta(g), delta(b));
    let mut hue = if r == max {
        bd - gd
    } else if g == max {
        HLS_MAX / 3 + rd - bd
    } else {
        2 * HLS_MAX / 3 + gd - rd
    };
    if hue < 0 {
        hue += HLS_MAX;
    }
    if hue > HLS_MAX {
        hue -= HLS_MAX;
    }
    Hls { hue, lum, sat }
}

fn hue_to_rgb(n1: i32, n2: i32, hue: i32) -> i32 {
    let hue = if hue < 0 {
        hue + HLS_MAX
    } else if hue > HLS_MAX {
        hue - HLS_MAX
    } else {
        hue
    };
    if hue < HLS_MAX / 6 {
        n1 + ((n2 - n1) * hue + HLS_MAX / 12) / (HLS_MAX / 6)
    } else if hue < HLS_MAX / 2 {
        n2
    } else if hue < HLS_MAX * 2 / 3 {
        n1 + ((n2 - n1) * (HLS_MAX * 2 / 3 - hue) + HLS_MAX / 12) / (HLS_MAX / 6)
    } else {
        n1
    }
}

fn from_hls(hue: i32, lum: i32, sat: i32) -> Color {
    if sat == 0 {
        let v = (lum * RGB_MAX / HLS_MAX) as u8;
        return Color::rgb(v, v, v);
    }
    let m2 = if lum <= HLS_MAX / 2 {
        (lum * (HLS_MAX + sat) + HLS_MAX / 2) / HLS_MAX
    } else {
        lum + sat - (lum * sat + HLS_MAX / 2) / HLS_MAX
    };
    let m1 = 2 * lum - m2;
    let ch = |h: i32| ((hue_to_rgb(m1, m2, h) * RGB_MAX + HLS_MAX / 2) / HLS_MAX) as u8;
    Color::rgb(ch(hue + HLS_MAX / 3), ch(hue), ch(hue - HLS_MAX / 3))
}

/// `ControlPaint.Dark` (`Darker(0.5)`).
fn dark(c: Color) -> Color {
    let h = hls(c);
    let zero = h.lum * (-333 + 1000) / 1000;
    from_hls(h.hue, zero - (zero as f32 * 0.5) as i32, h.sat)
}

/// `ControlPaint.LightLight` (`Lighter(1.0)`).
fn light_light(c: Color) -> Color {
    let h = hls(c);
    let one = ((i64::from(h.lum) * (1000 - 500) + i64::from(HLS_MAX + 1) * 500) / 1000) as i32;
    from_hls(h.hue, one, h.sat)
}

/// `Color.GetBrightness`.
fn brightness(c: Color) -> f32 {
    let max = c.r.max(c.g).max(c.b) as f32;
    let min = c.r.min(c.g).min(c.b) as f32;
    (max + min) / (255.0 * 2.0)
}

fn adjust(c: Color, factor: f32) -> Color {
    let a = |v: u8| ((factor * f32::from(v)) as i32).min(255) as u8;
    Color::rgb(a(c.r), a(c.g), a(c.b))
}

/// `ColorData.LowButtonFace`.
fn low_button_face(face: Color) -> Color {
    adjust(face, if brightness(face) < 0.5 { 1.2 } else { 0.9 })
}

/// `ColorData.LowHighlight`.
fn low_highlight(face: Color) -> Color {
    let hi = light_light(face);
    adjust(hi, if brightness(hi) < 0.5 { 1.2 } else { 0.9 })
}

// ---------------------------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------------------------

struct ButtonData {
    spec: RefCell<ButtonSpec>,
    back: Cell<Color>,
    fore: Cell<Color>,
    hover: Cell<bool>,
    is_default: Cell<bool>,
}

struct EditData {
    spec: EditSpec,
    brush: Brush,
}

struct ListData {
    spec: ListSpec,
    brush: Brush,
    checked: RefCell<Vec<bool>>,
    kill_next_select: Cell<bool>,
}

enum Data {
    Button(ButtonData),
    Label(RefCell<LabelSpec>),
    Edit(EditData),
    List(ListData),
    Progress,
}

/// Per-control state, owned by the control's subclass (dropped on `WM_NCDESTROY`).
pub(crate) struct CtlState {
    id: u16,
    hwnd: HWND,
    font: Cell<HFONT>,
    dpi: Cell<u32>,
    padding: Cell<Pad>,
    back: Cell<Color>,
    data: Data,
}

/// Returns the state of a kit control, or `None` for any other window.
pub(crate) fn state_of(hwnd: HWND) -> Option<Rc<CtlState>> {
    // Raw HWNDs can be reconstructed safely on a worker. Only the owning thread may clone
    // this non-atomic Rc; otherwise WM_NCDESTROY could free it between GetPropW and increment.
    if !super::window::on_window_thread(hwnd) {
        return None;
    }
    // The state lives in a window property, not in GetWindowSubclass: comctl32 v5 (loaded by
    // unmanifested test binaries) exports GetWindowSubclass only by ordinal.
    // SAFETY: Reads a property of a window handle; other windows do not carry this name.
    let data = unsafe { GetPropW(hwnd, STATE_PROP) };
    if data.is_invalid() {
        return None;
    }
    let ptr = data.0 as *const CtlState;
    // SAFETY: The property holds Rc::into_raw from `create` until WM_NCDESTROY removes it;
    // the extra strong count keeps the state alive for the caller.
    unsafe {
        Rc::increment_strong_count(ptr);
        Some(Rc::from_raw(ptr))
    }
}

/// Creates the native control for a leaf node and attaches its state.
pub(crate) fn create(
    parent: HWND,
    node: &Node,
    back: Color,
    font: HFONT,
    dpi: u32,
) -> win::Result<HWND> {
    let Kind::Leaf(ctl) = &node.kind else {
        return Err(win::Error::msg("controls::create", "not a leaf node"));
    };
    let (id, padding) = (node.id, node.padding);
    let vis = if node.visible {
        WS_VISIBLE
    } else {
        WINDOW_STYLE(0)
    };
    let base = WS_CHILD | WS_CLIPSIBLINGS | vis;
    let (class, text, style, ex, data) = match ctl {
        Ctl::Button(b) => (
            WC_BUTTONW,
            b.text.as_str(),
            base | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            WINDOW_EX_STYLE(0),
            Data::Button(ButtonData {
                spec: RefCell::new(b.clone()),
                back: Cell::new(b.back),
                fore: Cell::new(b.fore),
                hover: Cell::new(false),
                is_default: Cell::new(false),
            }),
        ),
        Ctl::Label(l) => (
            WC_STATICW,
            l.text.as_str(),
            base,
            WINDOW_EX_STYLE(0),
            Data::Label(RefCell::new(l.clone())),
        ),
        Ctl::Edit(e) => {
            let mut style = base
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32);
            if !e.word_wrap {
                style |= WS_HSCROLL | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
            }
            let ex = match e.border {
                EditBorder::Fixed3D => WS_EX_CLIENTEDGE,
                EditBorder::FixedSingle => {
                    style |= WS_BORDER;
                    WINDOW_EX_STYLE(0)
                }
            };
            (
                WC_EDITW,
                "",
                style,
                ex,
                Data::Edit(EditData {
                    spec: e.clone(),
                    brush: Brush::new(e.back),
                }),
            )
        }
        Ctl::CheckedList(l) => (
            WC_LISTBOXW,
            "",
            base | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE(
                    (LBS_OWNERDRAWFIXED
                        | LBS_HASSTRINGS
                        | LBS_NOTIFY
                        | LBS_NOINTEGRALHEIGHT
                        | LBS_WANTKEYBOARDINPUT) as u32,
                ),
            WS_EX_CLIENTEDGE,
            Data::List(ListData {
                spec: l.clone(),
                brush: Brush::new(l.back),
                checked: RefCell::new(Vec::new()),
                kill_next_select: Cell::new(false),
            }),
        ),
        Ctl::Progress => (
            PROGRESS_CLASSW,
            "",
            base | WINDOW_STYLE(PBS_SMOOTH),
            WINDOW_EX_STYLE(0),
            Data::Progress,
        ),
    };
    let text = to_wide(text);
    // SAFETY: Class names are static, the text buffer is NUL-terminated and outlives the call,
    // and `parent` is a live window of this thread. The id travels in the HMENU slot.
    let hwnd = unsafe {
        CreateWindowExW(
            ex,
            class,
            PCWSTR(text.as_ptr()),
            style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(usize::from(id) as *mut core::ffi::c_void)),
            None,
            None,
        )
    }
    .map_err(|e| win::Error::from_win("CreateWindowExW", e))?;
    let state = Rc::new(CtlState {
        id,
        hwnd,
        font: Cell::new(font),
        dpi: Cell::new(dpi),
        padding: Cell::new(padding),
        back: Cell::new(back),
        data,
    });
    let raw = Rc::into_raw(state);
    // SAFETY: The property owns `raw` until WM_NCDESTROY reclaims it in `subclass_proc`.
    if unsafe {
        SetPropW(
            hwnd,
            STATE_PROP,
            Some(HANDLE(raw as *mut core::ffi::c_void)),
        )
    }
    .is_err()
    {
        // SAFETY: The property was not set, so this is the only owner of `raw`.
        drop(unsafe { Rc::from_raw(raw) });
        return Err(win::Error::last("SetPropW"));
    }
    // SAFETY: Installs our subclass on our own new window; the state is found via the property.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) };
    if !ok.as_bool() {
        // The window still owns the property; WM_NCDESTROY cannot reclaim it without the
        // subclass, so reclaim it here.
        // SAFETY: Removes the property we just set and takes back its only reference.
        unsafe {
            if let Ok(h) = RemovePropW(hwnd, STATE_PROP) {
                drop(Rc::from_raw(h.0 as *const CtlState));
            }
        }
        return Err(win::Error::last("SetWindowSubclass"));
    }
    send(hwnd, WM_SETFONT, font.0 as usize, 0);
    match ctl {
        Ctl::Edit(_) => {
            send(hwnd, EM_SETLIMITTEXT, 0, 0);
        }
        Ctl::CheckedList(_) => {
            if let Some(st) = state_of(hwnd) {
                st.update_item_height();
            }
        }
        Ctl::Progress => {
            send(hwnd, PBM_SETRANGE32, 0, 100);
        }
        _ => {}
    }
    Ok(hwnd)
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    catch_unwind(AssertUnwindSafe(|| {
        if msg == WM_NCDESTROY {
            // SAFETY: Removes our own subclass and property from our own window inside its window
            // procedure, then reclaims the reference stored in `create` exactly once. Live callers
            // hold their own strong counts.
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                if let Ok(h) = RemovePropW(hwnd, STATE_PROP)
                    && !h.is_invalid()
                {
                    drop(Rc::from_raw(h.0 as *const CtlState));
                }
            }
            // SAFETY: Forwards to the next procedure in the chain with unchanged parameters.
            return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        }
        let Some(state) = state_of(hwnd) else {
            // SAFETY: Forwards with unchanged parameters.
            return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        };
        state
            .message(msg, wparam, lparam)
            .unwrap_or_else(|| def(hwnd, msg, wparam, lparam))
    }))
    .unwrap_or_else(|_| def(hwnd, msg, wparam, lparam))
}

fn def(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Forwards to the next subclass procedure with unchanged parameters.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn ctrl_down() -> bool {
    // SAFETY: Reads the calling thread's key state.
    unsafe { GetKeyState(i32::from(VK_CONTROL.0)) < 0 }
}

impl CtlState {
    /// Handles one message for the control; `None` = default processing.
    fn message(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        let hwnd = self.hwnd;
        match (&self.data, msg) {
            (_, WM_SETFOCUS) => {
                super::window::focus_changed(hwnd);
                None
            }
            (Data::Button(_) | Data::Label(_), WM_ERASEBKGND) => Some(LRESULT(1)),
            (Data::Label(_), WM_PAINT) => {
                self.paint_label();
                Some(LRESULT(0))
            }
            (Data::Button(b), WM_MOUSEMOVE) => {
                if !b.hover.get() {
                    b.hover.set(true);
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: `tme` is a valid, initialized structure for this window.
                    unsafe {
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    self.on_mouse_enter(b);
                }
                None
            }
            (Data::Button(b), WM_MOUSELEAVE) => {
                b.hover.set(false);
                self.on_mouse_leave(b);
                None
            }
            (Data::Button(b), WM_LBUTTONDOWN | WM_LBUTTONDBLCLK) => {
                // C# parity: Buttons.cs MouseDown (WinForms has no separate double-click down).
                if let Hover::Shared { pressed, .. } = b.spec.borrow().hover
                    && self.enabled()
                {
                    b.back.set(pressed);
                }
                invalidate(hwnd);
                None
            }
            (Data::Button(b), WM_LBUTTONUP) => {
                // C# parity: Button.OnMouseUp raises Click before the MouseUp handler runs.
                let r = def(hwnd, msg, wparam, lparam);
                if let Hover::Shared { hover, .. } = b.spec.borrow().hover
                    && self.enabled()
                {
                    b.back.set(hover);
                }
                invalidate(hwnd);
                Some(r)
            }
            (Data::Button(_), WM_SETCURSOR) => {
                if (lparam.0 & 0xFFFF) as u32 == HTCLIENT {
                    // SAFETY: IDC_HAND is a system cursor; no ownership is transferred.
                    unsafe {
                        if let Ok(cursor) = LoadCursorW(None, IDC_HAND) {
                            SetCursor(Some(cursor));
                        }
                    }
                    return Some(LRESULT(1));
                }
                None
            }
            (Data::Button(_), WM_UPDATEUISTATE) => {
                let r = def(hwnd, msg, wparam, lparam);
                invalidate(hwnd);
                Some(r)
            }
            (Data::Edit(_), WM_GETDLGCODE) => {
                // C# parity: a multiline TextBox with AcceptsTab/AcceptsReturn false lets Tab,
                // Enter, and Esc go to the form.
                let r = def(hwnd, msg, wparam, lparam);
                Some(LRESULT(r.0 & !(DLGC_WANTALLKEYS as isize)))
            }
            (Data::Edit(_), WM_KEYDOWN) if wparam.0 == usize::from(b'A') && ctrl_down() => {
                // C# parity: TextBox handles Ctrl+A as select all.
                send(hwnd, EM_SETSEL, 0, -1);
                Some(LRESULT(0))
            }
            (Data::Edit(_), WM_CHAR) if wparam.0 == 1 => Some(LRESULT(0)),
            (Data::List(l), WM_LBUTTONDOWN) => {
                l.kill_next_select.set(false);
                None
            }
            (Data::List(_), WM_CHAR) if wparam.0 == usize::from(b' ') => {
                // C# parity: CheckedListBox.OnKeyPress toggles the selected item on Space.
                if let Some((index, checked)) = self.list_sel_change() {
                    self.notify(CtlEvent::ItemCheck(index, checked));
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn enabled(&self) -> bool {
        // SAFETY: Reads window state of our own control.
        unsafe { IsWindowEnabled(self.hwnd).as_bool() }
    }

    fn on_mouse_enter(&self, b: &ButtonData) {
        match b.spec.borrow().hover {
            Hover::Shared { hover, .. } if self.enabled() => b.back.set(hover),
            Hover::EnterLeave { hover, .. } => b.back.set(hover),
            _ => {}
        }
        invalidate(self.hwnd);
    }

    fn on_mouse_leave(&self, b: &ButtonData) {
        match b.spec.borrow().hover {
            Hover::Shared { normal, .. } if self.enabled() => b.back.set(normal),
            Hover::EnterLeave { normal, .. } => b.back.set(normal),
            _ => {}
        }
        invalidate(self.hwnd);
    }

    /// Sends a control event to the form (`window.rs` turns it into `Event`).
    fn notify(&self, event: CtlEvent) {
        super::window::control_event(self.hwnd, self.id, event);
    }

    fn ui_state(&self) -> (bool, bool) {
        let s = send(self.hwnd, WM_QUERYUISTATE, 0, 0).0 as u32;
        (s & UISF_HIDEFOCUS == 0, s & UISF_HIDEACCEL == 0)
    }

    // -- button ----------------------------------------------------------------------------

    /// `ButtonFlatAdapter.PaintUp/PaintOver/PaintDown` for the current state.
    fn paint_button(&self, b: &ButtonData, hdc: HDC, area: Rect) {
        let spec = b.spec.borrow();
        let enabled = self.enabled();
        let pushed = send(self.hwnd, BM_GETSTATE, 0, 0).0 & BST_PUSHED != 0;
        // SAFETY: Reads the focus window of this thread.
        let focused = unsafe { GetFocus() } == self.hwnd;
        let (show_focus, show_accel) = self.ui_state();
        let back = b.back.get();
        let bg = if !enabled {
            back
        } else if pushed {
            spec.down_back.unwrap_or_else(|| low_highlight(back))
        } else if b.hover.get() {
            spec.over_back.unwrap_or_else(|| low_button_face(back))
        } else {
            back
        };
        // C# parity: ColorOptions.Calculate paints disabled text in ControlPaint.Dark(BackColor),
        // ignoring ForeColor (so ThemeColors.DisabledText never shows).
        let text_color = if enabled { b.fore.get() } else { dark(back) };
        let contrast_shadow = if brightness(back) < 0.5 {
            low_highlight(back)
        } else {
            dark(back)
        };
        let bs = spec.border_size;
        let is_default = b.is_default.get();
        let client_rect = area;
        buffered(hdc, area, |hdc| {
            fill(
                hdc,
                Rect {
                    x: client_rect.x + bs,
                    y: client_rect.y + bs,
                    w: client_rect.w - 2 * bs,
                    h: client_rect.h - 2 * bs,
                },
                bg,
            );
            // Text layout: Client = client - Padding; Face = Client - border; Field = Face - 2.
            let pad = self.padding.get();
            let field = client_rect.deflate(pad).deflate(Pad {
                l: bs + 2,
                t: bs + 2,
                r: bs + 2,
                b: bs + 2,
            });
            let mut max_bounds = field.deflate(Pad {
                l: TEXT_IMAGE_INSET,
                t: TEXT_IMAGE_INSET,
                r: TEXT_IMAGE_INSET,
                b: TEXT_IMAGE_INSET,
            });
            if is_default {
                max_bounds = max_bounds.deflate(Pad {
                    l: -1,
                    t: -1,
                    r: -1,
                    b: -1,
                });
            }
            let mut flags = align_flags(spec.align) | DT_WORDBREAK | DT_EDITCONTROL;
            if !show_accel {
                flags |= DT_HIDEPREFIX;
            }
            let font = self.font.get();
            let size = measure_text(hdc, &spec.text, font, max_bounds.size(), flags);
            let text_bounds = align_in(size, max_bounds, spec.align);
            draw_text(hdc, &spec.text, font, text_bounds, text_color, flags);
            if focused && show_focus {
                let focus = field.deflate(Pad {
                    l: 1,
                    t: 1,
                    r: 1,
                    b: 1,
                });
                let focus = Rect {
                    x: focus.x - pad.l,
                    y: focus.y - pad.t,
                    w: focus.w + pad.horizontal(),
                    h: focus.h + pad.vertical(),
                };
                frame(hdc, focus, contrast_shadow);
            }
            let mut r = client_rect;
            if is_default {
                // ButtonBaseAdapter.DrawDefaultBorder: one extra ring at the client edge.
                frame(hdc, r, spec.border);
                r = r.deflate(Pad {
                    l: 1,
                    t: 1,
                    r: 1,
                    b: 1,
                });
            }
            if bs == 1 {
                frame(hdc, r, spec.border);
            } else {
                border_with_size(hdc, r, spec.border, bs);
            }
        });
    }

    // -- label -----------------------------------------------------------------------------

    fn label_flags(&self, spec: &LabelSpec, hdc: HDC, constraining: Size) -> DRAW_TEXT_FORMAT {
        let mut flags = align_flags(spec.align) | DT_WORDBREAK | DT_EDITCONTROL | DT_NOPREFIX;
        if spec.ellipsis {
            flags |= DT_END_ELLIPSIS;
        }
        // C# parity: Label.CreateTextFormatFlags drops WordBreak when the text fits one line.
        let one_line = measure_text(
            hdc,
            &spec.text,
            self.font.get(),
            Size {
                w: UNBOUNDED,
                h: UNBOUNDED,
            },
            flags,
        );
        if one_line.w <= constraining.w {
            flags &= !(DT_WORDBREAK | DT_EDITCONTROL);
        }
        flags
    }

    fn paint_label(&self) {
        let Data::Label(spec) = &self.data else {
            return;
        };
        let spec = spec.borrow();
        let paint = Paint::begin(self.hwnd);
        let hdc = paint.hdc();
        let area = client(self.hwnd);
        let face = area.deflate(self.padding.get());
        let back = self.back.get();
        buffered(hdc, area, |hdc| {
            fill(hdc, area, back);
            let flags = self.label_flags(&spec, hdc, face.size());
            draw_text(hdc, &spec.text, self.font.get(), face, spec.fore, flags);
        });
    }

    // -- checked list ----------------------------------------------------------------------

    fn update_item_height(&self) {
        let Data::List(_) = &self.data else {
            return;
        };
        let dc = ScreenDc::new();
        let font_h = text_height(dc.0, self.font.get());
        let dpi = self.dpi.get();
        let glyph = dpi::scale(theme::CHECK_GLYPH, dpi);
        let h = (font_h + 2 * dpi::scale(theme::LIST_ITEM_BORDER, dpi)).max(glyph + 2);
        send(self.hwnd, LB_SETITEMHEIGHT, 0, h as isize);
    }

    /// `CheckedListBox.LbnSelChange`: toggles the selected item; returns it and its new state.
    fn list_sel_change(&self) -> Option<(usize, bool)> {
        let Data::List(l) = &self.data else {
            return None;
        };
        let index = send(self.hwnd, LB_GETCURSEL, 0, 0).0 as i32;
        let count = l.checked.borrow().len() as i32;
        if index < 0 || index >= count {
            return None;
        }
        let mut result = None;
        if !l.kill_next_select.get() {
            let mut checked = l.checked.borrow_mut();
            let i = index as usize;
            checked[i] = !checked[i];
            result = Some((i, checked[i]));
        }
        invalidate(self.hwnd);
        result
    }

    /// `CheckedListBox.OnDrawItem`.
    fn draw_list_item(&self, l: &ListData, dis: &DRAWITEMSTRUCT) {
        let index = dis.itemID as i32;
        let bounds = Rect {
            x: dis.rcItem.left,
            y: dis.rcItem.top,
            w: dis.rcItem.right - dis.rcItem.left,
            h: dis.rcItem.bottom - dis.rcItem.top,
        };
        let checked = l.checked.borrow();
        if index < 0 || index as usize >= checked.len() {
            fill(dis.hDC, bounds, l.spec.back);
            return;
        }
        let is_checked = checked[index as usize];
        let dpi = self.dpi.get();
        let start = dpi::scale(theme::LIST_ITEM_START, dpi);
        let glyph = dpi::scale(theme::CHECK_GLYPH, dpi);
        let selected = dis.itemState.0 & ODS_SELECTED.0 != 0;
        let (back, fore) = if selected {
            (l.spec.selected_back, l.spec.selected_fore)
        } else {
            (l.spec.back, l.spec.fore)
        };
        let len = send(self.hwnd, LB_GETTEXTLEN, dis.itemID as usize, 0).0;
        let mut buf = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
        send(
            self.hwnd,
            LB_GETTEXT,
            dis.itemID as usize,
            buf.as_mut_ptr() as isize,
        );
        let text = from_wide(&buf);
        let focus = dis.itemState.0 & ODS_FOCUS.0 != 0 && dis.itemState.0 & ODS_NOFOCUSRECT.0 == 0;
        let font = self.font.get();
        let hwnd = self.hwnd;
        buffered(dis.hDC, bounds, |hdc| {
            fill(hdc, bounds, l.spec.back);
            let mut centering = ((bounds.h - glyph) / 2).max(0);
            if centering + glyph > bounds.h {
                centering = bounds.h - glyph;
            }
            let box_rect = Rect {
                x: bounds.x + start,
                y: bounds.y + centering,
                w: glyph,
                h: glyph,
            };
            draw_check(hwnd, hdc, box_rect, is_checked);
            let text_bounds = Rect {
                x: bounds.x + glyph + start * 2,
                y: bounds.y,
                w: bounds.w - (glyph + start * 2),
                h: bounds.h,
            };
            fill(hdc, text_bounds, back);
            let string_bounds = Rect {
                x: text_bounds.x + 1,
                y: text_bounds.y,
                w: text_bounds.w - 1,
                h: text_bounds.h - 2,
            };
            draw_text(hdc, &text, font, string_bounds, fore, DT_NOPREFIX);
            if focus {
                let rc = rect(text_bounds);
                // SAFETY: Valid DC and rectangle.
                unsafe {
                    let _ = DrawFocusRect(hdc, &rc);
                }
            }
        });
    }

    /// Handles a message reflected from the parent; `None` = not ours.
    fn reflected(
        &self,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<(LRESULT, Option<CtlEvent>)> {
        match (&self.data, msg) {
            (Data::Button(b), WM_DRAWITEM) => {
                // SAFETY: For WM_DRAWITEM, lParam points to a DRAWITEMSTRUCT valid during the call.
                let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                let area = Rect {
                    x: dis.rcItem.left,
                    y: dis.rcItem.top,
                    w: dis.rcItem.right - dis.rcItem.left,
                    h: dis.rcItem.bottom - dis.rcItem.top,
                };
                self.paint_button(b, dis.hDC, area);
                Some((LRESULT(1), None))
            }
            (Data::List(l), WM_DRAWITEM) => {
                // SAFETY: For WM_DRAWITEM, lParam points to a DRAWITEMSTRUCT valid during the call.
                let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                self.draw_list_item(l, dis);
                Some((LRESULT(1), None))
            }
            (Data::Edit(e), WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT) => {
                let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
                // SAFETY: The DC belongs to the control for this message.
                unsafe {
                    SetTextColor(hdc, e.spec.fore.colorref());
                    SetBkColor(hdc, e.spec.back.colorref());
                }
                Some((LRESULT(e.brush.handle().0 as isize), None))
            }
            (Data::List(l), WM_CTLCOLORLISTBOX) => {
                let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
                // SAFETY: The DC belongs to the control for this message.
                unsafe {
                    SetTextColor(hdc, l.spec.fore.colorref());
                    SetBkColor(hdc, l.spec.back.colorref());
                }
                Some((LRESULT(l.brush.handle().0 as isize), None))
            }
            (Data::Label(_), WM_CTLCOLORSTATIC) => Some((LRESULT(0), None)),
            (Data::Button(_), WM_COMMAND_ID) => {
                let code = (wparam.0 >> 16) as u32;
                // C# parity: WinForms raises Click on every mouse up, so the BN_DBLCLK of an
                // owner-drawn button is a click too (two fast clicks = two clicks).
                if code == BN_CLICKED || code == BN_DBLCLK {
                    Some((LRESULT(0), Some(CtlEvent::Click)))
                } else {
                    Some((LRESULT(0), None))
                }
            }
            (Data::List(_), WM_COMMAND_ID) => {
                let code = (wparam.0 >> 16) as u32;
                if code == LBN_SELCHANGE || code == LBN_DBLCLK {
                    let ev = self
                        .list_sel_change()
                        .map(|(i, c)| CtlEvent::ItemCheck(i, c));
                    Some((LRESULT(0), ev))
                } else {
                    Some((LRESULT(0), None))
                }
            }
            (Data::List(l), WM_VKEYTOITEM) => {
                let vk = (wparam.0 & 0xFFFF) as u16;
                let nav = [
                    VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_HOME, VK_END, VK_LEFT, VK_RIGHT,
                ];
                l.kill_next_select.set(nav.iter().any(|k| k.0 == vk));
                Some((LRESULT(-1), None))
            }
            _ => None,
        }
    }
}

const WM_COMMAND_ID: u32 = windows::Win32::UI::WindowsAndMessaging::WM_COMMAND;

fn draw_check(hwnd: HWND, hdc: HDC, r: Rect, checked: bool) {
    // SAFETY: Opens the BUTTON theme of our own window; closed by the guard.
    let theme = Theme(unsafe { OpenThemeData(Some(hwnd), w!("BUTTON")) });
    let mut rc = rect(r);
    if !theme.0.is_invalid() {
        let state = if checked {
            CBS_CHECKEDNORMAL
        } else {
            CBS_UNCHECKEDNORMAL
        };
        // SAFETY: Valid theme, DC, and rectangle.
        if unsafe { DrawThemeBackground(theme.0, hdc, BP_CHECKBOX, state, &rc, None) }.is_ok() {
            return;
        }
    }
    let mut state = DFCS_BUTTONCHECK | DFCS_FLAT;
    if checked {
        state |= DFCS_CHECKED;
    }
    // SAFETY: Valid DC and writable rectangle.
    unsafe {
        let _ = DrawFrameControl(hdc, &mut rc, DFC_BUTTON, state);
    }
}

/// Result of [`reflect`]: the message result and an optional (id, control, event) for the form.
pub(crate) type Reflected = (LRESULT, Option<(u16, HWND, CtlEvent)>);

/// Routes a message a container received from a child control back to that control.
///
/// Returns the result to give the container's caller and an optional event for the form.
pub(crate) fn reflect(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<Reflected> {
    let child = match msg {
        WM_DRAWITEM => {
            if lparam.0 == 0 {
                return None;
            }
            // SAFETY: For WM_DRAWITEM, lParam points to a DRAWITEMSTRUCT valid during the call.
            unsafe { (*(lparam.0 as *const DRAWITEMSTRUCT)).hwndItem }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_COMMAND_ID
        | WM_VKEYTOITEM => HWND(lparam.0 as *mut core::ffi::c_void),
        _ => return None,
    };
    if child.is_invalid() {
        return None;
    }
    let state = state_of(child)?;
    let (r, ev) = state.reflected(msg, wparam, lparam)?;
    Some((r, ev.map(|e| (state.id, child, e))))
}

// ---------------------------------------------------------------------------------------------
// Measuring (for AutoSize leaves)
// ---------------------------------------------------------------------------------------------

/// Preferred size of an auto-sized leaf (`GetPreferredSizeCore`), before min/max clamping.
pub(crate) fn measure(ctl: &Ctl, font: HFONT, padding: Pad, min: Size, proposed: Size) -> Size {
    let dc = ScreenDc::new();
    match ctl {
        Ctl::Button(b) => {
            // ButtonFlatAdapter.PaintFlatLayout(up: false, check: true): border + 1, padding 1,
            // plus 2 for GrowBorderBy1PxWhenDefault.
            let linear = (b.border_size + 1) * 2 + 2 + 2;
            let inset = TEXT_IMAGE_INSET * 2;
            // Prefix processing hides `&` with or without cues, so the width is the same.
            let flags = align_flags(b.align) | DT_EDITCONTROL | DT_HIDEPREFIX;
            let avail = Size {
                w: proposed.w.saturating_sub(linear + inset),
                h: proposed.h.saturating_sub(linear + inset),
            };
            let text = measure_text(dc.0, &b.text, font, avail, flags);
            let text = if b.text.is_empty() {
                Size::default()
            } else {
                Size {
                    w: text.w + inset,
                    h: text.h + inset,
                }
            };
            Size {
                w: (text.w + linear + padding.horizontal()).max(min.w),
                h: (text.h + linear + padding.vertical()).max(min.h),
            }
        }
        Ctl::Label(l) => {
            let avail = Size {
                w: proposed.w.saturating_sub(padding.horizontal()).max(0),
                h: proposed.h.saturating_sub(padding.vertical()).max(0),
            };
            let required = if l.text.is_empty() {
                Size {
                    w: 0,
                    h: text_height(dc.0, font),
                }
            } else {
                let mut flags = align_flags(l.align) | DT_WORDBREAK | DT_EDITCONTROL | DT_NOPREFIX;
                if l.ellipsis {
                    flags |= DT_END_ELLIPSIS;
                }
                let one = measure_text(
                    dc.0,
                    &l.text,
                    font,
                    Size {
                        w: UNBOUNDED,
                        h: UNBOUNDED,
                    },
                    flags,
                );
                if one.w <= avail.w {
                    flags &= !(DT_WORDBREAK | DT_EDITCONTROL);
                }
                measure_text(dc.0, &l.text, font, avail, flags)
            };
            Size {
                w: required.w + padding.horizontal(),
                h: required.h + padding.vertical(),
            }
        }
        _ => min,
    }
}

/// Measures the current caption, which may differ from the form's initial declaration.
pub(crate) fn measure_live(
    hwnd: HWND,
    declared: &Ctl,
    font: HFONT,
    padding: Pad,
    min: Size,
    proposed: Size,
) -> Size {
    let mut ctl = declared.clone();
    if let Some(state) = state_of(hwnd) {
        match (&mut ctl, &state.data) {
            (Ctl::Button(spec), Data::Button(b)) => *spec = b.spec.borrow().clone(),
            (Ctl::Label(spec), Data::Label(l)) => *spec = l.borrow().clone(),
            _ => {}
        }
    }
    measure(&ctl, font, padding, min, proposed)
}

// ---------------------------------------------------------------------------------------------
// Public operations (used through `window::Form`)
// ---------------------------------------------------------------------------------------------

/// Applies a new font, padding, and DPI after a DPI change.
pub(crate) fn apply_dpi(hwnd: HWND, font: HFONT, padding: Pad, dpi: u32) {
    if let Some(st) = state_of(hwnd) {
        st.font.set(font);
        st.padding.set(padding);
        st.dpi.set(dpi);
        // The form invalidates after the DPI layout; avoid drawing halfway through re-fonting.
        send(hwnd, WM_SETFONT, font.0 as usize, 0);
        st.update_item_height();
        invalidate(hwnd);
    }
}

/// Marks a button as the form's default button (extra border ring) or not.
pub(crate) fn set_default(hwnd: HWND, is_default: bool) {
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
        && b.is_default.get() != is_default
    {
        b.is_default.set(is_default);
        invalidate(hwnd);
    }
}

/// Whether `hwnd` is a kit button.
pub(crate) fn is_button(hwnd: HWND) -> bool {
    state_of(hwnd).is_some_and(|s| matches!(s.data, Data::Button(_)))
}

/// Sets the text of a button, label, or edit.
pub fn set_text(hwnd: HWND, text: &str) {
    if let Some(st) = state_of(hwnd) {
        match &st.data {
            Data::Button(b) => b.spec.borrow_mut().text = text.to_owned(),
            Data::Label(l) => l.borrow_mut().text = text.to_owned(),
            _ => {}
        }
    }
    let wide = to_wide(text);
    // SAFETY: NUL-terminated buffer valid for the call.
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
    }
    invalidate(hwnd);
}

/// Returns the window text of a control.
pub fn text(hwnd: HWND) -> String {
    // SAFETY: Length query on a window handle.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buf = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
    // SAFETY: `buf` is writable and sized for the text plus NUL.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    buf.truncate(usize::try_from(n).unwrap_or(0));
    String::from_utf16_lossy(&buf)
}

/// `Control.Enabled = value`, with the C# `EnabledChanged` color swap of `Buttons.ApplyStyle`.
pub fn set_enabled(hwnd: HWND, enabled: bool) {
    // SAFETY: Enables or disables a child window of this thread.
    let was_disabled = unsafe { EnableWindow(hwnd, enabled) }.as_bool();
    let was_enabled = !was_disabled;
    if was_enabled == enabled {
        return;
    }
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
    {
        if !enabled {
            b.hover.set(false);
        }
        if let Hover::Shared { normal, .. } = b.spec.borrow().hover {
            if enabled {
                b.back.set(normal);
                b.fore.set(theme::PRIMARY_TEXT);
            } else {
                b.back.set(theme::DISABLED_BUTTON);
                b.fore.set(theme::DISABLED_TEXT);
            }
        }
    }
    invalidate(hwnd);
}

/// Sets a button's current back, text, and border colors (for example the active sidebar item).
pub fn set_button_colors(hwnd: HWND, back: Color, fore: Color, border: Color) {
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
    {
        b.back.set(back);
        b.fore.set(fore);
        b.spec.borrow_mut().border = border;
        invalidate(hwnd);
    }
}

/// Returns a button's current `BackColor`.
pub fn button_back(hwnd: HWND) -> Option<Color> {
    state_of(hwnd).and_then(|s| match &s.data {
        Data::Button(b) => Some(b.back.get()),
        _ => None,
    })
}

/// Replaces the whole text of an edit.
pub fn edit_set_text(hwnd: HWND, text: &str) {
    let wide = to_wide(text);
    // SAFETY: NUL-terminated buffer valid for the call.
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
    }
}

/// `TextBox.AppendText` + `SelectionStart = TextLength` + `ScrollToCaret`.
pub fn edit_append(hwnd: HWND, text: &str) {
    let wide = to_wide(text);
    // SAFETY: Length query on our edit.
    let len = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    send(hwnd, EM_SETSEL, len, len as isize);
    send(hwnd, EM_REPLACESEL, 0, wide.as_ptr() as isize);
    // SAFETY: Length query on our edit.
    let len = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    send(hwnd, EM_SETSEL, len, len as isize);
    send(hwnd, EM_SCROLLCARET, 0, 0);
}

/// Appends many chunks with redraw off, then repaints once (bulk status output).
pub fn edit_append_batch(hwnd: HWND, chunks: &[&str]) {
    send(hwnd, WM_SETREDRAW, 0, 0);
    for c in chunks {
        edit_append(hwnd, c);
    }
    send(hwnd, WM_SETREDRAW, 1, 0);
    // SAFETY: Repaint request for our own control, including its frame and scroll bars.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
            Some(hwnd),
            None,
            None,
            windows::Win32::Graphics::Gdi::RDW_ERASE
                | windows::Win32::Graphics::Gdi::RDW_FRAME
                | windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
        );
    }
    send(hwnd, EM_SCROLLCARET, 0, 0);
}

/// `SelectionStart = 0` + `ScrollToCaret`.
pub fn edit_scroll_to_top(hwnd: HWND) {
    send(hwnd, EM_SETSEL, 0, 0);
    send(hwnd, EM_SCROLLCARET, 0, 0);
}

/// Replaces the items of a checked list.
pub fn list_set_items(hwnd: HWND, items: &[(String, bool)]) {
    let Some(st) = state_of(hwnd) else {
        return;
    };
    let Data::List(l) = &st.data else {
        return;
    };
    send(hwnd, LB_RESETCONTENT, 0, 0);
    *l.checked.borrow_mut() = items.iter().map(|(_, c)| *c).collect();
    for (text, _) in items {
        let wide = to_wide(text);
        if send(hwnd, LB_ADDSTRING, 0, wide.as_ptr() as isize).0 == LB_ERR as isize {
            break;
        }
    }
    invalidate(hwnd);
}

/// Check states of a checked list, in item order.
pub fn list_checked(hwnd: HWND) -> Vec<bool> {
    state_of(hwnd)
        .and_then(|s| match &s.data {
            Data::List(l) => Some(l.checked.borrow().clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Sets one item's check state.
pub fn list_set_checked(hwnd: HWND, index: usize, checked: bool) {
    if let Some(st) = state_of(hwnd)
        && let Data::List(l) = &st.data
    {
        if let Some(c) = l.checked.borrow_mut().get_mut(index) {
            *c = checked;
        }
        invalidate(hwnd);
    }
}

/// `ProgressBar.Value`.
pub fn progress_set(hwnd: HWND, value: u32) {
    send(hwnd, PBM_SETPOS, value.min(100) as usize, 0);
}

/// `ProgressBarStyle.Marquee` on or off.
pub fn progress_marquee(hwnd: HWND, on: bool) {
    // SAFETY: Reads and writes the style of our own progress bar.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let style = if on {
            style | PBS_MARQUEE as isize
        } else {
            style & !(PBS_MARQUEE as isize)
        };
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
    }
    send(hwnd, PBM_SETMARQUEE, usize::from(on), 30);
}
