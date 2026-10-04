//! Guarded native, process and file clearing attempts.

use super::{
    PROCESS_TIMEOUT, Status,
    batch::Attempt,
    checkpoint,
    restore::{RestoreChannel, RestoreFailures},
};
use crate::win::{
    self, Error, Result, evt,
    process::{self, Cancel},
};
use std::{fs, path::PathBuf, sync::Arc};

pub(super) const FAILED_CLEAR: &str = "Failed to clear log after trying all available methods";
pub(super) fn guarded<T>(
    what: &str,
    emit: &Status,
    action: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    match super::super::destructive(what, action) {
        Some(result) => result.map(Some),
        None if cfg!(debug_assertions) => {
            emit(&format!("[DRY RUN] {what}"));
            Ok(None)
        }
        None => Err(Error::msg(
            "Destructive operation guard",
            "operation refused",
        )),
    }
}
pub(super) fn clear_advanced(
    name: &str,
    cancel: &Cancel,
    emit: &Status,
    restores: &Arc<RestoreFailures>,
) -> Result<Attempt> {
    checkpoint(cancel)?;
    let kind = match evt::channel_type(name) {
        Ok(kind) => Some(kind),
        Err(error) => {
            win::record(error);
            None
        }
    };
    if matches!(kind, Some(2 | 3)) {
        let original = match evt::is_channel_enabled(name) {
            Ok(enabled) => Some(enabled),
            Err(error) => {
                win::record(error);
                None
            }
        };
        let mut restore = RestoreChannel {
            name: name.into(),
            original: original.unwrap_or(false),
            armed: false,
            emit: emit.clone(),
            failures: restores.clone(),
        };
        // AD-33: unknown state is never toggled; an already-disabled channel stays disabled.
        if original == Some(true) {
            checkpoint(cancel)?;
            restore.armed = true;
            if guarded(&format!("Disable channel: {name}"), emit, || {
                evt::set_channel_enabled(name, false)
            })?
            .is_none()
            {
                restore.armed = false;
                return Ok(Attempt::DryRun);
            }
        }
        let result = native_then_process(name, cancel, emit);
        if let Err(error) = restore.restore() {
            restores.report(name, error.clone());
            let cleared = matches!(result, Ok(Attempt::Cleared));
            if let Err(clear_error) = result {
                win::record(clear_error);
            }
            if cleared {
                emit(&format!("Cleared: {name}"));
            }
            // AD-33: the summary adds exactly one restoration failure line, with channel and code.
            return Ok(Attempt::RestoreFailed {
                cleared,
                message: error.to_string(),
            });
        }
        return finish_clear(name, result?, emit);
    }
    let first = native_then_process(name, cancel, emit)?;
    if !matches!(first, Attempt::Failed(_)) {
        return finish_clear(name, first, emit);
    }
    // C# parity: EventLogCleaningService.cs:127-128 (OPT-6 retains the wrong '-' and space substitutions).
    let path = process::system32("wevtutil.exe")
        .with_file_name("Winevt")
        .join("Logs")
        .join(format!(
            "{}.evtx",
            name.replace('/', "%4").replace(['-', ' '], "_")
        ));
    if file_exists(&path) {
        let file = path.to_string_lossy();
        let output = run_destructive("takeown.exe", &["/f", &file, "/A"], cancel, emit)?;
        if let Some(output) = output
            && !output.stderr.is_empty()
        {
            win::record(Error::msg("Event log process", output.stderr));
        }
        let user = evt::user_name()?;
        for grant in [
            "Administrators:(F)".into(),
            "SYSTEM:(F)".into(),
            format!("{user}:(F)"),
        ] {
            // C# parity: EventLogCleaningService.cs:135-138 (permanent ACL changes; tool stderr does not stop retries).
            let output = run_destructive(
                "icacls.exe",
                &[&file, "/grant:r", &grant, "/T"],
                cancel,
                emit,
            )?;
            if let Some(output) = output
                && !output.stderr.is_empty()
            {
                win::record(Error::msg("Event log process", output.stderr));
            }
        }
        if process_clear(name, cancel, emit)? {
            return finish_clear(name, Attempt::Cleared, emit);
        }
    }
    checkpoint(cancel)?;
    if let Some(path) = guarded(
        "Create temporary event log export",
        emit,
        evt::temporary_file,
    )? {
        let temporary = TempExport {
            path,
            emit: emit.clone(),
        };
        let file = temporary.path.to_string_lossy();
        // C# parity: EventLogCleaningService.cs:150-152 (export isn't retained and its result doesn't gate the clear).
        let output = run_destructive("wevtutil.exe", &["epl", name, &file], cancel, emit)?;
        if let Some(output) = output
            && !output.stderr.is_empty()
        {
            win::record(Error::msg("Event log process", output.stderr));
        }
        if process_clear(name, cancel, emit)? {
            return finish_clear(name, Attempt::Cleared, emit);
        }
    } else {
        return Ok(Attempt::DryRun);
    }
    // C# parity: EventLogCleaningService.cs:167-174 (file deletion counts as clearing without a service recheck).
    if file_exists(&path) {
        checkpoint(cancel)?;
        match guarded(&format!("Delete {}", path.display()), emit, || {
            remove_file(&path)
        }) {
            Ok(Some(())) => return finish_clear(name, Attempt::Cleared, emit),
            Ok(None) => return Ok(Attempt::DryRun),
            Err(error) => return Err(error),
        }
    }
    Ok(Attempt::Failed(FAILED_CLEAR.into()))
}

fn native_then_process(name: &str, cancel: &Cancel, emit: &Status) -> Result<Attempt> {
    checkpoint(cancel)?;
    match guarded(&format!("EvtClearLog {name}"), emit, || {
        evt::clear_log(name)
    }) {
        Ok(Some(())) => return Ok(Attempt::Cleared),
        Ok(None) => return Ok(Attempt::DryRun),
        Err(error) => win::record(error),
    }
    if process_clear(name, cancel, emit)? {
        Ok(Attempt::Cleared)
    } else {
        Ok(Attempt::Failed(FAILED_CLEAR.into()))
    }
}
fn finish_clear(name: &str, result: Attempt, emit: &Status) -> Result<Attempt> {
    if matches!(result, Attempt::Cleared) {
        emit(&format!("Cleared: {name}"));
    }
    Ok(result)
}
fn process_clear(name: &str, cancel: &Cancel, emit: &Status) -> Result<bool> {
    // C# parity: EventLogCleaningService.cs:121,372-375 (empty stderr, not ExitCode, is the success rule).
    match run_destructive("wevtutil.exe", &["cl", name], cancel, emit)? {
        Some(output) => {
            let success = output.stderr.is_empty();
            if !success {
                win::record(Error::msg(
                    "Event log process",
                    format!("{name}: {}", output.stderr),
                ));
            }
            Ok(success)
        }
        None => Ok(false),
    }
}
fn run_destructive(
    exe: &str,
    args: &[&str],
    cancel: &Cancel,
    emit: &Status,
) -> Result<Option<process::Output>> {
    checkpoint(cancel)?;
    guarded(&format!("{exe} {}", args.join(" ")), emit, || {
        process::run(&process::system32(exe), args, PROCESS_TIMEOUT, cancel)
            .map_err(|message| Error::msg("Event log process", message))
    })
}
fn file_exists(path: &PathBuf) -> bool {
    // C# parity: EventLogCleaningService.cs:132,168 (File.Exists also returns false on metadata errors).
    match fs::metadata(path) {
        Ok(info) => info.is_file(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            win::record(io_error("Read log file metadata", error));
            false
        }
    }
}
fn io_error(op: &'static str, error: std::io::Error) -> Error {
    Error {
        op,
        code: error.raw_os_error().unwrap_or(0) as u32,
        detail: error.to_string(),
    }
}
fn remove_file(path: &PathBuf) -> Result<()> {
    fs::remove_file(path).map_err(|error| io_error("Delete event log file", error))
}
struct TempExport {
    path: PathBuf,
    emit: Status,
}
impl Drop for TempExport {
    fn drop(&mut self) {
        // C# parity: EventLogCleaningService.cs:157-164 (the export is always discarded, including on cancellation).
        if let Err(error) = guarded(
            &format!("Delete temporary export {}", self.path.display()),
            &self.emit,
            || remove_file(&self.path),
        ) {
            win::record(error);
        }
    }
}
