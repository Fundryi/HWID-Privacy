//! Owned by WP-16: log cleaning window.

use super::{
    controls::{ButtonSpec, Ctl, EditSpec},
    layout::{FlowDir, Kind, Node, Track},
    msgbox::{self, Answer, Buttons, Icon},
    theme,
    window::{self, Event, Form, FormSpec, FormStyle, WindowSize},
};
use crate::{
    clean::eventlog::{self, CleanOutcome},
    win::{self, process::Cancel},
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::Foundation::HWND;

const OUTPUT: u16 = 1;
const CLOSE: u16 = 2;
const OUTPUT_PANEL: u16 = 3;
const ACTION_PANEL: u16 = 4;
const UPDATE_TIMER: usize = 1;
const CLOSE_WAIT: Duration = Duration::from_secs(10);
const ADMIN_TEXT: &str = "This operation requires administrative privileges. Please run the application as administrator.";
const STOP_TEXT: &str = "Log cleaning is still running. Force-stop and close this window?";

type Lines = Arc<Mutex<Vec<String>>>;
type Cleaner = fn(Cancel, &(dyn Fn(&str) + Sync)) -> CleanOutcome;

struct State {
    running: Cell<bool>,
    closing_since: Cell<Option<Instant>>,
    cancel: Cancel,
    lines: Lines,
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Shows the modal event log cleaning window.
pub fn show(owner: HWND) {
    // C# parity: CleanLogsForm.cs:154-166 (check before starting the cleaner).
    if !win::security::is_admin() {
        msgbox::show(
            owner,
            ADMIN_TEXT,
            "Administrator Rights Required",
            Buttons::Ok,
            Icon::Warning,
        );
        return;
    }
    if let Err(error) = run(owner, eventlog::clean) {
        win::record(error.clone());
        window::show_error(
            owner,
            &format!("Error opening log cleaning: {error}"),
            "Log Cleaning Error",
        );
    }
}

fn spec() -> FormSpec {
    // C# parity: CleanLogsForm.cs:13-16,79-89 (MinimumSize is an outer size).
    let mut spec = FormSpec::new(
        "Log Cleaning",
        WindowSize::Client(theme::CLEAN_LOGS_CLIENT_SIZE),
    );
    spec.min = Some(theme::CLEAN_LOGS_MIN_SIZE);
    spec.style = FormStyle::Sizable {
        maximize: true,
        minimize: false,
    };
    spec
}

fn layout() -> Vec<Node> {
    // C# parity: CleanLogsForm.cs:91-149 (native read-only EDIT, Fill panels and table).
    let output = Node::leaf(
        OUTPUT,
        Ctl::Edit(EditSpec::new(
            theme::CLEANER_OUTPUT_FONT,
            theme::TEXT_BOX_TEXT,
            theme::CLEANER_OUTPUT_BACKGROUND,
        )),
    )
    .fill();
    let output_panel = Node::panel(vec![output])
        .id(OUTPUT_PANEL)
        .fill()
        .padding(theme::OUTPUT_PANEL_PADDING)
        .back(theme::MAIN_BACKGROUND)
        .cell(0, 0);
    // C# parity: CleanLogsForm.cs:104-115; Buttons.cs:30 overrides the constructor padding.
    // DESIGN.md 6: the only button, so it is the primary one.
    let close = Node::leaf(CLOSE, Ctl::Button(ButtonSpec::primary("Close")))
        .auto_size()
        .min(theme::CLEAN_LOGS_CLOSE_MIN)
        .padding(theme::SHARED_BUTTON_PADDING)
        .margin(theme::ACTION_BUTTON_MARGIN);
    let actions = Node::flow(FlowDir::RightToLeft, false, vec![close])
        .id(ACTION_PANEL)
        .fill()
        .padding(theme::ACTION_PANEL_PADDING)
        .back(theme::BUTTON_PANEL_BACKGROUND)
        .cell(0, 1);
    vec![
        Node::table(
            vec![Track::Percent(100.0)],
            vec![
                Track::Percent(100.0),
                Track::Absolute(theme::ACTION_ROW_HEIGHT),
            ],
            vec![output_panel, actions],
        )
        .fill()
        .back(theme::MAIN_BACKGROUND),
    ]
}

fn drain(form: &Form, lines: &Lines) {
    let batch = std::mem::take(&mut *lines.lock().unwrap_or_else(|poison| poison.into_inner()));
    if !batch.is_empty() {
        let chunks: Vec<&str> = batch.iter().map(String::as_str).collect();
        // C# parity: CleanLogsForm.cs:43-45 (append CRLF and follow the caret).
        form.edit_append_batch(OUTPUT, &chunks);
    }
}

fn button_text(form: &Form, text: &str) {
    form.set_text(CLOSE, text);
    // Keep autosizing and DPI rescaling in sync with the native button's caption.
    form.with_tree(|tree| {
        if let Some(node) = tree.find_mut(CLOSE)
            && let Kind::Leaf(Ctl::Button(button)) = &mut node.kind
        {
            button.text = text.to_owned();
        }
    });
}

fn finish(form: &Form, state: &State, outcome: CleanOutcome) {
    drain(form, &state.lines);
    // C# parity: CleanLogsForm.cs:185-210 (including blank lines).
    match outcome {
        CleanOutcome::Done => form.edit_append_batch(
            OUTPUT,
            &[
                "\r\nLog cleaning process completed.\r\n",
                "Review the summary above. This window will stay open.\r\n",
            ],
        ),
        CleanOutcome::Cancelled => {
            form.edit_append_batch(OUTPUT, &["\r\nLog cleaning canceled by user.\r\n"])
        }
        CleanOutcome::Failed(error) => {
            form.edit_append_batch(
                OUTPUT,
                &[&format!("Error in Log Cleaning Process: {error}\r\n")],
            );
            // Raised after the worker result: the box belongs to the active window (8.5).
            window::show_error(
                msgbox::active_window(),
                &format!("Error during log cleaning process: {error}"),
                "Error",
            );
            form.edit_append_batch(
                OUTPUT,
                &["Log cleaning encountered an error. Review details above.\r\n"],
            );
        }
    }
    state.running.set(false);
    form.kill_timer(UPDATE_TIMER);
    button_text(form, "Close");
    form.relayout();
    if state.closing_since.get().is_some() {
        form.close();
    }
}

fn request_close(form: &Form, state: &State) -> bool {
    if !state.running.get() {
        return true;
    }
    if state.closing_since.get().is_some() {
        return false;
    }
    // C# parity: CleanLogsForm.cs:218-260 (the button and X use the same question).
    if msgbox::show(
        form.hwnd(),
        STOP_TEXT,
        "Confirm Stop",
        Buttons::YesNo,
        Icon::Warning,
    ) != Answer::Yes
    {
        return false;
    }
    state.closing_since.set(Some(Instant::now()));
    state.cancel.cancel();
    // The modal question pumps worker messages; cleaning may have finished while it was open.
    !state.running.get()
}

fn run(owner: HWND, cleaner: Cleaner) -> win::Result<()> {
    let state = Rc::new(State {
        running: Cell::new(false),
        closing_since: Cell::new(None),
        cancel: Cancel::new(),
        lines: Arc::new(Mutex::new(Vec::new())),
    });
    window::run_modal(
        owner,
        spec(),
        layout(),
        move |form, event| match win::catch_panic(|| handle(form, &state, event, cleaner)) {
            Ok(allow) => allow,
            Err(panic) => {
                state.cancel.cancel();
                finish(
                    form,
                    &state,
                    CleanOutcome::Failed(
                        win::Error::msg("Log Cleaning Process", panic).to_string(),
                    ),
                );
                true
            }
        },
    )
}

fn handle(form: &Form, state: &State, event: Event, cleaner: Cleaner) -> bool {
    match event {
        Event::Created => {
            // C# parity: CleanLogsForm.cs:169-183 (auto-start, enabled stop button, opening lines).
            state.running.set(true);
            button_text(form, "Stop & Close");
            form.edit_set_text(OUTPUT, "");
            form.edit_append_batch(
                OUTPUT,
                &[
                    "Starting event log cleanup...\r\n",
                    "Enumerating and clearing logs. This may take a moment.\r\n\r\n",
                ],
            );
            form.relayout();
            form.next_generation();
            form.set_timer(UPDATE_TIMER, 50);
            let Some(poster) = form.poster() else {
                state.cancel.cancel();
                return true;
            };
            let cancel = state.cancel.clone();
            let lines = state.lines.clone();
            if let Err(error) = thread::Builder::new()
                .name("log-cleaning-window".into())
                .spawn(move || {
                    let outcome = win::catch_panic(|| {
                        let status = |line: &str| {
                            lines
                                .lock()
                                .unwrap_or_else(|poison| poison.into_inner())
                                .push(format!("{line}\r\n"));
                        };
                        let outcome = cleaner(cancel, &status);
                        // C# parity: SystemCleaningService.cs:40-43 (facade error before form error).
                        if let CleanOutcome::Failed(error) = &outcome {
                            status(&format!("Error in Cleaning Process: {error}"));
                        }
                        outcome
                    })
                    .unwrap_or_else(|panic| {
                        CleanOutcome::Failed(
                            win::Error::msg("Log Cleaning Process", panic).to_string(),
                        )
                    });
                    if !poster.post(outcome) {
                        win::record(win::Error::msg(
                            "Log Cleaning Process",
                            "window closed before completion",
                        ));
                    }
                })
            {
                finish(
                    form,
                    state,
                    CleanOutcome::Failed(
                        win::Error::msg("Start log cleaning worker", error.to_string()).to_string(),
                    ),
                );
            }
        }
        Event::Timer(UPDATE_TIMER) => {
            drain(form, &state.lines);
            if state
                .closing_since
                .get()
                .is_some_and(|start| start.elapsed() >= CLOSE_WAIT)
            {
                state.cancel.cancel();
                win::record(win::Error::msg(
                    "Log Cleaning Process",
                    "stop wait timed out after 10000ms",
                ));
                form.destroy();
            }
        }
        Event::Worker(value) => {
            if let Ok(outcome) = value.downcast::<CleanOutcome>() {
                finish(form, state, *outcome);
            }
        }
        Event::Click(CLOSE) => form.close(),
        Event::CloseRequest => return request_close(form, state),
        Event::Resize { .. } => button_text(
            form,
            if state.running.get() {
                "Stop & Close"
            } else {
                "Close"
            },
        ),
        Event::Destroyed => {
            state.cancel.cancel();
            form.kill_timer(UPDATE_TIMER);
        }
        _ => {}
    }
    true
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    use windows::{
        Win32::{
            Foundation::{LPARAM, RECT, WPARAM},
            System::Threading::GetCurrentThreadId,
            UI::WindowsAndMessaging::{
                BM_CLICK, EnumThreadWindows, GetDlgItem, GetWindowRect, HWND_TOPMOST, IDNO, IDYES,
                IsWindow, PostMessageW, SC_CLOSE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WM_CLOSE,
                WM_SYSCOMMAND, WM_TIMER,
            },
        },
        core::BOOL,
    };

    fn wait_for(mut ready: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ready() {
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "UI step deadline"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn dialog(thread_id: u32, title: &str) -> Option<HWND> {
        struct Search<'a> {
            title: &'a str,
            found: Option<HWND>,
        }
        unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
            // SAFETY: EnumThreadWindows synchronously passes the live Search pointer below.
            let search = unsafe { &mut *(data.0 as *mut Search<'_>) };
            if super::super::controls::text(hwnd) == search.title {
                search.found = Some(hwnd);
            }
            BOOL(1)
        }
        let mut search = Search { title, found: None };
        // SAFETY: Enumerates only this test's UI thread, with a live callback context.
        unsafe {
            EnumThreadWindows(
                thread_id,
                Some(visit),
                LPARAM((&mut search as *mut Search<'_>) as isize),
            )
        }
        .expect("enumerate test dialogs");
        search.found
    }

    fn click(hwnd: HWND) {
        // SAFETY: Posts a pointer-free click to a button from this test's live window.
        unsafe { PostMessageW(Some(hwnd), BM_CLICK, WPARAM(0), LPARAM(0)) }.expect("test click");
    }

    fn screenshot(hwnd: HWND, name: &str) {
        let mut rect = RECT::default();
        // SAFETY: Raises and queries only this test's own window; rect is writable.
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            )
            .expect("raise test window");
            GetWindowRect(hwnd, &mut rect).expect("test window rectangle");
        }
        thread::sleep(Duration::from_millis(100));
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; $image=[System.Drawing.Bitmap]::new({},{}); $graphics=[System.Drawing.Graphics]::FromImage($image); $graphics.CopyFromScreen({},{},0,0,$image.Size); $image.Save('D:/GIT/HWID-Privacy/app/rust/golden/wp-16/{name}.png'); $graphics.Dispose(); $image.Dispose()",
            rect.right - rect.left,
            rect.bottom - rect.top,
            rect.left,
            rect.top,
        );
        let result = win::process::run(
            &win::process::system32("WindowsPowerShell/v1.0/powershell.exe"),
            &["-NoProfile", "-NonInteractive", "-Command", &script],
            Duration::from_secs(5),
            &Cancel::new(),
        )
        .expect("screen capture");
        assert_eq!(result.code, 0, "{}", result.stderr);
    }

    // The real dry-run finishes quickly. Hold only its completion so No/Yes and X can be
    // exercised reproducibly; the production window always calls eventlog::clean directly.
    fn interactive_clean(cancel: Cancel, status: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
        let outcome = eventlog::clean(cancel.clone(), status);
        if matches!(outcome, CleanOutcome::Done) {
            let start = Instant::now();
            while !cancel.is_cancelled() && start.elapsed() < Duration::from_secs(240) {
                thread::sleep(Duration::from_millis(10));
            }
            if cancel.is_cancelled() {
                thread::sleep(Duration::from_millis(200));
                status("");
                status("Log cleaning canceled by user.");
                return CleanOutcome::Cancelled;
            }
        }
        outcome
    }

    fn failed_clean(_: Cancel, _: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
        CleanOutcome::Failed(
            win::Error {
                op: "WP-16 failure rehearsal",
                code: 5,
                detail: "Access is denied.".into(),
            }
            .to_string(),
        )
    }

    fn panicking_clean(_: Cancel, _: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
        panic!("WP-16 caught worker panic rehearsal");
    }

    fn unresponsive_clean(cancel: Cancel, status: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
        let outcome = eventlog::clean(cancel.clone(), status);
        let start = Instant::now();
        while !cancel.is_cancelled() && start.elapsed() < Duration::from_secs(30) {
            thread::sleep(Duration::from_millis(10));
        }
        // No OS operation is pending: only delay delivery to exercise the UI watchdog.
        thread::sleep(Duration::from_secs(11));
        outcome
    }

    #[test]
    #[ignore = "opens the debug dry-run window; private owner-PC UI verification"]
    fn wp16_test_window() {
        assert_ne!(
            std::env::var("HWID_ALLOW_DESTRUCTIVE").ok().as_deref(),
            Some("1")
        );
        assert!(
            !win::security::is_admin(),
            "test shell must not be elevated"
        );
        assert!(super::super::dpi::set_per_monitor_v2_for_tests());
        let case = std::env::var("HWID_WP16_TEST_CASE").unwrap_or_else(|_| "done".into());
        let directory = PathBuf::from("D:/GIT/HWID-Privacy/app/rust/golden/wp-16");
        fs::create_dir_all(&directory).expect("private evidence directory");
        // SAFETY: The current test thread owns every window created below.
        let ui_thread = unsafe { GetCurrentThreadId() };
        if case == "admin" {
            let driver = thread::spawn(move || {
                wait_for(|| dialog(ui_thread, "Administrator Rights Required").is_some());
                let box_hwnd =
                    dialog(ui_thread, "Administrator Rights Required").expect("admin box");
                assert!(dialog(ui_thread, "Log Cleaning").is_none());
                screenshot(box_hwnd, "admin-refusal");
                // SAFETY: Closing an OK-only message box dismisses it; only our test dialog is targeted.
                unsafe { PostMessageW(Some(box_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }
                    .expect("dismiss admin box");
            });
            show(HWND::default());
            driver.join().expect("admin driver");
            println!("RESULT admin refusal: no Log Cleaning window created");
            return;
        }
        let cleaner: Cleaner = match case.as_str() {
            "stop" | "x" | "destroy" => interactive_clean,
            "failed" => failed_clean,
            "panic" => panicking_clean,
            "timeout" => unresponsive_clean,
            "done" => eventlog::clean,
            _ => panic!("unknown UI rehearsal case"),
        };
        let state = Rc::new(State {
            running: Cell::new(false),
            closing_since: Cell::new(None),
            cancel: Cancel::new(),
            lines: Arc::new(Mutex::new(Vec::new())),
        });
        let observed = state.clone();
        let started = Instant::now();
        let cancelled_at = Cell::new(None);
        let label = case.clone();
        let full_output = case == "done";
        let form = Form::create(HWND::default(), spec(), layout(), move |form, event| {
            let completion = matches!(&event, Event::Worker(_));
            let destroyed = matches!(&event, Event::Destroyed);
            if matches!(&event, Event::Timer(999)) {
                form.destroy();
                return true;
            }
            let result = win::catch_panic(|| handle(form, &observed, event, cleaner))
                .expect("handler must catch worker panics");
            if observed.closing_since.get().is_some() && cancelled_at.get().is_none() {
                cancelled_at.set(observed.closing_since.get());
            }
            if form.is_alive() {
                fs::write(
                    directory.join(format!("{label}-output.txt")),
                    form.text(OUTPUT),
                )
                .expect("private output");
            }
            let mut geometry = format!(
                "dpi={} client={:?} button={:?} running={} cancel={} elapsed_ms={}\r\n",
                form.dpi(),
                form.client_size(),
                form.text(CLOSE),
                observed.running.get(),
                observed.cancel.is_cancelled(),
                started.elapsed().as_millis(),
            );
            form.with_tree(|tree| {
                for id in [OUTPUT_PANEL, OUTPUT, ACTION_PANEL, CLOSE] {
                    geometry.push_str(&format!(
                        "id={id}: {:?}\r\n",
                        tree.find(id).expect("layout node").bounds
                    ));
                }
            });
            if destroyed {
                if let Some(time) = cancelled_at.get() {
                    geometry.push_str(&format!("close_wait_ms={}\r\n", time.elapsed().as_millis()));
                    assert!(time.elapsed() < Duration::from_secs(11), "bounded close");
                }
                assert!(observed.cancel.is_cancelled(), "destroy cancels the worker");
            }
            fs::write(directory.join(format!("{label}-geometry.txt")), geometry).expect("geometry");
            if full_output && completion && !observed.running.get() && form.is_alive() {
                assert!(
                    form.text(OUTPUT).encode_utf16().count() > 40_000,
                    "unlimited append"
                );
            }
            if started.elapsed() >= Duration::from_secs(45) {
                form.destroy();
                panic!("UI rehearsal deadline exceeded");
            }
            result
        })
        .expect("test window");
        form.show();
        let hwnd = form.hwnd().0 as isize;
        let close = form.control(CLOSE).expect("close button").0 as isize;
        let driver_case = case.clone();
        let driver = thread::spawn(move || {
            win::catch_panic(|| {
                let hwnd = HWND(hwnd as *mut core::ffi::c_void);
                let close = HWND(close as *mut core::ffi::c_void);
                if driver_case == "failed" || driver_case == "panic" {
                    wait_for(|| dialog(ui_thread, "Error").is_some());
                    let error = dialog(ui_thread, "Error").expect("error box");
                    screenshot(error, &format!("{driver_case}-error"));
                    // SAFETY: Closing an OK-only message box dismisses it; only our test dialog is targeted.
                    unsafe { PostMessageW(Some(error), WM_CLOSE, WPARAM(0), LPARAM(0)) }
                        .expect("dismiss error box");
                }
                if driver_case == "done" || driver_case == "failed" || driver_case == "panic" {
                    wait_for(|| super::super::controls::text(close) == "Close");
                    screenshot(hwnd, &format!("{driver_case}-final"));
                    click(close);
                } else {
                    wait_for(|| {
                        fs::read_to_string(
                            "D:/GIT/HWID-Privacy/app/rust/golden/wp-16/stop-output.txt",
                        )
                        .is_ok_and(|text| text.contains("CLEAN LOGS OVERVIEW"))
                            || driver_case != "stop"
                    });
                    screenshot(hwnd, &format!("{driver_case}-running"));
                    if driver_case == "destroy" {
                        // SAFETY: Test-only destruction request to our own handler.
                        unsafe { PostMessageW(Some(hwnd), WM_TIMER, WPARAM(999), LPARAM(0)) }
                            .expect("destroy request");
                    } else {
                        for answer in [IDNO, IDYES] {
                            if driver_case == "x" {
                                // SAFETY: SC_CLOSE follows the actual title-bar X path for our form.
                                unsafe {
                                    PostMessageW(
                                        Some(hwnd),
                                        WM_SYSCOMMAND,
                                        WPARAM(SC_CLOSE as usize),
                                        LPARAM(0),
                                    )
                                }
                                .expect("X");
                            } else {
                                click(close);
                            }
                            wait_for(|| dialog(ui_thread, "Confirm Stop").is_some());
                            let question =
                                dialog(ui_thread, "Confirm Stop").expect("stop question");
                            screenshot(question, &format!("{driver_case}-confirm-{}", answer.0));
                            // SAFETY: Standard Yes/No button of this test's own confirmation box.
                            click(unsafe { GetDlgItem(Some(question), answer.0) }.expect("answer"));
                            wait_for(|| dialog(ui_thread, "Confirm Stop").is_none());
                            if answer == IDNO {
                                // SAFETY: Read-only validity query on our form.
                                let alive = unsafe { IsWindow(Some(hwnd)) }.as_bool();
                                assert!(alive, "No keeps window open");
                                assert_eq!(super::super::controls::text(close), "Stop & Close");
                                screenshot(hwnd, &format!("{driver_case}-after-no"));
                            }
                        }
                    }
                }
                // SAFETY: Read-only validity queries until our own window is destroyed.
                wait_for(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
            })
            .expect("UI driver");
        });
        window::pump_until(|| !form.is_alive());
        driver.join().expect("UI driver thread");
        assert!(state.cancel.is_cancelled());
        println!(
            "RESULT {case}: window closed, cancellation signalled, elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
}
