//! Owned by WP-10a (themed by WP-19): `MessageBox.Show` with the C# button and icon sets.
//!
//! The box is a kit form (`DESIGN.md` section 15): the status glyph in its status color, the
//! wrapped text, and the buttons of the C# set. It keeps the native rules: Enter presses the
//! default button, Esc presses OK (a Yes/No box has no Esc and a grayed X), Ctrl+C copies the
//! text, the box is modal to its owner. Tests find it by `window::MESSAGE_BOX_CLASS` and its
//! title, read the text from the control `TEXT_ID`, and press a button by posting
//! `WM_COMMAND(IDOK | IDYES | IDNO, 0)`. If the kit form cannot be created, the native
//! `MessageBoxW` shows instead, so a message is never lost.

use super::controls::{Align, ButtonSpec, Ctl, EditSpec, LabelSpec};
use super::layout::{Anchor, FlowDir, Node, Size, Track};
use super::theme::{self, glyph};
use super::window::{self, Event, FormSpec, FormStyle, StartPosition, WindowSize};
use super::{controls, dpi};
use crate::win::{self, wide::to_wide};
use std::cell::Cell;
use std::rc::Rc;
use windows::{
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            WindowsAndMessaging::{
                EnableMenuItem, GetSystemMenu, IDNO, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
                MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_YESNO, MESSAGEBOX_STYLE, MF_BYCOMMAND,
                MF_GRAYED, MessageBoxW, SC_CLOSE,
            },
        },
    },
    core::PCWSTR,
};

/// Control id of the message text (a label or scrolling well; tests read its window text).
pub const TEXT_ID: u16 = 0x0100;
/// Control id of the status glyph.
const ICON_ID: u16 = 0x0101;
/// Control id of the hidden edit that serves Ctrl+C.
const COPY_ID: u16 = 0x0102;

/// The owner for a box raised after async work (DESIGN.md 8.5): C# `MessageBox.Show` without an
/// owner uses the UI thread's active window, including a modal child. Never pass a form that a
/// modal child disabled: the box would enable it again when it closes.
pub fn active_window() -> HWND {
    // SAFETY: Read-only query of this UI thread's active window; a null result is valid.
    let active = unsafe { GetActiveWindow() };
    if active.is_invalid() {
        // Losing activation to another application must not detach an async result's box
        // from an existing modal dialog and leave both independently interactive.
        window::modal_window().unwrap_or_default()
    } else {
        active
    }
}

/// `MessageBoxButtons` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Buttons {
    /// `MessageBoxButtons.OK`.
    Ok,
    /// `MessageBoxButtons.YesNo`.
    YesNo,
}

/// `MessageBoxIcon` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Icon {
    /// `MessageBoxIcon.None`.
    None,
    /// `MessageBoxIcon.Information`.
    Information,
    /// `MessageBoxIcon.Warning`.
    Warning,
    /// `MessageBoxIcon.Error`.
    Error,
    /// `MessageBoxIcon.Question`.
    Question,
}

/// `DialogResult` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Answer {
    /// OK.
    Ok,
    /// Yes.
    Yes,
    /// No (also returned when the box could not be shown).
    No,
}

/// Shows a modal message box owned by `owner` (pass the form window, like C# does implicitly).
pub fn show(owner: HWND, text: &str, title: &str, buttons: Buttons, icon: Icon) -> Answer {
    match themed(owner, text, title, buttons, icon) {
        Ok(answer) => answer,
        Err(error) => {
            win::record(error);
            native(owner, text, title, buttons, icon)
        }
    }
}

/// The glyph and status color of an icon (DESIGN.md 3: color by meaning).
fn icon_of(icon: Icon) -> Option<(char, theme::Color)> {
    match icon {
        Icon::None => None,
        Icon::Information => Some((glyph::STATUS_INFO, theme::INFO)),
        Icon::Warning => Some((glyph::STATUS_WARNING, theme::WARNING)),
        Icon::Error => Some((glyph::STATUS_ERROR, theme::DANGER)),
        Icon::Question => Some((glyph::STATUS_QUESTION, theme::SECONDARY)),
    }
}

/// `(id, text, primary)` of the buttons, left to right.
fn buttons_of(buttons: Buttons) -> Vec<(u16, &'static str, bool)> {
    match buttons {
        Buttons::Ok => vec![(IDOK.0 as u16, "OK", true)],
        Buttons::YesNo => vec![(IDYES.0 as u16, "Yes", true), (IDNO.0 as u16, "No", false)],
    }
}

/// The logical client size: the wrapped text (at most `MSGBOX_TEXT_MAX_WIDTH` wide) next to
/// the icon, the button row below, the padding around. Measured at `dpi` (the monitor the box
/// opens on) so the wrap the layout produces there is the one that was measured. The flag is
/// true when the text is taller than `MSGBOX_BODY_MAX_HEIGHT`: the body is then a scrolling
/// well of that height at the full text width (DESIGN.md 15).
fn client_size(
    text: &str,
    has_icon: bool,
    button_count: i32,
    dpi: u32,
) -> win::Result<(Size, bool)> {
    let font = dpi::Font::new(theme::MSGBOX_FONT, dpi)?;
    let label = Ctl::Label(LabelSpec::new(text, theme::MSGBOX_FONT, theme::TEXT));
    let measured = controls::measure(
        &label,
        font.handle(),
        theme::NO_PAD,
        Size::default(),
        Size {
            w: dpi::scale(theme::MSGBOX_TEXT_MAX_WIDTH, dpi),
            h: 0,
        },
        dpi,
    );
    let mut text_w = dpi::unscale(measured.w, dpi).min(theme::MSGBOX_TEXT_MAX_WIDTH);
    let mut text_h = dpi::unscale(measured.h, dpi);
    let well = text_h > theme::MSGBOX_BODY_MAX_HEIGHT;
    if well {
        text_w = theme::MSGBOX_TEXT_MAX_WIDTH;
        text_h = theme::MSGBOX_BODY_MAX_HEIGHT;
    }
    let icon_w = if has_icon {
        theme::MSGBOX_ICON_PX + theme::MSGBOX_ICON_MARGIN.horizontal()
    } else {
        0
    };
    let pad = theme::MSGBOX_PADDING;
    let buttons_w =
        button_count * (theme::MSGBOX_BUTTON_MIN_WIDTH + theme::MSGBOX_BUTTON_MARGIN.horizontal());
    // Slack of a few pixels: the unscaled size must not round below the measured wrap.
    let w = (pad.horizontal() + icon_w + text_w + theme::MSGBOX_WIDTH_SLACK)
        .max(theme::MSGBOX_MIN_WIDTH)
        .max(pad.horizontal() + buttons_w);
    let body_h =
        text_h.max(if has_icon { theme::MSGBOX_ICON_PX } else { 0 }) + theme::MSGBOX_HEIGHT_SLACK;
    let h = pad.vertical()
        + body_h
        + theme::MSGBOX_BUTTON_ROW_MARGIN.t
        + theme::MSGBOX_BUTTON_HEIGHT
        + theme::MSGBOX_BUTTON_MARGIN.vertical();
    Ok((Size { w, h }, well))
}

/// The DPI the box will open at: the owner's, or the cursor monitor's without an owner
/// (`Form::create` centers an ownerless box on the mouse's monitor).
fn target_dpi(owner: HWND) -> u32 {
    if owner.is_invalid() {
        dpi::cursor_dpi()
    } else {
        dpi::window_dpi(owner)
    }
}

/// The edit control breaks lines at CR LF only.
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn tree(text: &str, icon: Icon, buttons: Buttons, well: bool) -> Vec<Node> {
    let mut body = Vec::new();
    if let Some((g, color)) = icon_of(icon) {
        // The symbol glyph sits on the ring glyph (the Fluent status set is layered).
        let layers: String = [glyph::STATUS_RING, g].iter().collect();
        body.push(
            Node::leaf(
                ICON_ID,
                Ctl::Label(
                    LabelSpec::new(&layers, theme::icon_font(theme::MSGBOX_ICON_PX), color)
                        .align(Align::MiddleCenter)
                        .stacked(),
                ),
            )
            .size(Size {
                w: theme::MSGBOX_ICON_PX,
                h: theme::MSGBOX_ICON_PX,
            })
            .anchor(Anchor::TOP)
            .margin(theme::MSGBOX_ICON_MARGIN)
            .cell(0, 0),
        );
    }
    let body_node = if well {
        // A read-only text well (DESIGN.md 15): `CARD` fill, `BORDER` outline, the 10 px
        // inner margins of every well, word-wrapped, so a vertical bar is the only one. The
        // text is set at `Created` (CR LF) and the well starts at the top. The default
        // `Fixed3D` edge is the Log Cleaning path: the kit paints the token frame over the
        // client edge, with the scroll bar inside it. A `WS_BORDER` multiline edit draws its
        // own frame inside the client area and leaves the scroll bar outside it.
        Node::leaf(
            TEXT_ID,
            Ctl::Edit(EditSpec::new(theme::MSGBOX_FONT, theme::TEXT, theme::CARD).word_wrap()),
        )
        .size(Size {
            w: theme::MSGBOX_TEXT_MAX_WIDTH,
            h: theme::MSGBOX_BODY_MAX_HEIGHT,
        })
    } else {
        Node::leaf(
            TEXT_ID,
            Ctl::Label(LabelSpec::new(text, theme::MSGBOX_FONT, theme::TEXT)),
        )
        .auto_size()
    };
    body.push(
        body_node
            .anchor(Anchor(Anchor::TOP.0 | Anchor::LEFT.0 | Anchor::RIGHT.0))
            .margin(theme::NO_PAD)
            .cell(1, 0),
    );
    let button_nodes = buttons_of(buttons)
        .into_iter()
        .rev()
        .map(|(id, caption, primary)| {
            let spec = if primary {
                ButtonSpec::primary(caption)
            } else {
                ButtonSpec::outline(caption)
            };
            Node::leaf(id, Ctl::Button(spec))
                .auto_size()
                .min(Size {
                    w: theme::MSGBOX_BUTTON_MIN_WIDTH,
                    h: theme::MSGBOX_BUTTON_HEIGHT,
                })
                .padding(theme::SHARED_BUTTON_PADDING)
                .margin(theme::MSGBOX_BUTTON_MARGIN)
        })
        .collect();
    // Hidden, read-only copy of the text: Ctrl+C copies it (`FormSpec::copy_on_ctrl_c`).
    let copy = Node::leaf(
        COPY_ID,
        Ctl::Edit(EditSpec::new(theme::MSGBOX_FONT, theme::TEXT, theme::BG)),
    )
    .visible(false);
    vec![
        Node::table(
            vec![Track::Percent(100.0)],
            vec![Track::Percent(100.0), Track::AutoSize],
            vec![
                Node::table(
                    vec![Track::AutoSize, Track::Percent(100.0)],
                    vec![Track::AutoSize],
                    body,
                )
                .fill()
                .margin(theme::NO_PAD)
                .cell(0, 0),
                // Right to left: the default button is the rightmost one, like the native box.
                Node::flow(FlowDir::RightToLeft, false, button_nodes)
                    .auto_size()
                    .anchor(Anchor::RIGHT)
                    .margin(theme::MSGBOX_BUTTON_ROW_MARGIN)
                    .cell(0, 1),
            ],
        )
        .fill()
        .padding(theme::MSGBOX_PADDING),
        copy,
    ]
}

fn themed(
    owner: HWND,
    text: &str,
    title: &str,
    buttons: Buttons,
    icon: Icon,
) -> win::Result<Answer> {
    let has_icon = icon_of(icon).is_some();
    let count = buttons_of(buttons).len() as i32;
    let (size, well) = client_size(text, has_icon, count, target_dpi(owner))?;
    let mut spec = FormSpec::new(title, WindowSize::Client(size));
    spec.style = FormStyle::FixedDialog;
    spec.start = if owner.is_invalid() {
        StartPosition::CenterScreen
    } else {
        StartPosition::CenterParent
    };
    spec.taskbar = owner.is_invalid();
    spec.message_box = true;
    spec.copy_on_ctrl_c = Some(COPY_ID);
    let (default, cancel) = match buttons {
        Buttons::Ok => (IDOK.0 as u16, Some(IDOK.0 as u16)),
        Buttons::YesNo => (IDYES.0 as u16, None),
    };
    spec.accept = Some(default);
    spec.cancel = cancel;
    let answer = Rc::new(Cell::new(match buttons {
        Buttons::Ok => Answer::Ok,
        Buttons::YesNo => Answer::No,
    }));
    let result = Rc::clone(&answer);
    let text = text.to_owned();
    window::run_modal(
        owner,
        spec,
        tree(&text, icon, buttons, well),
        move |form, event| {
            match event {
                Event::Created => {
                    form.edit_set_text(COPY_ID, &text);
                    if well {
                        form.edit_set_text(TEXT_ID, &crlf(&text));
                        form.edit_scroll_to_top(TEXT_ID);
                    }
                    if buttons == Buttons::YesNo {
                        // Like the native Yes/No box: no X, no Esc.
                        // SAFETY: Menu handle of our own window; the item id is a system command.
                        unsafe {
                            let menu = GetSystemMenu(form.hwnd(), false);
                            let _ = EnableMenuItem(menu, SC_CLOSE, MF_BYCOMMAND | MF_GRAYED);
                        }
                    }
                }
                Event::Click(id) => {
                    result.set(match id {
                        i if i == IDYES.0 as u16 => Answer::Yes,
                        i if i == IDNO.0 as u16 => Answer::No,
                        _ => Answer::Ok,
                    });
                    form.destroy();
                }
                // An OK box closes as OK; a Yes/No box cannot be closed without an answer.
                Event::CloseRequest => return buttons == Buttons::Ok,
                _ => {}
            }
            true
        },
    )?;
    Ok(answer.get())
}

/// The native box, used only when the themed one cannot be created.
fn native(owner: HWND, text: &str, title: &str, buttons: Buttons, icon: Icon) -> Answer {
    let mut style = match buttons {
        Buttons::Ok => MB_OK,
        Buttons::YesNo => MB_YESNO,
    };
    style |= match icon {
        Icon::None => MESSAGEBOX_STYLE(0),
        Icon::Information => MB_ICONINFORMATION,
        Icon::Warning => MB_ICONWARNING,
        Icon::Error => MB_ICONERROR,
        Icon::Question => MB_ICONQUESTION,
    };
    let text = to_wide(text);
    let title = to_wide(title);
    let owner = if owner.is_invalid() {
        None
    } else {
        Some(owner)
    };
    // SAFETY: The terminated buffers live through the modal call.
    let r = unsafe { MessageBoxW(owner, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), style) };
    match r {
        IDYES => Answer::Yes,
        IDOK => Answer::Ok,
        IDNO => Answer::No,
        _ => Answer::No,
    }
}

/// Helpers for the UI tests: find a themed box, read its text, press a button by HWND.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, FindWindowW, GetDlgCtrlID, GetWindowTextLengthW, GetWindowTextW,
        IsWindow, MESSAGEBOX_RESULT, PostMessageW, WM_COMMAND,
    };
    use windows::core::BOOL;

    /// The open themed box with `title` (any thread of this process).
    pub fn find(title: &str) -> Option<HWND> {
        let class = to_wide(window::MESSAGE_BOX_CLASS);
        let title = to_wide(title);
        // SAFETY: NUL-terminated buffers valid for the call.
        unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr())) }
            .ok()
            .filter(|h| !h.is_invalid())
    }

    /// Whether `h` is a themed message box.
    pub fn is_box(h: HWND) -> bool {
        let mut buf = [0u16; 64];
        // SAFETY: Writable buffer of the given length.
        let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize]) == window::MESSAGE_BOX_CLASS
    }

    fn window_text(h: HWND) -> String {
        // SAFETY: Length query and a read into a buffer sized for the text plus NUL.
        unsafe {
            let len = GetWindowTextLengthW(h).max(0) as usize;
            let mut buf = vec![0u16; len + 1];
            let n = GetWindowTextW(h, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..n])
        }
    }

    /// The child control `id` of a themed box.
    pub fn control(dialog: HWND, id: u16) -> Option<HWND> {
        unsafe extern "system" fn visit(h: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the pair passed below, alive for the enumeration.
            let (id, found) = unsafe { &mut *(data.0 as *mut (u16, Option<HWND>)) };
            // SAFETY: Read-only id query of a child window.
            if unsafe { GetDlgCtrlID(h) } == i32::from(*id) {
                *found = Some(h);
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut search: (u16, Option<HWND>) = (id, None);
        // SAFETY: Synchronous enumeration whose callback only writes `search.1`.
        unsafe {
            let _ = EnumChildWindows(
                Some(dialog),
                Some(visit),
                LPARAM(&mut search as *mut (u16, Option<HWND>) as isize),
            );
        }
        search.1
    }

    /// The message text of a themed box (empty while it is still being built).
    pub fn text(dialog: HWND) -> String {
        control(dialog, TEXT_ID)
            .map(window_text)
            .unwrap_or_default()
    }

    /// `WxH` of the box's client and of the text control, for the notes.
    pub fn body_note(dialog: HWND) -> String {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};
        let mut c = RECT::default();
        let mut t = RECT::default();
        // SAFETY: Writable RECTs of live windows.
        unsafe {
            let _ = GetClientRect(dialog, &mut c);
            if let Some(h) = control(dialog, TEXT_ID) {
                let _ = GetWindowRect(h, &mut t);
            }
        }
        format!(
            "client {}x{} text {}x{}",
            c.right - c.left,
            c.bottom - c.top,
            t.right - t.left,
            t.bottom - t.top
        )
    }

    /// Presses the button `id` (`IDOK`, `IDYES`, `IDNO`) by posting its command to the box.
    pub fn press(dialog: HWND, id: MESSAGEBOX_RESULT) {
        // SAFETY: Read-only validity check; posting a value-only message to a live window.
        unsafe {
            if IsWindow(Some(dialog)).as_bool() {
                let _ = PostMessageW(Some(dialog), WM_COMMAND, WPARAM(id.0 as usize), LPARAM(0));
            }
        }
    }

    /// Exercises dialog keys on real HWNDs with thread-local keyboard state, without sending
    /// input to the owner's desktop. Called by the existing update-window UI run.
    pub fn review_keyboard() {
        use super::super::window::Form;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetFocus, SetKeyboardState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SPACE, VK_TAB,
        };
        use windows::Win32::UI::WindowsAndMessaging::{MSG, SendMessageW, WM_KEYDOWN, WM_KEYUP};
        for (buttons, tab, key, expected) in [
            (Buttons::Ok, false, VK_ESCAPE, Answer::Ok),
            (Buttons::YesNo, false, VK_RETURN, Answer::Yes),
            (Buttons::YesNo, true, VK_RETURN, Answer::No),
            (Buttons::YesNo, true, VK_SPACE, Answer::No),
        ] {
            let answer = Rc::new(Cell::new(None));
            let result = Rc::clone(&answer);
            let default = if buttons == Buttons::Ok { IDOK } else { IDYES };
            let mut spec = FormSpec::new(
                "UI review keys",
                WindowSize::Client(theme::CONFIRM_CLIENT_SIZE),
            );
            spec.style = FormStyle::FixedDialog;
            spec.message_box = true;
            spec.accept = Some(default.0 as u16);
            spec.cancel = (buttons == Buttons::Ok).then_some(IDOK.0 as u16);
            let form = Form::create(
                HWND::default(),
                spec,
                tree(
                    "Synthetic keyboard check",
                    Icon::Information,
                    buttons,
                    false,
                ),
                move |form, event| {
                    if let Event::Click(id) = event {
                        result.set(Some(match i32::from(id) {
                            i if i == IDYES.0 => Answer::Yes,
                            i if i == IDNO.0 => Answer::No,
                            _ => Answer::Ok,
                        }));
                        form.destroy();
                    }
                    true
                },
            )
            .unwrap();
            form.show();
            // SAFETY: This only sets the calling test thread's keyboard state. It does not
            // inject desktop input; HWND queries and messages stay within this test's form.
            unsafe {
                SetKeyboardState(&[0; 256]).unwrap();
                assert_eq!(
                    GetFocus(),
                    form.control(default.0 as u16).unwrap(),
                    "initial default focus"
                );
                let key_msg = |vk: u16| MSG {
                    hwnd: form.hwnd(),
                    message: WM_KEYDOWN,
                    wParam: WPARAM(vk as usize),
                    ..Default::default()
                };
                for modifier in [VK_CONTROL, VK_MENU] {
                    let mut keys = [0; 256];
                    keys[modifier.0 as usize] = 0x80;
                    SetKeyboardState(&keys).unwrap();
                    assert!(
                        !window::pre_translate(&key_msg(VK_RETURN.0)),
                        "modified Enter does not confirm"
                    );
                    assert!(form.is_alive());
                }
                SetKeyboardState(&[0; 256]).unwrap();
                if buttons == Buttons::YesNo {
                    assert!(window::pre_translate(&key_msg(VK_ESCAPE.0)));
                    assert!(form.is_alive(), "Yes/No ignores Esc");
                }
                if tab {
                    assert!(window::pre_translate(&key_msg(VK_TAB.0)));
                    assert_eq!(
                        GetFocus(),
                        form.control(IDNO.0 as u16).unwrap(),
                        "Tab reaches No"
                    );
                }
                if key == VK_SPACE {
                    let focus = GetFocus();
                    SendMessageW(
                        focus,
                        WM_KEYDOWN,
                        Some(WPARAM(key.0 as usize)),
                        Some(LPARAM(0)),
                    );
                    SendMessageW(
                        focus,
                        WM_KEYUP,
                        Some(WPARAM(key.0 as usize)),
                        Some(LPARAM(0)),
                    );
                } else {
                    assert!(window::pre_translate(&key_msg(key.0)));
                }
            }
            assert_eq!(
                answer.get(),
                Some(expected),
                "{buttons:?}, Tab={tab}, key={key:?}"
            );
            assert!(!form.is_alive());
        }
        println!(
            "RESULT review: OK Esc; Yes/No default Enter, Tab/Enter, Tab/Space, Esc ignored and modified Enter passed"
        );
    }

    /// Presses `id` on `dialog` when dropped, so a test helper can never skip the click.
    pub struct PressOnDrop {
        /// The box.
        pub dialog: HWND,
        /// The button command.
        pub id: MESSAGEBOX_RESULT,
    }

    impl Drop for PressOnDrop {
        fn drop(&mut self) {
            press(self.dialog, self.id);
        }
    }

    /// Captures a window with `PrintWindow` (works while other apps cover it, so the owner
    /// can keep using the PC) and saves it as a PNG at `path` (best effort, 15 s budget).
    pub fn capture(h: HWND, path: &std::path::Path) -> Result<String, String> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
        };
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PrintWindow(
                hwnd: *mut core::ffi::c_void,
                hdc: *mut core::ffi::c_void,
                flags: u32,
            ) -> i32;
        }
        let mut r = RECT::default();
        // SAFETY: Writable RECT of a live window.
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut r) }
            .map_err(|e| e.to_string())?;
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || hgt <= 0 {
            return Err("empty window".to_owned());
        }
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
        let bmp_path = path.with_extension("bmp");
        std::fs::write(&bmp_path, out).map_err(|e| e.to_string())?;
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; \
             $i = [System.Drawing.Image]::FromFile('{b}'); \
             $i.Save('{p}', [System.Drawing.Imaging.ImageFormat]::Png); $i.Dispose(); \
             Remove-Item '{b}'",
            b = bmp_path.display(),
            p = path.display()
        );
        let out = crate::win::process::run(
            &crate::win::process::powershell(),
            &["-NoProfile", "-NonInteractive", "-Command", &script],
            std::time::Duration::from_secs(15),
            &crate::win::process::Cancel::new(),
        )
        .map_err(|e| e.to_string())?;
        if out.code == 0 {
            Ok(format!("{} outer {w}x{hgt}", path.display()))
        } else {
            Err(out.stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    //! `cargo test --locked --lib -- --ignored ui::msgbox::tests::wp19_boxes --nocapture`
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        IDNO, IDOK, IDYES, MESSAGEBOX_RESULT, SendMessageW, WM_DPICHANGED,
    };

    const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\wp-19";
    const DESIGN_GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\msgbox-design";

    // Real HWND checks inside the existing ignored UI run: no hardware queries or cleaning.
    fn review_regressions() {
        use super::super::window::Form;
        use windows::Win32::UI::Input::KeyboardAndMouse::{IsWindowEnabled, SetActiveWindow};
        use windows::Win32::UI::WindowsAndMessaging::{
            GW_OWNER, GWL_STYLE, GetWindow, GetWindowLongPtrW, SIZE_MINIMIZED, SIZE_RESTORED,
            WM_SIZE, WM_TIMER, WS_HSCROLL,
        };

        let ticks = Rc::new(Cell::new(0));
        let seen = Rc::clone(&ticks);
        let form = Form::create(
            HWND::default(),
            FormSpec::new("UI review", WindowSize::Client(Size { w: 600, h: 300 })),
            vec![
                Node::leaf(
                    1,
                    Ctl::Edit(EditSpec::new(theme::CONTENT_FONT, theme::TEXT, theme::CARD)),
                )
                .fill(),
            ],
            move |form, event| {
                if let Event::Timer(1) = event {
                    seen.set(seen.get() + 1);
                    form.kill_timer(1);
                }
                true
            },
        )
        .unwrap();
        let edit = form.control(1).unwrap();
        let horizontal = || {
            // SAFETY: Read-only style query of this test's edit window.
            unsafe { GetWindowLongPtrW(edit, GWL_STYLE) as u32 & WS_HSCROLL.0 != 0 }
        };
        form.edit_set_text(1, &"0123456789".repeat(5));
        assert!(!horizontal(), "short text has no horizontal bar");
        let r = form.window_rect();
        // Keep the physical viewport fixed while its font doubles, then restore the font.
        let original_dpi = form.dpi();
        for (dpi, bar) in [(original_dpi * 2, true), (original_dpi, false)] {
            // SAFETY: Synchronous DPI message with a live RECT to this test's form.
            unsafe {
                SendMessageW(
                    form.hwnd(),
                    WM_DPICHANGED,
                    Some(WPARAM((dpi | (dpi << 16)) as usize)),
                    Some(LPARAM(&r as *const RECT as isize)),
                );
            }
            assert_eq!(horizontal(), bar, "horizontal bar at {dpi} DPI");
        }
        form.set_timer(1, 60_000);
        // SAFETY: Value-only timer/size messages to this test's own form.
        unsafe {
            SendMessageW(form.hwnd(), WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
            SendMessageW(form.hwnd(), WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        assert_eq!(ticks.get(), 1, "a killed timer's queued message is ignored");
        form.set_timer(1, 60_000);
        // SAFETY: Value-only messages; the form's actual viewport is unchanged.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_SIZE,
                Some(WPARAM(SIZE_MINIMIZED as usize)),
                Some(LPARAM(0)),
            );
            SendMessageW(form.hwnd(), WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        assert_eq!(ticks.get(), 1, "no timer dispatch while minimized");
        // SAFETY: Restores the test state and delivers a value-only timer message.
        unsafe {
            SendMessageW(
                form.hwnd(),
                WM_SIZE,
                Some(WPARAM(SIZE_RESTORED as usize)),
                Some(LPARAM(0)),
            );
            SendMessageW(form.hwnd(), WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        assert_eq!(ticks.get(), 2, "recorded timers resume after restore");
        println!(
            "RESULT review: DPI scroll-bar refresh and queued timer stop/minimize/restore passed"
        );

        let owner = form.hwnd();
        let verified = Rc::new(Cell::new(false));
        let done = Rc::clone(&verified);
        let mut spec = FormSpec::new(
            "UI review modal",
            WindowSize::Client(Size { w: 320, h: 160 }),
        );
        spec.style = FormStyle::FixedDialog;
        window::run_modal(owner, spec, vec![], move |outer, event| {
            match event {
                Event::Created => outer.set_timer(1, 10),
                Event::Timer(1) => {
                    outer.kill_timer(1);
                    // SAFETY: Clears activation only for the test's own UI thread, without
                    // foregrounding another app; reads enabled state of its own owner.
                    unsafe {
                        assert!(!IsWindowEnabled(owner).as_bool());
                        let _ = SetActiveWindow(HWND::default());
                    }
                    assert_eq!(
                        active_window(),
                        outer.hwnd(),
                        "inactive modal remains the async box owner"
                    );
                    let expected_owner = outer.hwnd().0 as isize;
                    let closer = std::thread::spawn(move || {
                        let end = Instant::now() + Duration::from_secs(5);
                        let inner = loop {
                            if let Some(h) = testing::find("UI review nested") {
                                break h;
                            }
                            assert!(Instant::now() < end, "nested box did not open");
                            std::thread::sleep(Duration::from_millis(10));
                        };
                        let _press = testing::PressOnDrop {
                            dialog: inner,
                            id: IDOK,
                        };
                        std::thread::sleep(Duration::from_millis(50));
                        // SAFETY: Relationship and enabled-state queries of test windows.
                        unsafe {
                            assert_eq!(
                                GetWindow(inner, GW_OWNER).unwrap().0 as isize,
                                expected_owner
                            );
                            assert!(
                                !IsWindowEnabled(HWND(expected_owner as *mut core::ffi::c_void))
                                    .as_bool()
                            );
                        }
                    });
                    assert_eq!(
                        show(
                            active_window(),
                            "Synthetic worker result",
                            "UI review nested",
                            Buttons::Ok,
                            Icon::Information
                        ),
                        Answer::Ok
                    );
                    closer.join().unwrap();
                    // SAFETY: Read-only enabled-state queries of test windows.
                    unsafe {
                        assert!(
                            !IsWindowEnabled(owner).as_bool(),
                            "base owner stays disabled"
                        );
                        assert!(
                            IsWindowEnabled(outer.hwnd()).as_bool(),
                            "inner restores only outer"
                        );
                    }
                    done.set(true);
                    outer.destroy();
                }
                _ => {}
            }
            true
        })
        .unwrap();
        assert!(verified.get());
        // SAFETY: Read-only query of the surviving test owner.
        let owner_enabled = unsafe { IsWindowEnabled(owner) }.as_bool();
        assert!(owner_enabled, "outer restores its owner");
        assert!(
            window::modal_window().is_none(),
            "modal state removed after exit"
        );
        form.destroy();
        println!("RESULT review: nested modal ownership and inactive async owner passed");
    }

    /// Opens every icon kind, captures each at the real DPI and a synthetic 144 DPI, presses
    /// a button by HWND, and checks the answers (Enter, Esc and X rules included).
    #[test]
    #[ignore = "opens real windows"]
    fn wp19_boxes() {
        std::fs::create_dir_all(GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        review_regressions();
        let cases: Vec<Case> = vec![
            (
                "Update Check Failed",
                "Error checking for updates: Failed to get GitHub file SHA256 for HWIDChecker.exe: HTTP GET failed: 0x00000000 HTTP status 404".to_owned(),
                Buttons::Ok,
                Icon::Warning,
                IDOK,
                Answer::Ok,
            ),
            (
                "Update Available",
                "A new version is available. Do you want to update now?\n\nThe application will restart after the update.".to_owned(),
                Buttons::YesNo,
                Icon::Question,
                IDYES,
                Answer::Yes,
            ),
            (
                "Confirm Exit",
                "Operation in progress. Are you sure you want to close?".to_owned(),
                Buttons::YesNo,
                Icon::Warning,
                IDNO,
                Answer::No,
            ),
            (
                "Error",
                "Error during cleaning process: SetupDiGetClassDevsW failed: 0x00000005 Access is denied.".to_owned(),
                Buttons::Ok,
                Icon::Error,
                IDOK,
                Answer::Ok,
            ),
            (
                "Refresh",
                "Hardware data refreshed successfully!".to_owned(),
                Buttons::Ok,
                Icon::Information,
                IDOK,
                Answer::Ok,
            ),
        ];
        run_boxes(GOLDEN, cases, true);
    }

    /// A short box and a 200-line box (the scrolling well), captured at the real DPI and a
    /// synthetic 144 DPI into `golden/msgbox-design`; no keyboard checks.
    #[test]
    #[ignore = "opens real windows"]
    fn msgbox_design_boxes() {
        std::fs::create_dir_all(DESIGN_GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        let long = (1..=200)
            .map(|i| format!("Line {i}: SetupDiGetClassDevsW failed for a device instance path."))
            .collect::<Vec<_>>()
            .join("\n");
        review_message_body(&long);
        run_boxes(
            DESIGN_GOLDEN,
            vec![
                (
                    "Refresh",
                    "Hardware data refreshed successfully!".to_owned(),
                    Buttons::Ok,
                    Icon::Information,
                    IDOK,
                    Answer::Ok,
                ),
                ("Error", long, Buttons::Ok, Icon::Error, IDOK, Answer::Ok),
            ],
            false,
        );
    }

    // Real controls and the real dialog translator, with WM_COPY intercepted so the owner's
    // clipboard is untouched. Checks the full selection sent to native copy, not just text.
    fn review_message_body(long: &str) {
        use super::super::window::Form;
        use windows::Win32::Foundation::{LRESULT, SIZE};
        use windows::Win32::UI::Controls::{EM_GETFIRSTVISIBLELINE, EM_GETSEL};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetFocus, SetKeyboardState, VK_CONTROL, VK_END, VK_HOME, VK_NEXT, VK_TAB,
        };
        use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_STYLE, GetWindowLongPtrW, MSG, WM_COPY, WM_GETDPISCALEDSIZE, WM_KEYDOWN,
            WS_HSCROLL, WS_VSCROLL,
        };
        unsafe extern "system" fn copy_probe(
            h: HWND,
            msg: u32,
            w: WPARAM,
            l: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            if msg == WM_COPY {
                let (mut start, mut end) = (0u32, 0u32);
                // SAFETY: Synchronous selection query into live, writable u32 values. The
                // Cell passed as subclass data stays alive until the subclass is removed.
                unsafe {
                    SendMessageW(
                        h,
                        EM_GETSEL,
                        Some(WPARAM(&mut start as *mut u32 as usize)),
                        Some(LPARAM(&mut end as *mut u32 as isize)),
                    );
                    (*(data as *const Cell<(u32, u32)>)).set((start, end));
                }
                return LRESULT(0);
            }
            // SAFETY: Forwards unmodified parameters along the native subclass chain.
            unsafe { DefSubclassProc(h, msg, w, l) }
        }
        struct CopyProbe(HWND);
        impl Drop for CopyProbe {
            fn drop(&mut self) {
                // SAFETY: Removes only our subclass before its borrowed Cell goes out of scope.
                unsafe {
                    let _ = RemoveWindowSubclass(self.0, Some(copy_probe), 99);
                }
            }
        }
        for text in ["Hardware data refreshed successfully!", long] {
            for buttons in [Buttons::Ok, Buttons::YesNo] {
                let real = target_dpi(HWND::default());
                let (size, well) =
                    client_size(text, true, buttons_of(buttons).len() as i32, real).unwrap();
                let default = if buttons == Buttons::Ok { IDOK } else { IDYES };
                let mut spec = FormSpec::new("Codex message-body review", WindowSize::Client(size));
                spec.style = FormStyle::FixedDialog;
                spec.message_box = true;
                spec.accept = Some(default.0 as u16);
                spec.cancel = (buttons == Buttons::Ok).then_some(IDOK.0 as u16);
                spec.copy_on_ctrl_c = Some(COPY_ID);
                let form = Form::create(
                    HWND::default(),
                    spec.clone(),
                    tree(text, Icon::Error, buttons, well),
                    |_, _| true,
                )
                .unwrap();
                form.edit_set_text(COPY_ID, text);
                if well {
                    form.edit_set_text(TEXT_ID, &crlf(text));
                    form.edit_scroll_to_top(TEXT_ID);
                }
                form.show();
                let copy = form.control(COPY_ID).unwrap();
                let selection = Cell::new((u32::MAX, u32::MAX));
                // SAFETY: The subclass borrows selection only during synchronous messages;
                // its guard removes it before selection or the form is dropped.
                let installed = unsafe {
                    SetWindowSubclass(copy, Some(copy_probe), 99, &selection as *const _ as usize)
                };
                assert!(installed.as_bool());
                let probe = CopyProbe(copy);
                // SAFETY: Keyboard state belongs to this test thread; no desktop input is sent.
                unsafe {
                    SetKeyboardState(&[0; 256]).unwrap();
                    assert_eq!(GetFocus(), form.control(default.0 as u16).unwrap());
                    let key = |vk: u16| MSG {
                        hwnd: GetFocus(),
                        message: WM_KEYDOWN,
                        wParam: WPARAM(vk as usize),
                        ..Default::default()
                    };
                    assert!(window::pre_translate(&key(VK_TAB.0)));
                    assert_eq!(
                        GetFocus(),
                        if well {
                            form.control(TEXT_ID).unwrap()
                        } else if buttons == Buttons::YesNo {
                            form.control(IDNO.0 as u16).unwrap()
                        } else {
                            form.control(IDOK.0 as u16).unwrap()
                        }
                    );
                    let mut keys = [0; 256];
                    keys[VK_CONTROL.0 as usize] = 0x80;
                    SetKeyboardState(&keys).unwrap();
                    assert!(window::pre_translate(&key(u16::from(b'C'))));
                    SetKeyboardState(&[0; 256]).unwrap();
                    assert_eq!(
                        selection.get(),
                        (0, text.encode_utf16().count() as u32),
                        "Ctrl+C selects all original text"
                    );
                    assert_eq!(testing::text(form.hwnd()).replace("\r\n", "\n"), text);
                    if well {
                        let edit = form.control(TEXT_ID).unwrap();
                        let line = || SendMessageW(edit, EM_GETFIRSTVISIBLELINE, None, None).0;
                        assert_eq!(line(), 0, "well opens at top");
                        let page = key(VK_NEXT.0);
                        // IsDialogMessage may dispatch a key the edit requests itself.
                        if !window::pre_translate(&page) {
                            SendMessageW(edit, WM_KEYDOWN, Some(page.wParam), Some(LPARAM(0)));
                        }
                        assert!(line() > 0, "PageDown scrolls");
                        SetKeyboardState(&keys).unwrap();
                        SendMessageW(
                            edit,
                            WM_KEYDOWN,
                            Some(WPARAM(VK_END.0 as usize)),
                            Some(LPARAM(0)),
                        );
                        assert!(line() > 100, "Ctrl+End reaches tail");
                        SendMessageW(
                            edit,
                            WM_KEYDOWN,
                            Some(WPARAM(VK_HOME.0 as usize)),
                            Some(LPARAM(0)),
                        );
                        SetKeyboardState(&[0; 256]).unwrap();
                        assert_eq!(line(), 0, "Ctrl+Home returns to top");
                        assert!(window::pre_translate(&key(VK_TAB.0)));
                        assert_eq!(
                            GetFocus(),
                            form.control(if buttons == Buttons::Ok { IDOK } else { IDNO }.0 as u16)
                                .unwrap()
                        );
                    }
                    for dpi in [96, 144] {
                        let mut outer = SIZE::default();
                        assert_eq!(
                            SendMessageW(
                                form.hwnd(),
                                WM_GETDPISCALEDSIZE,
                                Some(WPARAM(dpi as usize)),
                                Some(LPARAM(&mut outer as *mut SIZE as isize))
                            )
                            .0,
                            1
                        );
                        let (style, ex) = window::styles(&spec);
                        let expected =
                            dpi::outer_for_client(dpi::scale_size(size, dpi), style, ex, dpi)
                                .unwrap();
                        assert_eq!(
                            (outer.cx, outer.cy),
                            (expected.w, expected.h),
                            "production DPI frame"
                        );
                        assert!(
                            outer.cx <= dpi::scale(1280, dpi) && outer.cy <= dpi::scale(672, dpi),
                            "smallest DESIGN work area"
                        );
                    }
                    if well {
                        let style =
                            GetWindowLongPtrW(form.control(TEXT_ID).unwrap(), GWL_STYLE) as u32;
                        assert_ne!(style & WS_VSCROLL.0, 0);
                        assert_eq!(style & WS_HSCROLL.0, 0);
                    }
                }
                drop(probe);
                form.destroy();
            }
        }
        testing::review_keyboard();
        println!(
            "RESULT Codex: short/long OK and YesNo default focus, Tab, full Ctrl+C selection/WM_COPY, PageDown/Ctrl+End/Ctrl+Home, and production frame/work-area fit at 96/144 DPI passed; clipboard untouched"
        );
    }

    /// `(title, text, buttons, icon, button to press, expected answer)`.
    type Case = (
        &'static str,
        String,
        Buttons,
        Icon,
        MESSAGEBOX_RESULT,
        Answer,
    );

    /// Shows every case ownerless, captures `msgbox-{i}-{dpi}dpi.png` and the synthetic other
    /// DPI into `golden`, presses the button by HWND and checks the text and answer. With
    /// `keyboard`, the first three cases also exercise Esc, Enter, X and Tab.
    fn run_boxes(golden: &'static str, cases: Vec<Case>, keyboard: bool) {
        for (i, (title, text, buttons, icon, reply, expected)) in cases.into_iter().enumerate() {
            let title_owned = title.to_owned();
            let helper = std::thread::spawn(move || {
                let start = Instant::now();
                let h = loop {
                    if let Some(h) = testing::find(&title_owned) {
                        break h;
                    }
                    assert!(
                        start.elapsed() < Duration::from_secs(10),
                        "no box {title_owned}"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                };
                let press = testing::PressOnDrop {
                    dialog: h,
                    id: reply,
                };
                std::thread::sleep(Duration::from_millis(250));
                let shown = testing::text(h);
                let mut notes = vec![format!("{title_owned}: {shown:?}")];
                let real = dpi::window_dpi(h);
                assert_eq!(target_dpi(HWND::default()), real, "ownerless box DPI");
                notes.push(
                    testing::capture(
                        h,
                        &Path::new(golden).join(format!("msgbox-{i}-{real}dpi.png")),
                    )
                    .unwrap_or_else(|e| e),
                );
                notes.push(format!("real {real}: {}", testing::body_note(h)));
                let other = if real == 96 { 144 } else { 96 };
                let mut r = RECT::default();
                let mut c = RECT::default();
                // SAFETY: Writable RECTs of a live window.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut r);
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(h, &mut c);
                }
                // Scale the client only: Windows keeps the real monitor's frame around a
                // synthetic DPI change, so a linearly scaled outer rect would starve the body.
                let f = |v: i32| v * other as i32 / real as i32;
                let frame_w = r.right - r.left - c.right;
                let frame_h = r.bottom - r.top - c.bottom;
                let s = RECT {
                    left: r.left,
                    top: r.top,
                    right: r.left + f(c.right) + frame_w,
                    bottom: r.top + f(c.bottom) + frame_h,
                };
                // SAFETY: Synchronous message with a pointer to a live RECT; the UI thread pumps.
                unsafe {
                    SendMessageW(
                        h,
                        WM_DPICHANGED,
                        Some(WPARAM((other | (other << 16)) as usize)),
                        Some(LPARAM(&s as *const RECT as isize)),
                    );
                }
                std::thread::sleep(Duration::from_millis(300));
                notes.push(
                    testing::capture(
                        h,
                        &Path::new(golden).join(format!("msgbox-{i}-synthetic-{other}.png")),
                    )
                    .unwrap_or_else(|e| e),
                );
                notes.push(format!("synthetic {other}: {}", testing::body_note(h)));
                // These cases exercise keyboard translation, not just command-by-id clicks.
                // A delayed fallback still closes the test window if a keyboard check fails.
                if keyboard && i <= 2 {
                    use windows::Win32::UI::Input::KeyboardAndMouse::{
                        VK_ESCAPE, VK_RETURN, VK_TAB,
                    };
                    use windows::Win32::UI::WindowsAndMessaging::{
                        PostMessageW, WM_CLOSE, WM_KEYDOWN,
                    };
                    // SAFETY: Value-only key/close messages posted by HWND to this test box.
                    unsafe {
                        if i == 2 {
                            PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                            std::thread::sleep(Duration::from_millis(50));
                            assert!(testing::find(&title_owned).is_some(), "Yes/No ignores X");
                            PostMessageW(Some(h), WM_KEYDOWN, WPARAM(VK_TAB.0 as usize), LPARAM(0))
                                .unwrap();
                        }
                        let key = if i == 0 { VK_ESCAPE } else { VK_RETURN };
                        PostMessageW(Some(h), WM_KEYDOWN, WPARAM(key.0 as usize), LPARAM(0))
                            .unwrap();
                    }
                    std::thread::sleep(Duration::from_millis(200));
                    assert!(
                        testing::find(&title_owned).is_none(),
                        "keyboard did not close {title_owned}"
                    );
                }
                drop(press);
                (shown, notes)
            });
            let answer = show(HWND::default(), &text, title, buttons, icon);
            let (shown, notes) = helper.join().unwrap();
            for n in notes {
                println!("RESULT {n}");
            }
            // The well stores CR LF; the label and the hidden copy keep the text as given.
            assert_eq!(shown.replace("\r\n", "\n"), text);
            assert_eq!(answer, expected, "{title}");
        }
    }
}
