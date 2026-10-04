//! Batch filtering and clean-attempt accounting.

use super::{
    Status, checkpoint,
    clear::{FAILED_CLEAR, clear_advanced},
    discovery::{UNCLEARABLE, contains},
    restore::RestoreFailures,
    workers::parallel,
};
use crate::{
    report,
    win::{Result, evt, process::Cancel},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

// Ten 15-second process attempts plus local config/clear/restore calls get a bounded budget.
const CLEAR_TIMEOUT: Duration = Duration::from_secs(180);
#[derive(Default)]
pub(super) struct Counts {
    pub(super) attempted: usize,
    pub(super) cleared: usize,
    pub(super) not_found: usize,
    pub(super) disabled: usize,
    pub(super) duplicate: usize,
    pub(super) locked: usize,
    pub(super) failed: Vec<(String, String)>,
}
pub(super) enum Attempt {
    Cleared,
    DryRun,
    NotFound,
    Disabled,
    Failed(String),
    RestoreFailed { cleared: bool, message: String },
}
pub(super) fn batch(
    names: Vec<String>,
    standard: bool,
    cancel: &Cancel,
    emit: &Status,
    restores: &Arc<RestoreFailures>,
    processed: &mut Vec<String>,
    counts: &mut Counts,
) -> Result<()> {
    let mut filtered = Vec::new();
    for name in names {
        if contains(processed, &name) {
            counts.duplicate += 1;
            emit(&format!("Skipped: {name} (duplicate)"));
        } else {
            processed.push(name.clone());
            if UNCLEARABLE
                .lines()
                .any(|locked| report::eq_ignore_case(locked, &name))
            {
                counts.locked += 1;
            } else {
                filtered.push(name);
            }
        }
    }
    let total = filtered.len();
    let attempted = Arc::new(AtomicUsize::new(0));
    let attempts = attempted.clone();
    let progress = emit.clone();
    let restores = restores.clone();
    let results = parallel(
        filtered.clone(),
        10,
        CLEAR_TIMEOUT,
        cancel,
        move |name, token| {
            // C# parity: EventLogCleaningService.cs:453-468 (standard logs skip the probe; unknown additional logs count as not found).
            if !standard {
                match evt::is_channel_enabled(name) {
                    Ok(false) => return Ok(Attempt::Disabled),
                    Ok(true) => {}
                    Err(error) => {
                        progress(&format!("Error in {name}: {error}"));
                        return Ok(Attempt::NotFound);
                    }
                }
            }
            checkpoint(token)?;
            let current = attempts.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&format!("Clearing log {current}/{total}: {name}"));
            match clear_advanced(name, token, &progress, &restores) {
                Ok(attempt) => Ok(attempt),
                Err(error) => {
                    checkpoint(token)?;
                    progress(&format!("Failed: {name} - {error}"));
                    Ok(Attempt::Failed(FAILED_CLEAR.into()))
                }
            }
        },
    )?;
    counts.attempted += attempted.load(Ordering::Relaxed);
    let mut timeout = None;
    for (name, result) in filtered.into_iter().zip(results) {
        match result {
            Ok(Attempt::Cleared) => counts.cleared += 1,
            Ok(Attempt::DryRun) => {}
            Ok(Attempt::NotFound) => counts.not_found += 1,
            Ok(Attempt::Disabled) => counts.disabled += 1,
            Ok(Attempt::Failed(message)) => counts.failed.push((name, message)),
            Ok(Attempt::RestoreFailed { cleared, message }) => {
                counts.cleared += usize::from(cleared);
                counts.failed.push((name, message));
            }
            Err(error) => {
                emit(&format!("Error in {name}: {error}"));
                counts.failed.push((name, error.to_string()));
                if error.code == 1460 {
                    timeout = Some(error);
                }
            }
        }
    }
    if let Some(error) = timeout {
        return Err(error);
    }
    Ok(())
}
