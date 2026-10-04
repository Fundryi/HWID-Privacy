//! Real subprocess capture and lifetime regression checks.

use super::capture::decode_oem;
use super::*;
use std::{fs, io, sync::atomic::AtomicUsize};
use std::{path::PathBuf, thread, time::Instant};
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};

fn ps(script: &str) -> Output {
    run(
        &powershell(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ],
        Duration::from_secs(15),
        &Cancel::new(),
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn normal_run_captures_both_streams_and_quotes() {
    let output = ps(
        r#"[Console]::Out.Write("hello `"quoted`" C:\folder\"); [Console]::Error.Write('warning')"#,
    );
    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "hello \"quoted\" C:\\folder\\");
    assert_eq!(output.stderr, "warning");
}

#[test]
fn reads_more_than_one_megabyte_from_each_pipe() {
    let output =
        ps("[Console]::Out.Write(('x' * 1100000)); [Console]::Error.Write(('y' * 1100000))");
    assert_eq!(output.code, 0);
    assert_eq!(output.stdout, "x".repeat(1_100_000));
    assert_eq!(output.stderr, "y".repeat(1_100_000));
}

#[test]
fn missing_and_relative_executables_have_exact_start_text() {
    for exe in [
        system32("__HWIDChecker_missing_process_test.exe"),
        PathBuf::from("cmd.exe"),
    ] {
        assert!(!exe.exists());
        let error = run(&exe, &[], Duration::from_secs(2), &Cancel::new())
            .err()
            .unwrap();
        assert_eq!(error, format!("Failed to start process: {}", exe.display()));
    }
}

#[test]
fn nonzero_exit_synthesizes_empty_or_whitespace_stderr() {
    let output = run(
        &system32("cmd.exe"),
        &["/d", "/s", "/c", "exit /b 13"],
        Duration::from_secs(5),
        &Cancel::new(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(output.code, 13);
    assert_eq!(output.stderr, "Process exited with code 13.");
    let output = ps("[Console]::Error.Write(\" `t`r`n\"); exit 13");
    assert_eq!(output.code, 13);
    assert_eq!(output.stderr, "Process exited with code 13.");
}

#[test]
fn nonzero_exit_preserves_stderr_and_signed_code() {
    let output = ps("[Console]::Error.Write('failure detail'); exit -1");
    assert_eq!(output.code, -1);
    assert_eq!(output.stderr, "failure detail");
}

#[test]
fn oem_decoding_preserves_embedded_nul() {
    assert_eq!(decode_oem(b"first\0last\r\n").unwrap(), "first\0last\r\n");
    assert_eq!(decode_oem(b"").unwrap(), "");
}

#[test]
fn system32_is_absolute_and_rejects_path_escape() {
    let exe = system32("cmd.exe");
    assert!(exe.is_absolute());
    assert!(exe.is_file());
    assert_eq!(exe.file_name().unwrap(), "cmd.exe");
    let ps = powershell();
    assert!(ps.is_file());
    assert_eq!(
        ps,
        exe.parent()
            .unwrap()
            .join(r"WindowsPowerShell\v1.0\powershell.exe")
    );
    assert_eq!(system32("WindowsPowerShell/v1.0/powershell.exe"), ps);
    for name in [
        "",
        ".",
        "..",
        "../cmd.exe",
        "sub\\..\\cmd.exe",
        r"\cmd.exe",
        "/cmd.exe",
        r"\\server\share\cmd.exe",
        "cmd.exe:stream",
        r"C:cmd.exe",
        "C:\\cmd.exe",
        "cmd.exe\0",
    ] {
        let path = system32(name);
        assert!(path.as_os_str().is_empty());
        assert_eq!(
            run(&path, &[], Duration::from_secs(1), &Cancel::new())
                .err()
                .unwrap(),
            "Failed to start process: "
        );
    }
}

#[test]
fn cloned_cancel_prevents_start_with_exact_csharp_text() {
    let cancel = Cancel::new();
    let shared = cancel.clone();
    assert!(!shared.is_cancelled());
    cancel.cancel();
    assert!(shared.is_cancelled());
    let exe = system32("cmd.exe");
    let error = run(&exe, &["/c", "exit 0"], Duration::from_secs(5), &shared)
        .err()
        .unwrap();
    assert_eq!(
        error,
        format!("Process canceled: {} /c exit 0", exe.display())
    );
}

// Private temporary markers contain only subprocess PIDs, never hardware IDs.
struct TreeProbe {
    marker: PathBuf,
    cancel: Cancel,
}
impl TreeProbe {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self {
            marker: std::env::temp_dir().join(format!(
                "hwid-process-test-{}-{}.txt",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )),
            cancel: Cancel::new(),
        }
    }

    fn script(&self, exit_parent: bool) -> String {
        let ping = system32("ping.exe")
            .display()
            .to_string()
            .replace('\'', "''");
        let marker = self.marker.display().to_string().replace('\'', "''");
        format!(
            "$child = Start-Process -FilePath '{ping}' -ArgumentList '-n 60 127.0.0.1' -NoNewWindow -PassThru; \
            [IO.File]::WriteAllText('{marker}', ('{{0}} {{1}}' -f $PID,$child.Id)); {}",
            if exit_parent {
                "exit 0"
            } else {
                "Start-Sleep -Seconds 60"
            }
        )
    }

    fn handles(&self, exit_parent: bool) -> Vec<OwnedHandle> {
        let started = Instant::now();
        loop {
            match fs::read_to_string(&self.marker) {
                Ok(text) => {
                    let pids: Vec<u32> = text
                        .split_whitespace()
                        .map(|pid| pid.parse().unwrap())
                        .collect();
                    if pids.len() == 2 {
                        return pids
                            .into_iter()
                            .skip(usize::from(exit_parent))
                            .map(|pid| {
                                // SAFETY: Only a test-created PID is opened, with synchronization access.
                                own(
                                    unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) },
                                    "OpenProcess(test)",
                                )
                                .unwrap()
                            })
                            .collect();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => panic!("read test marker: {error}"),
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "process tree never became ready"
            );
            thread::sleep(POLL);
        }
    }
}
impl Drop for TreeProbe {
    fn drop(&mut self) {
        self.cancel.cancel();
        match fs::remove_file(&self.marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("remove test marker: {error}"),
        }
    }
}

fn check_tree(cancelled: bool, exit_parent: bool) {
    let probe = TreeProbe::new();
    let script = probe.script(exit_parent);
    let args = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ];
    let exe = powershell();
    let cancel = probe.cancel.clone();
    let timeout = Duration::from_secs(if cancelled { 30 } else { 5 });
    thread::scope(|scope| {
        let started = Instant::now();
        let runner = scope.spawn(|| run(&exe, &args, timeout, &cancel));
        let handles = probe.handles(exit_parent);
        let cancel_started = Instant::now();
        if cancelled {
            probe.cancel.cancel();
        }
        let error = runner.join().unwrap().err().unwrap();
        let elapsed = if cancelled {
            cancel_started.elapsed()
        } else {
            started.elapsed().saturating_sub(timeout)
        };
        assert!(
            elapsed < Duration::from_millis(1250),
            "cleanup took {elapsed:?}"
        );
        let expected = if cancelled {
            format!("Process canceled: {} {}", exe.display(), args.join(" "))
        } else {
            format!(
                "Process timed out after 5000ms: {} {}",
                exe.display(),
                args.join(" ")
            )
        };
        assert_eq!(error, expected);
        for handle in handles {
            // SAFETY: This retained handle belongs to a test-created process.
            let status = unsafe { WaitForSingleObject(handle.as_raw(), 0) };
            assert_eq!(status, WAIT_OBJECT_0, "a process survived job termination");
        }
    });
}

#[test]
fn timeout_kills_parent_and_grandchild_holding_pipes() {
    check_tree(false, false);
}

#[test]
fn cancel_kills_parent_and_grandchild_holding_pipes() {
    check_tree(true, false);
}

#[test]
fn exited_parent_does_not_bypass_timeout_on_grandchild_pipes() {
    check_tree(false, true);
}
