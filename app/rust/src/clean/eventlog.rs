//! Owned by WP-12: log discovery and cancellable cleaning.

use crate::{
    report,
    win::{
        self, Error, Result, evt,
        process::{self, Cancel},
    },
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

type Status = Arc<dyn Fn(&str) + Send + Sync>;

// Keep restore failures until the summary consumes them. Cancellation can happen after a
// worker returns its failure, or even in a later batch, so the worker's token is not enough.
struct RestoreFailures {
    pending: Mutex<Option<Vec<(String, String)>>>,
    emit: Status,
}

impl RestoreFailures {
    fn new(emit: Status) -> Self {
        Self {
            pending: Mutex::new(Some(Vec::new())),
            emit,
        }
    }

    fn report(&self, name: &str, error: Error) {
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

    fn finish(&self, summary: Option<&mut Vec<(String, String)>>) {
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
const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(50);
// Reserve one second for the UI to receive completion within its ten-second close budget.
const CANCEL_WAIT: Duration = Duration::from_secs(9);
// Ten 15-second process attempts plus local config/clear/restore calls get a bounded budget.
const CLEAR_TIMEOUT: Duration = Duration::from_secs(180);
const FAILED_CLEAR: &str = "Failed to clear log after trying all available methods";

// C# parity: EventLogCleaningService.cs:192-300 (order and spellings are deliberate).
const STANDARD: &str = "Windows PowerShell
System
Security
Application
PowerShellCore/Operational
Microsoft-Windows-Storage-Storport/Operational
Microsoft-Windows-Storage-ClassPnP/Operational
Microsoft-Windows-Storage-Partition/Diagnostic
Microsoft-Windows-StorageSpaces-Driver/Operational
Microsoft-Windows-StorageVolume/Operational
Microsoft-Windows-Ntfs/Operational
Microsoft-Windows-VolumeSnapshot-Driver/Operational
Microsoft-Windows-DeviceSetupManager/Admin
Microsoft-Windows-DeviceSetupManager/Operational
Microsoft-Windows-Kernel-PnP/Device Management
Microsoft-Windows-Kernel-PnP/Configuration
Microsoft-Windows-UserPnp/DeviceInstall
Microsoft-Windows-DeviceManagement-Enterprise-Diagnostics-Provider/Admin
Microsoft-Windows-StateRepository/Operational
Microsoft-Windows-CodeIntegrity/Operational
Microsoft-Windows-Kernel-ShimEngine/Operational
Microsoft-Windows-Kernel-EventTracing/Admin
Microsoft-Windows-GroupPolicy/Operational
Microsoft-Windows-Known Folders API Service
Microsoft-Windows-DriverFrameworks-UserMode/Operational
Microsoft-Windows-Hardware-Events/Operational
Microsoft-Windows-DeviceGuard/Operational
Microsoft-Windows-DNS-Client/Operational
Microsoft-Windows-Hyper-V-Drivers/Operational
Microsoft-Windows-Resource-Exhaustion-Detector/Operational
Microsoft-Windows-Authentication/AuthenticationPolicyFailures-DomainController
Microsoft-Windows-Authentication/ProtectedUser-Client
Microsoft-Windows-Security-SPP/Operational
Microsoft-Windows-Security-Auditing/Operational
Microsoft-Windows-NetworkProfile/Operational
Microsoft-Windows-WLAN-AutoConfig/Operational
Microsoft-Windows-BranchCacheSMB/Operational
Microsoft-Windows-NetworkLocationWizard/Operational
Microsoft-Windows-NlaSvc/Operational
Microsoft-Windows-Dhcp-Client/Admin
Microsoft-Windows-Dhcp-Client/Operational
Microsoft-Windows-DHCPv6-Client/Operational
Microsoft-Windows-TCPIP/Operational
Microsoft-Windows-WLAN-AutoConfig/Diagnostic
Microsoft-Windows-Iphlpsvc/Operational
Microsoft-Windows-NetworkConnectivityStatus/Operational
Microsoft-Windows-NetCore/Operational
Microsoft-Windows-Bluetooth-BthLEPrepairing/Operational
Microsoft-Windows-Bluetooth-MTPEnum/Operational
Microsoft-Windows-WLAN/Diagnostic
Microsoft-Windows-WWAN-SVC-Events/Operational
Microsoft-Windows-WWAN-UI-Events/Operational
Microsoft-Windows-WWAN-MM-Events/Operational
Microsoft-Windows-DeviceAssociation/Operational
Microsoft-Windows-DeviceInstall/Operational
Microsoft-Windows-DriverFrameworks-UserMode/Diagnostic
Microsoft-Windows-PCW/Operational
Microsoft-Windows-EapHost/Operational
Microsoft-Windows-FilterManager/Operational
Microsoft-Windows-Dhcpv6-Client/Admin
Microsoft-Windows-WebAuthN/Operational
Microsoft-Windows-WFP/Operational
Microsoft-Windows-Windows Firewall With Advanced Security/Firewall
Microsoft-Windows-NetworkSecurity/Operational
Microsoft-Windows-WMI-Activity/Operational
Microsoft-Windows-Time-Service/Operational
Microsoft-Windows-Store/Operational
Microsoft-Windows-Shell-Core/Operational
Microsoft-Windows-Security-Mitigations/KernelMode
Microsoft-Windows-PushNotification-Platform/Operational
Microsoft-Windows-PowerShell/Operational
Microsoft-Windows-LiveId/Operational
Microsoft-Windows-Kernel-Cache/Operational
Microsoft-Windows-Diagnosis-PCW/Operational
Microsoft-Windows-AppModel-Runtime/Admin
Microsoft-Windows-Application-Experience/Program-Telemetry
Microsoft-Windows-AppxPackaging/Operational
Microsoft-Windows-Diagnostics-Performance/Operational
Microsoft-Windows-Diagnosis-Scripted/Operational
Microsoft-Windows-Diagnosis-Schedule/Operational
Microsoft-Windows-USB-USBHUB/Operational
Microsoft-Windows-USB-USBPORT/Operational
Microsoft-Windows-Winlogon/Operational
Microsoft-Windows-UAC/Operational";

// C# parity: EventLogCleaningService.cs:24-52 (these are skipped even when collected).
const UNCLEARABLE: &str = "Microsoft-Windows-Kernel-PnP/Device Management
Microsoft-Windows-Kernel-Cache/Operational
Microsoft-Windows-StorageVolume/Operational
Microsoft-Windows-Storage-Partition/Diagnostic
Microsoft-Windows-FilterManager/Operational
Microsoft-Windows-DriverFrameworks-UserMode/Diagnostic
Microsoft-Windows-Hyper-V-Drivers/Operational
Microsoft-Windows-USB-USBHUB/Operational
Microsoft-Windows-USB-USBPORT/Operational
Microsoft-Windows-Hardware-Events/Operational
Microsoft-Windows-NetworkSecurity/Operational
Microsoft-Windows-NetworkConnectivityStatus/Operational
Microsoft-Windows-NetCore/Operational
Microsoft-Windows-WLAN/Diagnostic
Microsoft-Windows-WWAN-MM-Events/Operational
Microsoft-Windows-WWAN-UI-Events/Operational
Microsoft-Windows-Security-Auditing/Operational
Microsoft-Windows-Security-SPP/Operational
Microsoft-Windows-LiveId/Operational
Microsoft-Windows-PCW/Operational
Microsoft-Windows-DeviceInstall/Operational
Microsoft-Windows-DeviceAssociation/Operational
Microsoft-Windows-Diagnosis-Schedule/Operational";

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
/// Returns standard and additional log names without changing any channel.
pub fn planned_logs() -> (Vec<String>, Vec<String>) {
    let standard = unique(STANDARD.lines()).0;
    let emit: Status = Arc::new(|line| win::record(Error::msg("Event Log Cleaning", line)));
    match discover(standard.clone(), &Cancel::new(), &emit) {
        Ok(additional) => (standard, additional),
        Err(error) => {
            win::record(error);
            (standard, Vec::new())
        }
    }
}

fn unique<'a>(names: impl IntoIterator<Item = &'a str>) -> (Vec<String>, usize) {
    let mut result = Vec::new();
    let mut duplicates = 0;
    for name in names {
        let name = report::trim_net(name);
        if name.is_empty() {
            continue;
        }
        if contains(&result, name) {
            duplicates += 1;
        } else {
            result.push(name.to_owned());
        }
    }
    (result, duplicates)
}
fn contains(names: &[String], name: &str) -> bool {
    names.iter().any(|n| report::eq_ignore_case(n, name))
}
fn checkpoint(cancel: &Cancel) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::msg("Event Log Cleaning", "canceled"))
    } else {
        Ok(())
    }
}
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

fn parallel<T: Send + 'static>(
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

fn discover(known: Vec<String>, cancel: &Cancel, emit: &Status) -> Result<Vec<String>> {
    let enumerated = parallel(
        vec![String::new()],
        1,
        PROCESS_TIMEOUT,
        cancel,
        |_, token| evt::enumerate_channels(token),
    )?;
    let mut candidates = Vec::new();
    let mut duplicates = 0;
    for result in enumerated {
        match result {
            Ok(channels) => {
                if let Some(error) = channels.failure {
                    emit(&format!("Skipped additional log discovery: {error}"));
                }
                for raw in channels.names {
                    checkpoint(cancel)?;
                    let name = report::trim_net(&raw);
                    if name.is_empty() {
                        continue;
                    }
                    if contains(&known, name) || contains(&candidates, name) {
                        duplicates += 1;
                    } else {
                        candidates.push(name.to_owned());
                    }
                }
            }
            Err(error) => emit(&format!("Skipped additional log discovery: {error}")),
        }
    }
    if duplicates > 0 {
        emit(&format!(
            "Skipped {duplicates} channels already known from standard/discovered sets."
        ));
    }
    let total = candidates.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    emit(&format!("Probing {total} additional channels..."));
    let processed = AtomicUsize::new(0);
    let progress = emit.clone();
    let results = parallel(
        candidates.clone(),
        12,
        PROCESS_TIMEOUT,
        cancel,
        move |name, _| {
            // C# parity: EventLogCleaningService.cs:628-634 (unknown state remains a candidate).
            let enabled = match evt::is_channel_enabled(name) {
                Ok(value) => Some(value),
                Err(error) => {
                    win::record(error);
                    None
                }
            };
            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if current.is_multiple_of(50) || current == total {
                progress(&format!("Discovering additional logs... {current}/{total}"));
            }
            Ok(enabled != Some(false))
        },
    )?;
    let mut additional = Vec::new();
    for (name, result) in candidates.into_iter().zip(results) {
        match result {
            Ok(true) => additional.push(name),
            Ok(false) => {}
            Err(error) => emit(&format!("Error in {name}: {error}")),
        }
    }
    // C# parity: EventLogCleaningService.cs:652 (ordinal casing, without Unicode expansion).
    evt::sort_channel_names(&mut additional)?;
    Ok(additional)
}

#[derive(Default)]
struct Counts {
    attempted: usize,
    cleared: usize,
    not_found: usize,
    disabled: usize,
    duplicate: usize,
    locked: usize,
    failed: Vec<(String, String)>,
}
enum Attempt {
    Cleared,
    DryRun,
    NotFound,
    Disabled,
    Failed(String),
    RestoreFailed { cleared: bool, message: String },
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
    let additional = loop {
        checkpoint(cancel)?;
        match rx.recv_timeout(POLL) {
            Ok(result) => break result?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::msg("Event Log Cleaning", "discovery disconnected"));
            }
        }
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
    summary(standard.len(), additional.len(), &counts, emit);
    Ok(())
}

fn batch(
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

fn summary(standard: usize, additional: usize, counts: &Counts, emit: &Status) {
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

fn guarded<T>(what: &str, emit: &Status, action: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    match super::destructive(what, action) {
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

struct RestoreChannel {
    name: String,
    original: bool,
    armed: bool,
    emit: Status,
    failures: Arc<RestoreFailures>,
}
impl RestoreChannel {
    fn restore(&mut self) -> Result<()> {
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

fn clear_advanced(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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
            include_str!("../../tests/fixtures/wp-12/overview.fixture").replace('\n', "\r\n")
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
}
