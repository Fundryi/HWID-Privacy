//! Cleaner regression seams and opt-in read-only/dry-run captures.

use super::workers::parallel;
use super::*;
use crate::win::evt;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn standard_probe_skips_only_missing_channels_and_preserves_additional_policy() {
    use super::batch::{Attempt, probe_skip};

    let missing = Err(Error {
        op: "EvtOpenChannelConfig",
        code: 15007,
        detail: "The specified channel could not be found.".into(),
    });
    let denied = Err(Error {
        op: "EvtOpenChannelConfig",
        code: 5,
        detail: "Access is denied.".into(),
    });
    let property_error = Err(Error {
        op: "EvtGetChannelConfigProperty",
        code: 15007,
        detail: "Property read failed after opening the channel.".into(),
    });
    assert!(matches!(
        probe_skip(true, &missing),
        Some(Attempt::NotFound)
    ));
    for enabled in [Ok(true), Ok(false), denied.clone(), property_error.clone()] {
        assert!(probe_skip(true, &enabled).is_none());
    }
    assert!(probe_skip(false, &Ok(true)).is_none());
    assert!(matches!(
        probe_skip(false, &Ok(false)),
        Some(Attempt::Disabled)
    ));
    for error in [missing, denied, property_error] {
        assert!(matches!(probe_skip(false, &error), Some(Attempt::NotFound)));
    }
}

#[test]
fn discovery_failure_keeps_standard_summary_but_cancel_stays_cancelled() {
    for failure in [Some("parallel failed"), Some("discovery panicked"), None] {
        for cancelled in [false, true] {
            let cancel = Cancel::new();
            let lines = Arc::new(Mutex::new(Vec::new()));
            let output = lines.clone();
            let emit: Status = Arc::new(move |line| {
                output.lock().expect("capture lock").push(line.to_owned());
            });
            let restores = Arc::new(RestoreFailures::new(emit.clone()));
            let (tx, rx) = mpsc::channel();
            if let Some(message) = failure {
                tx.send(Err(Error::msg("Event Log Cleaning", message)))
                    .expect("discovery result");
            }
            drop(tx);
            if cancelled {
                cancel.cancel();
            }
            let result = finish_clean(
                84,
                rx,
                &cancel,
                &emit,
                &restores,
                Vec::new(),
                Counts {
                    attempted: 61,
                    cleared: 61,
                    locked: 23,
                    ..Counts::default()
                },
            );
            let lines = lines.lock().expect("capture lock");
            if cancelled {
                assert!(result.is_err());
                assert!(lines.is_empty(), "cancel must not emit a skip or summary");
            } else {
                assert!(result.is_ok());
                let message = failure.unwrap_or("discovery disconnected");
                let expected = format!(
                    "Skipped additional log discovery: {}",
                    Error::msg("Event Log Cleaning", message)
                );
                assert_eq!(lines.iter().filter(|line| **line == expected).count(), 1);
                assert!(
                    lines
                        .iter()
                        .any(|line| line == "No additional event logs to process.")
                );
                assert!(
                    lines
                        .iter()
                        .any(|line| line == "Summary: 61 logs cleared successfully")
                );
                assert!(
                    lines
                        .iter()
                        .any(|line| line == "Collected logs (additional): 0")
                );
                assert!(
                    lines
                        .iter()
                        .any(|line| line == "Logs cleared               : 61")
                );
            }
        }
    }
}

#[test]
fn cancel_waits_for_running_restore_and_starts_no_replacements() {
    struct Restore(Arc<AtomicUsize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let cancel = Cancel::new();
    let token = cancel.clone();
    let restored = Arc::new(AtomicUsize::new(0));
    let completed = restored.clone();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let output = lines.clone();
    let restores = Arc::new(RestoreFailures::new(Arc::new(move |line| {
        output.lock().expect("output").push(line.to_owned());
    })));
    let failures = restores.clone();
    let (started, ready) = mpsc::channel();
    let canceller = thread::spawn(move || {
        for _ in 0..3 {
            ready
                .recv_timeout(Duration::from_secs(2))
                .expect("running job");
        }
        token.cancel();
    });
    let result = parallel(
        (0..12).map(|n| n.to_string()).collect(),
        3,
        Duration::from_secs(2),
        &cancel,
        move |name, token| {
            let _restore = Restore(completed.clone());
            assert!(win::is_guarded(), "panic hook must not block restoration");
            if name == "0" {
                failures.report(
                    name,
                    Error::msg("Restore channel enabled state", "before cancel"),
                );
            }
            started.send(()).expect("start notification");
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            thread::sleep(Duration::from_millis(150));
            if name != "0" {
                failures.report(
                    name,
                    Error::msg("Restore channel enabled state", "during drain"),
                );
            }
            checkpoint(token)
        },
    );
    canceller.join().expect("canceller");
    assert!(result.is_err());
    assert_eq!(
        restored.load(Ordering::SeqCst),
        3,
        "all restore guards must run before returning"
    );
    assert!(
        lines.lock().expect("output").is_empty(),
        "normal failures wait for summary"
    );
    restores.finish(None);
    restores.finish(None);
    let output = lines.lock().expect("output");
    assert_eq!(
        output.len(),
        3,
        "cancel preserves each restore failure exactly once"
    );
    assert!(output.iter().any(|line| line.ends_with("before cancel")));
    assert_eq!(
        output
            .iter()
            .filter(|line| line.ends_with("during drain"))
            .count(),
        2
    );
    drop(output);
    restores.report(
        "late",
        Error::msg("Restore channel enabled state", "after deadline"),
    );
    assert_eq!(
        lines.lock().expect("output").len(),
        4,
        "late restoration failure is still reported"
    );
}

#[test]
fn overview_matches_csharp_text_and_padding() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let output = lines.clone();
    let emit: Status =
        Arc::new(move |line| output.lock().expect("capture lock").push(line.to_owned()));
    summary(
        84,
        16,
        &Counts {
            attempted: 76,
            cleared: 76,
            not_found: 1,
            disabled: 0,
            duplicate: 0,
            locked: 23,
            failed: Vec::new(),
        },
        &emit,
    );
    let text = lines.lock().expect("capture lock").join("\r\n") + "\r\n";
    assert_eq!(
        text,
        include_str!("../../../tests/fixtures/wp-12/overview.fixture").replace('\n', "\r\n")
    );
}

#[test]
#[ignore = "read-only owner-PC capture, contains local channel names"]
fn wp12_read_only_capture() {
    let started = Instant::now();
    let (standard, additional) = planned_logs();
    let text = standard
        .iter()
        .chain(&additional)
        .map(|name| format!("{name}\r\n"))
        .collect::<String>();
    let directory = PathBuf::from("D:/GIT/HWID-Privacy/app/rust/golden/wp-12");
    fs::create_dir_all(&directory).expect("private capture folder");
    fs::write(directory.join("rust-logs.txt"), &text).expect("private log list");
    fs::write(
        directory.join("rust-discovery-timing.txt"),
        format!(
            "standard={} additional={} elapsed_ms={} elevated={}\r\n",
            standard.len(),
            additional.len(),
            started.elapsed().as_millis(),
            win::security::is_admin()
        ),
    )
    .expect("private timing");
    println!("{text}");
    println!(
        "standard={} additional={} elapsed_ms={} elevated={}",
        standard.len(),
        additional.len(),
        started.elapsed().as_millis(),
        win::security::is_admin()
    );
    assert_eq!(standard.len(), 84);
    assert!(
        !additional.is_empty(),
        "discovery must produce a real list on the owner PC"
    );
}

#[test]
#[ignore = "debug dry-run only; reads channel states before and after the cleaner"]
#[cfg(debug_assertions)]
fn wp12_dry_run_capture() {
    assert_ne!(
        std::env::var("HWID_ALLOW_DESTRUCTIVE").ok().as_deref(),
        Some("1"),
        "destructive authorization must be absent"
    );
    let names = evt::enumerate_channels(&Cancel::new())
        .expect("native enumeration")
        .names;
    let snapshot = || {
        parallel(
            names.clone(),
            12,
            PROCESS_TIMEOUT,
            &Cancel::new(),
            |name, _| evt::is_channel_enabled(name),
        )
        .expect("state snapshot")
    };
    let before = snapshot();
    let directory = PathBuf::from("D:/GIT/HWID-Privacy/app/rust/golden/wp-12");
    fs::create_dir_all(&directory).expect("private capture folder");
    let lines = Mutex::new(Vec::new());
    let started = Instant::now();
    let outcome = clean(Cancel::new(), &|line| {
        lines.lock().expect("capture lock").push(line.to_owned())
    });
    assert!(matches!(outcome, CleanOutcome::Done));
    let elapsed = started.elapsed();
    let after = snapshot();
    let states = names
        .iter()
        .zip(&before)
        .zip(&after)
        .map(|((name, before), after)| {
            let value = |result: &Result<bool>| match result {
                Ok(value) => value.to_string(),
                Err(error) => error.to_string(),
            };
            format!("{name}\t{}\t{}\r\n", value(before), value(after))
        })
        .collect::<String>();
    fs::write(
        directory.join("rust-channel-states-before-after.tsv"),
        states,
    )
    .expect("private state capture");
    for ((name, before), after) in names.iter().zip(before).zip(after) {
        match (before, after) {
            (Ok(before), Ok(after)) => assert_eq!(before, after, "{name}"),
            (Err(before), Err(after)) => assert_eq!(before.code, after.code, "{name}"),
            _ => panic!("channel readability changed: {name}"),
        }
    }
    let lines = lines.into_inner().expect("capture lock");
    assert!(lines.iter().any(|line| line.starts_with("[DRY RUN]")));
    assert!(!lines.iter().any(|line| line.starts_with("Cleared:")));
    let text = lines.join("\r\n") + "\r\n";
    fs::write(
        "D:/GIT/HWID-Privacy/app/rust/golden/wp-12/rust-dry-run.txt",
        &text,
    )
    .expect("private dry run");
    println!("{text}\nDry-run elapsed_ms={}", elapsed.as_millis());
}
