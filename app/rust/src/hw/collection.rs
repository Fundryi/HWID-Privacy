//! Parallel provider execution, deadlines and failure reporting.

use super::{Ctx, PROVIDERS, Provider};
use crate::{
    report::{self, Out, Section},
    win,
};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

/// Collects providers in parallel with individual 60-second deadlines and ordered results.
pub fn collect_all(only: Option<&str>, on_done: &(dyn Fn(usize, &Section) + Sync)) -> Vec<Section> {
    collect_with_deadline(&PROVIDERS, only, on_done, Duration::from_secs(60))
}

// The private provider/deadline hook exercises panic, error and timeout handling
// without hardware access, elevation, or a public timeout setting.
/// Runs the selected providers with independent deadlines and ordered results.
pub(super) fn collect_with_deadline(
    providers: &[Provider],
    only: Option<&str>,
    on_done: &(dyn Fn(usize, &Section) + Sync),
    deadline: Duration,
) -> Vec<Section> {
    let ctx = Arc::new(Ctx::new());
    let selected: Vec<_> = providers
        .iter()
        .enumerate()
        .filter(|(_, p)| only.is_none_or(|title| report::eq_ignore_case(title, p.title)))
        .collect();
    let mut sections = vec![None; selected.len()];
    let mut starts = Vec::with_capacity(selected.len());
    let (sender, receiver) = mpsc::channel();

    // C# parity: HardwareInfoManager.cs:55-70. Start every provider before waiting.
    for (slot, &(index, provider)) in selected.iter().enumerate() {
        let started = Instant::now();
        starts.push(started);
        let provider = Provider {
            title: provider.title,
            collect: provider.collect,
        };
        let title = provider.title;
        let ctx = Arc::clone(&ctx);
        let sender = sender.clone();
        match thread::Builder::new()
            .name(provider.title.to_owned())
            .spawn(move || {
                if let Err(message) = win::catch_panic(|| {
                    let section = collect_provider(&provider, &ctx);
                    if let Err(error) = sender.send((slot, Instant::now(), section)) {
                        // Expected for an abandoned provider that finishes after collection ends.
                        win::record(win::Error::msg(
                            "provider result delivery",
                            format!("{}: {error}", provider.title),
                        ));
                    }
                }) {
                    win::record(win::Error::msg("provider worker", message));
                }
            }) {
            // Dropping the join handle detaches the worker. A timed-out worker must
            // retain its Arc<Ctx> and be free to finish after this function returns.
            Ok(handle) => drop(handle),
            Err(error) => {
                let section = error_section(
                    title,
                    &win::Error::msg("thread::spawn", error.to_string()),
                    started,
                );
                on_done(index, &section);
                sections[slot] = Some(section);
            }
        }
    }
    drop(sender);

    // Results arrive in completion order; deadlines are measured from each spawn,
    // never from the preceding provider's completion or timeout.
    while let Some(wait) = starts
        .iter()
        .enumerate()
        .filter(|(slot, _)| sections[*slot].is_none())
        .map(|(_, started)| deadline.saturating_sub(started.elapsed()))
        .min()
    {
        let completed: Vec<_> = match receiver.recv_timeout(wait) {
            Ok((slot, finished, section)) => {
                if sections[slot].is_some() {
                    // A late result cannot replace a timeout or fire a second callback.
                    continue;
                }
                let section = if finished.duration_since(starts[slot]) >= deadline {
                    timeout_section(selected[slot].1.title, starts[slot])
                } else {
                    section
                };
                vec![(slot, section)]
            }
            Err(mpsc::RecvTimeoutError::Timeout) => starts
                .iter()
                .enumerate()
                .filter(|(slot, started)| {
                    sections[*slot].is_none() && started.elapsed() >= deadline
                })
                .map(|(slot, &started)| (slot, timeout_section(selected[slot].1.title, started)))
                .collect(),
            Err(mpsc::RecvTimeoutError::Disconnected) => starts
                .iter()
                .enumerate()
                .filter(|(slot, _)| sections[*slot].is_none())
                .map(|(slot, &started)| {
                    (
                        slot,
                        error_section(
                            selected[slot].1.title,
                            &win::Error::msg("provider worker", "result channel disconnected"),
                            started,
                        ),
                    )
                })
                .collect(),
        };
        for (slot, section) in completed {
            on_done(selected[slot].0, &section);
            sections[slot] = Some(section);
        }
    }
    // C# parity: HardwareInfoManager.cs:99-103. Return in provider order.
    sections.into_iter().flatten().collect()
}

/// Runs one provider for diagnostic timing, preserving errors and caught panics.
pub fn collect_provider(provider: &Provider, ctx: &Ctx) -> Section {
    let start = Instant::now();
    let mut out = Out::new();
    let result = match win::catch_panic(|| (provider.collect)(ctx, &mut out)) {
        Ok(result) => result,
        Err(message) => Err(win::Error::msg(
            provider.title,
            format!("provider panicked: {message}"),
        )),
    };
    if let Err(error) = result {
        record_error(&mut out, provider.title, &error);
    }
    finish_section(out, provider.title, start)
}

fn finish_section(out: Out, title: &'static str, start: Instant) -> Section {
    let mut section = out.finish();
    section.title = title;
    section.elapsed_ms = start.elapsed().as_millis();
    section
}

fn record_error(out: &mut Out, title: &'static str, error: &win::Error) {
    // Approved difference: retain partial output; C# drops it (HardwareInfoManager.cs:73-77).
    out.fallback_failed(title, error)
        .text(&format!("Error retrieving {title} information: {error}"));
}

fn error_section(title: &'static str, error: &win::Error, start: Instant) -> Section {
    let mut out = Out::new();
    record_error(&mut out, title, error);
    finish_section(out, title, start)
}

fn timeout_section(title: &'static str, start: Instant) -> Section {
    let mut out = Out::new();
    out.fallback_failed(title, &win::Error::msg(title, "timed out"))
        .text(&format!("Error retrieving {title} information: timed out"));
    finish_section(out, title, start)
}
