//! Owned by WP-10a: window lifetime, dispatch, and safe message-box boundary.
//!
//! A form is a top-level window built from a [`FormSpec`] and a tree of layout [`Node`]s. Every
//! container node gets a child window that paints its back color; every leaf gets a native
//! control (`controls.rs`). Form code receives [`Event`]s in one handler closure and drives the
//! window through the [`Form`] handle.
//!
//! Re-entrancy rule for form code: the handler is `Fn`, so keep form state in `Cell`/`RefCell`
//! and never hold a borrow across a call that can pump messages (`run_modal`, message boxes).

use super::controls::{self, CtlEvent};
use super::dpi::{self, Font};
use super::layout::{self, Kind, Node, Rect, Size};
use super::theme::{self, Color};
use crate::win::{self, wide::to_wide};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute},
            Gdi::{
                BeginPaint, EndPaint, FillRect, GetMonitorInfoW, HDC, MONITOR_DEFAULTTONEAREST,
                MONITORINFO, MonitorFromPoint, MonitorFromWindow, PAINTSTRUCT, UpdateWindow,
            },
        },
        System::{
            LibraryLoader::{
                GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
            },
            SystemInformation::OSVERSIONINFOW,
        },
        UI::{
            Controls::{
                BufferedPaintInit, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
                InitCommonControlsEx, SetScrollInfo, SetWindowTheme, ShowScrollBar,
            },
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, IsWindowEnabled, SetFocus, VK_ESCAPE, VK_RETURN,
            },
            WindowsAndMessaging::{
                BeginDeferWindowPos, CREATESTRUCTW, CS_DBLCLKS, CreateWindowExW, DefWindowProcW,
                DeferWindowPos, DestroyWindow, DispatchMessageW, EndDeferWindowPos, GA_ROOT,
                GCW_ATOM, GWLP_USERDATA, GetAncestor, GetClassLongPtrW, GetClientRect,
                GetCursorPos, GetMessageW, GetNextDlgTabItem, GetWindowLongPtrW, GetWindowRect,
                HICON, IDC_ARROW, IsDialogMessageW, IsWindow, KillTimer, LoadCursorW, LoadIconW,
                MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MINMAXINFO, MSG, MessageBoxW,
                PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SB_BOTTOM,
                SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBTRACK, SB_TOP, SB_VERT,
                SCROLLINFO, SIF_ALL, SIZE_MINIMIZED, SM_CXVSCROLL, SW_SHOWMAXIMIZED, SW_SHOWNORMAL,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetTimer,
                SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
                WA_INACTIVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CLOSE, WM_COMMAND,
                WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED,
                WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_MOUSEWHEEL,
                WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFOCUS, WM_SIZE, WM_TIMER, WM_VKEYTOITEM,
                WM_VSCROLL, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
                WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME, WS_MAXIMIZEBOX,
                WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
    core::{BOOL, PCSTR, PCWSTR, w},
};

/// Shows a blocking error message with the supplied owner and title.
pub fn show_error(owner: HWND, text: &str, title: &str) {
    let text = to_wide(text);
    let title = to_wide(title);
    // SAFETY: The terminated buffers live through the modal call; a null owner is valid.
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// Shows a blocking informational message with the supplied owner and title.
pub fn show_info(owner: HWND, text: &str, title: &str) {
    let text = to_wide(text);
    let title = to_wide(title);
    // SAFETY: The terminated buffers live through the modal call; a null owner is valid.
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        )
    };
}

// ---------------------------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------------------------

/// Something that happened to a form; delivered to its handler.
#[derive(Debug)]
pub enum Event {
    /// All controls exist and the layout ran; the window is not visible yet (C# `Load`).
    Created,
    /// A button was clicked (mouse, Space, Enter, or a double click counted as a click).
    Click(u16),
    /// A checked list item was toggled.
    ItemCheck {
        /// List control id.
        id: u16,
        /// Item index.
        index: usize,
        /// New check state.
        checked: bool,
    },
    /// Title-bar X, Alt+F4, or `Form::close`; return `false` from the handler to keep the form.
    CloseRequest,
    /// The client area changed size (also during creation, before `Created`, and after a DPI
    /// change); the layout pass runs right after the handler returns.
    Resize {
        /// Client size in device pixels.
        client: Size,
        /// Current DPI.
        dpi: u32,
    },
    /// A timer from `Form::set_timer` fired.
    Timer(usize),
    /// A worker message of the current generation (see `Form::poster`).
    Worker(Box<dyn Any + Send>),
    /// The window is being destroyed.
    Destroyed,
}

/// `FormBorderStyle` and the min/max boxes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormStyle {
    /// `FormBorderStyle.Sizable`.
    Sizable {
        /// `MaximizeBox`.
        maximize: bool,
        /// `MinimizeBox`.
        minimize: bool,
    },
    /// `FormBorderStyle.FixedDialog`, no boxes.
    FixedDialog,
}

/// `StartPosition`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartPosition {
    /// Centered on the work area of the monitor under the mouse.
    CenterScreen,
    /// Centered on the owner window (kept inside the owner's work area).
    CenterParent,
}

/// The initial window size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowSize {
    /// `ClientSize` (logical pixels, scaled by DPI like `AutoScaleMode.Dpi`).
    Client(Size),
    /// Outer `Size`; `scaled` = multiply by DPI scale (approved for Old View and update, AD-39).
    Outer {
        /// Logical outer size.
        size: Size,
        /// Scale by DPI.
        scaled: bool,
    },
}

/// Everything about a form except its controls.
#[derive(Clone, Debug, PartialEq)]
pub struct FormSpec {
    /// Window title.
    pub title: String,
    /// Initial size.
    pub size: WindowSize,
    /// `MinimumSize` (outer, logical pixels, scaled by DPI).
    pub min: Option<Size>,
    /// Border style.
    pub style: FormStyle,
    /// Start position.
    pub start: StartPosition,
    /// `BackColor`.
    pub back: Color,
    /// `AcceptButton` (Enter).
    pub accept: Option<u16>,
    /// `CancelButton` (Esc).
    pub cancel: Option<u16>,
    /// `ShowInTaskbar` (`WS_EX_APPWINDOW`).
    pub taskbar: bool,
    /// Start maximized when the outer size does not fit the work area (owner Q1, main window).
    pub maximize_if_too_big: bool,
}

impl FormSpec {
    /// A sizable, centered-on-parent, taskbar-visible form with the main theme background.
    pub fn new(title: &str, size: WindowSize) -> Self {
        Self {
            title: title.to_owned(),
            size,
            min: None,
            style: FormStyle::Sizable {
                maximize: true,
                minimize: true,
            },
            start: StartPosition::CenterParent,
            back: theme::MAIN_BACKGROUND,
            accept: None,
            cancel: None,
            taskbar: true,
            maximize_if_too_big: false,
        }
    }
}

/// A cheap handle to a live form; every call is a no-op once the window is gone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Form {
    hwnd: HWND,
    serial: u64,
}

/// Sends values from a worker thread to the form's handler as `Event::Worker`.
///
/// Values posted under an older generation (see `Form::next_generation`) are dropped.
#[derive(Clone, Debug)]
pub struct Poster {
    hwnd: isize,
    generation: u64,
    tx: Sender<(u64, Box<dyn Any + Send>)>,
}

impl Poster {
    /// Queues `value` for the form; returns false when the form is gone.
    pub fn post<T: Any + Send>(&self, value: T) -> bool {
        if self.tx.send((self.generation, Box::new(value))).is_err() {
            return false;
        }
        // SAFETY: Posting a registered (system-unique) message carries no pointers; a stale or
        // reused handle only receives a message id it does not know.
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut core::ffi::c_void)),
                wake_message(),
                WPARAM(0),
                LPARAM(0),
            )
        }
        .is_ok()
    }
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

const ROOT_ID: u16 = 0xFFFF;
const FIRST_AUTO_ID: u16 = 0xFFFE;

struct FormState {
    hwnd: Cell<HWND>,
    serial: u64,
    spec: FormSpec,
    owner: HWND,
    modal: Cell<bool>,
    handler: HandlerRc,
    tree: RefCell<Node>,
    hwnds: RefCell<HashMap<u16, HWND>>,
    fonts: RefCell<Vec<Font>>,
    dpi: Cell<u32>,
    brush: RefCell<Option<controls::Brush>>,
    generation: Cell<u64>,
    tx: Sender<(u64, Box<dyn Any + Send>)>,
    rx: Receiver<(u64, Box<dyn Any + Send>)>,
    layout_count: Cell<u32>,
    in_dpi_change: Cell<bool>,
    default_button: Cell<HWND>,
    last_focus: Cell<HWND>,
    destroyed: Cell<bool>,
    maximize: Cell<bool>,
}

type HandlerRc = Rc<dyn Fn(&Form, Event) -> bool>;

struct PanelState {
    brush: controls::Brush,
}

thread_local! {
    static NEXT_SERIAL: Cell<u64> = const { Cell::new(1) };
}

fn form_class() -> PCWSTR {
    w!("HWIDChecker.Form")
}

fn panel_class() -> PCWSTR {
    w!("HWIDChecker.Panel")
}

struct Atoms {
    form: u16,
    panel: u16,
}

fn atoms() -> &'static Atoms {
    static ATOMS: OnceLock<Atoms> = OnceLock::new();
    ATOMS.get_or_init(|| {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_PROGRESS_CLASS,
        };
        // SAFETY: Registers the common control classes (progress bar) for this process; a
        // failure surfaces later as a CreateWindowExW error. Buffered paint is initialized once
        // for the UI thread and kept for the process lifetime; without it BeginBufferedPaint
        // still works, only slower.
        unsafe {
            let _ = InitCommonControlsEx(&icc);
            let _ = BufferedPaintInit();
        }
        // SAFETY: Module handle of this process; no ownership.
        let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        // SAFETY: Resource id 1 is the app icon from app.rc; a missing icon gives an error.
        let icon = unsafe {
            LoadIconW(
                Some(instance.into()),
                PCWSTR(std::ptr::without_provenance(1)),
            )
        }
        .unwrap_or(HICON::default());
        // SAFETY: IDC_ARROW is a shared system cursor.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
        let form = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(form_proc),
            hInstance: instance.into(),
            hIcon: icon,
            hCursor: cursor,
            lpszClassName: form_class(),
            ..Default::default()
        };
        let panel = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(panel_proc),
            hInstance: instance.into(),
            hCursor: cursor,
            lpszClassName: panel_class(),
            ..Default::default()
        };
        // SAFETY: Both structures are fully initialized; class names are static strings.
        unsafe {
            Atoms {
                form: RegisterClassExW(&form),
                panel: RegisterClassExW(&panel),
            }
        }
    })
}

fn wake_message() -> u32 {
    static MSG: OnceLock<u32> = OnceLock::new();
    // SAFETY: Static NUL-terminated name.
    *MSG.get_or_init(|| unsafe { RegisterWindowMessageW(w!("HWIDChecker.Kit.Wake")) })
}

fn class_atom(hwnd: HWND) -> u16 {
    // SAFETY: Reads the class atom of a window handle; 0 for invalid handles.
    unsafe { GetClassLongPtrW(hwnd, GCW_ATOM) as u16 }
}

/// Clones the `Rc` stored in `GWLP_USERDATA` of a window of class `atom`.
fn user_rc<T>(hwnd: HWND, atom: u16) -> Option<Rc<T>> {
    if atom == 0 || class_atom(hwnd) != atom {
        return None;
    }
    // SAFETY: Windows of our classes store Rc::into_raw of T here (or 0 before NCCREATE/after
    // NCDESTROY); the incremented count gives the caller its own strong reference.
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const T;
        if ptr.is_null() {
            return None;
        }
        Rc::increment_strong_count(ptr);
        Some(Rc::from_raw(ptr))
    }
}

fn form_state(hwnd: HWND) -> Option<Rc<FormState>> {
    user_rc(hwnd, atoms().form)
}

fn root_of(hwnd: HWND) -> HWND {
    // SAFETY: Read-only window relationship query.
    unsafe { GetAncestor(hwnd, GA_ROOT) }
}

fn client_size(hwnd: HWND) -> Size {
    let mut rc = RECT::default();
    // SAFETY: `rc` is writable.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    Size {
        w: rc.right - rc.left,
        h: rc.bottom - rc.top,
    }
}

fn fill_client(hwnd: HWND, hdc: HDC, brush: &controls::Brush) {
    let mut rc = RECT::default();
    // SAFETY: Valid window, DC from the erase message, and owned brush.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, brush.handle());
    }
}

fn validate_paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint pair inside WM_PAINT of our own window.
    unsafe {
        BeginPaint(hwnd, &mut ps);
        let _ = EndPaint(hwnd, &ps);
    }
}

impl FormState {
    fn form(&self) -> Form {
        Form {
            hwnd: self.hwnd.get(),
            serial: self.serial,
        }
    }

    /// Runs the form handler; a panic in it is caught (the panic hook already reported it)
    /// and counts as `true`, so a broken handler can never keep a window from closing.
    fn dispatch(&self, event: Event) -> bool {
        let handler = Rc::clone(&self.handler);
        let form = self.form();
        catch_unwind(AssertUnwindSafe(|| handler(&form, event))).unwrap_or(true)
    }

    fn font(&self, spec: theme::FontSpec) -> windows::Win32::Graphics::Gdi::HFONT {
        let fonts = self.fonts.borrow();
        fonts
            .iter()
            .find(|f| f.spec() == spec)
            .or_else(|| fonts.first())
            .map(Font::handle)
            .unwrap_or_default()
    }

    fn ensure_fonts(&self, node: &Node) -> win::Result<()> {
        if let Kind::Leaf(ctl) = &node.kind {
            let spec = ctl.font();
            let dpi = self.dpi.get();
            if !self.fonts.borrow().iter().any(|f| f.spec() == spec) {
                self.fonts.borrow_mut().push(Font::new(spec, dpi)?);
            }
        }
        for c in node.children() {
            self.ensure_fonts(c)?;
        }
        Ok(())
    }

    fn hwnd_of(&self, id: u16) -> Option<HWND> {
        self.hwnds.borrow().get(&id).copied()
    }

    /// Creates the child windows of `node` under `parent`.
    fn build(&self, parent: HWND, node: &Node, inherited: Color) -> win::Result<()> {
        for child in node.children() {
            let back = child.back.unwrap_or(inherited);
            match &child.kind {
                Kind::Leaf(ctl) => {
                    let font = self.font(ctl.font());
                    let hwnd = controls::create(parent, child, back, font, self.dpi.get())?;
                    if matches!(ctl, controls::Ctl::Edit(_) | controls::Ctl::CheckedList(_)) {
                        dark_scrollbars(hwnd);
                    }
                    self.hwnds.borrow_mut().insert(child.id, hwnd);
                }
                _ => {
                    let hwnd = create_panel(parent, child, back)?;
                    if child.scroll {
                        dark_scrollbars(hwnd);
                    }
                    self.hwnds.borrow_mut().insert(child.id, hwnd);
                    self.build(hwnd, child, back)?;
                }
            }
        }
        Ok(())
    }

    /// One layout pass: arrange the tree, then move every child window (one `DeferWindowPos`
    /// batch per parent), then update scroll bars and repaint resized painted controls.
    fn relayout(&self) {
        let hwnd = self.hwnd.get();
        let client = client_size(hwnd);
        let dpi = self.dpi.get();
        let bar = dpi::metric(SM_CXVSCROLL, dpi);
        let mut placements: Vec<(HWND, HWND, Rect, Rect)> = Vec::new();
        let mut scrolls: Vec<(HWND, bool, i32, i32, i32)> = Vec::new();
        {
            let Ok(mut tree) = self.tree.try_borrow_mut() else {
                return;
            };
            let mut old = HashMap::new();
            collect_bounds(&tree, &mut old);
            set_scroll_bar_width(&mut tree, bar);
            let fonts = self.fonts.borrow();
            let font_of = |spec: theme::FontSpec| {
                fonts
                    .iter()
                    .find(|f| f.spec() == spec)
                    .map(Font::handle)
                    .unwrap_or_default()
            };
            let mut measure = |node: &Node, proposed: Size| -> Size {
                match &node.kind {
                    Kind::Leaf(ctl) => controls::measure(
                        ctl,
                        font_of(ctl.font()),
                        node.padding,
                        node.min,
                        proposed,
                    ),
                    _ => Size::default(),
                }
            };
            layout::arrange(
                &mut tree,
                Rect {
                    x: 0,
                    y: 0,
                    w: client.w,
                    h: client.h,
                },
                &mut measure,
            );
            let hwnds = self.hwnds.borrow();
            collect_placements(&tree, hwnd, &hwnds, &old, &mut placements, &mut scrolls);
        }
        let mut parents: Vec<HWND> = placements.iter().map(|p| p.0).collect();
        parents.sort_by_key(|h| h.0 as usize);
        parents.dedup();
        for parent in parents {
            let group: Vec<_> = placements.iter().filter(|p| p.0 == parent).collect();
            // SAFETY: Every window in one batch shares `parent`, as DeferWindowPos requires.
            unsafe {
                let Ok(mut hdwp) = BeginDeferWindowPos(group.len() as i32) else {
                    continue;
                };
                for (_, child, r, _) in &group {
                    match DeferWindowPos(
                        hdwp,
                        *child,
                        None,
                        r.x,
                        r.y,
                        r.w.max(0),
                        r.h.max(0),
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    ) {
                        Ok(h) => hdwp = h,
                        Err(_) => break,
                    }
                }
                let _ = EndDeferWindowPos(hdwp);
            }
        }
        for (h, show, content, page, pos) in scrolls {
            let si = SCROLLINFO {
                cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                fMask: SIF_ALL,
                nMin: 0,
                nMax: (content - 1).max(0),
                nPage: page.max(0) as u32,
                nPos: pos,
                nTrackPos: 0,
            };
            // SAFETY: Scroll bar updates on our own panel window with a valid SCROLLINFO.
            unsafe {
                let _ = ShowScrollBar(h, SB_VERT, show);
                SetScrollInfo(h, SB_VERT, &si, true);
            }
        }
        for (_, child, new, old) in &placements {
            if new.w != old.w || new.h != old.h {
                // SAFETY: Repaint request for a child of this form.
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(*child), None, true);
                }
            }
        }
        self.layout_count.set(self.layout_count.get() + 1);
    }

    fn destroy(&self) {
        if self.destroyed.get() {
            return;
        }
        if self.modal.get() && !self.owner.is_invalid() {
            // Re-enable the owner first so Windows activates it, not another app.
            // SAFETY: Enabling the owner window that `run_modal` disabled.
            unsafe {
                let _ = EnableWindow(self.owner, true);
            }
        }
        // SAFETY: Destroys our own top-level window on its thread.
        unsafe {
            let _ = DestroyWindow(self.hwnd.get());
        }
    }

    fn on_dpi_changed(&self, new_dpi: u32, suggested: RECT) {
        let old = self.dpi.get();
        self.in_dpi_change.set(true);
        if let Ok(mut tree) = self.tree.try_borrow_mut() {
            tree.rescale(old, new_dpi);
        }
        self.dpi.set(new_dpi);
        let specs: Vec<theme::FontSpec> = self.fonts.borrow().iter().map(Font::spec).collect();
        let fresh: Vec<Font> = specs
            .into_iter()
            .filter_map(|s| Font::new(s, new_dpi).ok())
            .collect();
        let old_fonts = std::mem::replace(&mut *self.fonts.borrow_mut(), fresh);
        {
            let tree = self.tree.borrow();
            let hwnds = self.hwnds.borrow();
            apply_fonts(&tree, &hwnds, self, new_dpi);
        }
        drop(old_fonts);
        // SAFETY: Moves our own window to the rectangle Windows suggested for the new DPI.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd.get(),
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.in_dpi_change.set(false);
        self.dispatch(Event::Resize {
            client: client_size(self.hwnd.get()),
            dpi: new_dpi,
        });
        self.relayout();
        // SAFETY: Repaint everything at the new scale.
        unsafe {
            let _ =
                windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd.get()), None, true);
        }
    }

    fn focus_changed(&self, hwnd: HWND) {
        self.last_focus.set(hwnd);
        let accept = self
            .spec
            .accept
            .and_then(|id| self.hwnd_of(id))
            .unwrap_or_default();
        let new_default = if controls::is_button(hwnd) {
            hwnd
        } else {
            accept
        };
        let old = self.default_button.get();
        if old != new_default {
            if !old.is_invalid() {
                controls::set_default(old, false);
            }
            if !new_default.is_invalid() {
                controls::set_default(new_default, true);
            }
            self.default_button.set(new_default);
        }
    }

    fn scroll(&self, panel: HWND, request: ScrollRequest) {
        let id = self
            .hwnds
            .borrow()
            .iter()
            .find(|(_, h)| **h == panel)
            .map(|(id, _)| *id);
        let Some(id) = id else {
            return;
        };
        let line = dpi::scale(theme::SCROLL_LINE, self.dpi.get());
        {
            let Ok(mut tree) = self.tree.try_borrow_mut() else {
                return;
            };
            let Some(node) = tree.find_mut(id) else {
                return;
            };
            let page = node.bounds.h;
            let max = (node.content_height - page).max(0);
            let pos = node.scroll_pos;
            node.scroll_pos = match request {
                ScrollRequest::Wheel(delta) => pos - delta,
                ScrollRequest::Bar(code, track) => match code {
                    c if c == SB_LINEUP.0 => pos - line,
                    c if c == SB_LINEDOWN.0 => pos + line,
                    c if c == SB_PAGEUP.0 => pos - page,
                    c if c == SB_PAGEDOWN.0 => pos + page,
                    c if c == SB_THUMBTRACK.0 => track,
                    c if c == SB_TOP.0 => 0,
                    c if c == SB_BOTTOM.0 => max,
                    _ => pos,
                },
            }
            .clamp(0, max);
            if node.scroll_pos == pos {
                return;
            }
        }
        self.relayout();
    }
}

enum ScrollRequest {
    Wheel(i32),
    Bar(i32, i32),
}

fn collect_bounds(node: &Node, out: &mut HashMap<u16, Rect>) {
    for c in node.children() {
        out.insert(c.id, c.bounds);
        collect_bounds(c, out);
    }
}

fn set_scroll_bar_width(node: &mut Node, bar: i32) {
    node.scroll_bar_width = bar;
    for c in node.children_mut() {
        set_scroll_bar_width(c, bar);
    }
}

fn collect_placements(
    node: &Node,
    parent: HWND,
    hwnds: &HashMap<u16, HWND>,
    old: &HashMap<u16, Rect>,
    out: &mut Vec<(HWND, HWND, Rect, Rect)>,
    scrolls: &mut Vec<(HWND, bool, i32, i32, i32)>,
) {
    for c in node.children() {
        let Some(&h) = hwnds.get(&c.id) else {
            continue;
        };
        if c.visible {
            out.push((
                parent,
                h,
                c.bounds,
                old.get(&c.id).copied().unwrap_or_default(),
            ));
        }
        if c.scroll {
            scrolls.push((h, c.vscroll, c.content_height, c.bounds.h, c.scroll_pos));
        }
        if !c.is_leaf() {
            collect_placements(c, h, hwnds, old, out, scrolls);
        }
    }
}

fn apply_fonts(node: &Node, hwnds: &HashMap<u16, HWND>, state: &FormState, dpi: u32) {
    for c in node.children() {
        if let (Kind::Leaf(ctl), Some(&h)) = (&c.kind, hwnds.get(&c.id)) {
            controls::apply_dpi(h, state.font(ctl.font()), c.padding, dpi);
        }
        apply_fonts(c, hwnds, state, dpi);
    }
}

fn assign_ids(node: &mut Node, next: &mut u16) {
    for c in node.children_mut() {
        if c.id == 0 {
            c.id = *next;
            *next -= 1;
        }
        assign_ids(c, next);
    }
}

fn create_panel(parent: HWND, node: &Node, back: Color) -> win::Result<HWND> {
    let state = Rc::new(PanelState {
        brush: controls::Brush::new(back),
    });
    let raw = Rc::into_raw(state);
    let vis = if node.visible {
        WS_VISIBLE
    } else {
        WINDOW_STYLE(0)
    };
    let scroll = if node.scroll {
        WS_VSCROLL
    } else {
        WINDOW_STYLE(0)
    };
    // SAFETY: Static class name; `raw` is handed to WM_NCCREATE, which stores it, or reclaimed
    // below if creation fails before that.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            panel_class(),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | vis | scroll,
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            None,
            Some(raw as *const core::ffi::c_void),
        )
    };
    match hwnd {
        Ok(h) => Ok(h),
        Err(e) => {
            // Creation failed. If WM_NCCREATE ran, WM_NCDESTROY already freed `raw`; we cannot
            // tell which, so the rare early failure leaks one small allocation rather than risk
            // a double free.
            let _ = raw;
            Err(win::Error::from_win("CreateWindowExW", e))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Window procedures
// ---------------------------------------------------------------------------------------------

unsafe extern "system" fn panel_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = catch_unwind(AssertUnwindSafe(|| {
        panel_message(hwnd, msg, wparam, lparam)
    }));
    match result {
        Ok(Some(r)) => r,
        // SAFETY: Default processing with unchanged parameters (also after a caught panic).
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn panel_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_NCCREATE => {
            // SAFETY: For WM_NCCREATE, lParam points to the CREATESTRUCTW of this window.
            let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            // SAFETY: Stores the Rc pointer passed by `create_panel`.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
            None
        }
        WM_NCDESTROY => {
            // SAFETY: Takes back the pointer stored at WM_NCCREATE exactly once.
            unsafe {
                let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *const PanelState;
                if !ptr.is_null() {
                    drop(Rc::from_raw(ptr));
                }
            }
            None
        }
        WM_ERASEBKGND => {
            let state: Rc<PanelState> = user_rc(hwnd, atoms().panel)?;
            fill_client(hwnd, HDC(wparam.0 as *mut core::ffi::c_void), &state.brush);
            Some(LRESULT(1))
        }
        WM_PAINT => {
            validate_paint(hwnd);
            Some(LRESULT(0))
        }
        WM_VSCROLL => {
            let state = form_state(root_of(hwnd))?;
            let code = (wparam.0 & 0xFFFF) as i32;
            let track = ((wparam.0 >> 16) & 0xFFFF) as i32;
            state.scroll(hwnd, ScrollRequest::Bar(code, track));
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL => {
            let state = form_state(root_of(hwnd))?;
            // C# parity: ScrollableControl.OnMouseWheel scrolls by the raw wheel delta in pixels.
            let delta = i32::from((wparam.0 >> 16) as u16 as i16);
            state.scroll(hwnd, ScrollRequest::Wheel(delta));
            Some(LRESULT(0))
        }
        _ => reflect_to_control(msg, wparam, lparam),
    }
}

/// Reflects child-control notifications to the control and forwards its event to the form.
fn reflect_to_control(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_COMMAND | WM_DRAWITEM | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX
        | WM_VKEYTOITEM => {
            let (r, event) = controls::reflect(msg, wparam, lparam)?;
            if let Some((id, child, ev)) = event {
                control_event(child, id, ev);
            }
            Some(r)
        }
        _ => None,
    }
}

/// Delivers a control event to the form that owns `child`.
pub(crate) fn control_event(child: HWND, id: u16, event: CtlEvent) {
    let Some(state) = form_state(root_of(child)) else {
        return;
    };
    let ev = match event {
        CtlEvent::Click => Event::Click(id),
        CtlEvent::ItemCheck(index, checked) => Event::ItemCheck { id, index, checked },
    };
    state.dispatch(ev);
}

/// Called by kit controls on `WM_SETFOCUS` (default button and focus restore tracking).
pub(crate) fn focus_changed(child: HWND) {
    if let Some(state) = form_state(root_of(child)) {
        state.focus_changed(child);
    }
}

unsafe extern "system" fn form_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = catch_unwind(AssertUnwindSafe(|| form_message(hwnd, msg, wparam, lparam)));
    match result {
        Ok(Some(r)) => r,
        // SAFETY: Default processing with unchanged parameters (also after a caught panic).
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn form_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg == WM_NCCREATE {
        // SAFETY: For WM_NCCREATE, lParam points to the CREATESTRUCTW of this window.
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        // SAFETY: Stores the Rc pointer passed by `Form::create`.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
        if let Some(state) = form_state(hwnd) {
            state.hwnd.set(hwnd);
        }
        return None;
    }
    if msg == WM_NCDESTROY {
        // SAFETY: Takes back the pointer stored at WM_NCCREATE exactly once.
        unsafe {
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *const FormState;
            if !ptr.is_null() {
                drop(Rc::from_raw(ptr));
            }
        }
        return None;
    }
    let state = form_state(hwnd)?;
    match msg {
        WM_ERASEBKGND => {
            let brush = state.brush.borrow();
            let b = brush.as_ref()?;
            fill_client(hwnd, HDC(wparam.0 as *mut core::ffi::c_void), b);
            Some(LRESULT(1))
        }
        WM_PAINT => {
            validate_paint(hwnd);
            Some(LRESULT(0))
        }
        WM_SIZE => {
            if wparam.0 as u32 == SIZE_MINIMIZED {
                return Some(LRESULT(0));
            }
            if state.in_dpi_change.get() || state.tree.try_borrow().is_err() {
                return Some(LRESULT(0));
            }
            state.dispatch(Event::Resize {
                client: client_size(hwnd),
                dpi: state.dpi.get(),
            });
            state.relayout();
            Some(LRESULT(0))
        }
        WM_GETMINMAXINFO => {
            let min = state.spec.min?;
            let dpi = state.dpi.get();
            // SAFETY: For WM_GETMINMAXINFO, lParam points to a writable MINMAXINFO.
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            mmi.ptMinTrackSize.x = dpi::scale(min.w, dpi);
            mmi.ptMinTrackSize.y = dpi::scale(min.h, dpi);
            Some(LRESULT(0))
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            // SAFETY: For WM_DPICHANGED, lParam points to the suggested window RECT.
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            state.on_dpi_changed(new_dpi, suggested);
            Some(LRESULT(0))
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                return None;
            }
            restore_focus(&state);
            Some(LRESULT(0))
        }
        WM_SETFOCUS => {
            restore_focus(&state);
            Some(LRESULT(0))
        }
        WM_CLOSE => {
            if state.dispatch(Event::CloseRequest) {
                state.destroy();
            }
            Some(LRESULT(0))
        }
        WM_DESTROY => {
            state.dispatch(Event::Destroyed);
            state.destroyed.set(true);
            Some(LRESULT(0))
        }
        WM_TIMER => {
            state.dispatch(Event::Timer(wparam.0));
            Some(LRESULT(0))
        }
        WM_COMMAND if lparam.0 == 0 => Some(LRESULT(0)),
        m if m == wake_message() => {
            let pending: Vec<_> = state.rx.try_iter().collect();
            for (generation, value) in pending {
                if generation == state.generation.get() && !state.destroyed.get() {
                    state.dispatch(Event::Worker(value));
                }
            }
            Some(LRESULT(0))
        }
        _ => reflect_to_control(msg, wparam, lparam),
    }
}

fn restore_focus(state: &FormState) {
    let hwnd = state.hwnd.get();
    let last = state.last_focus.get();
    // SAFETY: Window queries and focus changes within this thread's windows.
    unsafe {
        let target =
            if !last.is_invalid() && IsWindow(Some(last)).as_bool() && root_of(last) == hwnd {
                last
            } else {
                GetNextDlgTabItem(hwnd, None, false).unwrap_or_default()
            };
        if !target.is_invalid() {
            let _ = SetFocus(Some(target));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Form creation and the Form handle
// ---------------------------------------------------------------------------------------------

fn styles(spec: &FormSpec) -> (WINDOW_STYLE, WINDOW_EX_STYLE) {
    let mut style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let mut ex = WS_EX_CONTROLPARENT;
    match spec.style {
        FormStyle::Sizable { maximize, minimize } => {
            style |= WS_THICKFRAME;
            if maximize {
                style |= WS_MAXIMIZEBOX;
            }
            if minimize {
                style |= WS_MINIMIZEBOX;
            }
        }
        FormStyle::FixedDialog => ex |= WS_EX_DLGMODALFRAME,
    }
    if spec.taskbar {
        ex |= WS_EX_APPWINDOW;
    }
    (style, ex)
}

fn work_area(owner: HWND, start: StartPosition) -> RECT {
    // SAFETY: Monitor queries with valid out-parameters.
    unsafe {
        let monitor = if start == StartPosition::CenterParent && !owner.is_invalid() {
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST)
        } else {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
        };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut mi);
        mi.rcWork
    }
}

impl Form {
    /// Creates a hidden form owned by `owner` (null = none); call [`Form::show`] next.
    pub fn create(
        owner: HWND,
        spec: FormSpec,
        children: Vec<Node>,
        handler: impl Fn(&Form, Event) -> bool + 'static,
    ) -> win::Result<Form> {
        let _ = atoms();
        let (style, ex) = styles(&spec);
        let serial = NEXT_SERIAL.with(|s| {
            let v = s.get();
            s.set(v + 1);
            v
        });
        let (tx, rx) = channel();
        let mut root = Node::panel(children).id(ROOT_ID);
        root.margin = theme::NO_PAD;
        let mut next = FIRST_AUTO_ID;
        assign_ids(&mut root, &mut next);
        let state = Rc::new(FormState {
            hwnd: Cell::new(HWND::default()),
            serial,
            spec: spec.clone(),
            owner,
            modal: Cell::new(false),
            handler: Rc::new(handler),
            tree: RefCell::new(Node::panel(Vec::new())),
            hwnds: RefCell::new(HashMap::new()),
            fonts: RefCell::new(Vec::new()),
            dpi: Cell::new(dpi::BASE_DPI),
            brush: RefCell::new(Some(controls::Brush::new(spec.back))),
            generation: Cell::new(0),
            tx,
            rx,
            layout_count: Cell::new(0),
            in_dpi_change: Cell::new(false),
            default_button: Cell::new(HWND::default()),
            last_focus: Cell::new(HWND::default()),
            destroyed: Cell::new(false),
            maximize: Cell::new(false),
        });
        let work = work_area(owner, spec.start);
        let title = to_wide(&spec.title);
        let raw = Rc::into_raw(Rc::clone(&state));
        // SAFETY: Static class name, NUL-terminated title; `raw` is stored at WM_NCCREATE and
        // released at WM_NCDESTROY.
        let created = unsafe {
            CreateWindowExW(
                ex,
                form_class(),
                PCWSTR(title.as_ptr()),
                style,
                work.left,
                work.top,
                1,
                1,
                if owner.is_invalid() {
                    None
                } else {
                    Some(owner)
                },
                None,
                None,
                Some(raw as *const core::ffi::c_void),
            )
        };
        let hwnd = match created {
            Ok(h) => h,
            Err(e) => {
                // Creation fails before WM_NCCREATE only when the class is missing; otherwise
                // WM_NCDESTROY already reclaimed `raw`, so it is never freed here.
                let _ = raw;
                return Err(win::Error::from_win("CreateWindowExW", e));
            }
        };
        let dpi = dpi::window_dpi(hwnd);
        state.dpi.set(dpi);
        // Always called (also at 96 DPI) so the logical values are captured.
        root.rescale(dpi::BASE_DPI, dpi);
        let built = state
            .ensure_fonts(&root)
            .and_then(|()| state.build(hwnd, &root, spec.back));
        *state.tree.borrow_mut() = root;
        if let Err(e) = built {
            state.destroy();
            return Err(e);
        }
        if let Some(accept) = spec.accept.and_then(|id| state.hwnd_of(id)) {
            controls::set_default(accept, true);
            state.default_button.set(accept);
        }
        let outer = match spec.size {
            WindowSize::Client(s) => {
                dpi::outer_for_client(dpi::scale_size(s, dpi), style, ex, dpi)?
            }
            WindowSize::Outer { size, scaled } => {
                if scaled {
                    dpi::scale_size(size, dpi)
                } else {
                    size
                }
            }
        };
        let (work_w, work_h) = (work.right - work.left, work.bottom - work.top);
        let (mut x, mut y) = (
            work.left + (work_w - outer.w) / 2,
            work.top + (work_h - outer.h) / 2,
        );
        if spec.start == StartPosition::CenterParent && !owner.is_invalid() {
            let mut orc = RECT::default();
            // SAFETY: `orc` is writable; `owner` is a live window.
            unsafe {
                let _ = GetWindowRect(owner, &mut orc);
            }
            x = orc.left + (orc.right - orc.left - outer.w) / 2;
            y = orc.top + (orc.bottom - orc.top - outer.h) / 2;
            x = x.min(work.right - outer.w);
            y = y.min(work.bottom - outer.h);
        }
        x = x.max(work.left);
        y = y.max(work.top);
        state
            .maximize
            .set(spec.maximize_if_too_big && (outer.w > work_w || outer.h > work_h));
        // SAFETY: Sizes and places our own hidden window; WM_SIZE runs the first layout.
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                x,
                y,
                outer.w,
                outer.h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        state.relayout();
        dark_title_bar(hwnd);
        let form = state.form();
        state.dispatch(Event::Created);
        Ok(form)
    }

    fn state(&self) -> Option<Rc<FormState>> {
        form_state(self.hwnd).filter(|s| s.serial == self.serial)
    }

    /// The window handle (for owners of message boxes and child forms).
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Whether the window still exists.
    pub fn is_alive(&self) -> bool {
        self.state().is_some_and(|s| !s.destroyed.get())
    }

    /// Shows the window (maximized when `maximize_if_too_big` applied) and focuses the first
    /// tab stop.
    pub fn show(&self) {
        let Some(state) = self.state() else {
            return;
        };
        let cmd = if state.maximize.get() {
            SW_SHOWMAXIMIZED
        } else {
            SW_SHOWNORMAL
        };
        // SAFETY: Shows and paints our own window.
        unsafe {
            let _ = ShowWindow(self.hwnd, cmd);
            let _ = UpdateWindow(self.hwnd);
        }
        restore_focus(&state);
    }

    /// The native window of control or container `id`.
    pub fn control(&self, id: u16) -> Option<HWND> {
        self.state()?.hwnd_of(id)
    }

    fn with_control(&self, id: u16, f: impl FnOnce(HWND)) {
        if let Some(h) = self.control(id) {
            f(h);
        }
    }

    /// Sets the text of a button, label, or edit.
    pub fn set_text(&self, id: u16, text: &str) {
        self.with_control(id, |h| controls::set_text(h, text));
    }

    /// The text of a control.
    pub fn text(&self, id: u16) -> String {
        self.control(id).map(controls::text).unwrap_or_default()
    }

    /// `Control.Enabled`.
    pub fn set_enabled(&self, id: u16, enabled: bool) {
        self.with_control(id, |h| controls::set_enabled(h, enabled));
    }

    /// Whether a control is enabled.
    pub fn is_enabled(&self, id: u16) -> bool {
        // SAFETY: Read-only window query.
        self.control(id)
            .is_some_and(|h| unsafe { IsWindowEnabled(h) }.as_bool())
    }

    /// `Control.Visible` (takes part in layout only when visible); runs a layout pass.
    pub fn set_visible(&self, id: u16, visible: bool) {
        let Some(state) = self.state() else {
            return;
        };
        if let Ok(mut tree) = state.tree.try_borrow_mut()
            && let Some(node) = tree.find_mut(id)
        {
            node.visible = visible;
        }
        self.with_control(id, |h| {
            let cmd = if visible {
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNA
            } else {
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE
            };
            // SAFETY: Shows or hides a child of this form.
            unsafe {
                let _ = ShowWindow(h, cmd);
            }
        });
        state.relayout();
    }

    /// Moves a control to the top of its siblings' z-order (`BringToFront`).
    pub fn bring_to_front(&self, id: u16) {
        self.with_control(id, |h| {
            // SAFETY: Z-order change of a child of this form.
            unsafe {
                let _ = SetWindowPos(
                    h,
                    Some(windows::Win32::UI::WindowsAndMessaging::HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        });
    }

    /// Gives keyboard focus to a control.
    pub fn focus(&self, id: u16) {
        self.with_control(id, |h| {
            // SAFETY: Focus change within this thread.
            unsafe {
                let _ = SetFocus(Some(h));
            }
        });
    }

    /// `PerformClick`: raises `Event::Click(id)` when the control is enabled.
    pub fn click(&self, id: u16) {
        if self.is_enabled(id)
            && let Some(state) = self.state()
        {
            state.dispatch(Event::Click(id));
        }
    }

    /// Appends text to an edit and scrolls to it (`AppendText` + `ScrollToCaret`).
    pub fn edit_append(&self, id: u16, text: &str) {
        self.with_control(id, |h| controls::edit_append(h, text));
    }

    /// Appends many chunks with redraw off, then repaints once.
    pub fn edit_append_batch(&self, id: u16, chunks: &[&str]) {
        self.with_control(id, |h| controls::edit_append_batch(h, chunks));
    }

    /// Replaces an edit's text.
    pub fn edit_set_text(&self, id: u16, text: &str) {
        self.with_control(id, |h| controls::edit_set_text(h, text));
    }

    /// Moves the caret to the start and scrolls there.
    pub fn edit_scroll_to_top(&self, id: u16) {
        self.with_control(id, controls::edit_scroll_to_top);
    }

    /// Replaces the items of a checked list (`(text, checked)`).
    pub fn list_set_items(&self, id: u16, items: &[(String, bool)]) {
        self.with_control(id, |h| controls::list_set_items(h, items));
    }

    /// Check states of a checked list.
    pub fn list_checked(&self, id: u16) -> Vec<bool> {
        self.control(id)
            .map(controls::list_checked)
            .unwrap_or_default()
    }

    /// Sets one item's check state.
    pub fn list_set_checked(&self, id: u16, index: usize, checked: bool) {
        self.with_control(id, |h| controls::list_set_checked(h, index, checked));
    }

    /// `ProgressBar.Value` (0 to 100).
    pub fn progress_set(&self, id: u16, value: u32) {
        self.with_control(id, |h| controls::progress_set(h, value));
    }

    /// Marquee style on or off.
    pub fn progress_marquee(&self, id: u16, on: bool) {
        self.with_control(id, |h| controls::progress_marquee(h, on));
    }

    /// Sets a button's back, text, and border colors.
    pub fn set_button_colors(&self, id: u16, back: Color, fore: Color, border: Color) {
        self.with_control(id, |h| controls::set_button_colors(h, back, fore, border));
    }

    /// A button's current back color.
    pub fn button_back(&self, id: u16) -> Option<Color> {
        self.control(id).and_then(controls::button_back)
    }

    /// Sets the window title.
    pub fn set_title(&self, title: &str) {
        let wide = to_wide(title);
        // SAFETY: NUL-terminated buffer valid for the call.
        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr()));
        }
    }

    /// Current DPI of the form.
    pub fn dpi(&self) -> u32 {
        self.state().map_or(dpi::BASE_DPI, |s| s.dpi.get())
    }

    /// Scales a logical length to the form's DPI.
    pub fn scale(&self, value: i32) -> i32 {
        dpi::scale(value, self.dpi())
    }

    /// Client size of the form in device pixels.
    pub fn client_size(&self) -> Size {
        client_size(self.hwnd)
    }

    /// Client size of a container in device pixels (scroll bar excluded), from the last layout.
    pub fn node_client(&self, id: u16) -> Option<Size> {
        let state = self.state()?;
        let tree = state.tree.try_borrow().ok()?;
        let node = tree.find(id)?;
        let bar = if node.vscroll {
            node.scroll_bar_width
        } else {
            0
        };
        Some(Size {
            w: node.bounds.w - bar,
            h: node.bounds.h,
        })
    }

    /// Whether a scroll container shows its vertical scroll bar.
    pub fn vscroll_visible(&self, id: u16) -> bool {
        self.state()
            .and_then(|s| {
                s.tree
                    .try_borrow()
                    .ok()
                    .and_then(|t| t.find(id).map(|n| n.vscroll))
            })
            .unwrap_or(false)
    }

    /// Edits the live layout tree (device pixels); call [`Form::relayout`] afterwards.
    ///
    /// Returns `None` when the form is gone or the tree is busy (inside a layout pass).
    pub fn with_tree<R>(&self, f: impl FnOnce(&mut Node) -> R) -> Option<R> {
        let state = self.state()?;
        let mut tree = state.tree.try_borrow_mut().ok()?;
        Some(f(&mut tree))
    }

    /// Runs one layout pass now.
    pub fn relayout(&self) {
        if let Some(state) = self.state() {
            state.relayout();
        }
    }

    /// Number of layout passes so far (diagnostics and the WP-10a spike).
    pub fn layout_count(&self) -> u32 {
        self.state().map_or(0, |s| s.layout_count.get())
    }

    /// Starts or restarts timer `id` (`Event::Timer(id)` every `ms`).
    pub fn set_timer(&self, id: usize, ms: u32) {
        // SAFETY: Window timer on our own window, no callback.
        unsafe {
            SetTimer(Some(self.hwnd), id, ms, None);
        }
    }

    /// Stops timer `id`.
    pub fn kill_timer(&self, id: usize) {
        // SAFETY: Window timer on our own window.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id);
        }
    }

    /// Starts a new worker generation; posters of older generations are ignored from now on.
    pub fn next_generation(&self) -> u64 {
        self.state().map_or(0, |s| {
            s.generation.set(s.generation.get() + 1);
            s.generation.get()
        })
    }

    /// A sender for worker threads, bound to the current generation.
    pub fn poster(&self) -> Option<Poster> {
        let state = self.state()?;
        Some(Poster {
            hwnd: self.hwnd.0 as isize,
            generation: state.generation.get(),
            tx: state.tx.clone(),
        })
    }

    /// Requests a close (`Event::CloseRequest` decides).
    pub fn close(&self) {
        // SAFETY: Posts WM_CLOSE to our own window.
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    /// Destroys the window now without asking.
    pub fn destroy(&self) {
        if let Some(state) = self.state() {
            state.destroy();
        }
    }
}

/// Creates a modal form, runs it until it closes, and re-enables `owner` (C# `ShowDialog`).
pub fn run_modal(
    owner: HWND,
    spec: FormSpec,
    children: Vec<Node>,
    handler: impl Fn(&Form, Event) -> bool + 'static,
) -> win::Result<()> {
    let form = Form::create(owner, spec, children, handler)?;
    let Some(state) = form.state() else {
        return Ok(());
    };
    if !owner.is_invalid() {
        state.modal.set(true);
        // SAFETY: Disables the owner for the modal loop; `destroy` re-enables it.
        unsafe {
            let _ = EnableWindow(owner, false);
        }
    }
    form.show();
    pump_until(|| state.destroyed.get());
    if !owner.is_invalid() {
        // SAFETY: Safety net if the window died without `destroy` (for example WM_QUIT).
        unsafe {
            let _ = EnableWindow(owner, true);
        }
    }
    Ok(())
}

/// Creates the main form and runs the message loop until it is destroyed.
pub fn run_main(
    spec: FormSpec,
    children: Vec<Node>,
    handler: impl Fn(&Form, Event) -> bool + 'static,
) -> win::Result<()> {
    let form = Form::create(HWND::default(), spec, children, handler)?;
    let Some(state) = form.state() else {
        return Ok(());
    };
    form.show();
    pump_until(|| state.destroyed.get());
    Ok(())
}

/// Runs the message loop with the kit's keyboard rules until `done` is true or `WM_QUIT`.
///
/// `WM_QUIT` is posted again so an outer loop also ends.
pub fn pump_until(done: impl Fn() -> bool) {
    while !done() {
        let mut msg = MSG::default();
        // SAFETY: `msg` is writable; all windows of this thread are pumped.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 == 0 {
            // SAFETY: Re-posts WM_QUIT for the next loop level.
            unsafe { PostQuitMessage(msg.wParam.0 as i32) };
            return;
        }
        if r.0 == -1 {
            return;
        }
        if !pre_translate(&msg) {
            // SAFETY: Standard translate/dispatch of a message from GetMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

/// Enter, Esc, and Tab handling for kit forms (WinForms `ProcessDialogKey`).
fn pre_translate(msg: &MSG) -> bool {
    if msg.hwnd.is_invalid() {
        return false;
    }
    let root = root_of(msg.hwnd);
    let Some(state) = form_state(root) else {
        return false;
    };
    if msg.message == WM_KEYDOWN {
        let vk = msg.wParam.0 as u16;
        let form = state.form();
        if vk == VK_RETURN.0 {
            // SAFETY: Reads this thread's focus window.
            let focus = unsafe { GetFocus() };
            if controls::is_button(focus) {
                // C# parity: a focused button is the default button; Enter clicks it.
                if let Some((id, _)) = state.hwnds.borrow().iter().find(|(_, h)| **h == focus) {
                    let id = *id;
                    form.click(id);
                }
            } else if let Some(accept) = state.spec.accept {
                form.click(accept);
            }
            return true;
        }
        if vk == VK_ESCAPE.0 {
            // C# parity: Esc does nothing on a form without CancelButton.
            if let Some(cancel) = state.spec.cancel {
                form.click(cancel);
            }
            return true;
        }
    }
    // SAFETY: Dialog navigation (Tab, Shift+Tab, arrows, mnemonics) for our top-level form.
    unsafe { IsDialogMessageW(root, msg) }.as_bool()
}

// ---------------------------------------------------------------------------------------------
// Dark title bar and scroll bars (OPT-17, AD-35, AD-36)
// ---------------------------------------------------------------------------------------------

type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type RtlGetVersion = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;

struct DarkApi {
    allow_window: Option<AllowDarkModeForWindow>,
}

/// Windows build number from `RtlGetVersion` (not subject to manifest version lies).
pub fn os_build() -> u32 {
    // SAFETY: ntdll is always loaded; the export is resolved by name and called with a
    // correctly sized structure.
    unsafe {
        let Ok(ntdll) = GetModuleHandleW(w!("ntdll.dll")) else {
            return 0;
        };
        let Some(f) = GetProcAddress(ntdll, PCSTR(c"RtlGetVersion".as_ptr().cast())) else {
            return 0;
        };
        let f: RtlGetVersion = std::mem::transmute(f);
        let mut v = OSVERSIONINFOW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        if f(&mut v) == 0 { v.dwBuildNumber } else { 0 }
    }
}

fn dark_api() -> &'static DarkApi {
    static API: OnceLock<DarkApi> = OnceLock::new();
    API.get_or_init(|| {
        if os_build() < 17763 {
            return DarkApi { allow_window: None };
        }
        // SAFETY: uxtheme is a System32 DLL loaded by full search path; the module stays
        // loaded for the process lifetime (it is also a static import), so the function
        // pointers stay valid. Ordinals 135 and 133 exist from build 17763.
        unsafe {
            let Ok(ux) = LoadLibraryExW(w!("uxtheme.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
            else {
                return DarkApi { allow_window: None };
            };
            if let Some(f) = GetProcAddress(ux, PCSTR(std::ptr::without_provenance(135))) {
                let f: SetPreferredAppMode = std::mem::transmute(f);
                // 1 = AllowDark (SetPreferredAppMode) or TRUE (AllowDarkModeForApp on 17763).
                f(1);
            }
            let allow_window = GetProcAddress(ux, PCSTR(std::ptr::without_provenance(133)))
                .map(|f| std::mem::transmute::<_, AllowDarkModeForWindow>(f));
            DarkApi { allow_window }
        }
    })
}

/// Dark title bar (`DWMWA_USE_IMMERSIVE_DARK_MODE`: 20, or 19 before build 18985).
pub fn dark_title_bar(hwnd: HWND) {
    let on = BOOL(1);
    for attr in [20, 19] {
        // SAFETY: `on` is a 4-byte BOOL that lives through the call.
        let r = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(attr),
                (&on as *const BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
            )
        };
        if r.is_ok() {
            return;
        }
    }
}

/// Dark scroll bars on a scrollable control (build 17763 or later; otherwise unchanged).
pub fn dark_scrollbars(hwnd: HWND) {
    let api = dark_api();
    let Some(allow) = api.allow_window else {
        return;
    };
    // SAFETY: `allow` is the uxtheme export resolved in `dark_api`; SetWindowTheme takes
    // static strings.
    unsafe {
        // The return value is the previous opt-in state, not an error.
        let _ = allow(hwnd, BOOL(1));
        let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
    }
}

#[cfg(test)]
mod spike {
    //! WP-10a spike: `cargo test --locked --lib -- --ignored ui::window::spike --nocapture`.
    //! Opens real windows on the desktop for about half a minute and drives them by itself.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ui::controls::{
        Align, ButtonSpec, Ctl, EditBorder, EditSpec, Hover, LabelSpec, ListSpec,
    };
    use crate::ui::layout::{Anchor, FlowDir, Point, Track};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP, HFONT,
        LOGFONTW, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RedrawWindow, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardState, SetKeyboardState, VK_CONTROL, VK_SHIFT, VK_SPACE, VK_TAB,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowTextLengthW, HWND_NOTOPMOST, HWND_TOPMOST, PM_REMOVE,
        PeekMessageW, SM_CMONITORS, SendMessageW, WM_CHAR, WM_GETFONT, WM_KEYUP, WM_LBUTTONDBLCLK,
        WM_LBUTTONDOWN, WM_LBUTTONUP,
    };

    const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\wp-10a";

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
        fn GlobalLock(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(h: *mut core::ffi::c_void) -> i32;
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: *mut core::ffi::c_void, hdc: *mut core::ffi::c_void, f: u32) -> i32;
        fn OpenClipboard(hwnd: *mut core::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut core::ffi::c_void;
        fn SetClipboardData(format: u32, mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }

    const CF_UNICODETEXT: u32 = 13;

    /// The test exe has no manifest; activate Common Controls 6 like the app manifest does.
    fn activate_comctl6() -> bool {
        let path = Path::new(GOLDEN).join("comctl6.manifest");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
</assembly>"#;
        std::fs::write(&path, xml).unwrap();
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
        // SAFETY: `ctx` and the path buffer live through the call; the context stays active
        // for the rest of the test thread (never deactivated on purpose).
        unsafe {
            let h = CreateActCtxW(&ctx);
            if h.is_null() || h as isize == -1 {
                return false;
            }
            let mut cookie = 0usize;
            ActivateActCtx(h, &mut cookie) != 0
        }
    }

    fn pump_for(ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            let mut msg = MSG::default();
            // SAFETY: Standard non-blocking message pump on the test (UI) thread.
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if !pre_translate(&msg) {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn post(h: HWND, msg: u32, w: usize, l: isize) {
        // SAFETY: Plain value messages to windows of this thread.
        unsafe { PostMessageW(Some(h), msg, WPARAM(w), LPARAM(l)).unwrap() };
    }

    fn send(h: HWND, msg: u32, w: usize, l: isize) -> isize {
        // SAFETY: Plain value messages (or pointers to live locals) to windows of this thread.
        unsafe { SendMessageW(h, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
    }

    fn set_keys(keys: &[u16], down: bool) {
        let mut state = [0u8; 256];
        // SAFETY: Reads and writes this thread's synchronous key state (test input only).
        unsafe {
            GetKeyboardState(&mut state).unwrap();
            for k in keys {
                state[usize::from(*k)] = if down { 0x80 } else { 0 };
            }
            SetKeyboardState(&state).unwrap();
        }
    }

    /// Makes `h` the foreground window even when another app is active (test only).
    fn ensure_active(h: HWND) {
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        };
        // SAFETY: Foreground and input-attach calls on window handles; detached again below.
        unsafe {
            let fg = GetForegroundWindow();
            if fg == h {
                return;
            }
            let other = GetWindowThreadProcessId(fg, None);
            let me = GetCurrentThreadId();
            let attached =
                other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
            let _ = SetForegroundWindow(h);
            if attached {
                let _ = AttachThreadInput(me, other, false);
            }
        }
        pump_for(100);
    }

    fn focus() -> HWND {
        // SAFETY: Reads this thread's focus window.
        unsafe { GetFocus() }
    }

    fn window_rect(h: HWND) -> RECT {
        let mut r = RECT::default();
        // SAFETY: Writable RECT.
        unsafe { GetWindowRect(h, &mut r).unwrap() };
        r
    }

    /// PrintWindow capture of a top-level window as top-down BGRA pixels.
    fn capture(h: HWND) -> (i32, i32, Vec<u8>) {
        let r = window_rect(h);
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        let mut bits = vec![0u8; (w * hgt * 4) as usize];
        // SAFETY: Memory DC and bitmap are created, used, and released here; buffers are
        // sized for the requested 32-bit top-down DIB.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp: HBITMAP = CreateCompatibleBitmap(screen, w, hgt);
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
        (w, hgt, bits)
    }

    fn save_bmp(path: &Path, w: i32, h: i32, bgra: &[u8]) {
        let mut out = Vec::with_capacity(54 + bgra.len());
        let file_size = 54 + bgra.len() as u32;
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&file_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&(-h).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        out.extend_from_slice(bgra);
        std::fs::write(path, out).unwrap();
    }

    fn print_shot(h: HWND, name: &str) -> PathBuf {
        let (w, hgt, bits) = capture(h);
        let path = Path::new(GOLDEN).join(format!("{name}.bmp"));
        save_bmp(&path, w, hgt, &bits);
        path
    }

    /// Screen capture through PowerShell `CopyFromScreen` (what the user really sees).
    fn screen_shot(h: HWND, name: &str) {
        // SAFETY: Z-order changes of our own window around the capture.
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
        pump_for(300);
        let mut r = window_rect(h);
        // SAFETY: Reads the visible frame rectangle of our own window into a RECT.
        unsafe {
            let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                h,
                windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut r as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            );
        }
        let path = Path::new(GOLDEN).join(format!("{name}.png"));
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; \
             Add-Type -Namespace W -Name U -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);'; \
             [void][W.U]::SetProcessDpiAwarenessContext([IntPtr](-4)); \
             $b = New-Object System.Drawing.Bitmap {w}, {h}; \
             $g = [System.Drawing.Graphics]::FromImage($b); \
             $g.CopyFromScreen({x}, {y}, 0, 0, $b.Size); \
             $b.Save('{p}', [System.Drawing.Imaging.ImageFormat]::Png)",
            w = r.right - r.left,
            h = r.bottom - r.top,
            x = r.left,
            y = r.top,
            p = path.display()
        );
        let mut child = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .spawn()
            .unwrap();
        loop {
            pump_for(50);
            if child.try_wait().unwrap().is_some() {
                break;
            }
        }
        // SAFETY: Restores normal z-order.
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
        println!("screenshot {}", path.display());
    }

    fn bmps_to_png() {
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; \
             Get-ChildItem '{g}' -Filter *.bmp | ForEach-Object {{ \
               $i = [System.Drawing.Image]::FromFile($_.FullName); \
               $i.Save(($_.FullName -replace '\\.bmp$', '.png'), [System.Drawing.Imaging.ImageFormat]::Png); \
               $i.Dispose(); Remove-Item $_.FullName }}",
            g = GOLDEN
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn clipboard_text() -> Option<String> {
        // SAFETY: Standard clipboard read; the handle is only locked while copying out.
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let h = GetClipboardData(CF_UNICODETEXT);
            let mut out = None;
            if !h.is_null() {
                let p = GlobalLock(h) as *const u16;
                if !p.is_null() {
                    let mut n = 0;
                    while *p.add(n) != 0 {
                        n += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                    GlobalUnlock(h);
                }
            }
            CloseClipboard();
            out
        }
    }

    fn set_clipboard_text(text: &str) {
        let wide = to_wide(text);
        // SAFETY: Allocates movable global memory, copies the text in, and hands ownership to
        // the clipboard.
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return;
            }
            EmptyClipboard();
            let h = GlobalAlloc(0x0002, wide.len() * 2);
            let p = GlobalLock(h) as *mut u16;
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            GlobalUnlock(h);
            SetClipboardData(CF_UNICODETEXT, h);
            CloseClipboard();
        }
    }

    fn lf_height(font: HFONT) -> i32 {
        let mut lf = LOGFONTW::default();
        // SAFETY: Reads the LOGFONTW of a font handle into a correctly sized buffer.
        unsafe {
            GetObjectW(
                font.into(),
                std::mem::size_of::<LOGFONTW>() as i32,
                Some((&mut lf as *mut LOGFONTW).cast()),
            )
        };
        lf.lfHeight
    }

    /// (leaves checked, wrong font handle, wrong font height) after a DPI change.
    fn check_fonts(form: &Form) -> (usize, usize, usize) {
        let state = form.state().unwrap();
        let tree = state.tree.borrow();
        let mut leaves = Vec::new();
        fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
            for c in n.children() {
                if c.is_leaf() {
                    out.push(c);
                }
                walk(c, out);
            }
        }
        walk(&tree, &mut leaves);
        let dpi = state.dpi.get();
        let (mut wrong_handle, mut wrong_height) = (0, 0);
        for leaf in &leaves {
            let Kind::Leaf(ctl) = &leaf.kind else {
                continue;
            };
            let h = state.hwnd_of(leaf.id).unwrap();
            let got = send(h, WM_GETFONT, 0, 0);
            let want = state.font(ctl.font());
            if got != want.0 as isize {
                wrong_handle += 1;
            }
            if lf_height(want) != dpi::font_height(ctl.font().points, dpi) {
                wrong_height += 1;
            }
        }
        (leaves.len(), wrong_handle, wrong_height)
    }

    /// Border pixels of a child window that no scroll bar covers: its left column and top row
    /// (minus the vertical scroll bar width at the right end). The scroll bars themselves sit
    /// on the right and bottom edges and change color by design (dark scroll bars, AD-36).
    fn border_ring(top: HWND, child: HWND, cap: &(i32, i32, Vec<u8>)) -> Vec<u8> {
        let tr = window_rect(top);
        let cr = window_rect(child);
        let (w, _, bits) = cap;
        let bar = dpi::metric(SM_CXVSCROLL, dpi::window_dpi(child)) + 1;
        let (x0, y0) = (cr.left - tr.left, cr.top - tr.top);
        let (x1, y1) = (cr.right - tr.left, cr.bottom - tr.top);
        let mut out = Vec::new();
        let mut px = |x: i32, y: i32| {
            let i = ((y * w + x) * 4) as usize;
            out.extend_from_slice(&bits[i..i + 3]);
        };
        for x in x0..(x1 - bar) {
            px(x, y0);
        }
        for y in y0..(y1 - bar) {
            px(x0, y);
        }
        out
    }

    fn redraw(h: HWND) {
        // SAFETY: Repaint request including the non-client frame.
        unsafe {
            let _ = RedrawWindow(Some(h), None, None, RDW_ERASE | RDW_FRAME | RDW_INVALIDATE);
        }
    }

    // ------------------------------------------------------------------ test forms

    const MAIN_TABLE: u16 = 90;
    const SIDEBAR: u16 = 100;
    const SIDEBAR_TITLE: u16 = 101;
    const SIDEBAR_SUBTITLE: u16 = 102;
    const FIRST_SECTION: u16 = 110;
    const CONTENT_TITLE: u16 = 130;
    const CONTENT_META: u16 = 131;
    const CONTENT_EDIT: u16 = 132;
    const FOOTER: u16 = 140;
    const FIRST_FOOTER_BUTTON: u16 = 141;
    const LOADING: u16 = 150;
    const TITLES: [&str; 14] = [
        "💿 DISK DRIVES",
        "🔌 MOTHERBOARD",
        "📋 CHASSIS",
        "⚙️ (SM)BIOS",
        "💻 SYSTEM INFORMATION",
        "💾 RAM MODULES",
        "🖥️ CPU",
        "🔒 TPM MODULES",
        "🔌 USB DEVICES",
        "🎮 GPU INFO",
        "🖥️ MONITOR INFORMATION",
        "🌐 NETWORK ADAPTERS (NIC's)",
        "📋 BLUETOOTH ADAPTERS",
        "📡 ARP INFO/CACHE",
    ];
    const FOOTER_TEXTS: [&str; 6] = [
        "↻ Refresh",
        "💾 Export",
        "🧹 Clean Devices",
        "📝 Clean Logs",
        "⟳ Updates",
        "📜 Old View",
    ];

    fn sidebar_button(i: usize) -> Node {
        let spec = ButtonSpec {
            text: TITLES[i].to_owned(),
            font: theme::SECTION_BUTTON_FONT,
            back: theme::SIDEBAR_ITEM_BACKGROUND,
            fore: theme::SIDEBAR_ITEM_TEXT,
            border: theme::BORDER_SUBTLE,
            border_size: 1,
            over_back: Some(theme::SIDEBAR_ITEM_HOVER),
            down_back: Some(theme::SIDEBAR_ITEM_ACTIVE),
            align: Align::MiddleLeft,
            hover: Hover::None,
        };
        Node::leaf(FIRST_SECTION + i as u16, Ctl::Button(spec))
            .size(Size {
                w: 216,
                h: theme::SECTION_BUTTON_HEIGHT,
            })
            .padding(theme::SECTION_BUTTON_PADDING)
            .margin(theme::SECTION_BUTTON_MARGIN)
    }

    fn main_replica() -> Vec<Node> {
        let mut side = vec![
            Node::leaf(
                SIDEBAR_TITLE,
                Ctl::Label(LabelSpec::new(
                    "Hardware Sections",
                    theme::SIDEBAR_TITLE_FONT,
                    theme::SIDEBAR_HEADER_TEXT,
                )),
            )
            .size(Size {
                w: 216,
                h: theme::SIDEBAR_TITLE_HEIGHT,
            })
            .margin(theme::SIDEBAR_TITLE_MARGIN),
            Node::leaf(
                SIDEBAR_SUBTITLE,
                Ctl::Label(LabelSpec::new(
                    "14 sections",
                    theme::SIDEBAR_SUBTITLE_FONT,
                    theme::MUTED_TEXT,
                )),
            )
            .size(Size {
                w: 216,
                h: theme::SIDEBAR_SUBTITLE_HEIGHT,
            })
            .margin(theme::SIDEBAR_SUBTITLE_MARGIN),
        ];
        side.extend((0..14).map(sidebar_button));
        let header = Node::panel(vec![
            Node::leaf(
                CONTENT_TITLE,
                Ctl::Label(
                    LabelSpec::new(
                        "DISK DRIVES",
                        theme::SECTION_TITLE_FONT,
                        theme::SIDEBAR_HEADER_TEXT,
                    )
                    .ellipsis(),
                ),
            )
            .top()
            .height(theme::SECTION_TITLE_HEIGHT),
            Node::leaf(
                CONTENT_META,
                Ctl::Label(
                    LabelSpec::new(
                        "Section 1 of 14",
                        theme::SECTION_META_FONT,
                        theme::MUTED_TEXT,
                    )
                    .ellipsis(),
                ),
            )
            .top()
            .height(theme::SECTION_META_HEIGHT),
        ])
        .fill()
        .auto_size()
        .back(theme::CONTENT_BACKGROUND)
        .padding(theme::HEADER_PADDING)
        .margin(theme::NO_PAD)
        .cell(0, 0);
        let content = Node::panel(vec![
            Node::table(
                vec![Track::Percent(100.0)],
                vec![
                    Track::AutoSize,
                    Track::Absolute(theme::DIVIDER_HEIGHT),
                    Track::Percent(100.0),
                ],
                vec![
                    header,
                    Node::panel(vec![])
                        .fill()
                        .back(theme::BORDER_SUBTLE)
                        .margin(theme::NO_PAD)
                        .cell(0, 1),
                    Node::leaf(
                        CONTENT_EDIT,
                        Ctl::Edit(
                            EditSpec::new(
                                theme::CONTENT_FONT,
                                theme::TEXT_BOX_TEXT,
                                theme::TEXT_BOX_BACKGROUND,
                            )
                            .border(EditBorder::FixedSingle),
                        ),
                    )
                    .fill()
                    .cell(0, 2),
                ],
            )
            .fill()
            .back(theme::SURFACE_BACKGROUND),
        ])
        .fill()
        .padding(theme::CONTENT_PADDING)
        .back(theme::SURFACE_BACKGROUND)
        .cell(1, 0);
        let footer_buttons = FOOTER_TEXTS
            .iter()
            .enumerate()
            .map(|(i, t)| {
                Node::leaf(
                    FIRST_FOOTER_BUTTON + i as u16,
                    Ctl::Button(ButtonSpec::secondary(t)),
                )
                .auto_size()
                .min(theme::FOOTER_BUTTON_MIN)
                .padding(theme::SHARED_BUTTON_PADDING)
                .margin(theme::FOOTER_BUTTON_MARGIN)
            })
            .collect();
        vec![
            Node::table(
                vec![Track::Absolute(291), Track::Percent(100.0)],
                vec![Track::Percent(100.0), Track::AutoSize],
                vec![
                    Node::flow(FlowDir::TopDown, false, side)
                        .id(SIDEBAR)
                        .fill()
                        .scroll()
                        .padding(theme::SIDEBAR_PADDING)
                        .back(theme::SIDEBAR_BACKGROUND)
                        .cell(0, 0),
                    content,
                    Node::flow(FlowDir::LeftToRight, true, footer_buttons)
                        .id(FOOTER)
                        .fill()
                        .auto_size()
                        .padding(theme::FOOTER_PADDING)
                        .margin(theme::NO_PAD)
                        .back(theme::BUTTON_PANEL_BACKGROUND)
                        .cell(0, 1)
                        .span(2),
                ],
            )
            .id(MAIN_TABLE)
            .fill()
            .back(theme::MAIN_BACKGROUND),
            Node::leaf(
                LOADING,
                Ctl::Label(LabelSpec::new(
                    "Loading hardware information...",
                    theme::LOADING_FONT,
                    theme::LOADING_LABEL_TEXT,
                )),
            )
            .auto_size()
            .anchor(Anchor::NONE)
            .visible(false),
        ]
    }

    /// The C# `UpdateResponsiveLayout` with the owner-approved DPI clamp (AD-37).
    fn responsive(form: &Form, client: Size) {
        let s = |v| form.scale(v);
        let sidebar = (client.w * theme::SIDEBAR_WIDTH_PERCENT / 100)
            .clamp(s(theme::SIDEBAR_MIN_WIDTH), s(theme::SIDEBAR_MAX_WIDTH));
        let bar = if form.vscroll_visible(SIDEBAR) {
            dpi::metric(SM_CXVSCROLL, form.dpi())
        } else {
            0
        };
        form.with_tree(|t| {
            if let Some(Kind::Table { cols, .. }) = t.find_mut(MAIN_TABLE).map(|n| &mut n.kind) {
                cols[0] = Track::Absolute(sidebar);
            }
            let Some(side) = t.find_mut(SIDEBAR) else {
                return;
            };
            // C# parity: sidebarPanel.ClientSize.Width (scroll bar already excluded) minus the
            // scroll bar width again (SectionedViewForm.cs:650-651).
            let client_w = sidebar - side.margin.horizontal() - bar;
            let item = (client_w - side.padding.horizontal() - bar - theme::SIDEBAR_ITEM_INSET)
                .max(theme::SIDEBAR_ITEM_MIN_WIDTH);
            for c in side.children_mut() {
                c.size.w = item;
            }
        });
    }

    fn confirm_replica() -> Vec<Node> {
        let button = |id: u16, text: &str, primary: bool| {
            let (back, border, hover, font, size, min_w) = if primary {
                (
                    theme::CONFIRM_PRIMARY,
                    theme::CONFIRM_PRIMARY_BORDER,
                    theme::CONFIRM_PRIMARY_HOVER,
                    theme::CONFIRM_PRIMARY_FONT,
                    theme::CONFIRM_PRIMARY_BORDER_SIZE,
                    theme::CONFIRM_PRIMARY_MIN_WIDTH,
                )
            } else {
                (
                    theme::CONFIRM_SECONDARY,
                    theme::CONFIRM_SECONDARY_BORDER,
                    theme::CONFIRM_SECONDARY_HOVER,
                    theme::CONFIRM_SECONDARY_FONT,
                    theme::CONFIRM_SECONDARY_BORDER_SIZE,
                    theme::CONFIRM_SECONDARY_MIN_WIDTH,
                )
            };
            Node::leaf(
                id,
                Ctl::Button(ButtonSpec {
                    text: text.to_owned(),
                    font,
                    back,
                    fore: theme::WHITE,
                    border,
                    border_size: size,
                    over_back: None,
                    down_back: None,
                    align: Align::MiddleCenter,
                    hover: Hover::EnterLeave {
                        normal: back,
                        hover,
                    },
                }),
            )
            .auto_size()
            .min(Size {
                w: min_w,
                h: theme::CONFIRM_BUTTON_HEIGHT,
            })
            .padding(theme::CONFIRM_BUTTON_PADDING)
            .margin(theme::CONFIRM_BUTTON_MARGIN)
        };
        let wrap = Size {
            w: (theme::CONFIRM_CLIENT_SIZE.w - theme::CONFIRM_LABEL_WRAP_INSET)
                .max(theme::CONFIRM_LABEL_WRAP_MIN),
            h: 0,
        };
        vec![
            Node::table(
                vec![Track::Percent(100.0)],
                vec![Track::AutoSize, Track::AutoSize, Track::AutoSize],
                vec![
                    Node::leaf(
                        301,
                        Ctl::Label(
                            LabelSpec::new(
                                "Remove 12 ghost devices?",
                                theme::CONFIRM_MESSAGE_FONT,
                                theme::WHITE,
                            )
                            .align(Align::MiddleCenter),
                        ),
                    )
                    .auto_size()
                    .anchor(Anchor::NONE)
                    .max(wrap)
                    .margin(theme::CONFIRM_MESSAGE_MARGIN)
                    .cell(0, 0),
                    Node::leaf(
                        302,
                        Ctl::Label(
                            LabelSpec::new(
                                "Warning: This action cannot be undone",
                                theme::CONFIRM_WARNING_FONT,
                                theme::ORANGE,
                            )
                            .align(Align::MiddleCenter),
                        ),
                    )
                    .auto_size()
                    .anchor(Anchor::NONE)
                    .max(wrap)
                    .margin(theme::CONFIRM_WARNING_MARGIN)
                    .cell(0, 1),
                    Node::flow(
                        FlowDir::LeftToRight,
                        true,
                        vec![
                            button(311, "Yes (Autoclose)", true),
                            button(312, "Yes", false),
                            button(313, "No", false),
                        ],
                    )
                    .auto_size()
                    .anchor(Anchor::NONE)
                    .margin(theme::NO_PAD)
                    .cell(0, 2),
                ],
            )
            .fill()
            .padding(theme::CONFIRM_PADDING),
        ]
    }

    fn extras_replica() -> Vec<Node> {
        let action = |id: u16, text: &str| {
            Node::leaf(id, Ctl::Button(ButtonSpec::primary(text)))
                .auto_size()
                .min(theme::ACTION_BUTTON_MIN)
                .padding(theme::SHARED_BUTTON_PADDING)
                .margin(theme::ACTION_BUTTON_MARGIN)
        };
        let list = ListSpec {
            font: theme::WHITELIST_LIST_FONT,
            fore: theme::TEXT_BOX_TEXT,
            back: theme::TEXT_BOX_BACKGROUND,
            selected_back: theme::SIDEBAR_ITEM_ACTIVE,
            selected_fore: theme::WHITE,
        };
        vec![
            Node::table(
                vec![Track::Percent(100.0)],
                vec![
                    Track::AutoSize,
                    Track::Percent(50.0),
                    Track::Percent(50.0),
                    Track::Absolute(30),
                    Track::Absolute(theme::ACTION_ROW_HEIGHT),
                ],
                vec![
                    Node::leaf(
                        200,
                        Ctl::Label(LabelSpec::new(
                            "Select ghost devices to keep in the whitelist:",
                            theme::WHITELIST_HEADER_FONT,
                            theme::PRIMARY_TEXT,
                        )),
                    )
                    .auto_size()
                    .anchor(Anchor::LEFT)
                    .margin(theme::WHITELIST_HEADER_MARGIN)
                    .cell(0, 0),
                    Node::panel(vec![Node::leaf(201, Ctl::CheckedList(list)).fill()])
                        .fill()
                        .padding(theme::OUTPUT_PANEL_PADDING)
                        .cell(0, 1),
                    Node::panel(vec![
                        Node::leaf(
                            202,
                            Ctl::Edit(EditSpec::new(
                                theme::CLEANER_OUTPUT_FONT,
                                theme::TEXT_BOX_TEXT,
                                theme::TEXT_BOX_BACKGROUND,
                            )),
                        )
                        .fill(),
                    ])
                    .fill()
                    .padding(theme::OUTPUT_PANEL_PADDING)
                    .cell(0, 2),
                    Node::leaf(203, Ctl::Progress).fill().cell(0, 3),
                    Node::flow(
                        FlowDir::RightToLeft,
                        false,
                        vec![
                            action(211, "Stop & Close"),
                            action(212, "Reclean"),
                            action(213, "Manage Whitelist"),
                        ],
                    )
                    .fill()
                    .padding(theme::ACTION_PANEL_PADDING)
                    .back(theme::BUTTON_PANEL_BACKGROUND)
                    .cell(0, 4),
                ],
            )
            .fill()
            .back(theme::MAIN_BACKGROUND),
        ]
    }

    #[derive(Default)]
    struct Log {
        clicks: RefCell<Vec<u16>>,
        checks: RefCell<Vec<(u16, usize, bool)>>,
        workers: RefCell<Vec<u32>>,
        other: RefCell<Vec<String>>,
    }

    fn rect_of(form: &Form, id: u16) -> Rect {
        let state = form.state().unwrap();
        let tree = state.tree.borrow();
        tree.find(id).unwrap().bounds
    }

    #[test]
    #[ignore = "opens real windows; run by hand for the WP-10a spike"]
    fn spike() {
        std::fs::create_dir_all(GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        assert!(activate_comctl6(), "comctl v6 activation context");
        // SAFETY: Plain system queries.
        let (sys_dpi, monitors) = unsafe { (GetDpiForSystem(), GetSystemMetrics(SM_CMONITORS)) };
        println!(
            "system DPI {sys_dpi}, OS build {}, monitors {monitors}",
            os_build()
        );
        let mut results: Vec<(String, String)> = Vec::new();
        let mut record = |name: &str, value: String| {
            println!("RESULT {name}: {value}");
            results.push((name.to_owned(), value));
        };

        // ---------------------------------------------------------------- main replica
        let log = Rc::new(Log::default());
        let l = Rc::clone(&log);
        let mut spec = FormSpec::new("HWID Checker", WindowSize::Client(theme::MAIN_CLIENT_SIZE));
        spec.min = Some(theme::MAIN_MIN_SIZE);
        spec.start = StartPosition::CenterScreen;
        spec.maximize_if_too_big = true;
        let main = Form::create(HWND::default(), spec, main_replica(), move |form, ev| {
            match ev {
                Event::Resize { client, .. } => responsive(form, client),
                Event::Click(id) => {
                    l.clicks.borrow_mut().push(id);
                    if id == FIRST_FOOTER_BUTTON + 5 {
                        panic!("spike: deliberate panic inside a handler");
                    }
                    if (FIRST_SECTION..FIRST_SECTION + 14).contains(&id) {
                        for i in 0..14 {
                            form.set_button_colors(
                                FIRST_SECTION + i,
                                theme::SIDEBAR_ITEM_BACKGROUND,
                                theme::SIDEBAR_ITEM_TEXT,
                                theme::BORDER_SUBTLE,
                            );
                        }
                        form.set_button_colors(
                            id,
                            theme::SIDEBAR_ITEM_ACTIVE,
                            theme::SIDEBAR_ITEM_ACTIVE_TEXT,
                            theme::PRIMARY_BUTTON_HOVER,
                        );
                    }
                }
                Event::Worker(v) => {
                    if let Ok(n) = v.downcast::<u32>() {
                        l.workers.borrow_mut().push(*n);
                    }
                }
                other => l.other.borrow_mut().push(format!("{other:?}")),
            }
            true
        })
        .unwrap();
        // First layout used the default sidebar width; C# runs UpdateResponsiveLayout in the
        // constructor, so run it once more before showing.
        main.relayout();
        main.set_button_colors(
            FIRST_SECTION,
            theme::SIDEBAR_ITEM_ACTIVE,
            theme::SIDEBAR_ITEM_ACTIVE_TEXT,
            theme::PRIMARY_BUTTON_HOVER,
        );
        main.show();
        pump_for(400);
        let mh = main.hwnd();
        record("main dpi", main.dpi().to_string());
        record("main client", format!("{:?}", main.client_size()));
        record("sidebar rect", format!("{:?}", rect_of(&main, SIDEBAR)));
        record("footer rect", format!("{:?}", rect_of(&main, FOOTER)));
        for i in 0..6 {
            record(
                &format!("footer button {i}"),
                format!("{:?}", rect_of(&main, FIRST_FOOTER_BUTTON + i)),
            );
        }
        record("edit rect", format!("{:?}", rect_of(&main, CONTENT_EDIT)));
        record(
            "section button 0",
            format!("{:?}", rect_of(&main, FIRST_SECTION)),
        );

        // 40,000+ characters appended in full.
        let edit = main.control(CONTENT_EDIT).unwrap();
        let line = "0123456789ABCDEFGHIJ 0123456789ABCDEFGHIJ 012345\r\n";
        let lines: Vec<&str> = std::iter::repeat_n(line, 1000).collect();
        main.edit_append_batch(CONTENT_EDIT, &lines);
        // SAFETY: Length query on our edit.
        let len = unsafe { GetWindowTextLengthW(edit) } as usize;
        record(
            "append 50,000 chars",
            format!("{} (expected {})", len, line.len() * 1000),
        );
        assert_eq!(len, line.len() * 1000);
        main.edit_scroll_to_top(CONTENT_EDIT);
        pump_for(200);
        screen_shot(mh, &format!("main-{}dpi-default", main.dpi()));

        // Tab and Shift+Tab order (retried when desktop activity steals the focus).
        let ids: HashMap<isize, u16> = {
            let st = main.state().unwrap();
            st.hwnds
                .borrow()
                .iter()
                .map(|(id, h)| (h.0 as isize, *id))
                .collect()
        };
        let (mut forward, mut backward) = (Vec::new(), Vec::new());
        for attempt in 0..3 {
            ensure_active(mh);
            main.focus(FIRST_SECTION);
            pump_for(50);
            forward.clear();
            backward.clear();
            for _ in 0..21 {
                post(focus(), WM_KEYDOWN, usize::from(VK_TAB.0), 0);
                pump_for(30);
                forward.push(ids.get(&(focus().0 as isize)).copied().unwrap_or(0));
            }
            set_keys(&[VK_SHIFT.0], true);
            for _ in 0..3 {
                post(focus(), WM_KEYDOWN, usize::from(VK_TAB.0), 0);
                pump_for(30);
                backward.push(ids.get(&(focus().0 as isize)).copied().unwrap_or(0));
            }
            set_keys(&[VK_SHIFT.0], false);
            if !forward.contains(&0) && !backward.contains(&0) {
                break;
            }
            println!("tab test attempt {attempt}: focus left the window (desktop activity)");
        }
        let mut expected: Vec<u16> = (FIRST_SECTION + 1..FIRST_SECTION + 14).collect();
        expected.push(CONTENT_EDIT);
        expected.extend(FIRST_FOOTER_BUTTON..FIRST_FOOTER_BUTTON + 6);
        expected.push(FIRST_SECTION);
        record("tab order", format!("{forward:?}"));
        assert_eq!(forward, expected, "Tab order");
        record("shift+tab order", format!("{backward:?}"));
        assert_eq!(
            backward,
            vec![
                FIRST_FOOTER_BUTTON + 5,
                FIRST_FOOTER_BUTTON + 4,
                FIRST_FOOTER_BUTTON + 3
            ]
        );

        // Space and Enter on the focused button.
        ensure_active(mh);
        log.clicks.borrow_mut().clear();
        main.focus(FIRST_FOOTER_BUTTON);
        pump_for(30);
        post(focus(), WM_KEYDOWN, usize::from(VK_SPACE.0), 0x0039_0001);
        post(
            focus(),
            WM_KEYUP,
            usize::from(VK_SPACE.0),
            0xC039_0001_u32 as isize,
        );
        pump_for(80);
        main.focus(FIRST_FOOTER_BUTTON + 1);
        pump_for(30);
        post(focus(), WM_KEYDOWN, usize::from(VK_RETURN.0), 0x001C_0001);
        pump_for(80);
        record("space+enter clicks", format!("{:?}", log.clicks.borrow()));
        assert_eq!(
            *log.clicks.borrow(),
            vec![FIRST_FOOTER_BUTTON, FIRST_FOOTER_BUTTON + 1]
        );

        // Esc does nothing on a non-dialog window.
        log.clicks.borrow_mut().clear();
        post(focus(), WM_KEYDOWN, usize::from(VK_ESCAPE.0), 0x0001_0001);
        pump_for(80);
        record(
            "esc on main",
            format!("alive={} clicks={:?}", main.is_alive(), log.clicks.borrow()),
        );
        assert!(main.is_alive() && log.clicks.borrow().is_empty());

        // Ctrl+A and copy in the EDIT (clipboard saved and restored).
        let saved_clip = clipboard_text();
        ensure_active(mh);
        main.focus(CONTENT_EDIT);
        pump_for(30);
        set_keys(&[VK_CONTROL.0], true);
        post(edit, WM_KEYDOWN, usize::from(b'A'), 0x001E_0001);
        pump_for(50);
        let (mut s, mut e) = (0u32, 0u32);
        send(
            edit,
            windows::Win32::UI::Controls::EM_GETSEL,
            (&mut s as *mut u32) as usize,
            (&mut e as *mut u32) as isize,
        );
        // Ctrl+C the way a keyboard sends it: TranslateMessage turns it into WM_CHAR 3.
        post(edit, WM_KEYDOWN, usize::from(b'C'), 0x002E_0001);
        pump_for(100);
        set_keys(&[VK_CONTROL.0], false);
        let copied = clipboard_text().map(|t| t.len()).unwrap_or(0);
        if let Some(t) = saved_clip {
            set_clipboard_text(&t);
        }
        record(
            "ctrl+a / copy",
            format!("selection {s}..{e} of {len}; clipboard {copied} chars"),
        );
        assert_eq!((s as usize, e as usize), (0, len));
        assert_eq!(copied, len);
        main.edit_scroll_to_top(CONTENT_EDIT);

        // Fast double click on a sidebar button fires twice.
        log.clicks.borrow_mut().clear();
        let sb = main.control(FIRST_SECTION + 3).unwrap();
        let at = (20 | (20 << 16)) as isize;
        post(sb, WM_LBUTTONDOWN, 1, at);
        post(sb, WM_LBUTTONUP, 0, at);
        post(sb, WM_LBUTTONDBLCLK, 1, at);
        post(sb, WM_LBUTTONUP, 0, at);
        pump_for(150);
        record("double click", format!("{:?}", log.clicks.borrow()));
        assert_eq!(*log.clicks.borrow(), vec![FIRST_SECTION + 3; 2]);

        // A panic inside a handler is caught; the window keeps working.
        log.clicks.borrow_mut().clear();
        let old_view = main.control(FIRST_FOOTER_BUTTON + 5).unwrap();
        post(old_view, WM_KEYDOWN, usize::from(VK_SPACE.0), 0x0039_0001);
        post(
            old_view,
            WM_KEYUP,
            usize::from(VK_SPACE.0),
            0xC039_0001_u32 as isize,
        );
        pump_for(100);
        main.click(FIRST_FOOTER_BUTTON + 2);
        record(
            "panic in handler",
            format!("alive={} clicks={:?}", main.is_alive(), log.clicks.borrow()),
        );
        assert!(main.is_alive());

        // Worker generations: stale posts are dropped.
        let p0 = main.poster().unwrap();
        let p0b = p0.clone();
        std::thread::spawn(move || assert!(p0b.post(1u32)))
            .join()
            .unwrap();
        pump_for(80);
        main.next_generation();
        let p1 = main.poster().unwrap();
        std::thread::spawn(move || {
            p0.post(2u32);
            p1.post(3u32);
        })
        .join()
        .unwrap();
        pump_for(80);
        record("worker generations", format!("{:?}", log.workers.borrow()));
        assert_eq!(*log.workers.borrow(), vec![1, 3]);

        // Focus rectangle (keyboard cues are on after the Tab test) and pressed colors.
        let pixel = |top: HWND, child: HWND, dx: i32, dy: i32| {
            let (w, _, bits) = capture(top);
            let (tr, cr) = (window_rect(top), window_rect(child));
            let (x, y) = (cr.left - tr.left + dx, cr.top - tr.top + dy);
            let i = ((y * w + x) * 4) as usize;
            (bits[i + 2], bits[i + 1], bits[i])
        };
        ensure_active(mh);
        main.focus(FIRST_FOOTER_BUTTON);
        pump_for(80);
        let fb = main.control(FIRST_FOOTER_BUTTON).unwrap();
        let fr = window_rect(fb);
        let half = (fr.bottom - fr.top) / 2;
        let focus_px = pixel(mh, fb, 4, half);
        let normal_px = pixel(mh, fb, 6, half);
        // BM_SETSTATE gives the pushed state without mouse capture, so the real cursor of the
        // live desktop cannot interfere.
        let bm_setstate = windows::Win32::UI::WindowsAndMessaging::BM_SETSTATE;
        send(fb, bm_setstate, 1, 0);
        pump_for(80);
        let pressed_px = pixel(mh, fb, 6, half);
        send(fb, bm_setstate, 0, 0);
        let sb0 = main.control(FIRST_SECTION + 1).unwrap();
        send(sb0, bm_setstate, 1, 0);
        pump_for(80);
        let side_pressed_px = pixel(mh, sb0, 6, 20);
        send(sb0, bm_setstate, 0, 0);
        pump_for(80);
        record(
            "focus / pressed colors",
            format!(
                "focus ring {focus_px:?} (WinForms LowHighlight of 45,45,48 = (133, 133, 138)), \
                 face {normal_px:?}, pressed {pressed_px:?} (expected (133, 133, 138)), \
                 sidebar pressed {side_pressed_px:?} (MouseDownBackColor (0, 120, 215))"
            ),
        );
        assert_eq!(focus_px, (133, 133, 138));
        assert_eq!(normal_px, (45, 45, 48));
        assert_eq!(pressed_px, (133, 133, 138));
        assert_eq!(side_pressed_px, (0, 120, 215));
        log.clicks.borrow_mut().clear();

        // Disabled button paint (C# Old View while loading).
        main.set_enabled(FIRST_FOOTER_BUTTON + 5, false);
        main.set_text(FIRST_FOOTER_BUTTON + 5, "📜 Loading...");
        main.set_visible(LOADING, true);
        let lr = rect_of(&main, LOADING);
        let c = main.client_size();
        main.with_tree(|t| {
            if let Some(n) = t.find_mut(LOADING) {
                n.pos = Point {
                    x: ((c.w - lr.w) / 2).max(0),
                    y: ((c.h - lr.h) / 2).max(0),
                };
            }
        });
        main.relayout();
        main.bring_to_front(LOADING);
        pump_for(200);
        screen_shot(mh, &format!("main-{}dpi-loading-disabled", main.dpi()));
        main.set_visible(LOADING, false);
        main.set_enabled(FIRST_FOOTER_BUTTON + 5, true);
        main.set_text(FIRST_FOOTER_BUTTON + 5, "📜 Old View");

        // Minimum size: the footer wraps like C#.
        // SAFETY: Resizes our own window below its minimum; WM_GETMINMAXINFO clamps it.
        unsafe {
            let _ = SetWindowPos(mh, None, 0, 0, 100, 100, SWP_NOMOVE | SWP_NOZORDER);
        }
        pump_for(200);
        let wr = window_rect(mh);
        record(
            "minimum outer size",
            format!("{}x{}", wr.right - wr.left, wr.bottom - wr.top),
        );
        record("footer at minimum", format!("{:?}", rect_of(&main, FOOTER)));
        for i in 0..6 {
            record(
                &format!("min footer button {i}"),
                format!("{:?}", rect_of(&main, FIRST_FOOTER_BUTTON + i)),
            );
        }
        record(
            "sidebar scroll at minimum",
            format!("{}", main.vscroll_visible(SIDEBAR)),
        );
        screen_shot(mh, &format!("main-{}dpi-minimum", main.dpi()));

        // Resizing: exactly one layout pass per WM_SIZE.
        let before = main.layout_count();
        for (i, w) in [950, 1000, 1100, 1200, 1300].iter().enumerate() {
            // SAFETY: Resizes our own window.
            unsafe {
                let _ = SetWindowPos(
                    mh,
                    None,
                    0,
                    0,
                    main.scale(*w),
                    main.scale(780 + i as i32 * 5),
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
            pump_for(20);
        }
        record(
            "layout passes for 5 resizes",
            format!("{}", main.layout_count() - before),
        );
        assert_eq!(main.layout_count() - before, 5);

        // Synthesized DPI changes: fonts, one layout, screenshots. The last step returns to the
        // real DPI of the monitor the window is on.
        let real_dpi = dpi::window_dpi(mh);
        for new_dpi in [144u32, 192, real_dpi] {
            let r = window_rect(mh);
            let factor = |v: i32| v * new_dpi as i32 / main.dpi() as i32;
            let suggested = RECT {
                left: r.left,
                top: r.top,
                right: r.left + factor(r.right - r.left),
                bottom: r.top + factor(r.bottom - r.top),
            };
            let before = main.layout_count();
            send(
                mh,
                WM_DPICHANGED,
                (new_dpi | (new_dpi << 16)) as usize,
                (&suggested as *const RECT) as isize,
            );
            pump_for(250);
            let (n, wrong_handle, wrong_height) = check_fonts(&main);
            record(
                &format!("dpi {new_dpi}"),
                format!(
                    "layout passes {}, leaves {n}, wrong font handle {wrong_handle}, wrong font height {wrong_height}, footer {:?}, sidebar {:?}",
                    main.layout_count() - before,
                    rect_of(&main, FOOTER),
                    rect_of(&main, SIDEBAR)
                ),
            );
            assert_eq!(main.layout_count() - before, 1);
            assert_eq!((wrong_handle, wrong_height), (0, 0));
            print_shot(mh, &format!("main-{new_dpi}-synthetic"));
        }

        // Real monitor moves: put the window on every monitor; Windows sends WM_DPICHANGED.
        let monitors = {
            use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HMONITOR};
            unsafe extern "system" fn collect(
                m: HMONITOR,
                _: HDC,
                _: *mut RECT,
                data: LPARAM,
            ) -> BOOL {
                // SAFETY: `data` is the Vec passed below, alive for the enumeration.
                unsafe { (*(data.0 as *mut Vec<HMONITOR>)).push(m) };
                BOOL(1)
            }
            let mut list: Vec<HMONITOR> = Vec::new();
            // SAFETY: Synchronous enumeration with a callback that only pushes into `list`.
            unsafe {
                let _ = EnumDisplayMonitors(
                    None,
                    None,
                    Some(collect),
                    LPARAM(&mut list as *mut Vec<HMONITOR> as isize),
                );
            }
            list
        };
        for (i, m) in monitors.iter().enumerate() {
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: Valid monitor handle and writable MONITORINFO.
            unsafe {
                let _ = GetMonitorInfoW(*m, &mut mi);
            }
            let before = main.layout_count();
            let old_dpi = main.dpi();
            // SAFETY: Moves our own window onto monitor `i`.
            unsafe {
                let _ = SetWindowPos(
                    mh,
                    None,
                    mi.rcWork.left + 20,
                    mi.rcWork.top + 20,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            pump_for(400);
            let (n, wrong_handle, wrong_height) = check_fonts(&main);
            let passes = main.layout_count() - before;
            record(
                &format!("real monitor {i}"),
                format!(
                    "work {:?}, dpi {old_dpi} -> {} (system says {}), layout passes {passes}, leaves {n}, wrong font handle {wrong_handle}, wrong font height {wrong_height}, footer {:?}",
                    (
                        mi.rcWork.left,
                        mi.rcWork.top,
                        mi.rcWork.right,
                        mi.rcWork.bottom
                    ),
                    main.dpi(),
                    dpi::window_dpi(mh),
                    rect_of(&main, FOOTER)
                ),
            );
            assert_eq!(
                main.dpi(),
                dpi::window_dpi(mh),
                "kit DPI follows the monitor"
            );
            assert_eq!((wrong_handle, wrong_height), (0, 0));
            if main.dpi() != old_dpi {
                assert_eq!(passes, 1, "one layout pass per real DPI change");
            }
            screen_shot(mh, &format!("main-real-monitor{i}-{}dpi", main.dpi()));
        }

        // ---------------------------------------------------------------- extras window
        let elog = Rc::new(Log::default());
        let el = Rc::clone(&elog);
        let mut espec = FormSpec::new(
            "Kit extras (whitelist list, 3D edit, progress, RTL footer)",
            WindowSize::Client(theme::CLEAN_DEVICES_CLIENT_SIZE),
        );
        espec.min = Some(theme::CLEAN_DEVICES_MIN_SIZE);
        espec.style = FormStyle::Sizable {
            maximize: true,
            minimize: false,
        };
        let extras = Form::create(mh, espec, extras_replica(), move |_, ev| {
            match ev {
                Event::ItemCheck { id, index, checked } => {
                    el.checks.borrow_mut().push((id, index, checked))
                }
                Event::Click(id) => el.clicks.borrow_mut().push(id),
                _ => {}
            }
            true
        })
        .unwrap();
        let items: Vec<(String, bool)> = (0..12)
            .map(|i| (format!("Generic USB Hub #{i} (USB)"), i % 3 == 0))
            .collect();
        extras.list_set_items(201, &items);
        extras.edit_append(202, "Scanning for non-present (ghost) devices...\r\n");
        extras.progress_set(203, 60);
        extras.set_enabled(212, false);
        extras.show();
        pump_for(300);
        let eh = extras.hwnd();
        for id in [211, 212, 213] {
            record(
                &format!("rtl button {id}"),
                format!("{:?}", rect_of(&extras, id)),
            );
        }
        let list = extras.control(201).unwrap();
        let item_h = send(
            list,
            windows::Win32::UI::WindowsAndMessaging::LB_GETITEMHEIGHT,
            0,
            0,
        ) as i32;
        let y = item_h + item_h / 2;
        post(list, WM_LBUTTONDOWN, 1, (30 | (y << 16)) as isize);
        post(list, WM_LBUTTONUP, 0, (30 | (y << 16)) as isize);
        pump_for(100);
        extras.focus(201);
        pump_for(30);
        post(list, WM_CHAR, usize::from(b' '), 0x0039_0001);
        pump_for(100);
        record(
            "list toggles (click item 1, then Space)",
            format!(
                "{:?} checked now {:?}",
                elog.checks.borrow(),
                &extras.list_checked(201)[..3]
            ),
        );
        assert_eq!(*elog.checks.borrow(), vec![(201, 1, true), (201, 1, false)]);
        screen_shot(eh, &format!("extras-{}dpi", extras.dpi()));

        // DarkMode_Explorer vs default theme: edit border pixels.
        let edit3d = extras.control(202).unwrap();
        let dark_cap = capture(eh);
        let dark_main = capture(mh);
        // SAFETY: Clears the theme association of our own edits, then repaints them.
        unsafe {
            let _ = SetWindowTheme(edit3d, PCWSTR::null(), PCWSTR::null());
            let _ = SetWindowTheme(edit, PCWSTR::null(), PCWSTR::null());
        }
        redraw(edit3d);
        redraw(edit);
        pump_for(200);
        let light_cap = capture(eh);
        let light_main = capture(mh);
        let same3d = border_ring(eh, edit3d, &dark_cap) == border_ring(eh, edit3d, &light_cap);
        let same_single = border_ring(mh, edit, &dark_main) == border_ring(mh, edit, &light_main);
        let colors = |ring: Vec<u8>| {
            let mut c: Vec<(u8, u8, u8)> = ring.chunks(3).map(|p| (p[2], p[1], p[0])).collect();
            c.sort_unstable();
            c.dedup();
            c
        };
        record(
            "DarkMode_Explorer border pixels unchanged",
            format!(
                "Fixed3D {same3d}, FixedSingle {same_single}; FixedSingle ring colors dark {:?} light {:?}",
                colors(border_ring(mh, edit, &dark_main)),
                colors(border_ring(mh, edit, &light_main))
            ),
        );
        assert!(
            same3d && same_single,
            "DarkMode_Explorer changed edit border pixels"
        );
        print_shot(eh, "extras-light-scrollbars");
        dark_scrollbars(edit3d);
        dark_scrollbars(edit);
        redraw(edit3d);
        redraw(edit);
        pump_for(100);
        let er = window_rect(eh);
        let suggested = RECT {
            left: er.left,
            top: er.top,
            right: er.left + (er.right - er.left) * 3 / 2,
            bottom: er.top + (er.bottom - er.top) * 3 / 2,
        };
        send(
            eh,
            WM_DPICHANGED,
            (144 | (144 << 16)) as usize,
            (&suggested as *const RECT) as isize,
        );
        pump_for(250);
        let (n, wrong_handle, wrong_height) = check_fonts(&extras);
        let item_h144 = send(
            list,
            windows::Win32::UI::WindowsAndMessaging::LB_GETITEMHEIGHT,
            0,
            0,
        );
        record(
            "extras dpi 144",
            format!(
                "leaves {n}, wrong font handle {wrong_handle}, wrong font height {wrong_height}, \
                 list item height {item_h} -> {item_h144}, close button {:?}",
                rect_of(&extras, 211)
            ),
        );
        assert_eq!((wrong_handle, wrong_height), (0, 0));
        print_shot(eh, "extras-144-synthetic");
        extras.destroy();
        pump_for(100);

        // ---------------------------------------------------------------- modal confirm
        let outcomes: Rc<RefCell<Vec<String>>> = Rc::default();
        for key in [VK_RETURN, VK_ESCAPE] {
            let o = Rc::clone(&outcomes);
            let mut cspec = FormSpec::new(
                "Confirm Device Removal",
                WindowSize::Client(theme::CONFIRM_CLIENT_SIZE),
            );
            cspec.min = Some(theme::CONFIRM_MIN_SIZE);
            cspec.style = FormStyle::FixedDialog;
            cspec.back = theme::CONFIRM_BACKGROUND;
            cspec.accept = Some(311);
            cspec.cancel = Some(313);
            let shot = key == VK_RETURN;
            run_modal(mh, cspec, confirm_replica(), move |form, ev| {
                match ev {
                    Event::Created => form.set_timer(1, 400),
                    Event::Timer(_) => {
                        form.kill_timer(1);
                        // SAFETY: Reads window state of the owner.
                        let owner_enabled = unsafe { IsWindowEnabled(mh) }.as_bool();
                        o.borrow_mut()
                            .push(format!("owner enabled during modal: {owner_enabled}"));
                        if shot {
                            print_shot(form.hwnd(), &format!("confirm-{}dpi", form.dpi()));
                            let r = window_rect(form.hwnd());
                            let s = RECT {
                                left: r.left,
                                top: r.top,
                                right: r.left + (r.right - r.left) * 2,
                                bottom: r.top + (r.bottom - r.top) * 2,
                            };
                            send(
                                form.hwnd(),
                                WM_DPICHANGED,
                                (192 | (192 << 16)) as usize,
                                (&s as *const RECT) as isize,
                            );
                            pump_for(200);
                            print_shot(form.hwnd(), "confirm-192-synthetic");
                        }
                        post(focus(), WM_KEYDOWN, usize::from(key.0), 0x0001_0001);
                    }
                    Event::Click(id) => {
                        o.borrow_mut().push(format!("click {id}"));
                        form.destroy();
                    }
                    _ => {}
                }
                true
            })
            .unwrap();
        }
        // SAFETY: Reads window state of the owner.
        let owner_after = unsafe { IsWindowEnabled(mh) }.as_bool();
        record(
            "modal confirm (Enter, then Esc)",
            format!(
                "{:?}; owner enabled after: {owner_after}",
                outcomes.borrow()
            ),
        );
        assert_eq!(
            *outcomes.borrow(),
            vec![
                "owner enabled during modal: false".to_owned(),
                "click 311".to_owned(),
                "owner enabled during modal: false".to_owned(),
                "click 313".to_owned()
            ]
        );
        assert!(owner_after);

        main.destroy();
        pump_for(100);
        bmps_to_png();
        let report: String = results.iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
        std::fs::write(Path::new(GOLDEN).join("spike-results.txt"), report).unwrap();
    }
}
