//! Channel restoration guards and retained restoration failures.

use super::{Status, clear::guarded};
use crate::win::{self, Error, Result, evt};
use std::sync::{Arc, Mutex};

// Keep restore failures until the summary consumes them. Cancellation can happen after a
// worker returns its failure, or even in a later batch, so the worker's token is not enough.
pub(super) struct RestoreFailures {
    pending: Mutex<Option<Vec<(String, String)>>>,
    emit: Status,
}

impl RestoreFailures {
    pub(super) fn new(emit: Status) -> Self {
        Self {
            pending: Mutex::new(Some(Vec::new())),
            emit,
        }
    }

    pub(super) fn report(&self, name: &str, error: Error) {
        let message = error.to_string();
        win::record(error);
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(lines) = pending.as_mut() {
            lines.push((name.to_owned(), message));
        } else {
            // The bounded drain expired; late failures still reach the diagnostic sink.
            drop(pending);
            (self.emit)(&format!("Failed: {name} - {message}"));
        }
    }

    pub(super) fn finish(&self, summary: Option<&mut Vec<(String, String)>>) {
        let lines = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(summary) = summary {
            // Explicit restore failures are already in Counts; unwinding guards may add others.
            for failure in lines.into_iter().flatten() {
                if !summary.contains(&failure) {
                    summary.push(failure);
                }
            }
        } else {
            for (name, message) in lines.into_iter().flatten() {
                (self.emit)(&format!("Failed: {name} - {message}"));
            }
        }
    }
}
pub(super) struct RestoreChannel {
    pub(super) name: String,
    pub(super) original: bool,
    pub(super) armed: bool,
    pub(super) emit: Status,
    pub(super) failures: Arc<RestoreFailures>,
}
impl RestoreChannel {
    pub(super) fn restore(&mut self) -> Result<()> {
        if !self.armed {
            return Ok(());
        }
        self.armed = false;
        guarded(
            &format!("Restore channel enabled state: {}", self.name),
            &self.emit,
            || evt::set_channel_enabled(&self.name, self.original),
        )
        .map(|_| ())
        .map_err(|error| Error {
            op: "Restore channel enabled state",
            ..error
        })
    }
}
impl Drop for RestoreChannel {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            self.failures.report(&self.name, error);
        }
    }
}
