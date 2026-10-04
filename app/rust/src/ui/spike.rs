//! WP-10a spike: `cargo test --locked --lib -- --ignored ui::spike --nocapture`.
//! Opens real windows on the desktop for about half a minute and drives them by itself.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::window::{self, *};
use super::{
    dpi,
    layout::{Kind, Node, Rect, Size},
    theme,
};
use crate::ui::controls::{Align, ButtonSpec, Ctl, EditSpec, LabelSpec, ListSpec};
use crate::ui::layout::{Anchor, FlowDir, Point, Track};
use crate::win::wide::to_wide;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP, HFONT, LOGFONTW,
    RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RedrawWindow, ReleaseDC, SelectObject,
};
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardState, SetKeyboardState, VK_CONTROL, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowTextLengthW, HWND_NOTOPMOST, HWND_TOPMOST, PM_REMOVE, PeekMessageW,
    SM_CMONITORS, SendMessageW, WM_CHAR, WM_GETFONT, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR},
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
    fn DeactivateActCtx(flags: u32, cookie: usize) -> i32;
    fn ReleaseActCtx(h: *mut core::ffi::c_void);
    fn GlobalLock(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalUnlock(h: *mut core::ffi::c_void) -> i32;
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
    fn GlobalFree(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalSize(h: *mut core::ffi::c_void) -> usize;
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

struct Activation {
    handle: *mut core::ffi::c_void,
    cookie: Option<usize>,
}

impl Drop for Activation {
    fn drop(&mut self) {
        // SAFETY: The activation and handle belong to this test thread; deactivate before release.
        unsafe {
            if let Some(cookie) = self.cookie {
                DeactivateActCtx(0, cookie);
            }
            ReleaseActCtx(self.handle);
        }
    }
}

struct Clipboard;

impl Clipboard {
    fn open(owner: HWND) -> Option<Self> {
        // SAFETY: Opens this thread's clipboard access; the guard closes it.
        (unsafe { OpenClipboard(owner.0) } != 0).then(|| Self)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: Balances this guard's successful OpenClipboard.
        unsafe {
            CloseClipboard();
        }
    }
}

struct GlobalMemory(*mut core::ffi::c_void);

impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Frees the GlobalAlloc allocation unless ownership went to the clipboard.
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}

struct GlobalMapping(*mut core::ffi::c_void, *mut u16);

impl Drop for GlobalMapping {
    fn drop(&mut self) {
        // SAFETY: Balances the successful GlobalLock stored in this guard.
        unsafe {
            GlobalUnlock(self.0);
        }
    }
}

/// The test exe has no manifest; activate Common Controls 6 like the app manifest does.
fn activate_comctl6() -> Option<Activation> {
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
    // SAFETY: `ctx` and the path buffer live through the call; the guard balances both APIs.
    unsafe {
        let h = CreateActCtxW(&ctx);
        if h.is_null() || h as isize == -1 {
            return None;
        }
        let mut activation = Activation {
            handle: h,
            cookie: None,
        };
        let mut cookie = 0usize;
        if ActivateActCtx(h, &mut cookie) == 0 {
            return None;
        }
        activation.cookie = Some(cookie);
        Some(activation)
    }
}

fn pump_for(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        let mut msg = MSG::default();
        // SAFETY: Standard non-blocking message pump on the test (UI) thread.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if !window::pre_translate(&msg) {
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
        let attached = other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
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
    // SAFETY: Memory DC and bitmap are created, used, and released by the guard; buffers are
    // sized for the requested 32-bit top-down DIB.
    unsafe {
        let mut capture = CaptureResources {
            screen: GetDC(None),
            ..Default::default()
        };
        assert!(!capture.screen.is_invalid(), "capture screen DC");
        capture.mem = CreateCompatibleDC(Some(capture.screen));
        assert!(!capture.mem.is_invalid(), "capture memory DC");
        capture.bmp = CreateCompatibleBitmap(capture.screen, w, hgt);
        assert!(!capture.bmp.is_invalid(), "capture bitmap");
        capture.old = SelectObject(capture.mem, capture.bmp.into());
        assert!(!capture.old.is_invalid(), "select capture bitmap");
        assert_ne!(PrintWindow(h.0, capture.mem.0, 2), 0, "PrintWindow capture");
        SelectObject(capture.mem, capture.old);
        capture.old = HGDIOBJ::default();
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
        let lines = GetDIBits(
            capture.mem,
            capture.bmp,
            0,
            hgt as u32,
            Some(bits.as_mut_ptr().cast()),
            &mut bi,
            DIB_RGB_COLORS,
        );
        assert_eq!(lines, hgt, "complete capture pixels");
    }
    (w, hgt, bits)
}

#[derive(Default)]
struct CaptureResources {
    screen: HDC,
    mem: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
}

impl Drop for CaptureResources {
    fn drop(&mut self) {
        // SAFETY: Restores the selected bitmap before deleting it and releases only handles
        // acquired by capture; also runs when a capture assertion unwinds.
        unsafe {
            if !self.old.is_invalid() {
                SelectObject(self.mem, self.old);
            }
            if !self.bmp.is_invalid() {
                let _ = DeleteObject(self.bmp.into());
            }
            if !self.mem.is_invalid() {
                let _ = DeleteDC(self.mem);
            }
            if !self.screen.is_invalid() {
                ReleaseDC(None, self.screen);
            }
        }
    }
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
    let _clipboard = Clipboard::open(HWND::default())?;
    // SAFETY: Clipboard is open. The HGLOBAL size bounds every read, and the mapping guard
    // holds the successful lock until the string is copied (including on allocation panic).
    unsafe {
        let h = GetClipboardData(CF_UNICODETEXT);
        if h.is_null() {
            return None;
        }
        let bytes = GlobalSize(h);
        if bytes == 0 || bytes > isize::MAX as usize {
            return None;
        }
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            return None;
        }
        let mapping = GlobalMapping(h, p);
        let units = std::slice::from_raw_parts(mapping.1, bytes / 2);
        let n = units.iter().position(|&u| u == 0)?;
        Some(String::from_utf16_lossy(&units[..n]))
    }
}

fn set_clipboard_text(owner: HWND, text: &str) -> bool {
    let wide = to_wide(text);
    let Some(_clipboard) = Clipboard::open(owner) else {
        return false;
    };
    // SAFETY: Both allocation and lock are checked before copying exactly the allocated length.
    // Guards unlock, free, and close on every failure; successful SetClipboardData takes ownership.
    unsafe {
        let mut memory = GlobalMemory(GlobalAlloc(0x0002, wide.len() * 2));
        if memory.0.is_null() {
            return false;
        }
        let p = GlobalLock(memory.0) as *mut u16;
        if p.is_null() {
            return false;
        }
        let mapping = GlobalMapping(memory.0, p);
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        drop(mapping);
        if EmptyClipboard() == 0 {
            return false;
        }
        if !SetClipboardData(CF_UNICODETEXT, memory.0).is_null() {
            memory.0 = std::ptr::null_mut();
            return true;
        }
        false
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
    let mut leaves = Vec::new();
    fn walk(n: &Node, out: &mut Vec<(u16, theme::FontSpec)>) {
        for c in n.children() {
            if let Kind::Leaf(ctl) = &c.kind {
                out.push((c.id, ctl.font()));
            }
            walk(c, out);
        }
    }
    form.with_tree(|tree| walk(tree, &mut leaves)).unwrap();
    let dpi = form.dpi();
    let (mut wrong_handle, mut wrong_height) = (0, 0);
    let mut fonts = Vec::new();
    for &(id, spec) in &leaves {
        let h = form.control(id).unwrap();
        let got = send(h, WM_GETFONT, 0, 0);
        if got == 0 || fonts.iter().any(|&(s, handle)| s == spec && handle != got) {
            wrong_handle += 1;
        }
        fonts.push((spec, got));
        if lf_height(HFONT(got as *mut core::ffi::c_void)) != dpi::font_height(spec.points, dpi) {
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
    let spec = ButtonSpec::sidebar(TITLES[i]);
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
                    &format!("Section 1 of {}", crate::hw::PROVIDERS.len()),
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
                // The WP-10a replica keeps its 1 px divider row (the main window dropped it).
                Track::Absolute(1),
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
                    Ctl::Edit(EditSpec::new(
                        theme::CONTENT_FONT,
                        theme::TEXT_BOX_TEXT,
                        theme::TEXT_BOX_BACKGROUND,
                    )),
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
            let spec = if i == 0 {
                ButtonSpec::primary(t)
            } else {
                ButtonSpec::outline(t)
            };
            Node::leaf(FIRST_FOOTER_BUTTON + i as u16, Ctl::Button(spec))
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
        let (spec, min_w) = if primary {
            (ButtonSpec::primary(text), theme::CONFIRM_PRIMARY_MIN_WIDTH)
        } else {
            (
                ButtonSpec::outline(text),
                theme::CONFIRM_SECONDARY_MIN_WIDTH,
            )
        };
        Node::leaf(id, Ctl::Button(spec))
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
                            theme::CONFIRM_MESSAGE_TEXT,
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
                            theme::CONFIRM_WARNING_TEXT,
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
        let spec = if text == "Reclean" {
            ButtonSpec::primary(text)
        } else {
            ButtonSpec::outline(text)
        };
        Node::leaf(id, Ctl::Button(spec))
            .auto_size()
            .min(theme::ACTION_BUTTON_MIN)
            .padding(theme::SHARED_BUTTON_PADDING)
            .margin(theme::ACTION_BUTTON_MARGIN)
    };
    let list = ListSpec {
        font: theme::WHITELIST_LIST_FONT,
        fore: theme::TEXT_BOX_TEXT,
        back: theme::TEXT_BOX_BACKGROUND,
        selected_back: theme::LIST_SELECTED_BACKGROUND,
        selected_fore: theme::LIST_SELECTED_TEXT,
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
    form.with_tree(|tree| tree.find(id).unwrap().bounds)
        .unwrap()
}

#[test]
#[ignore = "opens real windows; run by hand for the WP-10a spike"]
fn spike() {
    std::fs::create_dir_all(GOLDEN).unwrap();
    assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
    let _activation = activate_comctl6().expect("comctl v6 activation context");
    review_lifetimes();
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
                        form.set_active(FIRST_SECTION + i, FIRST_SECTION + i == id);
                    }
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
    main.set_active(FIRST_SECTION, true);
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
        fn walk(n: &Node, ids: &mut Vec<u16>) {
            for c in n.children() {
                ids.push(c.id);
                walk(c, ids);
            }
        }
        let mut ids = Vec::new();
        main.with_tree(|tree| walk(tree, &mut ids)).unwrap();
        ids.into_iter()
            .filter_map(|id| main.control(id).map(|h| (h.0 as isize, id)))
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
    log.clicks.borrow_mut().clear();
    for (modifier, message) in [(VK_CONTROL, WM_KEYDOWN), (VK_MENU, WM_SYSKEYDOWN)] {
        set_keys(&[modifier.0], true);
        post(focus(), message, usize::from(VK_RETURN.0), 0x001C_0001);
        pump_for(50);
        set_keys(&[modifier.0], false);
        assert!(
            log.clicks.borrow().is_empty(),
            "Ctrl/Alt+Enter must not perform a dialog click"
        );
    }
    record(
        "Ctrl/Alt+Enter dialog clicks suppressed",
        format!("{:?}", log.clicks.borrow()),
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
        assert!(
            set_clipboard_text(mh, &t),
            "restore clipboard with a live owner"
        );
        assert_eq!(
            clipboard_text().as_deref(),
            Some(t.as_str()),
            "clipboard text restored"
        );
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
    // The ring is 1 px at 3 px outside the button, painted by the footer panel.
    let focus_px = pixel(mh, fb, -main.scale(theme::FOCUS_RING_OFFSET), half);
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
    let rgb = |c: theme::Color| (c.r, c.g, c.b);
    record(
        "focus / pressed colors",
        format!(
            "focus ring {focus_px:?} (SECONDARY {:?}), primary face {normal_px:?} (TEXT {:?}), \
             pressed {pressed_px:?} (PRIMARY_PRESSED {:?}), sidebar pressed {side_pressed_px:?} \
             (HOVER {:?})",
            rgb(theme::FOCUS_RING),
            rgb(theme::PRIMARY_BUTTON),
            rgb(theme::PRIMARY_BUTTON_PRESSED),
            rgb(theme::SIDEBAR_ITEM_HOVER),
        ),
    );
    assert_eq!(focus_px, rgb(theme::FOCUS_RING));
    assert_eq!(normal_px, rgb(theme::PRIMARY_BUTTON));
    assert_eq!(pressed_px, rgb(theme::PRIMARY_BUTTON_PRESSED));
    assert_eq!(side_pressed_px, rgb(theme::SIDEBAR_ITEM_HOVER));
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
        unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
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
                Event::Created => {
                    form.set_timer(1, 400);
                    form.set_timer(2, 5000);
                }
                Event::Timer(2) => {
                    o.borrow_mut()
                        .push("timeout: modal keyboard input not delivered".to_owned());
                    form.destroy();
                }
                Event::Timer(1) => {
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
                    // Other UI work can steal desktop focus. Address the intended native
                    // control explicitly and set plain modifiers, preserving the click assertions.
                    ensure_active(form.hwnd());
                    form.focus(311);
                    set_keys(&[VK_CONTROL.0, VK_MENU.0], false);
                    post(
                        form.control(311).unwrap(),
                        WM_KEYDOWN,
                        usize::from(key.0),
                        0x0001_0001,
                    );
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

// Real-window checks for cross-review regressions, kept in the existing ignored spike.
fn review_lifetimes() {
    let spec = FormSpec::new(
        "Lifecycle review",
        WindowSize::Client(Size { w: 400, h: 160 }),
    );
    let owner = Form::create(HWND::default(), spec.clone(), vec![], |_, _| true).unwrap();
    owner.show();
    let modal = Rc::new(Cell::new(None::<Form>));
    let modal_copy = Rc::clone(&modal);
    let lifetime = Rc::new(());
    let weak = Rc::downgrade(&lifetime);
    // A disabled owner must stay disabled when the nested loop exits on WM_QUIT.
    // SAFETY: All windows and the message queue belong to this test thread.
    unsafe {
        let _ = EnableWindow(owner.hwnd(), false);
    }
    run_modal(owner.hwnd(), spec.clone(), vec![], move |form, event| {
        let _ = &lifetime;
        match event {
            Event::Created => {
                modal_copy.set(Some(*form));
                form.set_timer(90, 20);
            }
            Event::Timer(90) => {
                form.kill_timer(90);
                // SAFETY: Stops only this test thread's message loop.
                unsafe {
                    PostQuitMessage(42);
                }
            }
            _ => {}
        }
        true
    })
    .unwrap();
    let mut msg = MSG::default();
    // WM_QUIT is synthesized when posted messages (including the destroy wake) are drained.
    // SAFETY: Drains this test thread's queue until the outer-loop WM_QUIT is retrieved.
    unsafe {
        loop {
            assert!(
                PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool(),
                "outer-loop WM_QUIT retained"
            );
            if msg.message == WM_QUIT {
                break;
            }
            DispatchMessageW(&msg);
        }
    }
    assert_eq!(msg.wParam.0, 42);
    assert!(
        !modal.get().unwrap().is_alive(),
        "WM_QUIT must destroy the modal window"
    );
    assert!(
        weak.upgrade().is_none(),
        "modal handler state must be released"
    );
    // SAFETY: Window state query and restoration on our own owner window.
    unsafe {
        assert!(
            !IsWindowEnabled(owner.hwnd()).as_bool(),
            "preserve disabled owner"
        );
        let _ = EnableWindow(owner.hwnd(), true);
    }
    // A queued close request from an older Form serial must not close this window.
    send(owner.hwnd(), WM_CLOSE, usize::MAX, 0);
    assert!(owner.is_alive(), "reject a stale queued close");
    owner.destroy();

    // A cross-thread SendMessage can destroy the last window while GetMessage is waiting.
    // The watchdog wakes the queue only if the loop fails to notice that destruction.
    let worker = Rc::new(RefCell::new(None));
    let worker_copy = Rc::clone(&worker);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let done_rx = RefCell::new(Some(done_rx));
    // SAFETY: Reads the current test UI thread id for a pointer-free watchdog wake.
    let ui_thread = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let started = Instant::now();
    run_main(spec.clone(), vec![], move |form, event| {
        if matches!(event, Event::Created) {
            let hwnd = form.hwnd().0 as isize;
            let done_rx = done_rx.borrow_mut().take().unwrap();
            *worker_copy.borrow_mut() = Some(std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                // SAFETY: Sends a pointer-free WM_CLOSE to the test window on its UI thread;
                // the bounded send cannot leave this worker hanging after a failed check.
                let sent = unsafe {
                    SendMessageTimeoutW(
                        HWND(hwnd as *mut core::ffi::c_void),
                        WM_CLOSE,
                        WPARAM(0),
                        LPARAM(0),
                        SMTO_ABORTIFHUNG,
                        1000,
                        None,
                    )
                };
                if done_rx.recv_timeout(Duration::from_millis(700)).is_err() {
                    // SAFETY: Wakes this known test UI thread without pointers.
                    unsafe {
                        PostThreadMessageW(ui_thread, WM_NULL, WPARAM(0), LPARAM(0)).unwrap();
                    }
                }
                assert_ne!(sent.0, 0, "synchronous close delivered");
            }));
        }
        true
    })
    .unwrap();
    done_tx.send(()).unwrap();
    worker.borrow_mut().take().unwrap().join().unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "GetMessage must wake after synchronous window destruction"
    );
    println!(
        "RESULT cross-review synchronous close: message loop returned in {:?}",
        started.elapsed()
    );

    let button = Node::leaf(1, Ctl::Button(ButtonSpec::outline("Short"))).auto_size();
    let form = Form::create(HWND::default(), spec, vec![button], |form, event| {
        if matches!(event, Event::Click(1)) {
            form.destroy();
        }
        true
    })
    .unwrap();
    let before = rect_of(&form, 1).w;
    let raw_control = form.control(1).unwrap().0 as isize;
    let rejected = std::thread::spawn(move || {
        super::controls::state_of(HWND(raw_control as *mut core::ffi::c_void)).is_none()
    })
    .join()
    .unwrap();
    assert!(rejected, "workers must not borrow the UI thread's Rc state");
    form.set_text(1, "A much longer button caption after an update");
    assert!(
        rect_of(&form, 1).w > before,
        "AutoSize must measure the current text"
    );
    form.show();
    ensure_active(form.hwnd());
    let h = form.control(1).unwrap();
    send(h, WM_LBUTTONDOWN, 1, (5 | (5 << 16)) as isize);
    send(h, WM_LBUTTONUP, 0, (5 | (5 << 16)) as isize);
    assert!(
        !form.is_alive(),
        "destroy during a re-entrant native button click"
    );
    let mut huge = FormSpec::new(
        "Work area review",
        WindowSize::Client(Size { w: 6000, h: 4000 }),
    );
    huge.maximize_if_too_big = true;
    let huge = Form::create(HWND::default(), huge, vec![], |_, _| true).unwrap();
    huge.show();
    // SAFETY: Queries our own window's maximized state.
    let zoomed = unsafe { IsZoomed(huge.hwnd()) }.as_bool();
    assert!(zoomed, "Q1: maximize when default outer size does not fit");
    huge.destroy();
    use windows::Win32::System::Threading::{GR_GDIOBJECTS, GetCurrentProcess, GetGuiResources};
    fn gdi_count() -> u32 {
        // SAFETY: Read-only resource count for this test process, using a non-owned pseudo handle.
        unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
    }
    let resource_spec = FormSpec::new(
        "GDI lifetime review",
        WindowSize::Client(Size { w: 400, h: 160 }),
    );
    // Warm up native, font, and buffered-paint caches (at every DPI the loop uses) before
    // counting retained GDI objects.
    let warm = Form::create(
        HWND::default(),
        resource_spec.clone(),
        extras_replica(),
        |_, _| true,
    )
    .unwrap();
    let warm_rect = window_rect(warm.hwnd());
    for new_dpi in [96u32, 144, 192, 96] {
        send(
            warm.hwnd(),
            WM_DPICHANGED,
            (new_dpi | new_dpi << 16) as usize,
            (&warm_rect as *const RECT) as isize,
        );
        let _ = capture(warm.hwnd());
    }
    warm.destroy();
    let before_gdi = gdi_count();
    for _ in 0..20 {
        let form = Form::create(
            HWND::default(),
            resource_spec.clone(),
            extras_replica(),
            |_, _| true,
        )
        .unwrap();
        let rect = window_rect(form.hwnd());
        for new_dpi in [144u32, 192, 96] {
            send(
                form.hwnd(),
                WM_DPICHANGED,
                (new_dpi | new_dpi << 16) as usize,
                (&rect as *const RECT) as isize,
            );
            let _ = capture(form.hwnd());
        }
        form.destroy();
    }
    let after_gdi = gdi_count();
    assert!(
        after_gdi <= before_gdi,
        "retained GDI objects after 20 create / DPI / paint / destroy cycles: {before_gdi} -> {after_gdi}"
    );
    println!(
        "RESULT cross-review GDI lifetime: {before_gdi} -> {after_gdi} objects after 20 cycles"
    );
    println!(
        "RESULT cross-review lifecycle checks: modal quit cleanup, disabled owner, stale close, AutoSize text, destroy inside native click, Q1 maximize passed"
    );
}
