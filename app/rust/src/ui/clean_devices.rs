//! Owned by WP-14: device cleaning window (`CleanDevicesForm.cs`).
//!
//! The scan and the removal run on worker threads; their results come back as `Event::Worker`
//! messages of the current generation. The modal confirm dialog and the whitelist window run
//! on the UI thread from inside the handler, so no `RefCell` borrow is held across them.

use super::controls::{ButtonSpec, Ctl, EditSpec};
use super::layout::{FlowDir, Node, Track};
use super::msgbox::{self, Answer, Buttons, Icon};
use super::theme;
use super::window::{self, Event, Form, FormSpec, FormStyle, WindowSize};
use super::{confirm_removal, whitelist as whitelist_window};
use crate::clean::devices::{self, GhostDevice, Presence};
use crate::clean::whitelist;
use crate::win::{self, process::Cancel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::Win32::Foundation::HWND;

const OUTPUT: u16 = 1;
const CLOSE: u16 = 2;
const RECLEAN: u16 = 3;
const WHITELIST: u16 = 4;
const AUTO_CLOSE_TIMER: usize = 1;
// C# parity: CleanDevicesForm.cs:326. `ScheduleAutoClose(int delayMs = 1000)`.
const AUTO_CLOSE_MS: u32 = 1000;
const SEPARATOR: &str = "----------------------------------------";

/// Shows the modal device cleaning window.
pub fn show(owner: HWND) {
    // C# parity: CleanDevicesForm.cs:195-201. Checked in Load, so the window never shows.
    if !win::security::is_admin() {
        msgbox::show(
            owner,
            "This operation requires administrative privileges. Please run the application as administrator.",
            "Administrator Rights Required",
            Buttons::Ok,
            Icon::Warning,
        );
        return;
    }
    run(owner, real_scan);
}

/// Removes the selected devices of one scan (consumed: the snapshot is released afterwards).
type Remove = Box<dyn FnOnce(&[usize], &Cancel, &dyn Fn(&str)) + Send>;

/// A finished scan as the window sees it.
struct Found {
    /// Display copy (C# `ghostDevices`); the native snapshot stays inside `remove`.
    devices: Vec<GhostDevice>,
    /// Scan sources that left a device without data (AD-03).
    errors: Vec<String>,
    remove: Remove,
}

/// The production scan: WP-11 `clean::devices::scan` with its guarded `Scan::remove`.
fn real_scan() -> Result<Found, String> {
    let scan = devices::scan()?;
    let diagnostics = scan.diagnostics();
    for failure in &diagnostics.failures {
        win::record(win::Error::msg("Device scan", failure.clone()));
    }
    Ok(Found {
        devices: scan.devices().to_vec(),
        errors: diagnostics
            .body
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        // `Scan::remove` re-reads the whitelist, re-checks presence before each device, and
        // goes through the dry-run guard (`[DRY RUN] SetupDiRemoveDevice: ...`).
        remove: Box::new(move |selected, cancel, status| scan.remove(selected, cancel, status)),
    })
}

/// What a worker sends back to the window.
enum Msg {
    /// The scan with the whitelist read right after it (F18: read on every scan).
    Scanned(Result<(Found, Result<Vec<GhostDevice>, String>), String>),
    /// One removal status line.
    Line(String),
    /// Removal finished; `true` = `Yes (Autoclose)`.
    Removed(bool),
    /// A worker panicked; same path as a C# exception in `StartCleaningProcess`.
    Panic(String),
}

struct State {
    scanner: fn() -> Result<Found, String>,
    /// C# `isProcessing`.
    busy: Cell<bool>,
    /// C# `ghostDevices` (display copy; the native snapshot stays inside `Scan`).
    devices: RefCell<Option<Vec<GhostDevice>>>,
    /// Stops further removals once the user confirms a close.
    cancel: RefCell<Cancel>,
    /// The `Confirm Exit` box is open; worker messages wait until it is answered.
    prompting: Cell<bool>,
    pending: RefCell<Vec<Msg>>,
}

fn run(owner: HWND, scanner: fn() -> Result<Found, String>) {
    let state = Rc::new(State {
        scanner,
        busy: Cell::new(false),
        devices: RefCell::new(None),
        cancel: RefCell::new(Cancel::new()),
        prompting: Cell::new(false),
        pending: RefCell::new(Vec::new()),
    });
    let mut spec = FormSpec::new(
        "Device Cleaning",
        WindowSize::Client(theme::CLEAN_DEVICES_CLIENT_SIZE),
    );
    spec.min = Some(theme::CLEAN_DEVICES_MIN_SIZE);
    spec.style = FormStyle::Sizable {
        maximize: true,
        minimize: false,
    };
    let shown = window::run_modal(owner, spec, tree(), move |form, event| {
        let st = &state;
        match win::catch_panic(|| handle(form, st, event)) {
            Ok(keep) => keep,
            Err(panic) => {
                fail(form, st, &panic);
                true
            }
        }
    });
    if let Err(error) = shown {
        // C# parity: SectionedViewForm.cs:755.
        msgbox::show(
            owner,
            &format!("Error opening device cleaning: {error}"),
            "Device Cleaning Error",
            Buttons::Ok,
            Icon::Error,
        );
    }
}

fn handle(form: &Form, st: &State, event: Event) -> bool {
    match event {
        Event::Created => start(form, st),
        Event::Click(CLOSE) => form.close(),
        Event::Click(RECLEAN) => start(form, st),
        Event::Click(WHITELIST) => manage_whitelist(form, st),
        Event::Timer(AUTO_CLOSE_TIMER) => {
            form.kill_timer(AUTO_CLOSE_TIMER);
            form.close();
        }
        Event::Worker(value) => {
            if let Ok(msg) = value.downcast::<Msg>() {
                on_msg(form, st, *msg);
            }
        }
        Event::CloseRequest => return close_request(form, st),
        Event::Destroyed => {
            // Destroying the owner bypasses CloseRequest. Stop the removal worker on every
            // destruction path and release any scanned snapshot queued behind a modal box.
            st.cancel.borrow().cancel();
            st.pending.borrow_mut().clear();
        }
        _ => {}
    }
    true
}

/// C# `StartCleaningProcess` up to the scan.
fn start(form: &Form, st: &State) {
    // AD-29 (F20): Reclean during the auto-close delay keeps the window open.
    form.kill_timer(AUTO_CLOSE_TIMER);
    form.next_generation();
    *st.cancel.borrow_mut() = Cancel::new();
    st.busy.set(true);
    for id in [RECLEAN, WHITELIST, CLOSE] {
        form.set_enabled(id, false);
    }
    form.edit_set_text(OUTPUT, "");
    *st.devices.borrow_mut() = None;
    // C# parity: CleanDevicesForm.cs:218. The message's own CRLF plus the appended one.
    status(form, "Scanning for non-present (ghost) devices...\r\n");
    let Some(poster) = form.poster() else {
        return;
    };
    let scanner = st.scanner;
    let spawned = std::thread::Builder::new()
        .name("device-scan".into())
        .spawn(move || {
            let msg = match win::catch_panic(|| {
                scanner().map(|found| (found, whitelist::load_whitelist()))
            }) {
                Ok(result) => Msg::Scanned(result),
                Err(panic) => Msg::Panic(panic),
            };
            // false = the window is gone; dropping `msg` releases the snapshot.
            let _ = poster.post(msg);
        });
    if let Err(error) = spawned {
        fail(form, st, &thread_error(error));
    }
}

fn on_msg(form: &Form, st: &State, msg: Msg) {
    if st.prompting.get() {
        st.pending.borrow_mut().push(msg);
        return;
    }
    match msg {
        Msg::Scanned(Err(error)) | Msg::Panic(error) => fail(form, st, &error),
        Msg::Scanned(Ok((found, list))) => scanned(form, st, found, list),
        Msg::Line(line) => status(form, &line),
        Msg::Removed(auto_close) => {
            status(form, "\r\nDevice cleaning process completed.");
            if auto_close {
                form.set_timer(AUTO_CLOSE_TIMER, AUTO_CLOSE_MS);
            }
            finish(form, st);
        }
    }
}

/// C# `StartCleaningProcess` from the scan result to the confirmation.
fn scanned(form: &Form, st: &State, scan: Found, list: Result<Vec<GhostDevice>, String>) {
    let Found {
        devices: found,
        errors,
        remove,
    } = scan;
    *st.devices.borrow_mut() = Some(found.clone());
    for error in &errors {
        status(form, &format!("Error in Cleaning Process: {error}"));
    }
    let list = match list {
        Ok(list) => list,
        Err(error) => {
            // AD-26: a bad whitelist file never unlocks removal. C# would have thrown at the
            // first IsDeviceWhitelisted call (CleanDevicesForm.cs:227), before the list.
            status(form, whitelist::READ_FAILURE);
            fail(form, st, &error);
            return;
        }
    };
    if found.is_empty() {
        status(form, "No non-present devices were found.");
        form.set_timer(AUTO_CLOSE_TIMER, AUTO_CLOSE_MS);
        return finish(form, st);
    }
    let (lines, removable) = scan_lines(&found, &list);
    let text: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
    form.edit_append(OUTPUT, &text);
    if removable.is_empty() {
        if found.iter().all(|d| whitelist::is_whitelisted(d, &list)) {
            status(
                form,
                "\r\nAll devices are whitelisted. No devices need to be cleaned.",
            );
            form.set_timer(AUTO_CLOSE_TIMER, AUTO_CLOSE_MS);
        } else {
            // Unclear devices are not whitelisted; the C# sentence would be false here. No
            // auto-close, so the `Presence unclear` lines stay readable.
            status(
                form,
                "\r\nNo devices can be removed. Devices with unclear presence are kept.",
            );
        }
        return finish(form, st);
    }
    let answer = confirm_removal::show(form.hwnd(), removable.len());
    if !form.is_alive() {
        return;
    }
    let auto_close = match answer {
        confirm_removal::ConfirmResult::YesAutoClose => true,
        confirm_removal::ConfirmResult::Yes => false,
        confirm_removal::ConfirmResult::No => {
            drop(remove); // F21: release the snapshot now.
            status(form, "\r\nOperation cancelled. No devices were removed.");
            // C# parity: CleanDevicesForm.cs:268,285. No falls through to "completed".
            status(form, "\r\nDevice cleaning process completed.");
            return finish(form, st);
        }
    };
    let Some(poster) = form.poster() else {
        return;
    };
    let cancel = st.cancel.borrow().clone();
    let spawned = std::thread::Builder::new()
        .name("device-remove".into())
        .spawn(move || {
            let lines = poster.clone();
            let done = win::catch_panic(|| {
                remove(&removable, &cancel, &|line| {
                    // false = the window is gone; nothing left to show the line in.
                    let _ = lines.post(Msg::Line(line.to_owned()));
                });
            });
            let msg = match done {
                Ok(()) => Msg::Removed(auto_close),
                Err(panic) => Msg::Panic(panic),
            };
            // false = the window is gone.
            let _ = poster.post(msg);
        });
    if let Err(error) = spawned {
        fail(form, st, &thread_error(error));
    }
}

/// The device block, the three totals, and the indices that may be removed.
fn scan_lines(found: &[GhostDevice], list: &[GhostDevice]) -> (Vec<String>, Vec<usize>) {
    // C# parity: CleanDevicesForm.cs:233-249.
    let mut lines = vec!["The following non-present (ghost) devices were found:".to_owned()];
    let mut removable = Vec::new();
    let mut whitelisted = 0;
    for (index, device) in found.iter().enumerate() {
        lines.push(SEPARATOR.to_owned());
        lines.push(format!("Device Name       : {}", device.name));
        lines.push(format!("Device Description: {}", device.description));
        lines.push(format!("Hardware ID       : {}", device.hardware_id));
        lines.push(format!("Class             : {}", device.class));
        let is_whitelisted = whitelist::is_whitelisted(device, list);
        if is_whitelisted {
            whitelisted += 1;
            lines.push("Status            : Whitelisted (will not be removed)".to_owned());
        }
        if device.presence == Presence::Unclear {
            // AD-24.
            lines.push("Status            : Presence unclear (will not be removed)".to_owned());
        } else if !is_whitelisted {
            removable.push(index);
        }
    }
    lines.push(SEPARATOR.to_owned());
    lines.push(format!("Total non-present devices found: {}", found.len()));
    // AD-24: counted directly; C# `total - removable` would count Unclear as whitelisted.
    lines.push(format!("Whitelisted devices: {whitelisted}"));
    lines.push(format!("Devices that can be removed: {}", removable.len()));
    (lines, removable)
}

/// C# `catch` in `StartCleaningProcess`, then `finally`.
fn fail(form: &Form, st: &State, error: &str) {
    // C# parity: CleanDevicesForm.cs:289-291 (HandleError format, then the box).
    status(form, &format!("Error in Cleaning Process: {error}"));
    msgbox::show(
        form.hwnd(),
        &format!("Error during cleaning process: {error}"),
        "Error",
        Buttons::Ok,
        Icon::Error,
    );
    finish(form, st);
}

/// C# `finally` in `StartCleaningProcess`.
fn finish(form: &Form, st: &State) {
    st.busy.set(false);
    form.set_enabled(RECLEAN, true);
    let any = st.devices.borrow().as_ref().is_some_and(|d| !d.is_empty());
    form.set_enabled(WHITELIST, any);
    form.set_enabled(CLOSE, true);
}

/// Title-bar X, Alt+F4, the Close button and the auto-close timer all end up here (AD-28).
fn close_request(form: &Form, st: &State) -> bool {
    if !st.busy.get() {
        return true;
    }
    st.prompting.set(true);
    // C# parity: CleanDevicesForm.cs:124-131.
    let answer = msgbox::show(
        form.hwnd(),
        "Operation in progress. Are you sure you want to close?",
        "Confirm Exit",
        Buttons::YesNo,
        Icon::Warning,
    );
    st.prompting.set(false);
    if answer == Answer::Yes {
        // No new removal starts; a removal already inside SetupAPI finishes on its worker.
        st.cancel.borrow().cancel();
        return true;
    }
    // Messages that arrived during the box, in their original order.
    let pending = std::mem::take(&mut *st.pending.borrow_mut());
    for msg in pending {
        on_msg(form, st, msg);
    }
    false
}

fn manage_whitelist(form: &Form, st: &State) {
    // Cloned so no borrow is held while the modal window pumps messages.
    let devices = st.devices.borrow().clone();
    match devices {
        Some(list) if !list.is_empty() => {
            if whitelist_window::show(form.hwnd(), &list) {
                // C# parity: CleanDevicesForm.cs:310-313. No rescan; the next scan re-reads
                // the file (AD-27).
                status(form, "\r\nDevice whitelist has been updated.");
            }
        }
        _ => {
            msgbox::show(
                form.hwnd(),
                "No ghost devices found to whitelist. Please scan for devices first.",
                "No Devices Found",
                Buttons::Ok,
                Icon::Information,
            );
        }
    }
}

fn status(form: &Form, message: &str) {
    // C# parity: CleanDevicesForm.cs:48-50. AppendText(message + CRLF), then ScrollToCaret.
    form.edit_append(OUTPUT, &format!("{message}\r\n"));
}

fn thread_error(error: std::io::Error) -> String {
    win::Error {
        op: "Start worker thread",
        code: error.raw_os_error().unwrap_or(0) as u32,
        detail: error.to_string(),
    }
    .to_string()
}

fn tree() -> Vec<Node> {
    let action = |id: u16, text: &str| {
        // C# parity: CleanDevicesForm.cs:176-191. ApplyStyle overrides the 12,4 padding.
        Node::leaf(id, Ctl::Button(ButtonSpec::primary(text)))
            .auto_size()
            .min(theme::ACTION_BUTTON_MIN)
            .padding(theme::SHARED_BUTTON_PADDING)
            .margin(theme::ACTION_BUTTON_MARGIN)
    };
    vec![
        Node::table(
            vec![Track::Percent(100.0)],
            vec![
                Track::Percent(100.0),
                Track::Absolute(theme::ACTION_ROW_HEIGHT),
            ],
            vec![
                Node::panel(vec![
                    Node::leaf(
                        OUTPUT,
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
                .back(theme::MAIN_BACKGROUND)
                .cell(0, 0),
                // Add order Close, Reclean, Manage Whitelist; right to left on screen.
                Node::flow(
                    FlowDir::RightToLeft,
                    false,
                    vec![
                        action(CLOSE, "Close"),
                        action(RECLEAN, "Reclean"),
                        action(WHITELIST, "Manage Whitelist"),
                    ],
                )
                .fill()
                .padding(theme::ACTION_PANEL_PADDING)
                .back(theme::BUTTON_PANEL_BACKGROUND)
                .cell(0, 1),
            ],
        )
        .fill()
        .back(theme::MAIN_BACKGROUND),
    ]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Fabricated devices (AGENTS Rule 3): Absent, Absent + whitelisted, Unclear,
    /// Unclear + whitelisted.
    fn fabricated() -> Vec<GhostDevice> {
        let device = |description: &str, hardware_id: &str, class: &str, presence| GhostDevice {
            name: "True".to_owned(),
            description: description.to_owned(),
            hardware_id: hardware_id.to_owned(),
            class: class.to_owned(),
            instance_id: String::new(),
            presence,
        };
        vec![
            device(
                "USB Input Device",
                r"USB\VID_046D&PID_C534&REV_2901&MI_00USB\VID_046D&PID_C534&MI_00",
                "HIDClass",
                Presence::Absent,
            ),
            device(
                "Generic USB Hub",
                r"USB\VID_05E3&PID_0610&REV_9312USB\VID_05E3&PID_0610",
                "USB",
                Presence::Absent,
            ),
            device(
                "Intel(R) Wi-Fi 6 AX201 160MHz",
                r"PCI\VEN_8086&DEV_A0F0&SUBSYS_00748086&REV_20PCI\VEN_8086&DEV_A0F0&SUBSYS_00748086",
                "Net",
                Presence::Unclear,
            ),
            device(
                "Disk drive",
                r"SCSI\DiskNVMe____Samsung_SSD_980_PRO_5B2QGXA7SCSI\DiskNVMe____",
                "DiskDrive",
                Presence::Unclear,
            ),
        ]
    }

    #[test]
    fn device_block_and_counts_match_fixture() {
        let found = fabricated();
        let list = [found[1].clone(), found[3].clone()];
        let (lines, removable) = scan_lines(&found, &list);
        let text: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
        assert_eq!(
            text.as_bytes(),
            include_bytes!("../../tests/fixtures/wp-14/scan-lines.fixture")
        );
        // Only the Absent, non-whitelisted device; Unclear never reaches removal (AD-24).
        assert_eq!(removable, [0]);
    }

    mod live {
        //! `cargo test --locked --lib -- --ignored wp14_windows --nocapture`
        //! Opens the real windows and drives them from a second thread. Debug build only:
        //! every removal and whitelist write stays a `[DRY RUN]`. Output and screenshots go to
        //! the private `golden/wp-14` folder (the real-scan run shows real device names).

        use super::*;
        use std::path::Path;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};
        use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
        use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            IsWindowEnabled, VK_ESCAPE, VK_RETURN, VK_TAB,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumChildWindows, EnumWindows, GUITHREADINFO, GetClassNameW, GetClientRect,
            GetDlgCtrlID, GetDlgItem, GetGUIThreadInfo, GetParent, GetWindowRect,
            GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HWND_NOTOPMOST,
            HWND_TOPMOST, IDNO, IDOK, IDYES, IsWindow, IsWindowVisible, LB_GETCOUNT,
            MESSAGEBOX_RESULT, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SendMessageW, SetWindowPos, WM_CLOSE, WM_COMMAND, WM_DPICHANGED, WM_KEYDOWN,
        };
        use windows::core::BOOL;

        const GOLDEN: &str = r"D:\GIT\HWID-Privacy\app\rust\golden\wp-14";

        // ------------------------------------------------------------ fabricated scans

        static SLOW_SCAN: AtomicBool = AtomicBool::new(false);
        static REMOVAL_CANCELLED: AtomicBool = AtomicBool::new(false);
        static REMOVAL_DONE: AtomicBool = AtomicBool::new(false);

        fn owner_close_scan() -> Result<Found, String> {
            Ok(Found {
                devices: fabricated(),
                errors: Vec::new(),
                remove: Box::new(|_, cancel, status| {
                    status("Waiting for owner close.");
                    // No native removal: give the UI time to destroy the owner, then observe
                    // whether the worker was told to stop before another device could start.
                    let end = Instant::now() + Duration::from_secs(2);
                    while !cancel.is_cancelled() && Instant::now() < end {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    REMOVAL_CANCELLED.store(cancel.is_cancelled(), Ordering::SeqCst);
                    REMOVAL_DONE.store(true, Ordering::SeqCst);
                }),
            })
        }

        fn fake_scan() -> Result<Found, String> {
            // Slow mode: a close request always lands while the scan is still running.
            let ms = if SLOW_SCAN.load(Ordering::SeqCst) {
                600
            } else {
                150
            };
            std::thread::sleep(Duration::from_millis(ms));
            let devices = fabricated();
            let names = devices.clone();
            Ok(Found {
                devices,
                errors: Vec::new(),
                remove: Box::new(move |selected, cancel, status| {
                    // The same guard and texts as `Scan::remove` on its dry-run path.
                    status(&format!(
                        "\r\nAttempting to remove {} ghost device(s)...\r\n",
                        selected.len()
                    ));
                    for &i in selected {
                        if cancel.is_cancelled() {
                            break;
                        }
                        let what = format!("SetupDiRemoveDevice: {}", names[i].description);
                        assert!(
                            crate::clean::destructive(&what, || ()).is_none(),
                            "dry-run guard is off"
                        );
                        status(&format!("[DRY RUN] {what}"));
                    }
                    status("\r\nTotal devices removed: 0");
                    status(&format!("Failed to remove {} device(s)", selected.len()));
                }),
            })
        }

        // ------------------------------------------------------------ window helpers

        fn text_of(h: HWND) -> String {
            // SAFETY: Length query and a buffer one larger than the length; another thread's
            // window answers WM_GETTEXT while its thread pumps.
            unsafe {
                let len = GetWindowTextLengthW(h).max(0) as usize;
                let mut buf = vec![0u16; len + 1];
                let n = GetWindowTextW(h, &mut buf).max(0) as usize;
                String::from_utf16_lossy(&buf[..n])
            }
        }

        fn class_of(h: HWND) -> String {
            let mut buf = [0u16; 64];
            // SAFETY: Writable fixed buffer.
            let n = unsafe { GetClassNameW(h, &mut buf) }.max(0) as usize;
            String::from_utf16_lossy(&buf[..n])
        }

        unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the Vec passed by `windows_of`, alive for the enumeration.
            unsafe { (*(data.0 as *mut Vec<isize>)).push(h.0 as isize) };
            BOOL(1)
        }

        /// Visible top-level windows of this process, or every child of `parent`.
        fn windows_of(parent: Option<HWND>) -> Vec<HWND> {
            let mut list: Vec<isize> = Vec::new();
            let data = LPARAM(&mut list as *mut Vec<isize> as isize);
            // SAFETY: Synchronous enumeration; the callback only pushes into `list`.
            unsafe {
                match parent {
                    Some(p) => {
                        let _ = EnumChildWindows(Some(p), Some(collect), data);
                    }
                    None => {
                        let _ = EnumWindows(Some(collect), data);
                    }
                }
            }
            // SAFETY: Plain process id query.
            let me = unsafe { GetCurrentProcessId() };
            list.into_iter()
                .map(|h| HWND(h as *mut core::ffi::c_void))
                .filter(|h| {
                    parent.is_some() || {
                        let mut pid = 0u32;
                        // SAFETY: Writable pid; visibility query.
                        unsafe {
                            GetWindowThreadProcessId(*h, Some(&mut pid));
                            pid == me && IsWindowVisible(*h).as_bool()
                        }
                    }
                })
                .collect()
        }

        /// A visible top-level window; `dialog` = a native message box (`#32770`).
        fn top(title: &str, dialog: bool) -> Option<HWND> {
            windows_of(None)
                .into_iter()
                .find(|h| text_of(*h) == title && (class_of(*h) == "#32770") == dialog)
        }

        fn child(parent: HWND, text: &str) -> HWND {
            windows_of(Some(parent))
                .into_iter()
                .find(|h| text_of(*h) == text)
                .unwrap_or_else(|| panic!("no child {text:?}"))
        }

        fn child_class(parent: HWND, class: &str) -> HWND {
            windows_of(Some(parent))
                .into_iter()
                .find(|h| class_of(*h).eq_ignore_ascii_case(class))
                .unwrap_or_else(|| panic!("no child of class {class}"))
        }

        fn alive(h: HWND) -> bool {
            // SAFETY: Handle validity and visibility queries.
            unsafe { IsWindow(Some(h)).as_bool() && IsWindowVisible(h).as_bool() }
        }

        fn enabled(h: HWND) -> bool {
            // SAFETY: Read-only window query.
            unsafe { IsWindowEnabled(h).as_bool() }
        }

        fn post(h: HWND, msg: u32, w: usize, l: isize) {
            // SAFETY: Plain value messages.
            unsafe { PostMessageW(Some(h), msg, WPARAM(w), LPARAM(l)).unwrap() };
        }

        /// A button click as the button itself reports it (`BN_CLICKED` to its container).
        fn click(form: HWND, text: &str) {
            let b = child(form, text);
            assert!(enabled(b), "{text:?} is disabled");
            // SAFETY: Parent and id queries of a live child.
            let (parent, id) = unsafe { (GetParent(b).unwrap(), GetDlgCtrlID(b)) };
            post(parent, WM_COMMAND, id as u16 as usize, b.0 as isize);
        }

        fn key(form: HWND, vk: u16) {
            post(form, WM_KEYDOWN, usize::from(vk), 0x0001_0001);
        }

        fn answer(dialog: HWND, id: MESSAGEBOX_RESULT) {
            if id == IDOK {
                // An OK-only box ends with IDOK on close (the same as Esc).
                post(dialog, WM_CLOSE, 0, 0);
            } else {
                post(dialog, WM_COMMAND, id.0 as usize, 0);
            }
        }

        fn box_text(dialog: HWND) -> String {
            // SAFETY: 0xFFFF is the MessageBox text control.
            text_of(unsafe { GetDlgItem(Some(dialog), 0xFFFF) }.unwrap())
        }

        fn wait<T>(what: &str, secs: u64, f: impl Fn() -> Option<T>) -> T {
            let end = Instant::now() + Duration::from_secs(secs);
            loop {
                if let Some(v) = f() {
                    return v;
                }
                if Instant::now() >= end {
                    let open: Vec<_> = windows_of(None)
                        .into_iter()
                        .map(|h| {
                            let body = if class_of(h) == "#32770" {
                                box_text(h)
                            } else {
                                String::new()
                            };
                            format!("{:?}/{} {body:?}", text_of(h), class_of(h))
                        })
                        .collect();
                    panic!("timed out waiting for {what}; open windows: {open:?}");
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }

        fn gone(what: &str, h: HWND) {
            wait(what, 5, || (!alive(h)).then_some(()));
        }

        fn output(form: HWND) -> String {
            text_of(child_class(form, "Edit"))
        }

        /// Waits until the output contains `needle` `times` times.
        fn wait_output(form: HWND, needle: &str, times: usize) -> String {
            wait(needle, 10, || {
                let o = output(form);
                (o.matches(needle).count() >= times).then_some(o)
            })
        }

        fn wait_enabled(form: HWND, text: &str) {
            wait(text, 5, || enabled(child(form, text)).then_some(()));
        }

        fn focus_text(ui_thread: u32) -> String {
            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: Writable, sized GUITHREADINFO.
            unsafe { GetGUIThreadInfo(ui_thread, &mut info).unwrap() };
            text_of(info.hwndFocus)
        }

        fn client(h: HWND) -> (i32, i32) {
            let mut r = RECT::default();
            // SAFETY: Writable RECT.
            unsafe { GetClientRect(h, &mut r).unwrap() };
            (r.right, r.bottom)
        }

        fn rect(h: HWND) -> RECT {
            let mut r = RECT::default();
            // SAFETY: Writable RECT.
            unsafe { GetWindowRect(h, &mut r).unwrap() };
            r
        }

        fn dpi_of(h: HWND) -> u32 {
            // SAFETY: Read-only DPI query.
            unsafe { GetDpiForWindow(h) }
        }

        /// Screen capture through PowerShell `CopyFromScreen` (what the user sees).
        fn shot(h: HWND, name: &str) {
            // SAFETY: Z-order changes of our own window around the capture.
            unsafe {
                let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            std::thread::sleep(Duration::from_millis(350));
            let mut r = rect(h);
            // SAFETY: Reads the visible frame rectangle into a RECT.
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
            std::process::Command::new(
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            )
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .unwrap();
            // SAFETY: Restores normal z-order.
            unsafe {
                let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            println!("screenshot {}", path.display());
        }

        /// A `WM_DPICHANGED` from `from` to `dpi` (the window's real DPI does not change).
        fn synthetic_dpi(h: HWND, from: u32, dpi: u32) {
            let r = rect(h);
            let f = |v: i32| v * dpi as i32 / from as i32;
            let s = RECT {
                left: r.left,
                top: r.top,
                right: r.left + f(r.right - r.left),
                bottom: r.top + f(r.bottom - r.top),
            };
            // SAFETY: The RECT outlives the synchronous cross-thread send.
            unsafe {
                SendMessageW(
                    h,
                    WM_DPICHANGED,
                    Some(WPARAM((dpi | (dpi << 16)) as usize)),
                    Some(LPARAM(&s as *const RECT as isize)),
                );
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        /// Buttons left to right with their sizes.
        unsafe extern "system" fn monitor(
            _: windows::Win32::Graphics::Gdi::HMONITOR,
            _: windows::Win32::Graphics::Gdi::HDC,
            work: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            // SAFETY: `data` is the Vec passed by `monitors`; `work` is the monitor rectangle.
            unsafe { (*(data.0 as *mut Vec<RECT>)).push(*work) };
            BOOL(1)
        }

        /// Monitor rectangles (virtual-screen coordinates).
        fn monitors() -> Vec<RECT> {
            let mut list: Vec<RECT> = Vec::new();
            // SAFETY: Synchronous enumeration; the callback only pushes into `list`.
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::EnumDisplayMonitors(
                    None,
                    None,
                    Some(monitor),
                    LPARAM(&mut list as *mut Vec<RECT> as isize),
                );
            }
            list
        }

        fn move_to(h: HWND, x: i32, y: i32) {
            // SAFETY: Moves our own window; Windows sends WM_DPICHANGED across monitors.
            unsafe {
                SetWindowPos(
                    h,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .unwrap();
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        fn buttons(form: HWND, texts: &[&str]) -> String {
            let mut b: Vec<(i32, String, i32, i32)> = texts
                .iter()
                .map(|t| {
                    let r = rect(child(form, t));
                    (r.left, (*t).to_owned(), r.right - r.left, r.bottom - r.top)
                })
                .collect();
            b.sort();
            b.iter()
                .map(|(_, t, w, h)| format!("{t} {w}x{h}"))
                .collect::<Vec<_>>()
                .join(" | ")
        }

        fn save(name: &str, text: &str) {
            std::fs::write(Path::new(GOLDEN).join(name), text).unwrap();
        }

        // ------------------------------------------------------------ drivers

        type Log = Vec<String>;
        const DONE: &str = "Device cleaning process completed.";
        const FOOTER: [&str; 3] = ["Manage Whitelist", "Reclean", "Close"];

        fn drive_real(log: &mut Log) {
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let text = wait("scan result", 20, || {
                let o = output(form);
                (o.contains("No non-present devices were found.")
                    || o.contains("Devices that can be removed")
                    || o.contains("Error in Cleaning Process"))
                .then_some(o)
            });
            save("real-scan-output.txt", &text);
            log.push(format!(
                "real scan: dpi {}, client {:?}, output saved ({} chars)",
                dpi_of(form),
                client(form),
                text.len()
            ));
            if text.contains("No non-present devices were found.") {
                let seen = Instant::now();
                shot(form, &format!("clean-real-{}dpi", dpi_of(form)));
                gone("auto-close", form);
                log.push(format!(
                    "real scan: no ghosts; auto-closed {} ms after the line was seen \
                     (includes the screenshot)",
                    seen.elapsed().as_millis()
                ));
            } else {
                if let Some(confirm) = top("Confirm Device Removal", false) {
                    key(confirm, VK_ESCAPE.0);
                    wait_output(form, DONE, 1);
                }
                post(form, WM_CLOSE, 0, 0);
                gone("close", form);
                log.push("real scan: devices listed; answered No and closed".to_owned());
            }
        }

        fn drive_fake(log: &mut Log, ui_thread: u32, whitelist_file: &Path, valid: &[u8]) {
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let dpi = dpi_of(form);
            let s = |v: i32| v * dpi as i32 / 96;

            // -- 1. confirm dialog; Esc = No
            let confirm = wait("confirm", 10, || top("Confirm Device Removal", false));
            std::thread::sleep(Duration::from_millis(300));
            log.push(format!(
                "dpi {dpi}; cleaner client {:?} (C# {:?}); cleaner enabled during confirm: {}",
                client(form),
                (s(920), s(640)),
                enabled(form)
            ));
            log.push(format!(
                "confirm client {:?} (C# {:?}); focus {:?}; warning {:?}",
                client(confirm),
                (s(520), s(210)),
                focus_text(ui_thread),
                text_of(child(confirm, "Warning: This action cannot be undone"))
            ));
            let _ = child(confirm, "Remove 1 ghost devices?");
            log.push(format!(
                "confirm buttons: {}",
                buttons(confirm, &["Yes (Autoclose)", "Yes", "No"])
            ));
            shot(confirm, &format!("confirm-{dpi}dpi"));
            key(confirm, VK_ESCAPE.0);
            let text = wait_output(form, DONE, 1);
            save("fake-no.txt", &text);
            assert!(text.contains("\r\nOperation cancelled. No devices were removed.\r\n"));
            wait_enabled(form, "Reclean");
            assert!(enabled(child(form, "Close")) && enabled(child(form, "Manage Whitelist")));
            log.push(format!(
                "Esc = No: cancelled + completed; footer {}",
                buttons(form, &FOOTER)
            ));
            shot(form, &format!("clean-done-{dpi}dpi"));

            // Enter must activate the focused confirmation choice after Tab, rather than
            // always accepting Yes (Autoclose).
            click(form, "Reclean");
            let confirm = wait("confirm keyboard Yes", 10, || {
                top("Confirm Device Removal", false)
            });
            wait("initial confirm focus", 5, || {
                (focus_text(ui_thread) == "Yes (Autoclose)").then_some(())
            });
            key(child(confirm, "Yes (Autoclose)"), VK_TAB.0);
            wait("Yes focus", 5, || {
                (focus_text(ui_thread) == "Yes").then_some(())
            });
            key(child(confirm, "Yes"), VK_RETURN.0);
            wait_output(form, DONE, 1);
            std::thread::sleep(Duration::from_millis(1100));
            assert!(alive(form), "Enter on focused Yes chose Autoclose");
            wait_enabled(form, "Reclean");
            click(form, "Reclean");
            let confirm = wait("confirm keyboard No", 10, || {
                top("Confirm Device Removal", false)
            });
            wait("initial confirm focus", 5, || {
                (focus_text(ui_thread) == "Yes (Autoclose)").then_some(())
            });
            key(child(confirm, "Yes (Autoclose)"), VK_TAB.0);
            wait("Yes focus", 5, || {
                (focus_text(ui_thread) == "Yes").then_some(())
            });
            key(child(confirm, "Yes"), VK_TAB.0);
            wait("No focus", 5, || {
                (focus_text(ui_thread) == "No").then_some(())
            });
            key(child(confirm, "No"), VK_RETURN.0);
            let text = wait_output(form, DONE, 1);
            assert!(text.contains("Operation cancelled. No devices were removed."));
            assert!(!text.contains("[DRY RUN] SetupDiRemoveDevice"));
            wait_enabled(form, "Reclean");
            log.push(
                "Tab + Enter: focused Yes stays open; focused No cancels without removal"
                    .to_owned(),
            );

            // -- 2. Enter = Yes (Autoclose); Reclean inside the 1000 ms keeps the window (AD-29)
            click(form, "Reclean");
            let confirm = wait("confirm 2", 10, || top("Confirm Device Removal", false));
            std::thread::sleep(Duration::from_millis(200));
            key(confirm, VK_RETURN.0);
            let text = wait_output(form, DONE, 1);
            save("fake-yes-autoclose.txt", &text);
            assert!(text.contains("[DRY RUN] SetupDiRemoveDevice: USB Input Device\r\n"));
            assert_eq!(text.matches("[DRY RUN]").count(), 1);
            wait_enabled(form, "Reclean");
            click(form, "Reclean");
            let confirm = wait("confirm 3", 10, || top("Confirm Device Removal", false));
            std::thread::sleep(Duration::from_millis(1500));
            assert!(alive(form), "the auto-close timer survived Reclean");
            log.push(
                "Enter = Yes (Autoclose): one dry-run line; Reclean within 1 s kept the window"
                    .to_owned(),
            );

            // -- 3. plain Yes stays open
            click(confirm, "Yes");
            let text = wait_output(form, DONE, 1);
            save("fake-yes.txt", &text);
            std::thread::sleep(Duration::from_millis(1500));
            assert!(alive(form), "plain Yes closed the window");
            log.push("Yes: dry-run lines, window stays open".to_owned());

            // -- 4. whitelist window: pre-check, Reset (dry run), Save (dry run), Esc
            wait_enabled(form, "Manage Whitelist");
            click(form, "Manage Whitelist");
            let wl = wait("whitelist", 10, || top("Manage Device Whitelist", false));
            std::thread::sleep(Duration::from_millis(300));
            // SAFETY: Item count query of the list box.
            let count =
                unsafe { SendMessageW(child_class(wl, "ListBox"), LB_GETCOUNT, None, None).0 };
            log.push(format!(
                "whitelist: client {:?} (C# {:?}), items {count}, cleaner enabled {}, focus {:?}, \
                 footer {}",
                client(wl),
                (s(920), s(640)),
                enabled(form),
                focus_text(ui_thread),
                buttons(wl, &["Reset Whitelist", "Save Whitelist", "Cancel"])
            ));
            assert_eq!(count, 4);
            shot(wl, &format!("whitelist-{dpi}dpi"));
            click(wl, "Reset Whitelist");
            let ask = wait("Confirm Reset", 5, || top("Confirm Reset", true));
            log.push(format!("reset prompt: {:?}", box_text(ask)));
            answer(ask, IDYES);
            let dry = wait("reset dry run", 5, || top("Manage Device Whitelist", true));
            let reset_text = box_text(dry);
            answer(dry, IDOK);
            gone("reset box", dry);
            click(wl, "Save Whitelist");
            let dry = wait("save dry run", 5, || top("Manage Device Whitelist", true));
            let save_text = box_text(dry);
            shot(dry, "whitelist-save-dry-run");
            answer(dry, IDOK);
            gone("save box", dry);
            std::thread::sleep(Duration::from_millis(300));
            assert!(alive(wl), "a dry-run save closed the whitelist window");
            log.push(format!(
                "reset: {reset_text:?}; save: {save_text:?}; window stays open"
            ));
            assert_eq!(reset_text, "[DRY RUN] Reset device whitelist");
            assert_eq!(save_text, "[DRY RUN] Save device whitelist");
            assert_eq!(
                std::fs::read(whitelist_file).unwrap(),
                valid,
                "file changed"
            );
            key(wl, VK_ESCAPE.0);
            gone("whitelist", wl);
            assert!(!output(form).contains("Device whitelist has been updated."));
            log.push("Esc = Cancel closes the whitelist; no 'updated' line".to_owned());

            // -- 5. close while busy, No: the scan result that arrived meanwhile still shows
            SLOW_SCAN.store(true, Ordering::SeqCst);
            click(form, "Reclean");
            post(form, WM_CLOSE, 0, 0);
            let ask = wait("Confirm Exit", 5, || top("Confirm Exit", true));
            log.push(format!("X while busy: {:?}", box_text(ask)));
            std::thread::sleep(Duration::from_millis(900)); // the scan ends behind the box
            assert!(top("Confirm Device Removal", false).is_none());
            answer(ask, IDNO);
            let confirm = wait("confirm after No", 10, || {
                top("Confirm Device Removal", false)
            });
            key(confirm, VK_ESCAPE.0);
            wait_output(form, DONE, 1);
            log.push("Confirm Exit No: window stays, queued scan result shown after it".to_owned());

            // -- 6. real monitor moves (each sends WM_DPICHANGED); on a monitor above
            // 96 DPI also the confirm and whitelist windows and a synthetic 192 DPI.
            let home = rect(form);
            let mut seen = vec![dpi];
            for (i, m) in monitors().iter().enumerate() {
                move_to(form, m.left + 20, m.top + 20);
                let d = dpi_of(form);
                log.push(format!(
                    "monitor {i}: dpi {d}, client {:?} (C# {:?}), footer {}",
                    client(form),
                    (920 * d as i32 / 96, 640 * d as i32 / 96),
                    buttons(form, &FOOTER)
                ));
                if seen.contains(&d) {
                    continue;
                }
                seen.push(d);
                shot(form, &format!("clean-done-{d}dpi"));
                click(form, "Reclean");
                let confirm = wait("confirm (dpi)", 10, || top("Confirm Device Removal", false));
                std::thread::sleep(Duration::from_millis(300));
                log.push(format!(
                    "monitor {i}: confirm dpi {}, client {:?}, buttons {}",
                    dpi_of(confirm),
                    client(confirm),
                    buttons(confirm, &["Yes (Autoclose)", "Yes", "No"])
                ));
                shot(confirm, &format!("confirm-{d}dpi"));
                key(confirm, VK_ESCAPE.0);
                wait_output(form, DONE, 1);
                wait_enabled(form, "Manage Whitelist");
                click(form, "Manage Whitelist");
                let wl = wait("whitelist (dpi)", 10, || {
                    top("Manage Device Whitelist", false)
                });
                std::thread::sleep(Duration::from_millis(300));
                log.push(format!(
                    "monitor {i}: whitelist dpi {}, client {:?}, footer {}",
                    dpi_of(wl),
                    client(wl),
                    buttons(wl, &["Reset Whitelist", "Save Whitelist", "Cancel"])
                ));
                shot(wl, &format!("whitelist-{d}dpi"));
                key(wl, VK_ESCAPE.0);
                gone("whitelist (dpi)", wl);
                for to in [192, d] {
                    synthetic_dpi(form, if to == d { 192 } else { d }, to);
                    log.push(format!(
                        "monitor {i}: synthetic {to} dpi: client {:?}, footer {}",
                        client(form),
                        buttons(form, &FOOTER)
                    ));
                    if to == 192 {
                        shot(form, "clean-192-synthetic");
                    }
                }
            }
            move_to(form, home.left, home.top);

            // -- 7. corrupt whitelist file (AD-26)
            SLOW_SCAN.store(false, Ordering::SeqCst);
            std::fs::write(whitelist_file, b"null").unwrap();
            click(form, "Reclean");
            let err = wait("error box", 10, || top("Error", true));
            let err_text = box_text(err);
            answer(err, IDOK);
            let text = wait_output(form, "Error in Cleaning Process:", 1);
            save("fake-corrupt-whitelist.txt", &text);
            assert!(text.contains(crate::clean::whitelist::READ_FAILURE));
            assert!(top("Confirm Device Removal", false).is_none());
            wait_enabled(form, "Manage Whitelist");
            log.push(format!("corrupt whitelist: box {err_text:?}"));
            std::fs::write(whitelist_file, valid).unwrap();

            // -- 8. close while busy, Yes
            SLOW_SCAN.store(true, Ordering::SeqCst);
            click(form, "Reclean");
            post(form, WM_CLOSE, 0, 0);
            let ask = wait("Confirm Exit 2", 5, || top("Confirm Exit", true));
            answer(ask, IDYES);
            gone("cleaner", form);
            std::thread::sleep(Duration::from_millis(900));
            assert!(top("Confirm Device Removal", false).is_none());
            log.push("Confirm Exit Yes: window closed, late scan result dropped".to_owned());
            SLOW_SCAN.store(false, Ordering::SeqCst);
        }

        fn drive_auto_close(log: &mut Log) {
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let confirm = wait("confirm", 10, || top("Confirm Device Removal", false));
            std::thread::sleep(Duration::from_millis(200));
            click(confirm, "Yes (Autoclose)");
            wait_output(form, DONE, 1);
            let seen = Instant::now();
            gone("auto-close", form);
            log.push(format!(
                "Yes (Autoclose): closed {} ms after 'completed' was seen",
                seen.elapsed().as_millis()
            ));
        }

        fn drive_owner_close(log: &mut Log) {
            let owner = wait("cancellation owner", 10, || {
                top("WP-14 cancellation owner", false)
            });
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let confirm = wait("confirm", 10, || top("Confirm Device Removal", false));
            click(confirm, "Yes");
            wait_output(form, "Waiting for owner close.", 1);
            // WM_CLOSE on the owner destroys its owned cleaner without a cleaner CloseRequest.
            post(owner, WM_CLOSE, 0, 0);
            gone("owner", owner);
            gone("owned cleaner", form);
            wait("removal worker", 5, || {
                REMOVAL_DONE.load(Ordering::SeqCst).then_some(())
            });
            assert!(
                REMOVAL_CANCELLED.load(Ordering::SeqCst),
                "owner destruction left removal uncancelled"
            );
            log.push(
                "owner destruction during removal: cleaner destroyed, worker cancelled".to_owned(),
            );
        }

        /// The fabricated devices without the removable one, or only the
        /// whitelisted Absent one.
        fn subset(only_whitelisted: bool) -> Result<Found, String> {
            let mut found = fake_scan()?;
            found.devices = if only_whitelisted {
                vec![found.devices[1].clone()]
            } else {
                found.devices[1..].to_vec()
            };
            Ok(found)
        }

        fn none_removable() -> Result<Found, String> {
            subset(false)
        }

        fn all_whitelisted() -> Result<Found, String> {
            subset(true)
        }

        fn drive_none_removable(log: &mut Log) {
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let text = wait_output(form, "No devices can be removed.", 1);
            std::thread::sleep(Duration::from_millis(1500));
            assert!(alive(form), "auto-closed with Unclear devices listed");
            assert!(top("Confirm Device Removal", false).is_none());
            assert!(!text.contains("All devices are whitelisted"));
            save("fake-none-removable.txt", &text);
            wait_enabled(form, "Close");
            click(form, "Close");
            gone("cleaner", form);
            log.push("removable 0 with an Unclear device: new line, no confirm, no auto-close; Close button closes".to_owned());
        }

        fn drive_all_whitelisted(log: &mut Log) {
            let form = wait("Device Cleaning", 10, || top("Device Cleaning", false));
            let text = wait_output(form, "All devices are whitelisted.", 1);
            let seen = Instant::now();
            save("fake-all-whitelisted.txt", &text);
            gone("auto-close", form);
            log.push(format!(
                "all whitelisted: C# line, no confirm, auto-closed {} ms after the line was seen",
                seen.elapsed().as_millis()
            ));
        }

        fn drive_admin(log: &mut Log) {
            let found = wait("admin box or window", 10, || {
                top("Administrator Rights Required", true)
                    .map(|h| (h, true))
                    .or_else(|| top("Device Cleaning", false).map(|h| (h, false)))
            });
            match found {
                (b, true) => {
                    log.push(format!("not admin: box {:?}; no window", box_text(b)));
                    answer(b, IDOK);
                }
                (form, false) => {
                    log.push("shell is admin: the window opened; closed it".to_owned());
                    wait("scan", 20, || enabled(child(form, "Close")).then_some(()));
                    post(form, WM_CLOSE, 0, 0);
                    gone("cleaner", form);
                }
            }
        }

        /// Runs `drive` on a second thread while this thread runs the window.
        fn with_driver(
            scanner: fn() -> Result<Found, String>,
            drive: impl FnOnce() + Send + 'static,
        ) {
            with_driver_ui(move || run(HWND::default(), scanner), drive);
        }

        /// Runs `drive` on a second thread while this thread runs `ui`.
        fn with_driver_ui(ui: impl FnOnce(), drive: impl FnOnce() + Send + 'static) {
            let driver = std::thread::spawn(move || {
                if let Err(panic) = crate::win::catch_panic(drive) {
                    // The UI thread is blocked in the window; end the run loudly.
                    eprintln!("WP-14 driver failed: {panic}");
                    std::process::exit(101);
                }
            });
            ui();
            driver.join().unwrap();
        }

        #[test]
        #[ignore = "opens real windows; run by hand (WP-14)"]
        fn wp14_windows() {
            if !cfg!(debug_assertions) {
                panic!("debug build only (dry-run guard)");
            }
            assert!(
                std::env::var_os("HWID_ALLOW_DESTRUCTIVE").is_none(),
                "HWID_ALLOW_DESTRUCTIVE must not be set"
            );
            let golden = Path::new(GOLDEN);
            let temp = golden.join("temp");
            std::fs::create_dir_all(&temp).unwrap();
            // SAFETY: Set before any window or worker thread of this test exists; only this
            // test runs (name filter), so no other thread reads the environment meanwhile.
            unsafe {
                std::env::set_var("TMP", &temp);
                std::env::set_var("TEMP", &temp);
            }
            let whitelist_file = temp.join("hwid_device_whitelist.json");
            assert_eq!(
                std::env::temp_dir().join("hwid_device_whitelist.json"),
                whitelist_file,
                "throwaway TEMP for the whitelist"
            );
            let devices = fabricated();
            let entries: Vec<serde_json::Value> = [&devices[1], &devices[3]]
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "Name": d.name, "Description": d.description,
                        "HardwareId": d.hardware_id, "Class": d.class
                    })
                })
                .collect();
            let valid = serde_json::to_vec_pretty(&entries).unwrap();
            std::fs::write(&whitelist_file, &valid).unwrap();

            assert!(crate::ui::dpi::set_per_monitor_v2_for_tests());
            assert!(activate_comctl6());
            // SAFETY: Plain thread id query.
            let ui_thread = unsafe { GetCurrentThreadId() };
            let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            let l = log.clone();
            with_driver(real_scan, move || drive_real(&mut l.lock().unwrap()));
            let l = log.clone();
            let (wf, v) = (whitelist_file.clone(), valid.clone());
            with_driver(fake_scan, move || {
                drive_fake(&mut l.lock().unwrap(), ui_thread, &wf, &v)
            });
            let l = log.clone();
            with_driver(fake_scan, move || drive_auto_close(&mut l.lock().unwrap()));
            let l = log.clone();
            with_driver(none_removable, move || {
                drive_none_removable(&mut l.lock().unwrap())
            });
            let l = log.clone();
            with_driver(all_whitelisted, move || {
                drive_all_whitelisted(&mut l.lock().unwrap())
            });
            let l = log.clone();
            with_driver_ui(
                || {
                    let owner = Form::create(
                        HWND::default(),
                        FormSpec::new(
                            "WP-14 cancellation owner",
                            WindowSize::Client(theme::CLEAN_DEVICES_CLIENT_SIZE),
                        ),
                        Vec::new(),
                        |_, _| true,
                    )
                    .unwrap();
                    owner.show();
                    run(owner.hwnd(), owner_close_scan);
                    owner.destroy();
                },
                move || drive_owner_close(&mut l.lock().unwrap()),
            );
            let l = log.clone();
            with_driver_ui(
                || show(HWND::default()),
                move || drive_admin(&mut l.lock().unwrap()),
            );

            let log = log.lock().unwrap();
            for line in log.iter() {
                println!("RESULT {line}");
            }
            save("wp14-results.txt", &log.join("\r\n"));
            assert_eq!(std::fs::read(&whitelist_file).unwrap(), valid);
        }

        // ------------------------------------------------------------ comctl v6

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

        /// The test exe has no manifest; activate Common Controls 6 like the app manifest.
        fn activate_comctl6() -> bool {
            let path = Path::new(GOLDEN).join("comctl6.manifest");
            let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
</assembly>"#;
            std::fs::write(&path, xml).unwrap();
            let source = crate::win::wide::to_wide(&path.to_string_lossy());
            let ctx = ActCtx {
                cb_size: std::mem::size_of::<ActCtx>() as u32,
                flags: 0,
                source: source.as_ptr(),
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
                if h.is_null() || h as isize == -1 {
                    return false;
                }
                let mut cookie = 0usize;
                ActivateActCtx(h, &mut cookie) != 0
            }
        }
    }
}
