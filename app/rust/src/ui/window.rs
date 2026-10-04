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
                FillRect, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
                MonitorFromPoint, MonitorFromRect, MonitorFromWindow, UpdateWindow,
            },
        },
        System::{
            LibraryLoader::{
                GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
            },
            SystemInformation::OSVERSIONINFOW,
            Threading::GetCurrentThreadId,
        },
        UI::{
            Controls::{
                ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
                InitCommonControlsEx, SetScrollInfo, SetWindowTheme, ShowScrollBar,
            },
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, GetKeyState, IsWindowEnabled, SetFocus, VK_CONTROL,
                VK_ESCAPE, VK_MENU, VK_RETURN,
            },
            WindowsAndMessaging::{
                BeginDeferWindowPos, CREATESTRUCTW, CS_DBLCLKS, CreateWindowExW, DefWindowProcW,
                DeferWindowPos, DestroyWindow, DispatchMessageW, EndDeferWindowPos, GA_ROOT,
                GCW_ATOM, GWLP_USERDATA, GetAncestor, GetClassLongPtrW, GetClientRect,
                GetCursorPos, GetMessageW, GetNextDlgTabItem, GetSystemMetrics, GetWindowLongPtrW,
                GetWindowRect, GetWindowThreadProcessId, HICON, IDC_ARROW, IMAGE_ICON,
                IsDialogMessageW, IsIconic, IsWindow, IsWindowVisible, IsZoomed, KillTimer,
                LR_DEFAULTCOLOR, LoadCursorW, LoadIconW, LoadImageW, MB_ICONERROR,
                MB_ICONINFORMATION, MB_OK, MINMAXINFO, MSG, MessageBoxW, PostMessageW,
                PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SB_BOTTOM, SB_LINEDOWN,
                SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBTRACK, SB_TOP, SB_VERT, SCROLLINFO,
                SIF_ALL, SIZE_MINIMIZED, SIZE_RESTORED, SM_CXSMICON, SM_CXVSCROLL, SM_CYSMICON,
                SPI_SETWORKAREA, SW_SHOWMAXIMIZED, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowLongPtrW,
                SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, WA_INACTIVE,
                WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CLOSE, WM_COMMAND, WM_CTLCOLOREDIT,
                WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED,
                WM_DRAWITEM, WM_ERASEBKGND, WM_GETDPISCALEDSIZE, WM_GETMINMAXINFO, WM_KEYDOWN,
                WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_PAINT, WM_SETFOCUS,
                WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WM_VKEYTOITEM, WM_VSCROLL,
                WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
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
    /// A toggle changed through mouse, Space, Enter, or Form::click.
    Toggled(u16, bool),
    /// An editable single-line control changed text.
    TextChanged(u16),
    /// Opt-in find-key routing; return true only when the handler consumes it.
    Key(FindKey),
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

/// Find commands; the form decides whether its bar is shown and owns the focused control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FindKey {
    /// Ctrl+F anywhere in the form.
    Open,
    /// F3 / Shift+F3 anywhere in the form.
    Step { backwards: bool },
    /// Enter / Shift+Enter in an editable single-line control.
    Enter { id: u16, backwards: bool },
    /// Escape, with the focused control id for checking membership in the find bar.
    Escape { id: Option<u16> },
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
    /// Register the window under the `HWIDChecker.MessageBox` class (themed message boxes).
    pub message_box: bool,
    /// An edit whose whole text Ctrl+C copies while the form has the focus.
    pub copy_on_ctrl_c: Option<u16>,
    /// Routes find keys to Event::Key before normal dialog navigation (opt-in).
    pub find_keys: bool,
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
            message_box: false,
            copy_on_ctrl_c: None,
            find_keys: false,
        }
    }
}

/// Window class name of every kit form.
pub const FORM_CLASS: &str = "HWIDChecker.Form";
/// Window class name of the themed message boxes (tests find them by this name).
pub const MESSAGE_BOX_CLASS: &str = "HWIDChecker.MessageBox";

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
    last_focus: Cell<HWND>,
    destroyed: Cell<bool>,
    destroying: Cell<bool>,
    maximize: Cell<bool>,
    /// Live timers (id, interval): killed while minimized, restarted on restore (DESIGN.md 8.6).
    timers: RefCell<HashMap<usize, u32>>,
    minimized: Cell<bool>,
    /// The client size in 96-DPI pixels of the restored window, kept across monitor moves
    /// (`WM_GETDPISCALEDSIZE`, DESIGN.md 11).
    logical_client: Cell<Size>,
}

type HandlerRc = Rc<dyn Fn(&Form, Event) -> bool>;

struct PanelState {
    back: Color,
    brush: controls::Brush,
    /// Card outline: the parent's color under the rounded corners and the logical radius.
    card: Option<(Color, i32)>,
}

struct DpiChange<'a>(&'a Cell<bool>);

impl Drop for DpiChange<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

thread_local! {
    static NEXT_SERIAL: Cell<u64> = const { Cell::new(1) };
    static MODAL_WINDOW: Cell<Option<Form>> = const { Cell::new(None) };
}

/// Restores the previous modal on every exit, including a caught unwind.
struct ModalWindow(Option<Form>);

impl Drop for ModalWindow {
    fn drop(&mut self) {
        MODAL_WINDOW.with(|m| m.set(self.0));
    }
}

/// The innermost live modal on this thread, also when another application is active.
pub(crate) fn modal_window() -> Option<HWND> {
    MODAL_WINDOW
        .with(Cell::get)
        .filter(Form::is_alive)
        .map(|f| f.hwnd())
}

fn form_class() -> PCWSTR {
    w!("HWIDChecker.Form")
}

fn message_box_class() -> PCWSTR {
    w!("HWIDChecker.MessageBox")
}

fn panel_class() -> PCWSTR {
    w!("HWIDChecker.Panel")
}

struct Atoms {
    form: u16,
    message_box: u16,
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
        // failure surfaces later as a CreateWindowExW error.
        unsafe {
            let _ = InitCommonControlsEx(&icc);
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
        // The small icon is loaded at the small-icon size, so the title bar and the taskbar
        // get a sharp image instead of a scaled-down large one (DESIGN.md section 7).
        // SAFETY: Same resource; the handle is a shared icon owned by the module.
        let small = unsafe {
            LoadImageW(
                Some(instance.into()),
                PCWSTR(std::ptr::without_provenance(1)),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            )
        }
        .map_or(icon, |h| HICON(h.0));
        // SAFETY: IDC_ARROW is a shared system cursor.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
        let form = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(form_proc),
            hInstance: instance.into(),
            hIcon: icon,
            hIconSm: small,
            hCursor: cursor,
            lpszClassName: form_class(),
            ..Default::default()
        };
        let message_box = WNDCLASSEXW {
            lpszClassName: message_box_class(),
            ..form
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
        // SAFETY: All structures are fully initialized; class names are static strings.
        unsafe {
            Atoms {
                form: RegisterClassExW(&form),
                message_box: RegisterClassExW(&message_box),
                panel: RegisterClassExW(&panel),
            }
        }
    })
}

thread_local! {
    /// Test hook: a work area that replaces every monitor query (synthetic screen setups).
    static FORCED_WORK_AREA: Cell<Option<RECT>> = const { Cell::new(None) };
}

/// Replaces the monitor work area for every form on this thread (tests only; `None` ends it).
#[cfg(test)]
pub fn force_work_area(rect: Option<RECT>) {
    FORCED_WORK_AREA.with(|w| w.set(rect));
}

/// The work area of the monitor nearest to `hwnd` (or the forced test work area).
fn monitor_work_area(hwnd: HWND) -> win::Result<RECT> {
    if let Some(forced) = FORCED_WORK_AREA.with(Cell::get) {
        return Ok(forced);
    }
    // SAFETY: Read-only window-to-monitor query; the handle is checked by monitor_work.
    monitor_work(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) })
}

fn monitor_work(monitor: HMONITOR) -> win::Result<RECT> {
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Writable, correctly sized MONITORINFO. A disconnected monitor can fail.
    if !unsafe { GetMonitorInfoW(monitor, &mut mi) }.as_bool() {
        return Err(win::Error::last("GetMonitorInfoW"));
    }
    // Never shrink a live window to the zero RECT of a failed or unusable monitor query.
    if mi.rcWork.right <= mi.rcWork.left || mi.rcWork.bottom <= mi.rcWork.top {
        return Err(win::Error::msg("GetMonitorInfoW", "empty work area"));
    }
    Ok(mi.rcWork)
}

/// Clamps an outer rectangle into `work`: never larger than the work area, and moved inside
/// it when it hangs over an edge (DESIGN.md 11.1).
fn fit_to_work_area(mut r: RECT, work: RECT) -> RECT {
    let (work_w, work_h) = (work.right - work.left, work.bottom - work.top);
    let w = (r.right - r.left).min(work_w);
    let h = (r.bottom - r.top).min(work_h);
    r.left = r.left.min(work.right - w).max(work.left);
    r.top = r.top.min(work.bottom - h).max(work.top);
    r.right = r.left + w;
    r.bottom = r.top + h;
    r
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
    if !on_window_thread(hwnd) || atom == 0 || class_atom(hwnd) != atom {
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

/// Whether a live window belongs to the calling UI thread (required before borrowing its Rc).
pub(crate) fn on_window_thread(hwnd: HWND) -> bool {
    // SAFETY: Read-only thread id queries, with no pointers retained or messages dispatched.
    unsafe { GetWindowThreadProcessId(hwnd, None) == GetCurrentThreadId() }
}

fn form_state(hwnd: HWND) -> Option<Rc<FormState>> {
    let atoms = atoms();
    let atom = class_atom(hwnd);
    if atom == atoms.form || atom == atoms.message_box {
        user_rc(hwnd, atom)
    } else {
        None
    }
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
    // SAFETY: Valid window, paint buffer DC, and owned brush.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, brush.handle());
    }
}

/// Validates a container's update region, paints its card outline when it has one, and the
/// focus ring of its focused button (nothing is painted while the window is minimized,
/// DESIGN.md 8.6).
fn paint_container(hwnd: HWND, back: Color, card: Option<(Color, i32)>, brush: &controls::Brush) {
    let paint = controls::Paint::begin(hwnd);
    let root = root_of(hwnd);
    // SAFETY: Read-only state query of the top-level window.
    if unsafe { IsIconic(root) }.as_bool() {
        return;
    }
    let size = client_size(hwnd);
    let area = Rect {
        x: 0,
        y: 0,
        w: size.w,
        h: size.h,
    };
    // The background, rounded card and focus ring must reach the screen together. Drawing
    // the background in WM_ERASEBKGND exposes the outer color before the card is ready.
    controls::buffered(paint.hdc(), area, |hdc| {
        fill_client(hwnd, hdc, brush);
        if let Some((outer, radius)) = card {
            let dpi = form_state(root).map_or(dpi::BASE_DPI, |s| s.dpi.get());
            controls::paint_card(hwnd, hdc, outer, back, dpi::scale(radius, dpi));
        }
        controls::paint_focus_ring(hwnd, hdc, back);
        // The ring buffer fills its interior with the container color; restore the input's
        // frame and padding afterward (the native EDIT occupies only its centered text area).
        controls::paint_input_frames(hwnd, hdc, back);
    });
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
            let dpi = self.dpi.get();
            for spec in std::iter::once(ctl.font()).chain(ctl.icon_font()) {
                if !self.fonts.borrow().iter().any(|f| f.spec() == spec) {
                    self.fonts.borrow_mut().push(Font::new(spec, dpi)?);
                }
            }
        }
        for c in node.children() {
            self.ensure_fonts(c)?;
        }
        Ok(())
    }

    /// The icon font handle of a leaf (0 when it has none).
    fn icon_font_of(&self, ctl: &controls::Ctl) -> windows::Win32::Graphics::Gdi::HFONT {
        ctl.icon_font()
            .map_or_else(Default::default, |spec| self.font(spec))
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
                    let icon = self.icon_font_of(ctl);
                    let hwnd = controls::create(parent, child, back, font, icon, self.dpi.get())?;
                    if matches!(ctl, controls::Ctl::Edit(_) | controls::Ctl::CheckedList(_)) {
                        dark_scrollbars(hwnd);
                    }
                    self.hwnds.borrow_mut().insert(child.id, hwnd);
                }
                _ => {
                    let hwnd = create_panel(parent, child, back, inherited)?;
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
                    Kind::Leaf(ctl) => controls::measure_live(
                        self.hwnd_of(node.id).unwrap_or_default(),
                        ctl,
                        font_of(ctl.font()),
                        node.padding,
                        node.min,
                        proposed,
                        dpi,
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
                    let r = controls::input_bounds(*child, *r);
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
        for (parent, child, new, old) in &placements {
            if new.w != old.w || new.h != old.h {
                // SAFETY: Repaint request for a child of this form.
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(*child), None, true);
                }
            }
            if new != old
                && let Some(ring) = controls::focused_ring(*child)
            {
                // The ring sits outside the child, so a move leaves it behind: repaint the
                // parent around the old and the new place.
                let moved = RECT {
                    left: ring.left + old.x - new.x,
                    top: ring.top + old.y - new.y,
                    right: ring.right + old.x - new.x,
                    bottom: ring.bottom + old.y - new.y,
                };
                // SAFETY: Invalidates rectangles of a live parent window of this form.
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                        Some(*parent),
                        Some(&moved),
                        true,
                    );
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                        Some(*parent),
                        Some(&ring),
                        true,
                    );
                }
            }
        }
        self.layout_count.set(self.layout_count.get() + 1);
    }

    fn destroy(&self) {
        if self.destroyed.get() || self.destroying.replace(true) {
            return;
        }
        self.enable_owner();
        // SAFETY: Destroys our own top-level window on its thread.
        if let Err(error) = unsafe { DestroyWindow(self.hwnd.get()) } {
            self.destroying.set(false);
            if !self.destroyed.get() {
                show_error(
                    self.hwnd.get(),
                    &win::Error::from_win("DestroyWindow", error).to_string(),
                    "HWID Checker",
                );
            }
        }
    }

    fn enable_owner(&self) {
        if self.modal.replace(false) && !self.owner.is_invalid() {
            // Re-enable the owner first so Windows activates it, not another app.
            // SAFETY: Enabling the owner window that `run_modal` disabled.
            unsafe {
                let _ = EnableWindow(self.owner, true);
            }
        }
    }

    fn on_dpi_changed(&self, new_dpi: u32, suggested: RECT) {
        // Build the complete replacement before changing the cache. A failed font must not
        // delete the fonts still borrowed by native controls or silently substitute HFONT(0).
        let fresh: win::Result<Vec<Font>> = self
            .fonts
            .borrow()
            .iter()
            .map(|f| Font::new(f.spec(), new_dpi))
            .collect();
        let fresh = match fresh {
            Ok(fonts) => fonts,
            Err(error) => {
                show_error(self.hwnd.get(), &error.to_string(), "HWID Checker");
                return;
            }
        };
        let old = self.dpi.get();
        // A redraw lock cannot make nested native HWND surfaces present atomically. Keep a
        // complete client image above them until the single DPI layout and repaint finish.
        let cover =
            super::dpi_present::DpiCover::new(self.hwnd.get(), client_size(self.hwnd.get()));
        {
            let Ok(mut tree) = self.tree.try_borrow_mut() else {
                return;
            };
            tree.rescale(old, new_dpi);
        }
        self.in_dpi_change.set(true);
        let change = DpiChange(&self.in_dpi_change);
        self.dpi.set(new_dpi);
        let old_fonts = std::mem::replace(&mut *self.fonts.borrow_mut(), fresh);
        let placements = {
            let tree = self.tree.borrow();
            let hwnds = self.hwnds.borrow();
            let mut placements = Vec::new();
            collect_fonts(&tree, &hwnds, self, &mut placements);
            placements
        };
        // WM_SETFONT is synchronous. Release every tree/map/cache borrow before entering a
        // native control, and retain old fonts until every control received its replacement.
        for (h, font, icon, padding) in placements {
            controls::apply_dpi(h, font, icon, padding, new_dpi);
        }
        drop(old_fonts);
        // The suggested rectangle is clamped to the work area of the monitor it lands on
        // before the one move, so a maximized window keeps Windows' own placement and a
        // normal one never hangs over the screen (DESIGN.md 11.1; one layout pass, 8.8).
        // SAFETY: Read-only state query of our own window.
        let zoomed = unsafe { IsZoomed(self.hwnd.get()) }.as_bool();
        let target = if zoomed {
            suggested
        } else {
            // Match MonitorFromWindow: greatest intersection, not the rectangle's centre
            // (which can select a different monitor on vertically offset displays).
            match work_area_for_rect(suggested) {
                Ok(work) => fit_to_work_area(suggested, work),
                Err(error) => {
                    // Keep Windows' suggested placement if a monitor disappears mid-query.
                    win::record(error);
                    suggested
                }
            }
        };
        if let Some(cover) = &cover {
            let (style, ex) = styles(&self.spec);
            if let Ok(frame) = dpi::outer_for_client(Size::default(), style, ex, new_dpi) {
                let previous = client_size(self.hwnd.get());
                // Never shrink the cover before the parent: that exposes old native surfaces
                // around a smaller snapshot. The new client clips an oversized cover instead.
                cover.resize(Size {
                    w: (target.right - target.left - frame.w).max(previous.w),
                    h: (target.bottom - target.top - frame.h).max(previous.h),
                });
            }
        }
        // SAFETY: Moves our own window to the rectangle Windows suggested for the new DPI.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd.get(),
                None,
                target.left,
                target.top,
                target.right - target.left,
                target.bottom - target.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        drop(change);
        self.dispatch(Event::Resize {
            client: client_size(self.hwnd.get()),
            dpi: new_dpi,
        });
        self.relayout();
        // SAFETY: Repaint everything at the new scale.
        unsafe {
            use windows::Win32::Graphics::Gdi::{
                RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow,
            };
            let _ = RedrawWindow(
                Some(self.hwnd.get()),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
        }
        drop(cover);
    }

    fn focus_changed(&self, hwnd: HWND) {
        self.last_focus.set(hwnd);
    }

    /// Re-checks the work-area fit of a restored window after the display or the work area
    /// changed (DESIGN.md 11.1); maximized and minimized windows are left to Windows.
    fn refit(&self) {
        let hwnd = self.hwnd.get();
        // SAFETY: Read-only state queries of our own window.
        if unsafe { IsZoomed(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
            return;
        }
        let mut r = RECT::default();
        // SAFETY: Writable RECT of our own window.
        if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
            return;
        }
        let work = match monitor_work_area(hwnd) {
            Ok(work) => work,
            Err(error) => {
                win::record(error);
                return;
            }
        };
        let fitted = fit_to_work_area(r, work);
        if fitted != r {
            // SAFETY: Moves our own window inside its monitor's work area.
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    fitted.left,
                    fitted.top,
                    fitted.right - fitted.left,
                    fitted.bottom - fitted.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    /// `WM_SIZE`: timers stop while minimized and restart on restore (DESIGN.md 8.6).
    fn minimized_changed(&self, minimized: bool) {
        if self.minimized.replace(minimized) == minimized {
            return;
        }
        let timers: Vec<(usize, u32)> =
            self.timers.borrow().iter().map(|(i, m)| (*i, *m)).collect();
        for (id, ms) in timers {
            // SAFETY: Window timers on our own window, no callback.
            unsafe {
                if minimized {
                    let _ = KillTimer(Some(self.hwnd.get()), id);
                } else {
                    SetTimer(Some(self.hwnd.get()), id, ms, None);
                }
            }
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

type FontPlacement = (
    HWND,
    windows::Win32::Graphics::Gdi::HFONT,
    windows::Win32::Graphics::Gdi::HFONT,
    layout::Pad,
);

fn collect_fonts(
    node: &Node,
    hwnds: &HashMap<u16, HWND>,
    state: &FormState,
    out: &mut Vec<FontPlacement>,
) {
    for c in node.children() {
        if let (Kind::Leaf(ctl), Some(&h)) = (&c.kind, hwnds.get(&c.id)) {
            out.push((
                h,
                state.font(ctl.font()),
                state.icon_font_of(ctl),
                c.padding,
            ));
        }
        collect_fonts(c, hwnds, state, out);
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

fn create_panel(parent: HWND, node: &Node, back: Color, outer: Color) -> win::Result<HWND> {
    let state = Rc::new(PanelState {
        back,
        // The parent's color appears only under the card's rounded corners.
        brush: controls::Brush::new(if node.card_radius.is_some() {
            outer
        } else {
            back
        }),
        card: node.card_radius.map(|r| (outer, r)),
    });
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
    // SAFETY: Creation is synchronous. WM_NCCREATE borrows `state` during this call and stores
    // its own Rc clone, released by WM_NCDESTROY; an early failure acquires no reference.
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
            Some((&state as *const Rc<PanelState>).cast()),
        )
    };
    hwnd.map_err(|e| win::Error::from_win("CreateWindowExW", e))
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
    catch_unwind(AssertUnwindSafe(|| {
        panel_message(hwnd, msg, wparam, lparam).unwrap_or_else(|| {
            // SAFETY: Default processing with unchanged parameters.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        })
    }))
    .unwrap_or_else(|_| {
        // SAFETY: Default processing after a caught panic.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

fn panel_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN
        && controls::focus_input_frame(hwnd, lparam)
    {
        return Some(LRESULT(0));
    }
    match msg {
        WM_NCCREATE => {
            // SAFETY: For WM_NCCREATE, lParam points to the CREATESTRUCTW of this window.
            let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            // SAFETY: create_panel passes a live Rc for this synchronous creation callback.
            let state = unsafe { &*(cs.lpCreateParams as *const Rc<PanelState>) };
            let raw = Rc::into_raw(Rc::clone(state));
            // SAFETY: The window owns this clone until WM_NCDESTROY clears it.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize) };
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
        // WM_PAINT covers the background in the same buffer as the card and focus ring.
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_PAINT => {
            let state: Rc<PanelState> = user_rc(hwnd, atoms().panel)?;
            paint_container(hwnd, state.back, state.card, &state.brush);
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
        CtlEvent::Click => controls::toggle_click(child)
            .map_or(Event::Click(id), |checked| Event::Toggled(id, checked)),
        CtlEvent::TextChanged => Event::TextChanged(id),
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
    catch_unwind(AssertUnwindSafe(|| {
        form_message(hwnd, msg, wparam, lparam).unwrap_or_else(|| {
            // SAFETY: Default processing with unchanged parameters.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        })
    }))
    .unwrap_or_else(|_| {
        // SAFETY: Default processing after a caught panic.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

fn form_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN
        && controls::focus_input_frame(hwnd, lparam)
    {
        return Some(LRESULT(0));
    }
    if msg == WM_NCCREATE {
        // SAFETY: For WM_NCCREATE, lParam points to the CREATESTRUCTW of this window.
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        // SAFETY: Form::create passes a live Rc for this synchronous creation callback.
        let state = unsafe { &*(cs.lpCreateParams as *const Rc<FormState>) };
        let raw = Rc::into_raw(Rc::clone(state));
        // SAFETY: The window owns this clone until WM_NCDESTROY clears it.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize) };
        state.hwnd.set(hwnd);
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
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_PAINT => {
            let brush = state.brush.borrow();
            paint_container(hwnd, state.spec.back, None, brush.as_ref()?);
            Some(LRESULT(0))
        }
        WM_SIZE => {
            let minimized = wparam.0 as u32 == SIZE_MINIMIZED;
            state.minimized_changed(minimized);
            if minimized {
                return Some(LRESULT(0));
            }
            if wparam.0 as u32 == SIZE_RESTORED && !state.in_dpi_change.get() {
                // Remember the user's restored client size in logical pixels (F6).
                let client = client_size(hwnd);
                let dpi = state.dpi.get();
                state.logical_client.set(Size {
                    w: dpi::unscale(client.w, dpi),
                    h: dpi::unscale(client.h, dpi),
                });
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
            // The minimum never exceeds the work area of the window's monitor (DESIGN.md 11.1).
            let work = match monitor_work_area(hwnd) {
                Ok(work) => work,
                Err(error) => {
                    win::record(error);
                    return None;
                }
            };
            // SAFETY: For WM_GETMINMAXINFO, lParam points to a writable MINMAXINFO.
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            mmi.ptMinTrackSize.x = dpi::scale(min.w, dpi).min(work.right - work.left);
            mmi.ptMinTrackSize.y = dpi::scale(min.h, dpi).min(work.bottom - work.top);
            Some(LRESULT(0))
        }
        WM_GETDPISCALEDSIZE => {
            // Keep the logical client size across monitor moves: the new outer size is the
            // frame at the new DPI around the scaled client (audit F6), not the linear scale
            // of the old outer size.
            let new_dpi = wparam.0 as u32;
            // SAFETY: Reads the style of our own window.
            let zoomed = unsafe { IsZoomed(hwnd) }.as_bool();
            if zoomed || new_dpi == 0 {
                return None;
            }
            let (style, ex) = styles(&state.spec);
            let client = dpi::scale_size(state.logical_client.get(), new_dpi);
            let Ok(outer) = dpi::outer_for_client(client, style, ex, new_dpi) else {
                return None;
            };
            // SAFETY: For WM_GETDPISCALEDSIZE, lParam points to a writable SIZE.
            let size = unsafe { &mut *(lparam.0 as *mut windows::Win32::Foundation::SIZE) };
            size.cx = outer.w;
            size.cy = outer.h;
            Some(LRESULT(1))
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            // SAFETY: For WM_DPICHANGED, lParam points to the suggested window RECT.
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            state.on_dpi_changed(new_dpi, suggested);
            Some(LRESULT(0))
        }
        WM_DISPLAYCHANGE => {
            state.refit();
            None
        }
        WM_SETTINGCHANGE if wparam.0 as u32 == SPI_SETWORKAREA.0 => {
            state.refit();
            None
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
            // Form::close tags queued requests, so HWND reuse cannot close a newer form.
            // Native WM_CLOSE has wParam == 0.
            if wparam.0 != 0 && wparam.0 as u64 != state.serial {
                return Some(LRESULT(0));
            }
            if state.dispatch(Event::CloseRequest) {
                state.destroy();
            }
            Some(LRESULT(0))
        }
        WM_DESTROY => {
            let timers = std::mem::take(&mut *state.timers.borrow_mut());
            for id in timers.into_keys() {
                // SAFETY: Removes this window's timers before its destruction handler runs.
                unsafe {
                    let _ = KillTimer(Some(hwnd), id);
                }
            }
            state.destroyed.set(true);
            state.enable_owner();
            // GetMessage dispatches sent messages internally and can keep waiting after a
            // synchronous WM_CLOSE destroys this form. Wake it so `done` is checked again.
            // SAFETY: A pointer-free thread message to this window's UI thread.
            unsafe {
                let _ = PostMessageW(None, WM_NULL, WPARAM(0), LPARAM(0));
            }
            state.dispatch(Event::Destroyed);
            Some(LRESULT(0))
        }
        WM_TIMER => {
            // KillTimer does not remove an already queued WM_TIMER. Discard it after a
            // stop or minimize rather than repainting a hidden indicator or advancing work.
            let running = !state.minimized.get() && state.timers.borrow().contains_key(&wparam.0);
            if running {
                state.dispatch(Event::Timer(wparam.0));
            }
            Some(LRESULT(0))
        }
        WM_COMMAND if lparam.0 == 0 => {
            // A command by id (accelerator-style, also what tests post): click that button.
            let id = (wparam.0 & 0xFFFF) as u16;
            let target = state.hwnd_of(id);
            if let Some(h) = target
                && controls::is_button(h)
                // SAFETY: Read-only enabled-state query of our own control.
                && unsafe { IsWindowEnabled(h) }.as_bool()
            {
                control_event(h, id, CtlEvent::Click);
            }
            Some(LRESULT(0))
        }
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
        let target = if !last.is_invalid()
            && IsWindow(Some(last)).as_bool()
            && root_of(last) == hwnd
            && IsWindowEnabled(last).as_bool()
            && IsWindowVisible(last).as_bool()
        {
            last
        } else if state.spec.message_box {
            // The right-to-left button row is created No then Yes. Initial focus must
            // still be the declared default, so Enter answers Yes on a Yes/No box.
            state
                .spec
                .accept
                .and_then(|id| state.hwnd_of(id))
                .unwrap_or_default()
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

/// The window styles of a form spec (tests use them for frame arithmetic).
pub(crate) fn styles(spec: &FormSpec) -> (WINDOW_STYLE, WINDOW_EX_STYLE) {
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

/// The work area of the monitor at `pt` (or the forced test work area).
fn work_area_at(pt: POINT) -> win::Result<RECT> {
    if let Some(forced) = FORCED_WORK_AREA.with(Cell::get) {
        return Ok(forced);
    }
    // SAFETY: Read-only point-to-monitor query; the handle is checked by monitor_work.
    monitor_work(unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) })
}

fn work_area_for_rect(rect: RECT) -> win::Result<RECT> {
    if let Some(forced) = FORCED_WORK_AREA.with(Cell::get) {
        return Ok(forced);
    }
    // SAFETY: The live input rectangle is used only for this read-only monitor query.
    monitor_work(unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) })
}

fn work_area(owner: HWND, start: StartPosition) -> win::Result<RECT> {
    if start == StartPosition::CenterParent && !owner.is_invalid() {
        monitor_work_area(owner)
    } else {
        let mut pt = POINT::default();
        // SAFETY: Writable POINT.
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        work_area_at(pt)
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
            last_focus: Cell::new(HWND::default()),
            destroyed: Cell::new(false),
            destroying: Cell::new(false),
            maximize: Cell::new(false),
            timers: RefCell::new(HashMap::new()),
            minimized: Cell::new(false),
            logical_client: Cell::new(Size::default()),
        });
        let work = work_area(owner, spec.start)?;
        let title = to_wide(&spec.title);
        let class = if spec.message_box {
            message_box_class()
        } else {
            form_class()
        };
        // SAFETY: Static class name and terminated title. WM_NCCREATE borrows `state` during
        // this synchronous call and stores its own Rc clone, released at WM_NCDESTROY.
        let created = unsafe {
            CreateWindowExW(
                ex,
                class,
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
                Some((&state as *const Rc<FormState>).cast()),
            )
        };
        let hwnd = match created {
            Ok(h) => h,
            Err(e) => return Err(win::Error::from_win("CreateWindowExW", e)),
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
        let outer = match spec.size {
            WindowSize::Client(s) => {
                match dpi::outer_for_client(dpi::scale_size(s, dpi), style, ex, dpi) {
                    Ok(size) => size,
                    Err(error) => {
                        state.destroy();
                        return Err(error);
                    }
                }
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
        // AD-38 decides from the unclamped size; the restored window is then clamped to the
        // work area so it never hangs over the screen (DESIGN.md 11.1).
        state
            .maximize
            .set(spec.maximize_if_too_big && (outer.w > work_w || outer.h > work_h));
        let outer = Size {
            w: outer.w.min(work_w),
            h: outer.h.min(work_h),
        };
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
        form_state(self.hwnd)
            .filter(|s| s.serial == self.serial && !s.destroyed.get() && !s.destroying.get())
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
        if let Some(state) = self.state() {
            let auto_size = state
                .tree
                .try_borrow()
                .ok()
                .is_some_and(|tree| tree.find(id).is_some_and(|n| n.auto_size));
            if auto_size {
                state.relayout();
            }
        }
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

    /// Shows or hides a control's native window without a layout pass (for `Resize` handlers
    /// that already set the node's `visible`).
    pub fn show_control(&self, id: u16, visible: bool) {
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
            && let Some(h) = self.control(id)
        {
            control_event(h, id, CtlEvent::Click);
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

    /// Marks a sidebar button as the active item or not.
    pub fn set_active(&self, id: u16, active: bool) {
        self.with_control(id, |h| controls::set_active(h, active));
    }

    /// Sets a toggle without generating an event (initial load or failed-save rollback).
    pub fn set_checked(&self, id: u16, checked: bool) {
        self.with_control(id, |h| controls::set_checked(h, checked));
    }

    /// Reads a toggle's state.
    pub fn is_checked(&self, id: u16) -> bool {
        self.control(id).is_some_and(controls::is_checked)
    }

    /// Overrides text and icon color; None restores the button kind's color.
    pub fn set_button_fore(&self, id: u16, fore: Option<Color>) {
        self.with_control(id, |h| controls::set_button_fore(h, fore));
    }

    /// Marks a sidebar button as not collected yet (`FAINT`) or collected.
    pub fn set_pending(&self, id: u16, pending: bool) {
        self.with_control(id, |h| controls::set_pending(h, pending));
    }

    /// Repaints a spinner for its next frame (call from a form timer while it shows).
    pub fn spin(&self, id: u16) {
        self.with_control(id, controls::spin);
    }

    /// Whether a spinner animates (false when Windows "Show animations" is off).
    pub fn spinner_animates(&self, id: u16) -> bool {
        self.control(id).is_some_and(controls::spinner_animates)
    }

    /// Copies the whole text of an edit to the clipboard (its selection is kept).
    pub fn edit_copy_all(&self, id: u16) {
        self.with_control(id, controls::edit_copy_all);
    }

    /// Whether the window is maximized.
    pub fn is_maximized(&self) -> bool {
        // SAFETY: Read-only state query.
        self.state().is_some() && unsafe { IsZoomed(self.hwnd) }.as_bool()
    }

    /// The outer window rectangle in screen coordinates.
    pub fn window_rect(&self) -> RECT {
        let mut r = RECT::default();
        if self.state().is_some() {
            // SAFETY: Writable RECT of our own window.
            unsafe {
                let _ = GetWindowRect(self.hwnd, &mut r);
            }
        }
        r
    }

    /// Whether a sidebar button is the active item.
    pub fn is_active(&self, id: u16) -> bool {
        self.control(id).is_some_and(controls::is_active)
    }

    /// Sets a label's text color (status map).
    pub fn set_label_color(&self, id: u16, color: Color) {
        self.with_control(id, |h| controls::set_label_color(h, color));
    }

    /// Sets the window title.
    pub fn set_title(&self, title: &str) {
        let Some(_state) = self.state() else {
            return;
        };
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
        self.state()
            .map_or(Size::default(), |_| client_size(self.hwnd))
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

    /// Starts or restarts timer `id` (`Event::Timer(id)` every `ms`); while the window is
    /// minimized it is only recorded and starts on restore.
    pub fn set_timer(&self, id: usize, ms: u32) {
        let Some(state) = self.state() else {
            return;
        };
        state.timers.borrow_mut().insert(id, ms);
        if state.minimized.get() {
            return;
        }
        // SAFETY: Window timer on our own window, no callback.
        unsafe {
            SetTimer(Some(self.hwnd), id, ms, None);
        }
    }

    /// Stops timer `id`.
    pub fn kill_timer(&self, id: usize) {
        let Some(state) = self.state() else {
            return;
        };
        state.timers.borrow_mut().remove(&id);
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
        let Some(_state) = self.state() else {
            return;
        };
        // SAFETY: Posts WM_CLOSE to our own window.
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd),
                WM_CLOSE,
                WPARAM(self.serial as usize),
                LPARAM(0),
            );
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
    let _modal = ModalWindow(MODAL_WINDOW.with(|m| m.replace(Some(form))));
    // Only restore an owner that this loop actually disabled.
    // SAFETY: Queries the owner window on the UI thread.
    if !owner.is_invalid() && unsafe { IsWindowEnabled(owner) }.as_bool() {
        state.modal.set(true);
        // SAFETY: Disables the owner for the modal loop; `destroy` re-enables it.
        unsafe {
            let _ = EnableWindow(owner, false);
        }
    }
    form.show();
    pump_until(|| state.destroyed.get());
    // WM_QUIT or GetMessage failure can end the loop while the window is still alive.
    state.destroy();
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
    state.destroy();
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
pub(super) fn pre_translate(msg: &MSG) -> bool {
    if msg.hwnd.is_invalid() {
        return false;
    }
    let root = root_of(msg.hwnd);
    let Some(state) = form_state(root) else {
        return false;
    };
    if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
        let vk = msg.wParam.0 as u16;
        if vk == VK_RETURN.0 || vk == VK_ESCAPE.0 {
            // WinForms Form.ProcessDialogKey excludes Ctrl/Alt combinations. Bypass native
            // dialog translation too, so a modified Enter cannot activate a confirmation.
            // SAFETY: Reads this UI thread's synchronous modifier state.
            let modified = unsafe {
                GetKeyState(i32::from(VK_CONTROL.0)) < 0 || GetKeyState(i32::from(VK_MENU.0)) < 0
            };
            if modified {
                return false;
            }
        }
    }
    if msg.message == WM_KEYDOWN {
        let vk = msg.wParam.0 as u16;
        let form = state.form();
        if state.spec.find_keys {
            use windows::Win32::UI::Input::KeyboardAndMouse::{VK_F3, VK_SHIFT};
            // SAFETY: Reads this UI thread's focus and synchronous modifier states.
            let (focus, ctrl, alt, backwards) = unsafe {
                (
                    GetFocus(),
                    GetKeyState(i32::from(VK_CONTROL.0)) < 0,
                    GetKeyState(i32::from(VK_MENU.0)) < 0,
                    GetKeyState(i32::from(VK_SHIFT.0)) < 0,
                )
            };
            let id = state
                .hwnds
                .borrow()
                .iter()
                .find_map(|(&id, &h)| (h == focus).then_some(id));
            let key = if alt {
                None
            } else if ctrl && vk == u16::from(b'F') {
                Some(FindKey::Open)
            } else if !ctrl && vk == VK_F3.0 {
                Some(FindKey::Step { backwards })
            } else if !ctrl && vk == VK_RETURN.0 && controls::is_input(focus) {
                id.map(|id| FindKey::Enter { id, backwards })
            } else if !ctrl && vk == VK_ESCAPE.0 {
                Some(FindKey::Escape { id })
            } else {
                None
            };
            if let Some(key) = key
                && state.dispatch(Event::Key(key))
            {
                return true;
            }
        }
        if vk == u16::from(b'C')
            && let Some(id) = state.spec.copy_on_ctrl_c
            // SAFETY: Reads this UI thread's synchronous modifier state.
            && unsafe { GetKeyState(i32::from(VK_CONTROL.0)) < 0 }
        {
            // Ctrl+C anywhere in the form copies the whole message (message boxes).
            if let Some(h) = state.hwnd_of(id) {
                controls::edit_copy_all(h);
            }
            return true;
        }
        if vk == VK_RETURN.0 {
            // SAFETY: Reads this thread's focus window.
            let focus = unsafe { GetFocus() };
            if controls::is_button(focus) && root_of(focus) == root {
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

// uxtheme uses C++ bool (one byte), not Win32 BOOL (four bytes), for these exports.
type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, bool) -> bool;
type AllowDarkModeForApp = unsafe extern "system" fn(bool) -> bool;
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
        let build = os_build();
        if build < 17763 {
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
                if build < 18362 {
                    let f: AllowDarkModeForApp = std::mem::transmute(f);
                    f(true);
                } else {
                    let f: SetPreferredAppMode = std::mem::transmute(f);
                    f(1); // PreferredAppMode::AllowDark
                }
            }
            let allow_window = GetProcAddress(ux, PCSTR(std::ptr::without_provenance(133)))
                .map(|f| std::mem::transmute::<_, AllowDarkModeForWindow>(f));
            DarkApi { allow_window }
        }
    })
}

/// Dark title bar (`DWMWA_USE_IMMERSIVE_DARK_MODE`: 20, or 19 before build 18985) and, on
/// Windows 11, the token colors for the caption (35), border (34), and caption text (36).
/// Failures are recorded, never fatal: the window then keeps the system frame colors.
pub fn dark_title_bar(hwnd: HWND) {
    let build = os_build();
    if build < 17763 {
        return;
    }
    let on = BOOL(1);
    let attr = if build < 18985 { 19 } else { 20 };
    // SAFETY: `on` is a 4-byte BOOL that lives through the call.
    let r = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(attr),
            (&on as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
    if let Err(error) = r {
        win::record(win::Error::from_win(
            "DwmSetWindowAttribute dark mode",
            error,
        ));
    }
    if build < 22000 {
        return;
    }
    for (attr, name, color) in [
        (35, "DwmSetWindowAttribute caption color", theme::BG),
        (34, "DwmSetWindowAttribute border color", theme::BORDER),
        (36, "DwmSetWindowAttribute caption text color", theme::TEXT),
    ] {
        let value = color.colorref();
        // SAFETY: `value` is a 4-byte COLORREF that lives through the call.
        let r = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(attr),
                (&value as *const windows::Win32::Foundation::COLORREF).cast(),
                std::mem::size_of::<windows::Win32::Foundation::COLORREF>() as u32,
            )
        };
        if let Err(error) = r {
            win::record(win::Error::from_win(name, error));
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
        let _ = allow(hwnd, true);
        let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
    }
}
