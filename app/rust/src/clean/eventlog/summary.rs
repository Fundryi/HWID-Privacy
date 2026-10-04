//! C# summary text and overview columns.

use super::{Status, batch::Counts};

pub(super) fn summary(standard: usize, additional: usize, counts: &Counts, emit: &Status) {
    // C# parity: EventLogCleaningService.cs:514-543 (including intentional column widths).
    if counts.failed.is_empty() {
        emit(&format!(
            "Summary: {} logs cleared successfully",
            counts.cleared
        ));
    } else {
        emit(&format!(
            "Summary: {} logs cleared, {} failed",
            counts.cleared,
            counts.failed.len()
        ));
        for (name, message) in &counts.failed {
            emit(&format!("Failed: {name} - {message}"));
        }
    }
    emit("");
    emit("========== CLEAN LOGS OVERVIEW ==========");
    for (label, value) in [
        ("Collected logs (standard)  ", standard),
        ("Collected logs (additional)", additional),
        ("Collected logs (total)     ", standard + additional),
        ("Logs attempted             ", counts.attempted),
        ("Logs cleared               ", counts.cleared),
        ("Skipped (not found)        ", counts.not_found),
        ("Skipped (disabled)         ", counts.disabled),
        ("Skipped (duplicate)        ", counts.duplicate),
        ("Skipped (OS-locked)        ", counts.locked),
        ("Failed                     ", counts.failed.len()),
    ] {
        emit(&format!("{label}: {value}"));
    }
    emit("=========================================");
}
