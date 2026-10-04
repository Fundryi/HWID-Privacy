//! Owned by WP-12: cancellable event log cleaning orchestration.

mod batch;
mod clear;
mod discovery;
mod restore;
mod summary;
mod workers;

pub use discovery::planned_logs;

use crate::win::{self, Error, Result, process::Cancel};
use batch::{Counts, batch};
use discovery::{STANDARD, discover, unique};
use restore::RestoreFailures;
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use summary::summary;

type Status = Arc<dyn Fn(&str) + Send + Sync>;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(50);
/// Completion, cancellation, or a top-level cleaner failure.
pub enum CleanOutcome {
    Done,
    Cancelled,
    Failed(String),
}
/// Cleans planned logs and writes every status and failure through the callback.
pub fn clean(cancel: Cancel, status: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
    enum Message {
        Line(String),
        Done(CleanOutcome),
    }
    let (tx, rx) = mpsc::channel();
    let lines = tx.clone();
    let emit: Status = Arc::new(move |text| {
        if lines.send(Message::Line(text.to_owned())).is_err() {
            // A timed-out/closed consumer cannot receive late restoration diagnostics.
            win::record(Error::msg("Event Log Cleaning", text));
        }
    });
    let worker_cancel = cancel.clone();
    let spawn = thread::Builder::new()
        .name("event-log-cleaner".into())
        .spawn(move || {
            let restores = Arc::new(RestoreFailures::new(emit.clone()));
            let result = win::catch_panic(|| clean_inner(&worker_cancel, &emit, &restores))
                .unwrap_or_else(|panic| Err(Error::msg("Event Log Cleaning", panic)));
            // A cancelled/failed run never reaches the normal failed-summary list.
            restores.finish(None);
            let outcome = if worker_cancel.is_cancelled() {
                emit("");
                emit("Log cleaning canceled by user.");
                CleanOutcome::Cancelled
            } else {
                match result {
                    Ok(()) => CleanOutcome::Done,
                    Err(error) => {
                        emit(&format!("Error in Event Log Cleaning: {error}"));
                        CleanOutcome::Failed(error.to_string())
                    }
                }
            };
            // The receiver may have closed after its deadline; no shared resources depend on delivery.
            if tx.send(Message::Done(outcome)).is_err() {
                win::record(Error::msg("Event Log Cleaning", "cleaner consumer closed"));
            }
        });
    if let Err(error) = spawn {
        let text = Error::msg("Event Log Cleaning", error.to_string()).to_string();
        status(&format!("Error in Event Log Cleaning: {text}"));
        return CleanOutcome::Failed(text);
    }
    let started = Instant::now();
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Message::Line(line)) => status(&line),
            Ok(Message::Done(outcome)) => return outcome,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let text = Error::msg("Event Log Cleaning", "worker disconnected").to_string();
                status(&format!("Error in Event Log Cleaning: {text}"));
                return CleanOutcome::Failed(text);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if started.elapsed() >= Duration::from_secs(3600) {
            cancel.cancel();
            let text = Error::msg("Event Log Cleaning", "timed out after 3600000ms").to_string();
            status(&format!("Error in Event Log Cleaning: {text}"));
            return CleanOutcome::Failed(text);
        }
    }
}
fn checkpoint(cancel: &Cancel) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::msg("Event Log Cleaning", "canceled"))
    } else {
        Ok(())
    }
}
fn clean_inner(cancel: &Cancel, emit: &Status, restores: &Arc<RestoreFailures>) -> Result<()> {
    checkpoint(cancel)?;
    for privilege in ["SeSecurityPrivilege", "SeBackupPrivilege"] {
        if win::security::enable_privilege(privilege) {
            emit(&format!("Elevated: {privilege} enabled."));
        }
    }
    emit("Collecting standard event log channels...");
    let (standard, duplicates) = unique(STANDARD.lines());
    if duplicates > 0 {
        emit(&format!(
            "Skipped {duplicates} duplicate standard channels."
        ));
    }
    emit(&format!(
        "Collected {} standard logs to process.",
        standard.len()
    ));
    let known = standard.clone();
    let discovery_cancel = cancel.clone();
    let discovery_emit = emit.clone();
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("event-log-discovery".into())
        .spawn(move || {
            let result = win::catch_panic(|| discover(known, &discovery_cancel, &discovery_emit))
                .unwrap_or_else(|panic| Err(Error::msg("Event Log Cleaning", panic)));
            // The cleaner can close on cancellation before discovery completes.
            if tx.send(result).is_err() {
                win::record(Error::msg(
                    "Event Log Cleaning",
                    "discovery consumer closed",
                ));
            }
        })
        .map_err(|error| Error::msg("Event Log Cleaning", error.to_string()))?;
    emit("Discovering additional event log channels in background...");
    let mut counts = Counts::default();
    let mut processed = Vec::new();
    batch(
        standard.clone(),
        true,
        cancel,
        emit,
        restores,
        &mut processed,
        &mut counts,
    )?;
    finish_clean(
        standard.len(),
        rx,
        cancel,
        emit,
        restores,
        processed,
        counts,
    )
}

fn finish_clean(
    standard: usize,
    discovery: mpsc::Receiver<Result<Vec<String>>>,
    cancel: &Cancel,
    emit: &Status,
    restores: &Arc<RestoreFailures>,
    mut processed: Vec<String>,
    mut counts: Counts,
) -> Result<()> {
    let additional = loop {
        checkpoint(cancel)?;
        let error = match discovery.recv_timeout(POLL) {
            Ok(Ok(additional)) => {
                checkpoint(cancel)?;
                break additional;
            }
            Ok(Err(error)) => error,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Error::msg("Event Log Cleaning", "discovery disconnected")
            }
        };
        checkpoint(cancel)?;
        // C# parity: EventLogCleaningService.cs:558-571 (discovery failure keeps the standard summary).
        emit(&format!("Skipped additional log discovery: {error}"));
        break Vec::new();
    };
    if additional.is_empty() {
        emit("No additional event logs to process.");
    } else {
        emit(&format!(
            "Collected {} additional logs to process.",
            additional.len()
        ));
        batch(
            additional.clone(),
            false,
            cancel,
            emit,
            restores,
            &mut processed,
            &mut counts,
        )?;
    }
    restores.finish(Some(&mut counts.failed));
    summary(standard, additional.len(), &counts, emit);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
