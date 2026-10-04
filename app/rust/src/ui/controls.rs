//! Owned by WP-10a: native controls with WinForms look and behavior.
//!
//! Button (owner-drawn BUTTON, `DESIGN.md` section 6), label (painted STATIC), read-only
//! multiline EDIT, checked list (owner-drawn LISTBOX), progress bar. Each control keeps its own
//! state in its subclass; containers reflect `WM_DRAWITEM`, `WM_CTLCOLOR*`, `WM_COMMAND`, and
//! `WM_VKEYTOITEM` back to it through [`reflect`]. Text goes through GDI `DrawTextExW` with the
//! flags and margins of WinForms `TextRenderer`, so text positions match the C# app; shapes are
//! drawn with tiny-skia into a DIB.
//!
//! Portions ported from dotnet/winforms `ButtonBase.cs`, `Button.cs`,
//! `ButtonInternal/ButtonBaseAdapter*.cs`, `ButtonInternal/ButtonFlatAdapter.cs`,
//! `Rendering/TextExtensions.cs`, `Label.cs`, `CheckedListBox.cs`:
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
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, ClientToScreen, CreateSolidBrush,
            DFC_BUTTON, DFCS_BUTTONCHECK, DFCS_CHECKED, DFCS_FLAT, DIB_RGB_COLORS,
            DRAW_TEXT_FORMAT, DRAWTEXTPARAMS, DT_BOTTOM, DT_CALCRECT, DT_CENTER, DT_EDITCONTROL,
            DT_END_ELLIPSIS, DT_HIDEPREFIX, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
            DT_WORDBREAK, DeleteObject, DrawFocusRect, DrawFrameControl, DrawTextExW, EndPaint,
            FillRect, GetDC, GetTextMetricsW, GetWindowDC, HBRUSH, HDC, HFONT, HGDIOBJ,
            InvalidateRect, MapWindowPoints, PAINTSTRUCT, ReleaseDC, SelectObject, SetBkColor,
            SetBkMode, SetDIBitsToDevice, SetTextColor, TEXTMETRICW, TRANSPARENT,
        },
        System::SystemInformation::GetTickCount,
        UI::{
            Controls::{
                BPBF_COMPATIBLEBITMAP, BeginBufferedPaint, BufferedPaintInit, BufferedPaintUnInit,
                CloseThemeData, DRAWITEMSTRUCT, DrawThemeBackground, EM_REPLACESEL, EM_SCROLLCARET,
                EM_SETLIMITTEXT, EM_SETSEL, EndBufferedPaint, HTHEME, ODS_FOCUS, ODS_NOFOCUSRECT,
                ODS_SELECTED, OpenThemeData, WC_BUTTONW, WC_EDITW, WC_LISTBOXW, WC_STATICW,
                WM_MOUSELEAVE,
            },
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, GetKeyState, IsWindowEnabled, SetFocus, TME_LEAVE,
                TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL, VK_DOWN, VK_END, VK_HOME, VK_LEFT,
                VK_NEXT, VK_PRIOR, VK_RIGHT, VK_UP,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                BM_GETSTATE, BN_CLICKED, BN_DBLCLK, BS_OWNERDRAW, CreateWindowExW,
                DLGC_WANTALLKEYS, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY,
                GA_ROOT, GetAncestor, GetClientRect, GetNextDlgTabItem, GetParent, GetPropW,
                GetWindowRect, GetWindowTextLengthW, GetWindowTextW, HMENU, HTCLIENT, IDC_HAND,
                LB_ADDSTRING, LB_ERR, LB_GETCURSEL, LB_GETTEXT, LB_GETTEXTLEN, LB_RESETCONTENT,
                LB_SETITEMHEIGHT, LBN_DBLCLK, LBN_SELCHANGE, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT,
                LBS_NOTIFY, LBS_OWNERDRAWFIXED, LBS_WANTKEYBOARDINPUT, LoadCursorW, RemovePropW,
                SM_CXVSCROLL, SendMessageW, SetCursor, SetPropW, SetWindowTextW, UISF_HIDEACCEL,
                UISF_HIDEFOCUS, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CTLCOLOREDIT,
                WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_ERASEBKGND, WM_GETDLGCODE,
                WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCPAINT,
                WM_PAINT, WM_QUERYUISTATE, WM_SETCURSOR, WM_SETFOCUS, WM_SETFONT, WM_SETREDRAW,
                WM_SIZE, WM_TIMER, WM_UPDATEUISTATE, WM_VKEYTOITEM, WS_BORDER, WS_CHILD,
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

/// The button kinds of `DESIGN.md` section 6 (`Buttons.ButtonVariant` in C#).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonKind {
    /// White fill, dark text, no border: the one main action of a window.
    Primary,
    /// `CARD` fill with a 1 px `BORDER` (every other button; C# `Secondary`).
    Outline,
    /// An outline button with `DANGER` text (C# `Danger` fill is not used).
    Destructive,
    /// No fill at rest, `HOVER` fill when hovered or active (the main window sections).
    Sidebar,
}

/// An owner-drawn button (`FlatStyle.Flat` in C#; the look comes from `kind`).
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonSpec {
    /// Text; `&` marks a mnemonic like WinForms `UseMnemonic`.
    pub text: String,
    /// Font.
    pub font: FontSpec,
    /// Visual kind.
    pub kind: ButtonKind,
    /// `TextAlign`.
    pub align: Align,
    /// Icon-font glyph drawn before the text (`DESIGN.md` section 14), tinted like the text.
    pub icon: Option<char>,
}

impl ButtonSpec {
    fn new(text: &str, kind: ButtonKind) -> Self {
        Self {
            text: text.to_owned(),
            font: theme::BUTTON_FONT,
            kind,
            align: Align::MiddleCenter,
            icon: None,
        }
    }

    /// Adds an icon glyph before the text.
    pub fn icon(mut self, glyph: char) -> Self {
        self.icon = Some(glyph);
        self
    }

    /// The window's main action (`Buttons.ApplyStyle(button, ButtonVariant.Primary)`).
    pub fn primary(text: &str) -> Self {
        Self::new(text, ButtonKind::Primary)
    }

    /// Any other action (`Buttons.ApplyStyle(button, ButtonVariant.Secondary)`).
    pub fn outline(text: &str) -> Self {
        Self::new(text, ButtonKind::Outline)
    }

    /// An action that deletes something.
    pub fn destructive(text: &str) -> Self {
        Self::new(text, ButtonKind::Destructive)
    }

    /// A main window sidebar section item (left aligned, body font).
    pub fn sidebar(text: &str) -> Self {
        Self {
            font: theme::SECTION_BUTTON_FONT,
            align: Align::MiddleLeft,
            ..Self::new(text, ButtonKind::Sidebar)
        }
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
    /// Every character is an icon-font glyph drawn centered on the same spot (a layered
    /// status icon, `DESIGN.md` 15).
    pub stacked: bool,
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
            stacked: false,
        }
    }

    /// Draws the characters stacked on one spot (icon-font layers).
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
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
    /// The loading indicator (`DESIGN.md` 8.9): a ring with a turning arc, drawn by the form's
    /// timer; static when Windows animations are off.
    Spinner,
    /// The app icon (resource 1) at the node's size.
    AppIcon,
}

impl Ctl {
    /// The font this control uses.
    pub fn font(&self) -> FontSpec {
        match self {
            Ctl::Button(b) => b.font,
            Ctl::Label(l) => l.font,
            Ctl::Edit(e) => e.font,
            Ctl::CheckedList(l) => l.font,
            Ctl::Progress | Ctl::Spinner | Ctl::AppIcon => theme::DEFAULT_FONT,
        }
    }

    /// The icon font of a button with an icon.
    pub fn icon_font(&self) -> Option<FontSpec> {
        match self {
            Ctl::Button(b) if b.icon.is_some() => Some(theme::icon_font(theme::ICON_PX)),
            _ => None,
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
            Ctl::Spinner => (
                theme::NO_PAD,
                Size {
                    w: theme::SPINNER_SIZE,
                    h: theme::SPINNER_SIZE,
                },
            ),
            Ctl::AppIcon => (
                theme::NO_PAD,
                Size {
                    w: theme::UPDATE_ICON_SIZE,
                    h: theme::UPDATE_ICON_SIZE,
                },
            ),
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

/// A 1-pixel frame inside `r` (the square frame of text boxes and lists).
fn frame(hdc: HDC, b: Rect, color: Color) {
    let size = theme::STROKE.min(b.w.min(b.h));
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

/// A rounded rectangle path (`DESIGN.md` 5: anti-aliased shapes are drawn with tiny-skia).
fn rounded(r: Rect, radius: i32, inset: f32) -> Option<tiny_skia::Path> {
    let (x, y) = (r.x as f32 + inset, r.y as f32 + inset);
    let (w, h) = (r.w as f32 - 2.0 * inset, r.h as f32 - 2.0 * inset);
    let rad = (radius as f32).min(w / 2.0).min(h / 2.0).max(0.0);
    // Bezier circle-quadrant control distance.
    let k = rad * 0.552_284_8;
    let (right, bottom) = (x + w, y + h);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + rad, y);
    pb.line_to(right - rad, y);
    pb.cubic_to(right - rad + k, y, right, y + rad - k, right, y + rad);
    pb.line_to(right, bottom - rad);
    pb.cubic_to(
        right,
        bottom - rad + k,
        right - rad + k,
        bottom,
        right - rad,
        bottom,
    );
    pb.line_to(x + rad, bottom);
    pb.cubic_to(x + rad - k, bottom, x, bottom - rad + k, x, bottom - rad);
    pb.line_to(x, y + rad);
    pb.cubic_to(x, y + rad - k, x + rad - k, y, x + rad, y);
    pb.close();
    pb.finish()
}

fn skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, 255)
}

fn skia_paint(c: Color) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(skia_color(c));
    paint.anti_alias = true;
    paint
}

/// Draws a rounded rectangle over `background` into a pixmap of `r`'s size and copies it to the
/// DC at `r`. `fill` covers the whole rectangle; `border` is a 1 px stroke on its edge.
fn rounded_rect(
    hdc: HDC,
    r: Rect,
    radius: i32,
    background: Color,
    fill_color: Option<Color>,
    border: Option<Color>,
) {
    let Some(mut pixmap) = tiny_skia::Pixmap::new(r.w.max(0) as u32, r.h.max(0) as u32) else {
        return;
    };
    pixmap.fill(skia_color(background));
    let local = Rect { x: 0, y: 0, ..r };
    let id = tiny_skia::Transform::identity();
    if let (Some(color), Some(path)) = (fill_color, rounded(local, radius, 0.0)) {
        pixmap.fill_path(
            &path,
            &skia_paint(color),
            tiny_skia::FillRule::Winding,
            id,
            None,
        );
    }
    // A stroke centred half a pixel inside the edge covers exactly the outermost pixel ring.
    if let (Some(color), Some(path)) = (border, rounded(local, radius, 0.5)) {
        let stroke = tiny_skia::Stroke {
            width: theme::STROKE as f32,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &skia_paint(color), &stroke, id, None);
    }
    blit(hdc, r.x, r.y, &pixmap);
}

/// Copies an opaque pixmap (premultiplied RGBA) to the DC as a 32-bit top-down DIB.
fn blit(hdc: HDC, x: i32, y: i32, pixmap: &tiny_skia::Pixmap) {
    let (w, h) = (pixmap.width(), pixmap.height());
    if w == 0 || h == 0 {
        return;
    }
    let bgra: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| [p.blue(), p.green(), p.red(), p.alpha()])
        .collect();
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `bgra` holds w*h*4 bytes laid out as the header describes and lives through the
    // call; the DC is valid for the caller's paint.
    unsafe {
        SetDIBitsToDevice(
            hdc,
            x,
            y,
            w,
            h,
            0,
            0,
            0,
            h,
            bgra.as_ptr().cast(),
            &bmi,
            DIB_RGB_COLORS,
        );
    }
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

/// Draws one icon-font glyph centered in `bounds` (no WinForms overhang margins).
fn draw_glyph(hdc: HDC, glyph: char, font: HFONT, bounds: Rect, color: Color) {
    let mut buf: Vec<u16> = glyph.to_string().encode_utf16().collect();
    let _sel = Select::new(hdc, font);
    let mut rc = rect(bounds);
    // SAFETY: Plain DC state changes and a draw call with valid buffers.
    unsafe {
        SetTextColor(hdc, color.colorref());
        SetBkMode(hdc, TRANSPARENT);
        DrawTextExW(
            hdc,
            &mut buf,
            &mut rc,
            DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
            None,
        );
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
// Focus ring (DESIGN.md 5: 1 px, 3 px outside the control, so the container paints it)
// ---------------------------------------------------------------------------------------------

/// The control's rectangle in its parent's client coordinates.
fn rect_in_parent(hwnd: HWND) -> Option<Rect> {
    let mut rc = RECT::default();
    // SAFETY: Writable RECT; a dead handle fails and yields None.
    unsafe { GetWindowRect(hwnd, &mut rc) }.ok()?;
    // SAFETY: Read-only parent query.
    let parent = unsafe { GetParent(hwnd) }.ok()?;
    let mut pts = [
        POINT {
            x: rc.left,
            y: rc.top,
        },
        POINT {
            x: rc.right,
            y: rc.bottom,
        },
    ];
    // SAFETY: Converts two points of a live window; a failure (0 with no error) leaves them.
    unsafe { MapWindowPoints(None, Some(parent), &mut pts) };
    Some(Rect {
        x: pts[0].x,
        y: pts[0].y,
        w: pts[1].x - pts[0].x,
        h: pts[1].y - pts[0].y,
    })
}

/// The ring rectangle around a focused control, in parent coordinates.
fn ring_rect(hwnd: HWND, dpi: u32) -> Option<Rect> {
    let offset = dpi::scale(theme::FOCUS_RING_OFFSET, dpi);
    Some(rect_in_parent(hwnd)?.deflate(Pad {
        l: -offset,
        t: -offset,
        r: -offset,
        b: -offset,
    }))
}

/// Repaints the parent's area under a control's focus ring (focus or focus-cue change).
fn invalidate_ring(st: &CtlState) {
    // SAFETY: Read-only parent query.
    let Ok(parent) = (unsafe { GetParent(st.hwnd) }) else {
        return;
    };
    let Some(r) = ring_rect(st.hwnd, st.dpi.get()) else {
        return;
    };
    let rc = rect(r.deflate(Pad {
        l: -1,
        t: -1,
        r: -1,
        b: -1,
    }));
    // SAFETY: Invalidates a rectangle of a live parent window; erase repaints its back color.
    unsafe {
        let _ = InvalidateRect(Some(parent), Some(&rc), true);
    }
}

/// Paints the focus ring of `parent`'s focused kit button, if focus cues are on. Called from the
/// container's `WM_PAINT`; `background` is the container's back color.
pub(crate) fn paint_focus_ring(parent: HWND, hdc: HDC, background: Color) {
    // SAFETY: Reads this thread's focus window.
    let focus = unsafe { GetFocus() };
    // SAFETY: Read-only parent query of the focus window.
    if focus.is_invalid() || unsafe { GetParent(focus) } != Ok(parent) {
        return;
    }
    let Some(st) = state_of(focus) else {
        return;
    };
    if !matches!(st.data, Data::Button(_)) || !st.ui_state().0 {
        return;
    }
    let dpi = st.dpi.get();
    let Some(r) = ring_rect(focus, dpi) else {
        return;
    };
    rounded_rect(
        hdc,
        r,
        dpi::scale(theme::FOCUS_RING_RADIUS, dpi),
        background,
        None,
        Some(theme::FOCUS_RING),
    );
}

/// Paints a container as a card: `fill` with a 1 px `BORDER` outline, rounded by `radius`
/// device pixels, over `outer` (the parent's color under the corners).
pub(crate) fn paint_card(hwnd: HWND, hdc: HDC, outer: Color, fill_color: Color, radius: i32) {
    let area = client(hwnd);
    rounded_rect(
        hdc,
        area,
        radius,
        outer,
        Some(fill_color),
        Some(theme::BORDER),
    );
}

/// The focus ring rectangle (parent coordinates) of a kit button that has the focus, for the
/// layout pass to repaint when the button moves.
pub(crate) fn focused_ring(hwnd: HWND) -> Option<RECT> {
    // SAFETY: Reads this thread's focus window.
    if unsafe { GetFocus() } != hwnd || !is_button(hwnd) {
        return None;
    }
    let dpi = state_of(hwnd)?.dpi.get();
    Some(rect(ring_rect(hwnd, dpi)?.deflate(Pad {
        l: -1,
        t: -1,
        r: -1,
        b: -1,
    })))
}

// ---------------------------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------------------------

struct ButtonData {
    spec: RefCell<ButtonSpec>,
    hover: Cell<bool>,
    /// Sidebar item of the shown section (C# finds it by its `BackColor`).
    active: Cell<bool>,
    /// Sidebar item whose section is not collected yet (`FAINT` text while loading).
    pending: Cell<bool>,
}

struct EditData {
    spec: EditSpec,
    brush: Brush,
    /// Widest line in device pixels (kept while appending; remeasured on a text change).
    longest: Cell<i32>,
    /// Re-entrancy guard: showing a bar sends WM_SIZE to the edit.
    updating_bars: Cell<bool>,
}

struct SpinnerData {
    /// Windows "Show animations" at creation; false = the arc stays at its start angle.
    animate: bool,
}

struct ProgressData {
    /// `ProgressBar.Value` (0 to 100).
    pos: Cell<i32>,
    /// `ProgressBarStyle.Marquee`: a control timer repaints the moving block.
    marquee: Cell<bool>,
}

/// The marquee timer id on a progress control.
const MARQUEE_TIMER: usize = 1;

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
    Progress(ProgressData),
    Spinner(SpinnerData),
    AppIcon,
}

/// Per-control state, owned by the control's subclass (dropped on `WM_NCDESTROY`).
pub(crate) struct CtlState {
    id: u16,
    hwnd: HWND,
    font: Cell<HFONT>,
    /// The icon font of a button with an icon (0 otherwise).
    icon_font: Cell<HFONT>,
    dpi: Cell<u32>,
    padding: Cell<Pad>,
    back: Cell<Color>,
    data: Data,
}

/// Windows "Show animations in Windows" (`SPI_GETCLIENTAREAANIMATION`); true when unknown.
pub fn animations_enabled() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(1);
    // SAFETY: Writable BOOL out-parameter sized as the query requires.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut on as *mut windows::core::BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_err() || on.as_bool()
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
    icon_font: HFONT,
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
                hover: Cell::new(false),
                active: Cell::new(false),
                pending: Cell::new(false),
            }),
        ),
        Ctl::Label(l) => (
            WC_STATICW,
            // Layered status glyphs are decorative; keep private-use characters out of the
            // native accessible name. Painting and measurement still use the label spec.
            if l.stacked { "" } else { l.text.as_str() },
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
                    longest: Cell::new(0),
                    updating_bars: Cell::new(false),
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
            WC_STATICW,
            "",
            base,
            WINDOW_EX_STYLE(0),
            Data::Progress(ProgressData {
                pos: Cell::new(0),
                marquee: Cell::new(false),
            }),
        ),
        Ctl::Spinner => (
            WC_STATICW,
            "",
            base,
            WINDOW_EX_STYLE(0),
            Data::Spinner(SpinnerData {
                animate: animations_enabled(),
            }),
        ),
        Ctl::AppIcon => (WC_STATICW, "", base, WINDOW_EX_STYLE(0), Data::AppIcon),
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
        icon_font: Cell::new(icon_font),
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
            edit_set_margins(hwnd, dpi);
            edit_update_bars(hwnd);
        }
        Ctl::CheckedList(_) => {
            if let Some(st) = state_of(hwnd) {
                st.update_item_height();
            }
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
            (Data::Button(_), WM_SETFOCUS) => {
                super::window::focus_changed(hwnd);
                invalidate_ring(self);
                None
            }
            (_, WM_SETFOCUS) => {
                super::window::focus_changed(hwnd);
                None
            }
            (Data::Button(_), WM_KILLFOCUS) => {
                invalidate_ring(self);
                None
            }
            (
                Data::Button(_)
                | Data::Label(_)
                | Data::Progress(_)
                | Data::Spinner(_)
                | Data::AppIcon,
                WM_ERASEBKGND,
            ) => Some(LRESULT(1)),
            (Data::Label(_), WM_PAINT) => {
                self.paint_label();
                Some(LRESULT(0))
            }
            (Data::Progress(p), WM_PAINT) => {
                self.paint_progress(p);
                Some(LRESULT(0))
            }
            (Data::Progress(_), WM_TIMER) => {
                invalidate(hwnd);
                Some(LRESULT(0))
            }
            (Data::Spinner(s), WM_PAINT) => {
                self.paint_spinner(s);
                Some(LRESULT(0))
            }
            (Data::AppIcon, WM_PAINT) => {
                self.paint_app_icon();
                Some(LRESULT(0))
            }
            (Data::Edit(_) | Data::List(_), WM_NCPAINT) => {
                // Scroll bars and the native frame first, then the 1 px token frame over it.
                let r = def(hwnd, msg, wparam, lparam);
                self.paint_frame();
                Some(r)
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
                    invalidate(hwnd);
                }
                None
            }
            (Data::Button(b), WM_MOUSELEAVE) => {
                b.hover.set(false);
                invalidate(hwnd);
                None
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
                invalidate_ring(self);
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
            (Data::Edit(_), WM_SIZE | WM_SETFONT) => {
                // The native edit re-shows its scroll bars and resets its margins on both.
                let r = def(hwnd, msg, wparam, lparam);
                if msg == WM_SETFONT {
                    edit_set_margins(hwnd, self.dpi.get());
                    if let Data::Edit(e) = &self.data {
                        // The cached width is in device pixels of the previous font/DPI.
                        e.longest.set(widest_line(self.font.get(), &text(hwnd)));
                    }
                }
                edit_update_bars(hwnd);
                Some(r)
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

    /// The 1 px token frame over the native non-client border of a text box or list; the inner
    /// pixel of a `WS_EX_CLIENTEDGE` frame takes the box's back color.
    fn paint_frame(&self) {
        let back = match &self.data {
            Data::Edit(e) => e.spec.back,
            Data::List(l) => l.spec.back,
            _ => return,
        };
        let mut rc = RECT::default();
        let mut origin = POINT::default();
        // SAFETY: Writable RECT and POINT of our own window.
        let known = unsafe {
            GetWindowRect(self.hwnd, &mut rc).is_ok()
                && ClientToScreen(self.hwnd, &mut origin).as_bool()
        };
        if !known {
            return;
        }
        // The native frame width (1 px for WS_BORDER, SM_CXEDGE for WS_EX_CLIENTEDGE, which
        // scales with DPI) is the distance from the window edge to the client origin.
        let width = origin.x - rc.left;
        let outer = Rect {
            x: 0,
            y: 0,
            w: rc.right - rc.left,
            h: rc.bottom - rc.top,
        };
        // SAFETY: The window DC of our own control, released below.
        let hdc = unsafe { GetWindowDC(Some(self.hwnd)) };
        if hdc.is_invalid() {
            return;
        }
        for i in 0..width {
            let ring = outer.deflate(Pad {
                l: i,
                t: i,
                r: i,
                b: i,
            });
            frame(hdc, ring, if i == 0 { theme::BORDER } else { back });
        }
        // SAFETY: Releases the DC obtained above.
        unsafe {
            ReleaseDC(Some(self.hwnd), hdc);
        }
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

    /// Paints the button for its kind and state (`DESIGN.md` section 6); the text layout keeps
    /// the WinForms `ButtonFlatAdapter` math so the C# positions hold.
    fn paint_button(&self, b: &ButtonData, hdc: HDC, area: Rect) {
        let spec = b.spec.borrow();
        let enabled = self.enabled();
        let pushed = enabled && send(self.hwnd, BM_GETSTATE, 0, 0).0 & BST_PUSHED != 0;
        let hover = enabled && b.hover.get();
        let (_, show_accel) = self.ui_state();
        let container = self.back.get();
        let (fill_color, text_color, border) = match spec.kind {
            ButtonKind::Primary => (
                if !enabled {
                    theme::DISABLED_BUTTON
                } else if pushed {
                    theme::PRIMARY_BUTTON_PRESSED
                } else if hover {
                    theme::PRIMARY_BUTTON_HOVER
                } else {
                    theme::PRIMARY_BUTTON
                },
                if enabled {
                    theme::BG
                } else {
                    theme::DISABLED_TEXT
                },
                None,
            ),
            ButtonKind::Outline | ButtonKind::Destructive => (
                if pushed {
                    theme::BUTTON_BORDER
                } else if hover {
                    theme::BUTTON_HOVER
                } else {
                    theme::BUTTON_BACKGROUND
                },
                if !enabled {
                    theme::DISABLED_TEXT
                } else if spec.kind == ButtonKind::Destructive {
                    theme::DANGER_BUTTON
                } else {
                    theme::PRIMARY_TEXT
                },
                Some(if hover {
                    theme::BORDER_STRONG
                } else {
                    theme::BUTTON_BORDER
                }),
            ),
            ButtonKind::Sidebar => (
                if b.active.get() {
                    theme::SIDEBAR_ITEM_ACTIVE
                } else if hover || pushed {
                    theme::SIDEBAR_ITEM_HOVER
                } else {
                    container
                },
                if b.active.get() {
                    theme::SIDEBAR_ITEM_ACTIVE_TEXT
                } else if b.pending.get() {
                    theme::DISABLED_TEXT
                } else {
                    theme::SIDEBAR_ITEM_TEXT
                },
                None,
            ),
        };
        let bs = theme::BUTTON_BORDER_SIZE;
        let dpi = self.dpi.get();
        let radius = dpi::scale(theme::BUTTON_RADIUS, dpi);
        buffered(hdc, area, |hdc| {
            rounded_rect(hdc, area, radius, container, Some(fill_color), border);
            if spec.kind == ButtonKind::Sidebar && b.active.get() {
                // The accent bar of the active item (DESIGN.md 6): inside the item's left edge.
                let inset = dpi::scale(theme::SIDEBAR_ACCENT_INSET, dpi);
                let bar = Rect {
                    x: area.x,
                    y: area.y + inset,
                    w: dpi::scale(theme::SIDEBAR_ACCENT_WIDTH, dpi),
                    h: (area.h - 2 * inset).max(0),
                };
                rounded_rect(
                    hdc,
                    bar,
                    theme::SIDEBAR_ACCENT_RADIUS,
                    fill_color,
                    Some(theme::TEXT),
                    None,
                );
            }
            // Text layout: Client = client - Padding; Face = Client - border; Field = Face - 2.
            // A sidebar item has no border and no WinForms image inset: its padding is the
            // whole inset, so the caption gets the full width (DESIGN.md 11.3).
            let pad = self.padding.get();
            let inset = if spec.kind == ButtonKind::Sidebar {
                0
            } else {
                bs + 2 + TEXT_IMAGE_INSET
            };
            let mut max_bounds = area.deflate(pad).deflate(Pad {
                l: inset,
                t: inset,
                r: inset,
                b: inset,
            });
            // Sidebar items are one line with an end ellipsis (DESIGN.md 8.7, 11.3); other
            // buttons keep the WinForms word break.
            let mut flags = if spec.kind == ButtonKind::Sidebar {
                align_flags(spec.align) | DT_SINGLELINE | DT_END_ELLIPSIS
            } else {
                align_flags(spec.align) | DT_WORDBREAK | DT_EDITCONTROL
            };
            if !show_accel {
                flags |= DT_HIDEPREFIX;
            }
            let font = self.font.get();
            let icon = spec.icon.filter(|_| !self.icon_font.get().is_invalid());
            let icon_w = if icon.is_some() {
                dpi::scale(theme::ICON_PX, dpi) + dpi::scale(theme::ICON_GAP, dpi)
            } else {
                0
            };
            let text_avail = Size {
                w: (max_bounds.w - icon_w).max(1),
                h: max_bounds.h,
            };
            let size = measure_text(hdc, &spec.text, font, text_avail, flags);
            // Icon and text are placed as one block (centered or left, per TextAlign).
            let block = align_in(
                Size {
                    w: (size.w + icon_w).min(max_bounds.w),
                    h: size.h,
                },
                max_bounds,
                spec.align,
            );
            if let Some(glyph) = icon {
                let icon_px = dpi::scale(theme::ICON_PX, dpi);
                let icon_rect = Rect {
                    x: block.x,
                    y: max_bounds.y + (max_bounds.h - icon_px) / 2,
                    w: icon_px,
                    h: icon_px,
                };
                draw_glyph(hdc, glyph, self.icon_font.get(), icon_rect, text_color);
                max_bounds.x = block.x + icon_w;
                max_bounds.w = (block.right() - max_bounds.x).max(1);
            } else {
                max_bounds.x = block.x;
                max_bounds.w = block.w;
            }
            let text_bounds = align_in(size, max_bounds, Align::MiddleLeft);
            draw_text(hdc, &spec.text, font, text_bounds, text_color, flags);
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
            if spec.stacked {
                for glyph in spec.text.chars() {
                    draw_glyph(hdc, glyph, self.font.get(), face, spec.fore);
                }
                return;
            }
            let flags = self.label_flags(&spec, hdc, face.size());
            draw_text(hdc, &spec.text, self.font.get(), face, spec.fore, flags);
        });
    }

    // -- progress --------------------------------------------------------------------------

    /// A pill: `HOVER` track and `TEXT` fill, both rounded by half the height (the themed bar
    /// ignores colors, the classic one adds a sunken edge). The native control still owns the
    /// value and the marquee timer; a marquee shows a third-width block that moves with the
    /// clock.
    fn paint_progress(&self, p: &ProgressData) {
        let paint = Paint::begin(self.hwnd);
        let area = client(self.hwnd);
        let pos = p.pos.get().clamp(0, 100);
        let bar = if p.marquee.get() {
            let block = area.w / 3;
            // SAFETY: Plain tick count query.
            let tick = unsafe { GetTickCount() } as i32;
            let travel = area.w + block;
            let steps = (travel / theme::MARQUEE_STEP_PX).max(1);
            let x = (tick / theme::MARQUEE_STEP_MS).rem_euclid(steps) * theme::MARQUEE_STEP_PX;
            Rect {
                x: x - block,
                y: 0,
                w: block,
                h: area.h,
            }
        } else {
            Rect {
                x: 0,
                y: 0,
                w: area.w * pos / 100,
                h: area.h,
            }
        };
        let container = self.back.get();
        buffered(paint.hdc(), area, |hdc| {
            let Some(mut pixmap) =
                tiny_skia::Pixmap::new(area.w.max(0) as u32, area.h.max(0) as u32)
            else {
                return;
            };
            pixmap.fill(skia_color(container));
            let id = tiny_skia::Transform::identity();
            let radius = area.h / 2;
            if let Some(track) = rounded(area, radius, 0.0) {
                pixmap.fill_path(
                    &track,
                    &skia_paint(theme::PROGRESS_TRACK),
                    tiny_skia::FillRule::Winding,
                    id,
                    None,
                );
                // The fill is clipped by the track's own shape (a mask of the track path).
                let visible = Rect {
                    x: bar.x.max(0),
                    y: 0,
                    w: (bar.right().min(area.w) - bar.x.max(0)).max(0),
                    h: area.h,
                };
                if visible.w > 0
                    && let Some(fill_path) = rounded(visible, radius, 0.0)
                    && let Some(mut mask) = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
                {
                    mask.fill_path(&track, tiny_skia::FillRule::Winding, true, id);
                    pixmap.fill_path(
                        &fill_path,
                        &skia_paint(theme::PROGRESS_FILL),
                        tiny_skia::FillRule::Winding,
                        id,
                        Some(&mask),
                    );
                }
            }
            blit(hdc, 0, 0, &pixmap);
        });
    }

    // -- spinner and app icon ---------------------------------------------------------------

    /// A `BORDER` ring with a `TEXT` quarter arc; the angle follows the clock while the form's
    /// timer repaints it (`Form::spin`), and stays at the start when animations are off.
    fn paint_spinner(&self, s: &SpinnerData) {
        let paint = Paint::begin(self.hwnd);
        let area = client(self.hwnd);
        let container = self.back.get();
        let dpi = self.dpi.get();
        let stroke = dpi::scale(theme::SPINNER_STROKE, dpi) as f32;
        let angle = if s.animate {
            // SAFETY: Plain tick count query.
            let tick = unsafe { GetTickCount() } as i32;
            (tick.rem_euclid(theme::SPINNER_TURN_MS)) as f32 * 360.0 / theme::SPINNER_TURN_MS as f32
        } else {
            0.0
        };
        buffered(paint.hdc(), area, |hdc| {
            let Some(mut pixmap) =
                tiny_skia::Pixmap::new(area.w.max(0) as u32, area.h.max(0) as u32)
            else {
                return;
            };
            pixmap.fill(skia_color(container));
            let d = area.w.min(area.h) as f32;
            let (cx, cy) = (area.w as f32 / 2.0, area.h as f32 / 2.0);
            let r = (d - stroke) / 2.0;
            if r <= 0.0 {
                return;
            }
            let id = tiny_skia::Transform::identity();
            let paint_stroke = |color: Color, cap: tiny_skia::LineCap| {
                (
                    skia_paint(color),
                    tiny_skia::Stroke {
                        width: stroke,
                        line_cap: cap,
                        ..Default::default()
                    },
                )
            };
            // Ring: a full circle.
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_circle(cx, cy, r);
            if let Some(ring) = pb.finish() {
                let (p, st) = paint_stroke(theme::BORDER, tiny_skia::LineCap::Butt);
                pixmap.stroke_path(&ring, &p, &st, id, None);
            }
            // Arc: a quarter turn starting at `angle`, built from 12 short segments.
            let mut pb = tiny_skia::PathBuilder::new();
            let steps = 12;
            for i in 0..=steps {
                let a = (angle + 90.0 * i as f32 / steps as f32).to_radians();
                let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
                if i == 0 {
                    pb.move_to(x, y);
                } else {
                    pb.line_to(x, y);
                }
            }
            if let Some(arc) = pb.finish() {
                let (p, st) = paint_stroke(theme::TEXT, tiny_skia::LineCap::Round);
                pixmap.stroke_path(&arc, &p, &st, id, None);
            }
            blit(hdc, 0, 0, &pixmap);
        });
    }

    /// The app icon (resource 1) scaled to the control, over the container color.
    fn paint_app_icon(&self) {
        use windows::Win32::UI::WindowsAndMessaging::{
            DI_NORMAL, DrawIconEx, HICON, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW,
        };
        let paint = Paint::begin(self.hwnd);
        let area = client(self.hwnd);
        let container = self.back.get();
        let size = area.w.min(area.h);
        // SAFETY: Module handle of this process; the icon is a shared resource handle.
        let icon = unsafe {
            let instance =
                windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            LoadImageW(
                Some(instance.into()),
                PCWSTR(std::ptr::without_provenance(1)),
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            )
        }
        .map(|h| HICON(h.0));
        // Without the resource (the test exe has none) the stock application icon stands in,
        // which is also what the title bar shows then.
        let shared = icon.is_err();
        let icon = icon.or_else(|_| {
            // SAFETY: Loads the shared stock application icon.
            unsafe { windows::Win32::UI::WindowsAndMessaging::LoadIconW(None, IDI_APPLICATION) }
        });
        let hdc = paint.hdc();
        {
            fill(hdc, area, container);
            if let Ok(icon) = icon {
                // SAFETY: Draws a valid icon into the paint DC; no ownership transfer.
                unsafe {
                    let _ = DrawIconEx(
                        hdc,
                        (area.w - size) / 2,
                        (area.h - size) / 2,
                        icon,
                        size,
                        size,
                        0,
                        None,
                        DI_NORMAL,
                    );
                    if !shared {
                        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(icon);
                    }
                }
            }
        }
    }

    // -- checked list ----------------------------------------------------------------------

    fn set_list_checked(&self, l: &ListData, index: usize, checked: bool) {
        if l.checked
            .borrow()
            .get(index)
            .is_none_or(|old| *old == checked)
        {
            return;
        }
        use windows::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_NAMECHANGE, LB_DELETESTRING, LB_GETTOPINDEX, LB_INSERTSTRING,
            LB_SETCURSEL, LB_SETTOPINDEX, OBJID_CLIENT,
        };
        // The crate's Accessibility feature is not enabled; this existing user32 API
        // needs no additional dependency or Cargo feature.
        #[link(name = "user32")]
        unsafe extern "system" {
            fn NotifyWinEvent(event: u32, hwnd: HWND, object: i32, child: i32);
        }
        let len = send(self.hwnd, LB_GETTEXTLEN, index, 0).0;
        if len == LB_ERR as isize {
            win::record(win::Error::msg(
                "Update checked list",
                "item text unavailable",
            ));
            return;
        }
        let mut buf = vec![0u16; len as usize + 1];
        send(self.hwnd, LB_GETTEXT, index, buf.as_mut_ptr() as isize);
        let old = from_wide(&buf);
        let wide = to_wide(&list_accessible_name(list_display_name(&old), checked));
        let selected = send(self.hwnd, LB_GETCURSEL, 0, 0).0;
        let top = send(self.hwnd, LB_GETTOPINDEX, 0, 0).0;
        // LISTBOX has no set-item-text message. Replace in place without flashing or
        // changing the selected row/scroll position; owner drawing omits the state suffix.
        send(self.hwnd, WM_SETREDRAW, 0, 0);
        send(self.hwnd, LB_DELETESTRING, index, 0);
        if send(self.hwnd, LB_INSERTSTRING, index, wide.as_ptr() as isize).0 == index as isize {
            l.checked.borrow_mut()[index] = checked;
        } else {
            let original = to_wide(&old);
            let restored = send(
                self.hwnd,
                LB_INSERTSTRING,
                index,
                original.as_ptr() as isize,
            )
            .0;
            if restored != index as isize {
                l.checked.borrow_mut().remove(index);
            }
            win::record(win::Error::msg(
                "Update checked list",
                format!(
                    "item replacement failed; restored={}",
                    restored == index as isize
                ),
            ));
        }
        send(self.hwnd, LB_SETCURSEL, selected as usize, 0);
        send(self.hwnd, LB_SETTOPINDEX, top as usize, 0);
        send(self.hwnd, WM_SETREDRAW, 1, 0);
        invalidate(self.hwnd);
        // SAFETY: Announces the updated native name of this live list's 1-based child.
        unsafe {
            NotifyWinEvent(
                EVENT_OBJECT_NAMECHANGE,
                self.hwnd,
                OBJID_CLIENT.0,
                index as i32 + 1,
            );
        }
    }

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
            let i = index as usize;
            let checked = !l.checked.borrow()[i];
            self.set_list_checked(l, i, checked);
            result = l.checked.borrow().get(i).map(|checked| (i, *checked));
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
            draw_text(
                hdc,
                list_display_name(&text),
                font,
                string_bounds,
                fore,
                DT_NOPREFIX,
            );
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
pub(crate) fn measure(
    ctl: &Ctl,
    font: HFONT,
    padding: Pad,
    min: Size,
    proposed: Size,
    dpi: u32,
) -> Size {
    let dc = ScreenDc::new();
    match ctl {
        Ctl::Button(b) => {
            // ButtonFlatAdapter.PaintFlatLayout(up: false, check: true): border + 1, padding 1,
            // plus 2 for GrowBorderBy1PxWhenDefault. Sidebar items have neither (see paint).
            let sidebar = b.kind == ButtonKind::Sidebar;
            let linear = if sidebar {
                0
            } else {
                (theme::BUTTON_BORDER_SIZE + 1) * 2 + 2 + 2
            };
            let inset = if sidebar { 0 } else { TEXT_IMAGE_INSET * 2 };
            // Prefix processing hides `&` with or without cues, so the width is the same.
            let flags = align_flags(b.align) | DT_EDITCONTROL | DT_HIDEPREFIX;
            let avail = Size {
                w: proposed.w.saturating_sub(linear + inset),
                h: proposed.h.saturating_sub(linear + inset),
            };
            let text = measure_text(dc.0, &b.text, font, avail, flags);
            let mut text = if b.text.is_empty() {
                Size::default()
            } else {
                Size {
                    w: text.w + inset,
                    h: text.h + inset,
                }
            };
            if b.icon.is_some() {
                text.w += dpi::scale(theme::ICON_PX, dpi) + dpi::scale(theme::ICON_GAP, dpi);
                text.h = text.h.max(dpi::scale(theme::ICON_PX, dpi) + inset);
            }
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
    dpi: u32,
) -> Size {
    let mut ctl = declared.clone();
    if let Some(state) = state_of(hwnd) {
        match (&mut ctl, &state.data) {
            (Ctl::Button(spec), Data::Button(b)) => *spec = b.spec.borrow().clone(),
            (Ctl::Label(spec), Data::Label(l)) => *spec = l.borrow().clone(),
            _ => {}
        }
    }
    measure(&ctl, font, padding, min, proposed, dpi)
}

// ---------------------------------------------------------------------------------------------
// Public operations (used through `window::Form`)
// ---------------------------------------------------------------------------------------------

/// Applies new fonts, padding, and DPI after a DPI change.
pub(crate) fn apply_dpi(hwnd: HWND, font: HFONT, icon_font: HFONT, padding: Pad, dpi: u32) {
    if let Some(st) = state_of(hwnd) {
        st.font.set(font);
        st.icon_font.set(icon_font);
        st.padding.set(padding);
        st.dpi.set(dpi);
        // The form invalidates after the DPI layout; avoid drawing halfway through re-fonting.
        // (The edit subclass re-applies its margins and scroll bars on WM_SETFONT.)
        send(hwnd, WM_SETFONT, font.0 as usize, 0);
        st.update_item_height();
        invalidate(hwnd);
    }
}

/// Marks a sidebar item as not collected yet (`FAINT` text) or collected.
pub fn set_pending(hwnd: HWND, pending: bool) {
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
        && b.pending.get() != pending
    {
        b.pending.set(pending);
        invalidate(hwnd);
    }
}

/// Repaints a spinner (called by the form's timer while a load runs).
pub fn spin(hwnd: HWND) {
    invalidate(hwnd);
}

/// Whether a spinner animates (false when Windows "Show animations" is off).
pub fn spinner_animates(hwnd: HWND) -> bool {
    state_of(hwnd).is_some_and(|s| match &s.data {
        Data::Spinner(sp) => sp.animate,
        _ => false,
    })
}

/// Inner left and right margins of a text well (`EM_SETMARGINS`, DESIGN.md 4).
fn edit_set_margins(hwnd: HWND, dpi: u32) {
    use windows::Win32::UI::Controls::EM_SETMARGINS;
    use windows::Win32::UI::WindowsAndMessaging::{EC_LEFTMARGIN, EC_RIGHTMARGIN};
    let m = dpi::scale(theme::EDIT_INNER_MARGIN, dpi) as usize;
    send(
        hwnd,
        EM_SETMARGINS,
        (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
        (m | (m << 16)) as isize,
    );
}

/// Widest line of `text` in device pixels with the edit's font.
fn widest_line(font: HFONT, text: &str) -> i32 {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::GetTextExtentPoint32W;
    let dc = ScreenDc::new();
    let _sel = Select::new(dc.0, font);
    text.lines()
        .map(|line| {
            let wide: Vec<u16> = line.encode_utf16().collect();
            let mut size = SIZE::default();
            // SAFETY: Valid DC with the font selected; `wide` and `size` live for the call.
            unsafe {
                let _ = GetTextExtentPoint32W(dc.0, &wide, &mut size);
            }
            size.cx
        })
        .max()
        .unwrap_or(0)
}

/// Shows each scroll bar of a text well only when its text needs it (DESIGN.md 4). The
/// native edit shows both bars always; `longest` is kept across appends.
pub(crate) fn edit_update_bars(hwnd: HWND) {
    use windows::Win32::UI::Controls::{EM_GETLINECOUNT, ShowScrollBar};
    use windows::Win32::UI::WindowsAndMessaging::{SB_HORZ, SB_VERT, SM_CYHSCROLL};
    let Some(st) = state_of(hwnd) else {
        return;
    };
    let Data::Edit(e) = &st.data else {
        return;
    };
    if e.updating_bars.replace(true) {
        return;
    }
    struct Done<'a>(&'a Cell<bool>);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let _done = Done(&e.updating_bars);
    let dpi = st.dpi.get();
    // Window size minus the native frame: the client size with no scroll bars.
    let mut rc = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: Writable RECT and POINT of our own window.
    let known = unsafe {
        GetWindowRect(hwnd, &mut rc).is_ok() && ClientToScreen(hwnd, &mut origin).as_bool()
    };
    if !known {
        return;
    }
    let frame = origin.x - rc.left;
    let full_w = rc.right - rc.left - 2 * frame;
    let full_h = rc.bottom - rc.top - 2 * frame;
    let font = st.font.get();
    let dc = ScreenDc::new();
    let mut tm = TEXTMETRICW::default();
    {
        let _sel = Select::new(dc.0, font);
        // SAFETY: `tm` is writable; the DC has the font selected.
        let _ = unsafe { GetTextMetricsW(dc.0, &mut tm) };
    }
    let line_h = (tm.tmHeight + tm.tmExternalLeading).max(1);
    let lines = send(hwnd, EM_GETLINECOUNT, 0, 0).0 as i32;
    let text_h = lines * line_h;
    let margins = 2 * dpi::scale(theme::EDIT_INNER_MARGIN, dpi);
    let text_w = e.longest.get() + margins + 2;
    let bar_w = dpi::metric(SM_CXVSCROLL, dpi);
    let bar_h = dpi::metric(SM_CYHSCROLL, dpi);
    let horizontal_possible = !e.spec.word_wrap;
    let mut vertical = text_h > full_h;
    let horizontal = horizontal_possible && text_w > full_w - if vertical { bar_w } else { 0 };
    if horizontal {
        vertical = text_h > full_h - bar_h;
    }
    // SAFETY: Scroll bar visibility changes on our own control.
    unsafe {
        let _ = ShowScrollBar(hwnd, SB_VERT, vertical);
        if horizontal_possible {
            let _ = ShowScrollBar(hwnd, SB_HORZ, horizontal);
        }
    }
}

/// Copies the whole text of a read-only edit to the clipboard, keeping its selection.
pub fn edit_copy_all(hwnd: HWND) {
    use windows::Win32::UI::Controls::EM_GETSEL;
    use windows::Win32::UI::WindowsAndMessaging::WM_COPY;
    let (mut start, mut end) = (0u32, 0u32);
    send(
        hwnd,
        EM_GETSEL,
        &mut start as *mut u32 as usize,
        &mut end as *mut u32 as isize,
    );
    send(hwnd, EM_SETSEL, 0, -1);
    send(hwnd, WM_COPY, 0, 0);
    send(hwnd, EM_SETSEL, start as usize, end as isize);
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

/// `Control.Enabled = value`. Focus is never lost: disabling the focused control first moves
/// the focus to the next tab stop, like WinForms `SelectNextIfFocused` (DESIGN.md 8.2).
pub fn set_enabled(hwnd: HWND, enabled: bool) {
    // SAFETY: Focus, tab-order, and enable-state calls on windows of this thread.
    unsafe {
        if !enabled && GetFocus() == hwnd {
            let root = GetAncestor(hwnd, GA_ROOT);
            if let Ok(next) = GetNextDlgTabItem(root, Some(hwnd), false)
                && next != hwnd
            {
                let _ = SetFocus(Some(next));
            }
        }
        let _ = EnableWindow(hwnd, enabled);
    }
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
        && !enabled
    {
        b.hover.set(false);
    }
    invalidate(hwnd);
}

/// Marks a sidebar button as the active item (the shown section) or not.
pub fn set_active(hwnd: HWND, active: bool) {
    if let Some(st) = state_of(hwnd)
        && let Data::Button(b) = &st.data
        && b.active.get() != active
    {
        b.active.set(active);
        invalidate(hwnd);
    }
}

/// Whether a sidebar button is the active item.
pub fn is_active(hwnd: HWND) -> bool {
    state_of(hwnd).is_some_and(|s| match &s.data {
        Data::Button(b) => b.active.get(),
        _ => false,
    })
}

/// Sets a label's text color (the status map of `DESIGN.md` section 3).
pub fn set_label_color(hwnd: HWND, color: Color) {
    if let Some(st) = state_of(hwnd)
        && let Data::Label(l) = &st.data
    {
        l.borrow_mut().fore = color;
        invalidate(hwnd);
    }
}

/// Replaces the whole text of an edit.
pub fn edit_set_text(hwnd: HWND, text: &str) {
    let wide = to_wide(text);
    // SAFETY: NUL-terminated buffer valid for the call.
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
    }
    if let Some(st) = state_of(hwnd)
        && let Data::Edit(e) = &st.data
    {
        e.longest.set(widest_line(st.font.get(), text));
    }
    edit_update_bars(hwnd);
}

/// `TextBox.AppendText` + `SelectionStart = TextLength` + `ScrollToCaret`.
pub fn edit_append(hwnd: HWND, text: &str) {
    append_raw(hwnd, text);
    edit_update_bars(hwnd);
    send(hwnd, EM_SCROLLCARET, 0, 0);
}

fn append_raw(hwnd: HWND, text: &str) {
    let wide = to_wide(text);
    // SAFETY: Length query on our edit.
    let len = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    send(hwnd, EM_SETSEL, len, len as isize);
    send(hwnd, EM_REPLACESEL, 0, wide.as_ptr() as isize);
    // SAFETY: Length query on our edit.
    let len = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    send(hwnd, EM_SETSEL, len, len as isize);
    if let Some(st) = state_of(hwnd)
        && let Data::Edit(e) = &st.data
    {
        // The last line before the append may have grown: measure it with the new text.
        let w = widest_line(st.font.get(), text);
        e.longest.set(e.longest.get().max(w));
    }
}

/// Appends many chunks with redraw off, then repaints once (bulk status output).
pub fn edit_append_batch(hwnd: HWND, chunks: &[&str]) {
    send(hwnd, WM_SETREDRAW, 0, 0);
    for c in chunks {
        append_raw(hwnd, c);
    }
    edit_update_bars(hwnd);
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
    l.checked.borrow_mut().clear();
    for (text, checked) in items {
        let wide = to_wide(&list_accessible_name(text, *checked));
        if send(hwnd, LB_ADDSTRING, 0, wide.as_ptr() as isize).0 < 0 {
            win::record(win::Error::msg(
                "Populate checked list",
                "item insertion failed",
            ));
            break;
        }
        l.checked.borrow_mut().push(*checked);
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
        st.set_list_checked(l, index, checked);
    }
}

fn list_accessible_name(text: &str, checked: bool) -> String {
    format!("{text}, {}", if checked { "checked" } else { "unchecked" })
}

fn list_display_name(text: &str) -> &str {
    text.strip_suffix(", unchecked")
        .or_else(|| text.strip_suffix(", checked"))
        .unwrap_or(text)
}

/// `ProgressBar.Value`.
pub fn progress_set(hwnd: HWND, value: u32) {
    if let Some(st) = state_of(hwnd)
        && let Data::Progress(p) = &st.data
    {
        p.pos.set(value.min(100) as i32);
        invalidate(hwnd);
    }
}

/// `ProgressBarStyle.Marquee` on or off (the control's own 30 ms timer moves the block).
pub fn progress_marquee(hwnd: HWND, on: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};
    if let Some(st) = state_of(hwnd)
        && let Data::Progress(p) = &st.data
    {
        p.marquee.set(on);
        // SAFETY: A timer on our own control window, no callback.
        unsafe {
            if on {
                SetTimer(
                    Some(hwnd),
                    MARQUEE_TIMER,
                    theme::MARQUEE_STEP_MS as u32,
                    None,
                );
            } else {
                let _ = KillTimer(Some(hwnd), MARQUEE_TIMER);
            }
        }
        invalidate(hwnd);
    }
}
