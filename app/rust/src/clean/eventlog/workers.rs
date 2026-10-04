//! Bounded worker scheduling and cancellation drain.

use super::{POLL, checkpoint};
use crate::win::{self, Error, Result, process::Cancel};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

// Reserve one second for the UI to receive completion within its ten-second close budget.
const CANCEL_WAIT: Duration = Duration::from_secs(9);
fn panic_error(panic: Box<dyn std::any::Any + Send>) -> Error {
    let text = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("worker panicked");
    Error::msg("Event Log Cleaning", text)
}

// Threads own all OS state. Cancellation stops new work and drains running restore guards.
// A hung native call cannot be interrupted; the shared drain deadline prevents an unbounded wait.
fn stop_workers(local: &Cancel, workers: &mut Vec<thread::JoinHandle<()>>) {
    local.cancel();
    let deadline = Instant::now() + CANCEL_WAIT;
    while !workers.is_empty() {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                if let Err(panic) = workers.swap_remove(index).join() {
                    win::record(panic_error(panic));
                }
            } else {
                index += 1;
            }
        }
        if workers.is_empty() {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            win::record(Error {
                op: "Event Log Cleaning",
                code: 1460,
                detail: format!(
                    "stop wait timed out after 9000ms; {} operations still running",
                    workers.len()
                ),
            });
            break;
        }
        thread::sleep(POLL.min(remaining));
    }
}

pub(super) fn parallel<T: Send + 'static>(
    names: Vec<String>,
    limit: usize,
    timeout: Duration,
    cancel: &Cancel,
    job: impl Fn(&str, &Cancel) -> Result<T> + Send + Sync + 'static,
) -> Result<Vec<Result<T>>> {
    enum Message<T> {
        Started(usize, Instant),
        Done(usize, Result<T>),
    }
    checkpoint(cancel)?;
    let names = Arc::new(names);
    let job = Arc::new(job);
    let cursor = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel();
    let local = Cancel::new();
    let mut active = HashMap::new();
    let mut workers = Vec::new();
    let mut results: Vec<Option<Result<T>>> = (0..names.len()).map(|_| None).collect();
    for _ in 0..limit.min(names.len()) {
        let names = names.clone();
        let cursor = cursor.clone();
        let job = job.clone();
        let tx = tx.clone();
        let token = local.clone();
        let parent_cancel = cancel.clone();
        match thread::Builder::new()
            .name("event-log-operation".into())
            .spawn(move || {
                while !token.is_cancelled() && !parent_cancel.is_cancelled() {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(name) = names.get(index) else {
                        break;
                    };
                    if tx.send(Message::Started(index, Instant::now())).is_err() {
                        // A canceled/timed-out batch intentionally closes the receiver.
                        break;
                    }
                    let result = win::catch_panic(|| {
                        checkpoint(&token)?;
                        checkpoint(&parent_cancel)?;
                        job(name, &token)
                    })
                    .unwrap_or_else(|panic| Err(Error::msg("Event Log Cleaning", panic)));
                    if tx.send(Message::Done(index, result)).is_err() {
                        win::record(Error::msg(
                            "Event Log Cleaning",
                            "operation finished after its batch closed",
                        ));
                        break;
                    }
                }
            }) {
            Ok(worker) => workers.push(worker),
            Err(error) => {
                stop_workers(&local, &mut workers);
                return Err(Error::msg("Event Log Cleaning", error.to_string()));
            }
        }
    }
    drop(tx);
    let mut completed = 0;
    while completed < names.len() {
        if cancel.is_cancelled() {
            stop_workers(&local, &mut workers);
            return checkpoint(cancel).map(|_| Vec::new());
        }
        match rx.recv_timeout(POLL) {
            Ok(Message::Started(index, start)) => {
                active.insert(index, start);
            }
            Ok(Message::Done(index, result)) => {
                active.remove(&index);
                results[index] = Some(result);
                completed += 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::msg("Event Log Cleaning", "workers disconnected"));
            }
        }
        if active.values().any(|start| start.elapsed() >= timeout) {
            stop_workers(&local, &mut workers);
            for result in &mut results {
                if result.is_none() {
                    *result = Some(Err(Error {
                        op: "Event Log Cleaning",
                        code: 1460,
                        detail: format!(
                            "batch timed out after {}ms; no further operations started",
                            timeout.as_millis()
                        ),
                    }));
                }
            }
            break;
        }
    }
    results
        .into_iter()
        .map(|value| value.ok_or_else(|| Error::msg("Event Log Cleaning", "missing worker result")))
        .collect()
}
