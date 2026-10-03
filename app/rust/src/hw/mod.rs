//! Provider order and shared per-collection context.

pub mod arp;
pub mod bios;
pub mod bluetooth;
pub mod chassis;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod monitor;
pub mod motherboard;
pub mod network;
pub mod ram;
pub mod system;
pub mod tpm;
pub mod usb;

use crate::{
    report::{self, Out, Section},
    win::{self, firmware::Smbios},
};
use std::{
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

pub struct Provider {
    pub title: &'static str,
    pub collect: fn(&Ctx, &mut Out) -> Result<(), win::Error>,
}

// C# parity: HardwareInfoManager.cs:23-38.
pub static PROVIDERS: [Provider; 14] = [
    Provider {
        title: "DISK DRIVES",
        collect: disk::collect,
    },
    Provider {
        title: "MOTHERBOARD",
        collect: motherboard::collect,
    },
    Provider {
        title: "CHASSIS",
        collect: chassis::collect,
    },
    Provider {
        title: "(SM)BIOS",
        collect: bios::collect,
    },
    Provider {
        title: "SYSTEM INFORMATION",
        collect: system::collect,
    },
    Provider {
        title: "RAM MODULES",
        collect: ram::collect,
    },
    Provider {
        title: "CPU",
        collect: cpu::collect,
    },
    Provider {
        title: "TPM MODULES",
        collect: tpm::collect,
    },
    Provider {
        title: "USB DEVICES",
        collect: usb::collect,
    },
    Provider {
        title: "GPU INFO",
        collect: gpu::collect,
    },
    Provider {
        title: "MONITOR INFORMATION",
        collect: monitor::collect,
    },
    Provider {
        title: "NETWORK ADAPTERS (NIC's)",
        collect: network::collect,
    },
    Provider {
        title: "BLUETOOTH ADAPTERS",
        collect: bluetooth::collect,
    },
    Provider {
        title: "ARP INFO/CACHE",
        collect: arp::collect,
    },
];

#[derive(Default)]
pub struct Ctx {
    hardware_ids: OnceLock<win::Result<HashMap<String, String>>>,
    smbios: OnceLock<win::Result<Smbios>>,
    present_ids: OnceLock<win::Result<HashSet<String>>>,
}

impl Ctx {
    /// Creates caches shared only within this collection.
    pub fn new() -> Self {
        Self::default()
    }
    /// Returns a cached hardware ID using uppercase instance-ID normalization.
    pub fn hardware_id(&self, instance_id: &str) -> Option<&str> {
        // Keep a failed snapshot in OnceLock; hardware_ids() exposes it for diagnostics.
        self.hardware_ids()
            .ok()?
            .get(&instance_id.to_uppercase())
            .map(String::as_str)
    }
    /// Returns the cached SMBIOS table when that source succeeded.
    pub fn smbios(&self) -> Option<&Smbios> {
        // Keep a failed snapshot in OnceLock; smbios_result() exposes it for diagnostics.
        self.smbios_result().ok()
    }
    /// Returns the cached map or its error so providers can record a failed source.
    pub fn hardware_ids(&self) -> win::Result<&HashMap<String, String>> {
        self.hardware_ids
            .get_or_init(win::setupapi::hardware_id_map)
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Returns the cached firmware table or its error for fallback diagnostics.
    pub fn smbios_result(&self) -> win::Result<&Smbios> {
        self.smbios
            .get_or_init(win::firmware::smbios)
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Returns normalized present IDs or their error; unknown presence stays unknown.
    pub fn present_instance_ids(&self) -> win::Result<&HashSet<String>> {
        self.present_ids
            .get_or_init(win::setupapi::present_instance_ids)
            .as_ref()
            .map_err(Clone::clone)
    }
}

/// Collects providers in parallel with individual 60-second deadlines and ordered results.
pub fn collect_all(only: Option<&str>, on_done: &(dyn Fn(usize, &Section) + Sync)) -> Vec<Section> {
    collect_with_deadline(&PROVIDERS, only, on_done, Duration::from_secs(60))
}

// The private provider/deadline hook exercises panic, error and timeout handling
// without hardware access, elevation, or a public timeout setting.
fn collect_with_deadline(
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
                let section = collect_provider(&provider, &ctx);
                if let Err(error) = sender.send((slot, Instant::now(), section)) {
                    // Expected for an abandoned provider that finishes after collection ends.
                    eprintln!(
                        "Provider {} result delivery failed: {error}",
                        provider.title
                    );
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
    let result = match catch_unwind(AssertUnwindSafe(|| (provider.collect)(ctx, &mut out))) {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            Err(win::Error::msg(
                provider.title,
                format!("provider panicked: {message}"),
            ))
        }
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
    // C# parity: HardwareInfoManager.cs:77. Partial provider output is retained.
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

/// Formats a raw report with the legacy header and ordered section headings.
pub fn full_report(sections: &[Section]) -> String {
    // C# parity: HardwareInfoManager.cs:51-52,99-103; SectionedViewForm.cs:833.
    let mut body = report::format_header("Comprehensive HWID Checker");
    for section in sections {
        body.push_str(&report::format_section(section.title, &section.body));
    }
    body
}

/// Tries sources in order, recording failures and returning the first success or last error.
#[allow(clippy::type_complexity)] // PLAN 1.6a freezes this explicit callback signature.
pub fn first_ok<T>(
    out: &mut Out,
    op: &'static str,
    sources: &[(&'static str, &dyn Fn() -> Result<T, win::Error>)],
) -> Result<T, win::Error> {
    let mut last_error = win::Error::msg(op, "no sources provided");
    for &(source, collect) in sources {
        match collect() {
            Ok(value) => {
                out.source(source);
                return Ok(value);
            }
            Err(error) => {
                out.fallback_failed(source, &error);
                last_error = error;
            }
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Barrier, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn parallel_collection_reports_completion_and_shares_one_context() {
        static STARTED: Barrier = Barrier::new(2);
        static RELEASE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
        static INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
        fn seed(ctx: &Ctx) -> win::Result<()> {
            ctx.hardware_ids
                .get_or_init(|| {
                    INITIALIZATIONS.fetch_add(1, Ordering::SeqCst);
                    Ok(HashMap::from([(
                        "USB\\VID_046D&PID_C534\\SN8D4C2A9".to_owned(),
                        "ID7F29D4".to_owned(),
                    )]))
                })
                .as_ref()
                .map_err(Clone::clone)?;
            STARTED.wait();
            Ok(())
        }
        fn slow(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
            seed(ctx)?;
            let ready = RELEASE.0.lock().expect("release gate");
            let (ready, waited) = RELEASE
                .1
                .wait_timeout_while(ready, Duration::from_secs(2), |ready| !*ready)
                .expect("release wait");
            assert!(*ready && !waited.timed_out());
            out.id(
                "ID",
                ctx.hardware_id("usb\\vid_046d&pid_c534\\sn8d4c2a9")
                    .expect("shared cache"),
            );
            Ok(())
        }
        fn fast(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
            seed(ctx)?;
            out.id(
                "ID",
                ctx.hardware_id("USB\\VID_046D&PID_C534\\SN8D4C2A9")
                    .expect("shared cache"),
            );
            Ok(())
        }
        let providers = [
            Provider {
                title: "SLOW",
                collect: slow,
            },
            Provider {
                title: "FAST",
                collect: fast,
            },
        ];
        let callbacks = Mutex::new(Vec::new());
        let sections = collect_with_deadline(
            &providers,
            None,
            &|index, section| {
                callbacks
                    .lock()
                    .expect("callbacks")
                    .push((index, section.title));
                if index == 1 {
                    *RELEASE.0.lock().expect("release gate") = true;
                    RELEASE.1.notify_one();
                }
            },
            Duration::from_secs(3),
        );
        assert_eq!(
            *callbacks.lock().expect("callbacks"),
            [(1, "FAST"), (0, "SLOW")]
        );
        assert_eq!(
            sections.iter().map(|s| s.title).collect::<Vec<_>>(),
            ["SLOW", "FAST"]
        );
        assert!(sections.iter().all(|s| s.body == "ID: ID7F29D4\r\n"
            && s.ids == ["ID7F29D4"]
            && s.failures.is_empty()));
        assert_eq!(INITIALIZATIONS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn provider_errors_and_panics_preserve_partial_output_and_diagnostics() {
        fn panics(_: &Ctx, out: &mut Out) -> win::Result<()> {
            out.info("Name", "Partial");
            panic!("fabricated panic");
        }
        fn fails(_: &Ctx, out: &mut Out) -> win::Result<()> {
            out.id("Serial", "SN8D4C2A9").source("native");
            Err(win::Error {
                op: "query",
                code: 5,
                detail: "fabricated failure".to_owned(),
            })
        }
        let providers = [
            Provider {
                title: "PANIC",
                collect: panics,
            },
            Provider {
                title: "ERROR",
                collect: fails,
            },
        ];
        let calls = Mutex::new(Vec::new());
        let sections = collect_with_deadline(
            &providers,
            None,
            &|index, _| calls.lock().expect("callbacks").push(index),
            Duration::from_secs(2),
        );
        assert_eq!(
            sections[0].body,
            "Name: Partial\r\nError retrieving PANIC information: PANIC failed: 0x00000000 provider panicked: fabricated panic\r\n"
        );
        assert_eq!(
            sections[0].failures,
            ["PANIC: PANIC failed: 0x00000000 provider panicked: fabricated panic"]
        );
        assert_eq!(
            sections[1].body,
            "Serial: SN8D4C2A9\r\nError retrieving ERROR information: query failed: 0x00000005 fabricated failure\r\n"
        );
        assert_eq!(sections[1].ids, ["SN8D4C2A9"]);
        assert_eq!(sections[1].source, "native");
        assert_eq!(
            sections[1].failures,
            ["ERROR: query failed: 0x00000005 fabricated failure"]
        );
        let mut calls = calls.into_inner().expect("callbacks");
        calls.sort_unstable();
        assert_eq!(calls, [0, 1]);
    }

    #[test]
    fn timeout_returns_without_joining_and_late_worker_can_finish() {
        static RELEASE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
        static FINISHED: OnceLock<mpsc::Sender<()>> = OnceLock::new();
        fn blocked(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
            out.text("Unfinished body");
            let ready = RELEASE.0.lock().expect("release gate");
            let (ready, _) = RELEASE
                .1
                .wait_timeout_while(ready, Duration::from_secs(2), |ready| !*ready)
                .expect("release wait");
            assert!(*ready);
            // The detached worker still owns the collection context after return.
            assert!(ctx.present_ids.get().is_none());
            FINISHED
                .get()
                .expect("finish sender")
                .send(())
                .expect("finish signal");
            Ok(())
        }
        let (sender, receiver) = mpsc::channel();
        FINISHED.set(sender).expect("finish channel initialization");
        let providers = [Provider {
            title: "BLOCKED",
            collect: blocked,
        }];
        let callbacks = Mutex::new(Vec::new());
        let sections = collect_with_deadline(
            &providers,
            None,
            &|index, section| {
                callbacks
                    .lock()
                    .expect("callbacks")
                    .push((index, section.body.clone()))
            },
            Duration::from_millis(30),
        );
        let worker_still_running = matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty));
        *RELEASE.0.lock().expect("release gate") = true;
        RELEASE.1.notify_one();
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("late worker completed");
        assert!(worker_still_running);
        assert_eq!(
            sections[0].body,
            "Error retrieving BLOCKED information: timed out\r\n"
        );
        assert_eq!(
            sections[0].failures,
            ["BLOCKED: BLOCKED failed: 0x00000000 timed out"]
        );
        assert!(sections[0].elapsed_ms >= 30);
        assert_eq!(
            *callbacks.lock().expect("callbacks"),
            [(0, sections[0].body.clone())]
        );
    }

    #[test]
    fn title_filter_preserves_original_callback_index_and_skips_other_providers() {
        fn excluded(_: &Ctx, _: &mut Out) -> win::Result<()> {
            panic!("excluded provider ran")
        }
        fn included(_: &Ctx, out: &mut Out) -> win::Result<()> {
            out.text("Selected");
            Ok(())
        }
        let providers = [
            Provider {
                title: "OTHER",
                collect: excluded,
            },
            Provider {
                title: "GPU INFO",
                collect: included,
            },
        ];
        let callbacks = Mutex::new(Vec::new());
        let sections = collect_with_deadline(
            &providers,
            Some("gpu info"),
            &|index, _| callbacks.lock().expect("callbacks").push(index),
            Duration::from_secs(2),
        );
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "GPU INFO");
        assert_eq!(sections[0].body, "Selected\r\n");
        assert_eq!(*callbacks.lock().expect("callbacks"), [1]);
        assert!(
            collect_with_deadline(
                &providers,
                Some("UNKNOWN"),
                &|_, _| panic!("unexpected callback"),
                Duration::ZERO
            )
            .is_empty()
        );
        assert!(
            collect_with_deadline(
                &[],
                None,
                &|_, _| panic!("unexpected callback"),
                Duration::ZERO
            )
            .is_empty()
        );
    }

    #[test]
    fn fallback_chain_records_order_and_short_circuits_at_first_success() {
        let calls = std::cell::RefCell::new(Vec::new());
        let native = || {
            calls.borrow_mut().push("native");
            Err::<u32, _>(win::Error::msg("native query", "unavailable"))
        };
        let wmi = || {
            calls.borrow_mut().push("WMI");
            Ok(42)
        };
        let powershell = || panic!("fallback after success ran");
        let mut out = Out::new();
        let value = first_ok(
            &mut out,
            "collect",
            &[
                ("native", &native),
                ("WMI", &wmi),
                ("PowerShell", &powershell),
            ],
        )
        .expect("successful fallback");
        assert_eq!(value, 42);
        assert_eq!(*calls.borrow(), ["native", "WMI"]);
        let section = out.finish();
        assert_eq!(section.source, "WMI");
        assert_eq!(
            section.failures,
            ["native: native query failed: 0x00000000 unavailable"]
        );
        assert!(section.body.is_empty());
    }

    #[test]
    fn fallback_chain_returns_last_error_and_handles_an_empty_chain() {
        let first = || Err::<(), _>(win::Error::msg("first", "first failure"));
        let last = || {
            Err::<(), _>(win::Error {
                op: "last",
                code: 5,
                detail: "last failure".to_owned(),
            })
        };
        let mut out = Out::new();
        let error = first_ok(&mut out, "collect", &[("native", &first), ("WMI", &last)])
            .expect_err("all sources failed");
        assert_eq!(
            (error.op, error.code, error.detail.as_str()),
            ("last", 5, "last failure")
        );
        let section = out.finish();
        assert!(section.source.is_empty());
        assert_eq!(
            section.failures,
            [
                "native: first failed: 0x00000000 first failure",
                "WMI: last failed: 0x00000005 last failure"
            ]
        );
        let mut out = Out::new();
        let error = first_ok::<()>(&mut out, "empty", &[]).expect_err("empty chain");
        assert_eq!(error.op, "empty");
        assert_eq!(error.detail, "no sources provided");
        assert!(out.finish().failures.is_empty());
    }

    #[test]
    fn context_accessors_reuse_snapshots_and_retain_errors() {
        let ctx = Ctx::new();
        ctx.hardware_ids
            .set(Ok(HashMap::from([(
                "USB\\VID_046D&PID_C534\\SN8D4C2A9".to_owned(),
                "ID7F29D4".to_owned(),
            )])))
            .expect("hardware cache");
        assert_eq!(
            ctx.hardware_id("usb\\vid_046d&pid_c534\\sn8d4c2a9"),
            Some("ID7F29D4")
        );
        assert_eq!(ctx.hardware_id("MISSING"), None);
        assert!(std::ptr::eq(
            ctx.hardware_ids().expect("map"),
            ctx.hardware_ids().expect("map")
        ));
        assert!(
            ctx.smbios
                .set(Ok(Smbios {
                    major: 3,
                    minor: 5,
                    structures: Vec::new()
                }))
                .is_ok()
        );
        assert_eq!(ctx.smbios().expect("SMBIOS").major, 3);
        assert!(std::ptr::eq(
            ctx.smbios().expect("SMBIOS"),
            ctx.smbios_result().expect("SMBIOS")
        ));
        ctx.present_ids
            .set(Ok(HashSet::from([
                "USB\\VID_046D&PID_C534\\SN8D4C2A9".to_owned()
            ])))
            .expect("present cache");
        assert_eq!(ctx.present_instance_ids().expect("present set").len(), 1);
        let failed = Ctx::new();
        failed
            .hardware_ids
            .set(Err(win::Error::msg("SetupAPI", "unavailable")))
            .expect("failed hardware cache");
        assert!(
            failed
                .smbios
                .set(Err(win::Error::msg("SMBIOS", "unavailable")))
                .is_ok()
        );
        failed
            .present_ids
            .set(Err(win::Error::msg("present IDs", "unavailable")))
            .expect("failed presence cache");
        assert!(failed.hardware_id("MISSING").is_none());
        assert!(failed.smbios().is_none());
        assert_eq!(
            failed.hardware_ids().expect_err("snapshot failure").op,
            "SetupAPI"
        );
        assert_eq!(
            failed
                .smbios_result()
                .map(|_| ())
                .expect_err("firmware failure")
                .op,
            "SMBIOS"
        );
        assert_eq!(
            failed
                .present_instance_ids()
                .expect_err("presence failure")
                .op,
            "present IDs"
        );
    }

    #[test]
    fn raw_report_literal_preserves_empty_and_untrimmed_bodies() {
        // TODO(1.9): confirm against GoldenDump --format.
        let sections = [
            Section {
                title: "GPU",
                body: "  Name: Écran 😀\r\n\r\n".to_owned(),
                ..Section::default()
            },
            Section {
                title: "CPU",
                ..Section::default()
            },
        ];
        assert_eq!(
            full_report(&sections),
            concat!(
                "=============================================================================================\r\n",
                "                                 Comprehensive HWID Checker\r\n",
                "=============================================================================================\r\n",
                "=============================================================================================\r\n",
                "                                             GPU\r\n",
                "=============================================================================================\r\n",
                "  Name: Écran 😀\r\n\r\n",
                "=============================================================================================\r\n",
                "                                             CPU\r\n",
                "=============================================================================================\r\n",
                "\r\n"
            )
        );
    }
}
