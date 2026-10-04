//! Owned by WP-17: update check and download progress window.
//!
//! C# flow (`SectionedViewForm.CheckUpdatesButton_Click` + `AutoUpdateService`): the button
//! shows `⟳ Checking...` while the check runs, then the no-update box, the error box, or the
//! Yes/No prompt; after Yes the progress window and the restart. Here the check runs on a
//! worker and reports to a hidden form, so the UI stays live like the C# `await`.
//!
//! Progress window timing (AD-34 proposal): `update::check` already downloaded the file, so
//! after Yes the window replays the C# states from the retained file, each painted once:
//! `Preparing download...`, then `Downloading new version...` with the final MB line, then the
//! C# end state held 500 ms, then the install. No network traffic after Yes.

use super::controls::{self, Ctl, LabelSpec};
use super::layout::{Anchor, Node, Size, Track};
use super::msgbox::{self, Answer, Buttons, Icon};
use super::theme;
use super::window::{Event, Form, FormSpec, FormStyle, StartPosition, WindowSize};
use crate::update::{self, Downloaded, UpdateCheck};
use crate::win::{self, process::Cancel};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use windows::Win32::Foundation::HWND;

// C# parity: UI/Forms/SectionedViewForm.cs:255,778,812 (the `⟳` prefix is the button's
// icon-font glyph, DESIGN.md 14).
/// The Updates button text at rest.
pub const UPDATES_TEXT: &str = "Updates";
const CHECKING_TEXT: &str = "Checking...";
const PROMPT: &str = "A new version is available. Do you want to update now?\n\n\
                      The application will restart after the update.";

const LABEL: u16 = 1;
const BAR: u16 = 2;
const DETAIL: u16 = 3;
const ICON: u16 = 4;
const STEP_TIMER: usize = 1;
/// `USER_TIMER_MINIMUM`. `WM_PAINT` is retrieved before `WM_TIMER`, so every replayed state is
/// painted before the next one replaces it.
const PAINT_MS: u32 = 10;
// C# parity: Services/AutoUpdateService.cs:244 (`await Task.Delay(500)` after completion).
const HOLD_MS: u32 = 500;

enum Msg {
    Checked(Result<UpdateCheck, String>),
    Installed(Result<(), String>),
}

struct Flow {
    button: HWND,
    cancel: Cancel,
    /// Replay steps already shown after Yes (0 = still checking).
    step: Cell<u8>,
    pending: RefCell<Option<Downloaded>>,
}

#[derive(Default)]
struct StartCheck {
    cancel: Cancel,
    manual_started: Cell<bool>,
    pending: RefCell<Option<Downloaded>>,
}

thread_local! {
    // The owned hidden form owns the download. Only weak references live here, so closing
    // the main window releases it even when the user never clicks the notice.
    static START_CHECKS: RefCell<Vec<(Form, Weak<StartCheck>)>> = const { RefCell::new(Vec::new()) };
}

/// Checks silently on a worker, retaining an available update until this owner consumes it.
pub fn check_on_start(owner: Form, button: u16) {
    let state = Rc::new(StartCheck::default());
    let cancel = state.cancel.clone();
    let weak = Rc::downgrade(&state);
    let receiver = Form::create(owner.hwnd(), spec(), Vec::new(), move |_, event| {
        match event {
            Event::Worker(value) => {
                if let Ok(result) = value.downcast::<Result<UpdateCheck, String>>() {
                    match *result {
                        Ok(UpdateCheck::Available(downloaded))
                            if !state.manual_started.get()
                                && owner.text(button) == UPDATES_TEXT =>
                        {
                            *state.pending.borrow_mut() = Some(downloaded);
                            owner.set_button_fore(button, Some(theme::INFO));
                            owner.set_text(button, "Update available");
                        }
                        Ok(_) => {} // Up to date, or the manual flow won; release the bytes.
                        Err(error) => win::record(win::Error::msg("Update check (start)", error)),
                    }
                }
            }
            Event::Destroyed => {
                state.cancel.cancel();
                state.pending.borrow_mut().take();
                START_CHECKS.with_borrow_mut(|checks| checks.retain(|(form, _)| *form != owner));
            }
            _ => {}
        }
        true
    });
    let receiver = match receiver {
        Ok(receiver) => receiver,
        Err(error) => return win::record(error),
    };
    START_CHECKS.with_borrow_mut(|checks| checks.push((owner, weak)));
    let Some(poster) = receiver.poster() else {
        receiver.destroy();
        return win::record(win::Error::msg(
            "Update check (start)",
            "window closed before check",
        ));
    };
    if let Err(error) = spawn("update check (start)", move || {
        let result = win::catch_panic(|| update::check_with_progress(&cancel, &mut |_, _| {}))
            .unwrap_or_else(|panic| Err(panicked("Update check (start)", &panic)));
        // A closed owner drops the result, including any retained download.
        let _delivered = poster.post(result);
    }) {
        receiver.destroy();
        win::record(win::Error::msg("Update check (start)", error));
    }
}

/// Consumes the startup notice and gives manual intent priority over any late startup result.
pub fn take_startup_update(owner: Form) -> Option<Downloaded> {
    START_CHECKS.with_borrow(|checks| {
        let state = checks
            .iter()
            .find(|(form, _)| *form == owner)?
            .1
            .upgrade()?;
        state.manual_started.set(true);
        state.pending.borrow_mut().take()
    })
}

/// Offers the retained startup download without checking or downloading again.
pub fn offer(owner: HWND, button: HWND, downloaded: Downloaded) {
    controls::set_enabled(button, false);
    if let Err(error) = start(owner, button, Some(downloaded)) {
        msgbox::show(
            active_owner(),
            &format!("Update failed: {error}"),
            "Update Error",
            Buttons::Ok,
            Icon::Error,
        );
        restore(button);
    }
}

/// Checks for updates and drives the modal update flow for its originating button.
pub fn check_and_update(owner: HWND, button: HWND) {
    // C# parity: UI/Forms/SectionedViewForm.cs:775-779.
    controls::set_enabled(button, false);
    controls::set_text(button, CHECKING_TEXT);
    if let Err(error) = start(owner, button, None) {
        check_failed(&error);
        restore(button);
    }
}

fn start(owner: HWND, button: HWND, downloaded: Option<Downloaded>) -> Result<(), String> {
    let flow = Rc::new(Flow {
        button,
        cancel: Cancel::new(),
        step: Cell::new(0),
        pending: RefCell::new(None),
    });
    let cancel = flow.cancel.clone();
    // Hidden until Yes; it receives the worker result even inside another modal loop.
    let form = Form::create(owner, spec(), nodes(), move |form, event| {
        win::catch_panic(|| handle(&flow, form, event)).unwrap_or_else(|panic| {
            let error = panicked("Update window", &panic);
            form.kill_timer(STEP_TIMER);
            if flow.step.get() == 0 {
                form.destroy();
                check_failed(&error);
                restore(flow.button);
            } else {
                installed(&flow, form, Err(error));
            }
            true
        })
    })
    .map_err(|e| e.to_string())?;
    let Some(poster) = form.poster() else {
        return Err(win::Error::msg("Update window", "window closed before the check").to_string());
    };
    if let Some(downloaded) = downloaded {
        // Use the same prompt, replay and install path as a manual check, without a worker GET.
        let _delivered = poster.post(Msg::Checked(Ok(UpdateCheck::Available(downloaded))));
        return Ok(());
    }
    spawn("update check", move || {
        let result = win::catch_panic(|| update::check_with_progress(&cancel, &mut |_, _| {}))
            .unwrap_or_else(|panic| Err(panicked("Update check", &panic)));
        // false = the window is gone (owner closed); dropping `Downloaded` deletes its file.
        let _delivered = poster.post(Msg::Checked(result));
    })
    .inspect_err(|_| form.destroy())
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> Result<(), String> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(work)
        .map(drop)
        .map_err(|e| win::Error::msg("Start update worker", e.to_string()).to_string())
}

fn panicked(op: &'static str, message: &str) -> String {
    win::Error::msg(op, format!("panicked: {message}")).to_string()
}

fn spec() -> FormSpec {
    // C# parity: Services/AutoUpdateService.cs:148-156. Outer size scaled per AD-39.
    let mut spec = FormSpec::new(
        "Updating HWID Checker",
        WindowSize::Outer {
            size: theme::UPDATE_SIZE,
            scaled: true,
        },
    );
    spec.style = FormStyle::FixedDialog;
    spec.start = StartPosition::CenterScreen;
    spec.back = theme::UPDATE_BACKGROUND;
    spec
}

fn nodes() -> Vec<Node> {
    // C# parity: Services/AutoUpdateService.cs:158-183 (same three texts and states). Layout
    // per DESIGN.md 16: app icon, the step as the heading, the status line under it in its
    // status color, the progress pill across the bottom.
    let texts = Node::panel(vec![
        Node::leaf(
            LABEL,
            Ctl::Label(
                LabelSpec::new(
                    "Preparing download...",
                    theme::UPDATE_LABEL_FONT,
                    theme::UPDATE_TEXT,
                )
                .ellipsis(),
            ),
        )
        .top()
        .height(theme::UPDATE_LABEL_HEIGHT)
        .margin(theme::UPDATE_LABEL_MARGIN),
        Node::leaf(
            DETAIL,
            Ctl::Label(LabelSpec::new("", theme::UPDATE_DETAIL_FONT, theme::SECONDARY).ellipsis()),
        )
        .top()
        .height(theme::UPDATE_DETAIL_HEIGHT),
    ])
    .fill()
    .auto_size()
    .margin(theme::NO_PAD)
    .cell(1, 0);
    let icon = Node::leaf(ICON, Ctl::AppIcon)
        .size(Size {
            w: theme::UPDATE_ICON_SIZE,
            h: theme::UPDATE_ICON_SIZE,
        })
        .anchor(Anchor::TOP)
        .margin(theme::UPDATE_ICON_MARGIN)
        .cell(0, 0);
    // The bar sits in the text column, so icon, texts and bar share one left margin.
    let bar = Node::leaf(BAR, Ctl::Progress)
        .height(theme::PROGRESS_BAR_HEIGHT)
        .anchor(Anchor(Anchor::TOP.0 | Anchor::LEFT.0 | Anchor::RIGHT.0))
        .margin(theme::UPDATE_BAR_MARGIN)
        .cell(1, 1);
    vec![
        Node::table(
            vec![Track::AutoSize, Track::Percent(100.0)],
            vec![Track::AutoSize, Track::AutoSize],
            vec![icon, texts, bar],
        )
        .fill()
        .padding(theme::UPDATE_PADDING),
    ]
}

fn handle(flow: &Flow, form: &Form, event: Event) -> bool {
    match event {
        Event::Worker(value) => match value.downcast::<Msg>().map(|m| *m) {
            Ok(Msg::Checked(result)) => checked(flow, form, result),
            Ok(Msg::Installed(result)) => installed(flow, form, result),
            Err(_) => win::record(win::Error::msg("Update window", "unknown worker message")),
        },
        Event::Timer(STEP_TIMER) => step(flow, form),
        // The window only shows for the last second before the restart; closing it then would
        // drop the retained download mid-install (C# has no cancel either).
        Event::CloseRequest => return false,
        Event::Destroyed => flow.cancel.cancel(),
        _ => {}
    }
    true
}

fn checked(flow: &Flow, form: &Form, result: Result<UpdateCheck, String>) {
    let downloaded = match result {
        Err(error) => {
            form.destroy();
            check_failed(&error);
            return restore(flow.button);
        }
        Ok(UpdateCheck::UpToDate) => {
            form.destroy();
            // C# parity: UI/Forms/SectionedViewForm.cs:786-787.
            msgbox::show(
                active_owner(),
                "You are already running the latest version.",
                "No Updates Available",
                Buttons::Ok,
                Icon::Information,
            );
            return restore(flow.button);
        }
        Ok(UpdateCheck::Available(downloaded)) => downloaded,
    };
    // C# parity: Services/AutoUpdateService.cs:135-145 (decline shows nothing more).
    let answer = msgbox::show(
        active_owner(),
        PROMPT,
        "Update Available",
        Buttons::YesNo,
        Icon::Question,
    );
    if answer != Answer::Yes {
        drop(downloaded);
        form.destroy();
        return restore(flow.button);
    }
    *flow.pending.borrow_mut() = Some(downloaded);
    flow.step.set(1);
    form.show();
    form.set_timer(STEP_TIMER, PAINT_MS);
}

fn step(flow: &Flow, form: &Form) {
    form.kill_timer(STEP_TIMER);
    let step = flow.step.get();
    flow.step.set(step + 1);
    match step {
        1 => {
            if let Some(downloaded) = flow.pending.borrow().as_ref() {
                replay(form, downloaded);
            }
            form.set_timer(STEP_TIMER, PAINT_MS);
        }
        2 => {
            // C# parity: Services/AutoUpdateService.cs:238-244. Status map: done = SUCCESS.
            form.progress_set(BAR, 100);
            form.set_text(LABEL, "Preparing to restart...");
            form.set_label_color(DETAIL, theme::SUCCESS);
            form.set_text(DETAIL, "Download completed successfully");
            form.set_timer(STEP_TIMER, HOLD_MS);
        }
        3 => install(flow, form),
        _ => {}
    }
}

/// The C# download loop's last state, from the file the check retained.
fn replay(form: &Form, downloaded: &Downloaded) {
    // C# parity: Services/AutoUpdateService.cs:192-193.
    form.set_text(LABEL, "Downloading new version...");
    form.progress_set(BAR, 0);
    let size = downloaded.size;
    // C# parity: Services/AutoUpdateService.cs:211 (an empty body never enters the loop).
    if size == 0 {
        return;
    }
    // C# parity: Services/AutoUpdateService.cs:202,216 (`ContentLength ?? 0`, then `> 0`).
    match downloaded.content_length().filter(|&total| total > 0) {
        Some(total) => {
            form.progress_set(BAR, percent(size, total).min(100));
            form.set_text(DETAIL, &progress_text(size, total));
        }
        None => {
            // C# parity: Services/AutoUpdateService.cs:228 (Marquee stays on to the end).
            form.progress_marquee(BAR, true);
            form.set_text(DETAIL, &downloaded_text(size));
        }
    }
}

fn install(flow: &Flow, form: &Form) {
    let Some(downloaded) = flow.pending.borrow_mut().take() else {
        return;
    };
    let Some(poster) = form.poster() else {
        return;
    };
    // Hashing and checking the retained file can take a moment; keep the window painting.
    let started = spawn("update install", move || {
        let result = win::catch_panic(|| update::install_and_restart(downloaded))
            .unwrap_or_else(|panic| Err(panicked("Update install", &panic)));
        // false = the window is gone; nothing is left to report to.
        let _delivered = poster.post(Msg::Installed(result));
    });
    if let Err(error) = started {
        installed(flow, form, Err(error));
    }
}

fn installed(flow: &Flow, form: &Form, result: Result<(), String>) {
    // `install_and_restart` exits on success; an `Ok` return still ends the flow cleanly.
    if let Err(error) = result {
        // C# parity: Services/AutoUpdateService.cs:279-280 (box over the open progress window).
        msgbox::show(
            active_owner(),
            &format!("Update failed: {error}"),
            "Update Error",
            Buttons::Ok,
            Icon::Error,
        );
    }
    // C# never closes the progress window after a failure; it closes once the box is read.
    form.destroy();
    restore(flow.button);
}

fn check_failed(error: &str) {
    // C# parity: Services/AutoUpdateService.cs:82-83.
    msgbox::show(
        active_owner(),
        &format!("Error checking for updates: {error}"),
        "Update Check Failed",
        Buttons::Ok,
        Icon::Warning,
    );
}

fn active_owner() -> HWND {
    // C# parity: Services/AutoUpdateService.cs:82,135,279; UI/Forms/SectionedViewForm.cs:789.
    msgbox::active_window()
}

fn restore(button: HWND) {
    // C# parity: UI/Forms/SectionedViewForm.cs:810-813 (`finally`).
    controls::set_enabled(button, true);
    controls::set_button_fore(button, None);
    controls::set_text(button, UPDATES_TEXT);
}

/// C# `(int)((downloadedBytes * 100) / totalBytes)`; `total` is non-zero.
fn percent(done: u64, total: u64) -> u32 {
    u32::try_from(done.saturating_mul(100) / total).unwrap_or(u32::MAX)
}

/// C# `$"{downloadedMB:F1} MB / {totalMB:F1} MB ({progressPercentage}%)"`.
fn progress_text(done: u64, total: u64) -> String {
    format!(
        "{} MB / {} MB ({}%)",
        mb(done),
        mb(total),
        percent(done, total)
    )
}

/// C# `$"Downloaded: {downloadedMB:F1} MB"`.
fn downloaded_text(done: u64) -> String {
    format!("Downloaded: {} MB", mb(done))
}

/// .NET 10 invariant `F1` of `bytes / 1024.0 / 1024.0`, including ties-to-even.
// C# parity: Services/AutoUpdateService.cs:221-223,229-230; HWIDChecker.csproj:17.
fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1024.0 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_texts_match_dotnet_f1() {
        assert_eq!(mb(0), "0.0");
        assert_eq!(mb(52_428), "0.0");
        assert_eq!(mb(52_429), "0.1");
        // .NET 10 F1 uses ties-to-even, with the invariant decimal separator.
        assert_eq!(mb(1_310_720), "1.2");
        assert_eq!(mb(2_359_296), "2.2");
        assert_eq!(mb(2_883_584), "2.8");
        assert_eq!(mb(256 * 1_048_576), "256.0");
        assert_eq!(progress_text(1_310_720, 2_621_440), "1.2 MB / 2.5 MB (50%)");
        assert_eq!(
            progress_text(5_452_595, 5_452_595),
            "5.2 MB / 5.2 MB (100%)"
        );
        assert_eq!(progress_text(999, 1_000), "0.0 MB / 0.0 MB (99%)");
        assert_eq!(downloaded_text(15_728_640), "Downloaded: 15.0 MB");
    }

    /// The update window's three states on a real window, no network: captured for the
    /// DESIGN.md 16 review. `cargo test --locked --lib -- --ignored ui::update_progress::tests::wp19_layout --nocapture`
    #[test]
    #[ignore = "opens a real window"]
    #[allow(clippy::unwrap_used)]
    fn wp19_layout() {
        use crate::ui::dpi;
        use std::path::Path;
        use std::time::{Duration, Instant};
        const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\wp-19";
        std::fs::create_dir_all(GOLDEN).unwrap();
        assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
        assert!(flow::activate_comctl6(), "comctl v6");
        msgbox::testing::review_keyboard();
        let form = Form::create(HWND::default(), spec(), nodes(), |_, _| true).unwrap();
        form.show();
        let pump = |ms: u64| {
            let end = Instant::now() + Duration::from_millis(ms);
            crate::ui::window::pump_until(|| Instant::now() >= end);
            crate::ui::window::pump_until(|| true);
        };
        let states: [(&str, &str, &str, u32, bool); 3] = [
            ("update-1-preparing", "Preparing download...", "", 0, false),
            (
                "update-2-downloading",
                "Downloading new version...",
                "2.5 MB / 5.2 MB (48%)",
                48,
                false,
            ),
            (
                "update-3-restart",
                "Preparing to restart...",
                "Download completed successfully",
                100,
                true,
            ),
        ];
        for (name, label, detail, pos, done) in states {
            form.set_text(LABEL, label);
            form.set_label_color(
                DETAIL,
                if done {
                    theme::SUCCESS
                } else {
                    theme::SECONDARY
                },
            );
            form.set_text(DETAIL, detail);
            form.progress_set(BAR, pos);
            form.set_timer(1, 20);
            pump(250);
            let dpi = form.dpi();
            let shot = msgbox::testing::capture(
                form.hwnd(),
                &Path::new(GOLDEN).join(format!("{name}-{dpi}dpi.png")),
            );
            println!("RESULT {name}: {shot:?}");
        }
        // Marquee (unknown length) for the review of the moving block.
        form.progress_marquee(BAR, true);
        pump(300);
        println!(
            "RESULT update-marquee: {:?}",
            msgbox::testing::capture(
                form.hwnd(),
                &Path::new(GOLDEN).join(format!("update-4-marquee-{}dpi.png", form.dpi())),
            )
        );
        form.destroy();
    }

    /// WP-17 flow on real windows against a loopback server; install stays a dry run.
    /// `cargo test --locked --lib -- --ignored ui::update_progress::tests::wp17_flow --nocapture`
    mod flow {
        #![allow(clippy::unwrap_used, clippy::expect_used)]

        use super::super::*;
        use crate::ui::controls::ButtonSpec;
        use crate::ui::dpi;
        use crate::ui::layout::{Point, Size};
        use crate::win::wide::to_wide;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::path::Path;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread::JoinHandle;
        use std::time::{Duration, Instant};
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, FindWindowW, GW_OWNER, GetWindow, IDNO, IDOK, IDYES,
            MESSAGEBOX_RESULT, MSG, PM_REMOVE, PeekMessageW, SWP_NOSIZE, SWP_NOZORDER,
            SetWindowPos, TranslateMessage,
        };
        use windows::core::PCWSTR;

        const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\wp-17";
        const PROGRESS: &str = "Updating HWID Checker";

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

        /// The test exe has no manifest; activate Common Controls 6 like the app manifest does.
        pub(super) fn activate_comctl6() -> bool {
            let path = Path::new(GOLDEN).join("comctl6.manifest");
            std::fs::write(&path, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
</assembly>"#).unwrap();
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
            // SAFETY: `ctx` and the path buffer live through the call; the context stays
            // active for the rest of the test thread on purpose.
            unsafe {
                let h = CreateActCtxW(&ctx);
                let mut cookie = 0usize;
                !h.is_null() && h as isize != -1 && ActivateActCtx(h, &mut cookie) != 0
            }
        }

        fn pump_until(done: impl Fn() -> bool, secs: u64) -> bool {
            let end = Instant::now() + Duration::from_secs(secs);
            while !done() && Instant::now() < end {
                let mut msg = MSG::default();
                // SAFETY: Non-blocking pump of this UI thread's queue.
                unsafe {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            done()
        }

        fn find(class: Option<&str>, title: &str) -> Option<HWND> {
            let class = class.map(to_wide);
            let title = to_wide(title);
            // SAFETY: NUL-terminated buffers valid for the call.
            unsafe {
                FindWindowW(
                    class
                        .as_ref()
                        .map_or(PCWSTR::null(), |c| PCWSTR(c.as_ptr())),
                    PCWSTR(title.as_ptr()),
                )
            }
            .ok()
            .filter(|h| !h.is_invalid())
        }

        fn frame(h: HWND) -> RECT {
            let mut r = RECT::default();
            // SAFETY: Writes the visible frame of a live window into a local RECT.
            unsafe {
                DwmGetWindowAttribute(
                    h,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    (&mut r as *mut RECT).cast(),
                    std::mem::size_of::<RECT>() as u32,
                )
                .unwrap();
            }
            r
        }

        /// Best-effort screen capture with a timeout; never panics (the caller still clicks).
        fn screenshot(h: HWND, name: &str) -> String {
            let r = frame(h);
            // SAFETY: Read-only DPI query of a live window.
            let dpi = unsafe { GetDpiForWindow(h) };
            let path = Path::new(GOLDEN).join(format!("{name}-{dpi}dpi.png"));
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
            let result = crate::win::process::run(
                &crate::win::process::powershell(),
                &["-NoProfile", "-NonInteractive", "-Command", &script],
                Duration::from_secs(15),
                &Cancel::new(),
            );
            match result {
                Ok(out) if out.code == 0 => format!(
                    "{} at {dpi} DPI: frame {}x{} at ({}, {})",
                    path.display(),
                    r.right - r.left,
                    r.bottom - r.top,
                    r.left,
                    r.top
                ),
                Ok(out) => format!("screenshot {name} failed: {}", out.stderr),
                Err(error) => format!("screenshot {name} failed: {error}"),
            }
        }

        /// Answers message boxes in order on a helper thread; returns their texts and notes.
        /// Once a box is found, its button is pressed no matter what else fails (`PressOnDrop`),
        /// so the owner never has to click a box the test owns.
        fn answer(
            boxes: Vec<(&'static str, MESSAGEBOX_RESULT, Option<&'static str>)>,
            expected_owner: Option<&'static str>,
        ) -> JoinHandle<Vec<String>> {
            std::thread::spawn(move || {
                let mut seen = Vec::new();
                for (title, reply, shot) in boxes {
                    let start = Instant::now();
                    let h = loop {
                        if let Some(h) = msgbox::testing::find(title) {
                            break h;
                        }
                        if start.elapsed() > Duration::from_secs(110) {
                            let other = (|| {
                                let class = to_wide(crate::ui::window::MESSAGE_BOX_CLASS);
                                // SAFETY: NUL-terminated class name; any title.
                                let h =
                                    unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) }
                                        .ok()?;
                                Some(msgbox::testing::text(h))
                            })();
                            panic!("no box {title}; open box: {other:?}");
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    };
                    let press = msgbox::testing::PressOnDrop {
                        dialog: h,
                        id: reply,
                    };
                    let found = start.elapsed().as_millis();
                    let owner_matches = expected_owner.map(|title| {
                        // SAFETY: Read-only owner query of the message box we just found.
                        let actual = unsafe { GetWindow(h, GW_OWNER) }.ok();
                        actual.is_some() && actual == find(None, title)
                    });
                    std::thread::sleep(Duration::from_millis(200));
                    seen.push(format!(
                        "{title} (after {found} ms): {}",
                        msgbox::testing::text(h)
                    ));
                    if let Some(name) = shot {
                        match find(None, PROGRESS) {
                            Some(progress) => {
                                let r = frame(progress);
                                // SAFETY: Moves the message box below the progress window so
                                // the capture shows the progress window only.
                                unsafe {
                                    let _ = SetWindowPos(
                                        h,
                                        None,
                                        r.left,
                                        r.bottom + 20,
                                        0,
                                        0,
                                        SWP_NOSIZE | SWP_NOZORDER,
                                    );
                                }
                                std::thread::sleep(Duration::from_millis(300));
                                seen.push(screenshot(progress, name));
                            }
                            None => seen.push(format!("{name}: progress window not open")),
                        }
                    }
                    drop(press);
                    if let Some(matches) = owner_matches {
                        assert!(matches, "{title}: owner must be {expected_owner:?}");
                        seen.push(format!("owner matched {expected_owner:?}"));
                    }
                }
                seen
            })
        }

        /// Serves `response` to the first GET, then counts any further connection until `stop`.
        /// (`win::http::tests::serve` races here: an accepted socket inherits non-blocking.)
        fn serve(
            delay: Duration,
            response: Vec<u8>,
        ) -> (String, Arc<AtomicBool>, JoinHandle<usize>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/HWIDChecker.exe", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let handle = std::thread::spawn(move || {
                let mut count = 0;
                let start = Instant::now();
                while !flag.load(Ordering::Acquire) {
                    assert!(
                        // Matches the 240 s the UI pump waits; the 404 path of the check has
                        // taken 11 s to over 90 s on this PC (WinHTTP failure timing).
                        start.elapsed() < Duration::from_secs(240),
                        "server not stopped"
                    );
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    };
                    count += 1;
                    if count > 1 {
                        continue;
                    }
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut buffer = [0u8; 1024];
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0, "request closed early");
                        request.extend_from_slice(&buffer[..n]);
                    }
                    std::thread::sleep(delay);
                    stream.write_all(&response).unwrap();
                }
                count
            });
            (url, stop, handle)
        }

        fn reply(headers: &str, body: &[u8]) -> Vec<u8> {
            let mut response =
                format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n").into_bytes();
            response.extend_from_slice(body);
            response
        }

        fn fixture() -> Vec<u8> {
            let hex: String = include_str!("../../tests/fixtures/wp-13/x64-pe.hex")
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect()
        }

        #[test]
        #[ignore = "opens real windows and message boxes; install stays a dry run"]
        fn wp17_flow() {
            assert!(
                std::env::var_os("HWID_ALLOW_DESTRUCTIVE").is_none(),
                "destructive authorization must be absent"
            );
            std::fs::create_dir_all(GOLDEN).unwrap();
            assert!(dpi::set_per_monitor_v2_for_tests(), "PerMonitorV2");
            assert!(activate_comctl6(), "comctl v6");
            let owner = Form::create(
                HWND::default(),
                FormSpec::new("WP-17 owner", WindowSize::Client(Size { w: 300, h: 80 })),
                vec![
                    Node::leaf(1, Ctl::Button(ButtonSpec::primary(UPDATES_TEXT)))
                        .pos(Point { x: 10, y: 10 })
                        .size(Size { w: 140, h: 34 }),
                ],
                |_, _| true,
            )
            .unwrap();
            owner.show();
            let button = owner.control(1).unwrap();
            let restored = || owner.is_enabled(1) && controls::text(button) == UPDATES_TEXT;
            let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
            let pe = fixture();
            let not_found =
                b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                    .to_vec();
            let cases: Vec<(&str, Duration, Vec<u8>, Vec<_>)> = vec![
                (
                    "check error (404)",
                    Duration::from_millis(1000),
                    not_found,
                    vec![("Update Check Failed", IDOK, None)],
                ),
                (
                    "same hash",
                    Duration::ZERO,
                    reply(&format!("Content-Length: {}\r\n", exe.len()), &exe),
                    vec![("No Updates Available", IDOK, None)],
                ),
                (
                    "different hash, decline",
                    Duration::ZERO,
                    reply("Content-Length: 1024\r\n", &pe),
                    vec![("Update Available", IDNO, None)],
                ),
                (
                    "different hash, decline while modal open",
                    Duration::from_millis(1000),
                    reply("Content-Length: 1024\r\n", &pe),
                    vec![("Update Available", IDNO, None)],
                ),
                (
                    "different hash, Yes, Content-Length",
                    Duration::ZERO,
                    reply("Content-Length: 1024\r\n", &pe),
                    vec![
                        ("Update Available", IDYES, None),
                        ("Update Error", IDOK, Some("progress-known-length")),
                    ],
                ),
                (
                    "different hash, Yes, no Content-Length",
                    Duration::ZERO,
                    reply("", &pe),
                    vec![
                        ("Update Available", IDYES, None),
                        ("Update Error", IDOK, Some("progress-unknown-length")),
                    ],
                ),
            ];
            let mut report = String::new();
            for (name, delay, response, boxes) in cases {
                let (url, stop, server) = serve(delay, response);
                // SAFETY: This ignored test runs alone; no other thread reads the environment.
                unsafe { std::env::set_var("HWID_UPDATE_URL", &url) };
                let modal = name == "different hash, decline while modal open";
                let helper = answer(boxes, modal.then_some("WP-17 modal"));
                let start = Instant::now();
                check_and_update(owner.hwnd(), button);
                let busy = format!(
                    "button while checking: {:?}, enabled {}",
                    controls::text(button),
                    owner.is_enabled(1)
                );
                assert_eq!(controls::text(button), CHECKING_TEXT);
                assert!(!owner.is_enabled(1));
                if modal {
                    let timed_out = Rc::new(Cell::new(false));
                    let timeout = Rc::clone(&timed_out);
                    super::super::super::window::run_modal(
                        owner.hwnd(),
                        FormSpec::new("WP-17 modal", WindowSize::Client(theme::UPDATE_SIZE)),
                        vec![],
                        move |form, event| {
                            match event {
                                Event::Created => form.set_timer(1, 10),
                                Event::Timer(1) => {
                                    if controls::text(button) == UPDATES_TEXT {
                                        form.destroy();
                                    } else if start.elapsed() > Duration::from_secs(30) {
                                        timeout.set(true);
                                        form.destroy();
                                    }
                                }
                                _ => {}
                            }
                            true
                        },
                    )
                    .unwrap();
                    assert!(
                        !timed_out.get(),
                        "modal loop must deliver the update result"
                    );
                }
                assert!(
                    pump_until(|| helper.is_finished() && restored(), 240),
                    "{name}: no restore"
                );
                let elapsed = start.elapsed().as_millis();
                let seen = helper.join().unwrap();
                // Give a (wrong) second download time to show up before counting.
                pump_until(|| false, 2);
                stop.store(true, Ordering::Release);
                let requests = server.join().unwrap();
                assert!(
                    find(None, PROGRESS).is_none(),
                    "{name}: progress window left open"
                );
                let line = format!(
                    "{name}: {busy}; GET requests {requests}; {elapsed} ms; restored to {:?}\n  {}\n",
                    controls::text(button),
                    seen.join("\n  ")
                );
                print!("{line}");
                report.push_str(&line);
                assert_eq!(requests, 1, "{name}: exactly one download");
            }
            // SAFETY: See above.
            unsafe { std::env::remove_var("HWID_UPDATE_URL") };
            owner.destroy();
            std::fs::write(Path::new(GOLDEN).join("flow-results.txt"), report).unwrap();
        }
    }
}
