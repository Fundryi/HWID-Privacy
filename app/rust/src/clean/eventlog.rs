//! Owned by WP-12: log discovery and cancellable cleaning.

use crate::win::process::Cancel;

pub enum CleanOutcome {
    Done,
    Cancelled,
    Failed(String),
}
/// Cleans planned logs and writes every status and failure through the callback.
pub fn clean(_cancel: Cancel, status: &(dyn Fn(&str) + Sync)) -> CleanOutcome {
    status("Event log cleaner not ported yet");
    CleanOutcome::Failed("Event log cleaner not ported yet".to_owned())
}
/// Returns standard and additional log names without changing any channel.
pub fn planned_logs() -> (Vec<String>, Vec<String>) {
    (
        vec!["Event log planning not ported yet".to_owned()],
        Vec::new(),
    )
}
