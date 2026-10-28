use super::collection::collect_with_deadline;
use super::*;
use std::sync::{
    Barrier, Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::{sync::mpsc, time::Duration};

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
    assert!(
        sections.iter().all(|s| s.body == "ID: ID7F29D4\r\n"
            && s.ids == ["ID7F29D4"]
            && s.failures.is_empty())
    );
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
    ctx.anticheat
        .set(win::anticheat::Detection::default())
        .expect("fabricated unknown anti-cheat snapshot");
    assert_eq!(
        ctx.anticheat().presence(),
        win::anticheat::Presence::Unknown
    );
    assert!(std::ptr::eq(ctx.anticheat(), ctx.anticheat()));
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
fn anticheat_guidance_preserves_bytes_sources_ids_and_export_context() {
    use win::anticheat::{DetectedAntiCheat, Detection, Evidence, FailedLayer, Gate, Layer};

    let detected = vec![
        DetectedAntiCheat {
            id: "vanguard".into(),
            name: "Riot Vanguard".into(),
            gate: Gate::Must,
            kernel_mode: true,
            boot_resident: true,
            games_sample: vec!["VALORANT".into()],
            evidence: vec![
                Evidence {
                    layer: Layer::Service,
                    matched_name: "vgc".into(),
                },
                Evidence {
                    layer: Layer::Driver,
                    matched_name: "vgk.sys".into(),
                },
            ],
        },
        DetectedAntiCheat {
            id: "hyperion".into(),
            name: "Hyperion / Byfron".into(),
            gate: Gate::Should,
            kernel_mode: false,
            boot_resident: false,
            games_sample: Vec::new(),
            evidence: vec![Evidence {
                layer: Layer::Process,
                matched_name: "RobloxPlayerBeta.exe".into(),
            }],
        },
        DetectedAntiCheat {
            id: "vac".into(),
            name: "Valve Anti-Cheat / VACnet".into(),
            gate: Gate::Dont,
            kernel_mode: false,
            boot_resident: false,
            games_sample: Vec::new(),
            evidence: vec![Evidence {
                layer: Layer::Process,
                matched_name: "cs2.exe".into(),
            }],
        },
    ];
    let failure = FailedLayer {
        layer: Layer::Driver,
        error: win::Error {
            op: "K32EnumDeviceDrivers",
            code: 5,
            detail: "arbitrary enumeration string must stay private".into(),
        },
    };
    let clear = "Anti-cheat: No supported anti-cheat detected (snapshot only)\r\n";
    let unknown = concat!(
        "Anti-cheat: Status unavailable\r\n",
        "Hardware check may be less reliable. Optional driver reads are disabled because anti-cheat status could not be verified.\r\n"
    );
    let found = concat!(
        "Anti-cheat: Running or detected (Riot Vanguard, Hyperion / Byfron, Valve Anti-Cheat / VACnet)\r\n",
        "Hardware check may be less reliable while anti-cheat is active. Some hardware tools refuse to run.\r\n",
        "Anti-cheat evidence: Riot Vanguard (kernel anti-cheat); service: vgc; loaded driver: vgk.sys\r\n",
        "Anti-cheat evidence: Hyperion / Byfron (intrusive); process: RobloxPlayerBeta.exe\r\n",
        "Anti-cheat evidence: Valve Anti-Cheat / VACnet (lightweight); process: cs2.exe\r\n"
    );
    for (version, entries, failures, guidance) in [
        (Some(1), Vec::new(), Vec::new(), clear),
        (None, Vec::new(), Vec::new(), unknown),
        (Some(2), Vec::new(), Vec::new(), unknown),
        (Some(1), Vec::new(), vec![failure.clone()], unknown),
        (Some(1), detected.clone(), Vec::new(), found),
        (Some(1), detected, vec![failure], found),
    ] {
        let snapshot = Detection {
            database_version: version,
            detected: entries,
            failed_layers: failures,
        };
        let mut out = Out::new();
        out.id("Machine GUID", "b27f41e8-902c-4e63-a185-7d36c04982ba")
            .text("Existing field/error: Écran 😀")
            .separator()
            .blank()
            .source("existing source");
        let mut baseline = out.finish();
        baseline.title = "SYSTEM INFORMATION";
        let mut out = Out::new();
        out.text(baseline.body.strip_suffix("\r\n").unwrap())
            .id_value(&baseline.ids[0])
            .source(&baseline.source);
        system::append_anticheat(&snapshot, &mut out);
        let mut section = out.finish();
        section.title = baseline.title;
        assert_eq!(section.body, format!("{}{guidance}", baseline.body));
        assert_eq!(section.ids, baseline.ids);
        assert_eq!(section.source, baseline.source);
        assert_eq!(
            report::masked(&section).body,
            format!("{}{guidance}", report::masked(&baseline).body)
        );
        assert!(!section.body.contains("VALORANT"));
        assert!(!section.body.contains("Cannot read deep hardware"));
        assert!(!section.body.contains("Deep data unavailable"));
        let unchanged = Section {
            title: "RAM MODULES",
            body: "Name: Écran 😀\r\n  padded table  \r\n".into(),
            ..Section::default()
        };
        let before = full_report(&[baseline, unchanged.clone()]);
        let after = full_report(&[section.clone(), unchanged]);
        assert_eq!(
            after.replacen(guidance, "", 1).as_bytes(),
            before.as_bytes()
        );
        if snapshot.failed_layers.is_empty() {
            assert!(section.failures.is_empty());
        } else {
            assert_eq!(
                section.failures,
                [
                    "Anti-cheat: K32EnumDeviceDrivers failed: 0x00000005 anti-cheat Driver layer unavailable"
                ]
            );
            let diagnostics = report::diagnostics(&[section], &[], false);
            assert!(!diagnostics.contains("arbitrary enumeration string"));
        }
    }
    assert_eq!(PROVIDERS.len(), 15);
}

#[test]
fn raw_report_literal_preserves_empty_and_untrimmed_bodies() {
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

// Security decisions and title attribution cannot be exercised for every AC on one real PC.
#[test]
fn deep_option_preserves_off_bytes_and_names_only_blocking_evidence() {
    use win::anticheat::{DetectedAntiCheat, Detection, Evidence, FailedLayer, Gate, Layer};
    fn family(
        name: &str,
        gate: Gate,
        games: &[&str],
        layer: Layer,
        matched: &str,
    ) -> DetectedAntiCheat {
        DetectedAntiCheat {
            id: name.into(),
            name: name.into(),
            gate,
            kernel_mode: true,
            boot_resident: false,
            games_sample: games.iter().map(|game| (*game).into()).collect(),
            evidence: vec![Evidence {
                layer,
                matched_name: matched.into(),
            }],
        }
    }
    fn hardware(_: &Ctx, out: &mut Out) -> win::Result<()> {
        out.id("Serial", "SN8D4C2A9")
            .text("  Écran 😀 table padding  ");
        Ok(())
    }
    let clear = Detection {
        database_version: Some(1),
        ..Detection::default()
    };
    let dont = family(
        "Valve Anti-Cheat / VACnet",
        Gate::Dont,
        &["Counter-Strike 2"],
        Layer::Process,
        "steam.exe",
    );
    let must = family(
        "miHoYo / HoYoverse Protect",
        Gate::Must,
        &["Genshin Impact", "Honkai: Star Rail"],
        Layer::Process,
        "GENSHINIMPACT.EXE",
    );
    let should = family(
        "Hyperion / Byfron",
        Gate::Should,
        &["Roblox"],
        Layer::Service,
        "fixture-service",
    );
    let failure = FailedLayer {
        layer: Layer::Driver,
        error: win::Error::msg("fixture", "access denied"),
    };
    let providers = [
        Provider {
            title: "SYSTEM INFORMATION",
            collect: hardware,
        },
        Provider {
            title: "RAM MODULES",
            collect: hardware,
        },
    ];
    for only in [None, Some("system information"), Some("ram modules")] {
        let mut baseline: Option<String> = None;
        for deep_hardware_information in [false, true] {
            let ctx = Ctx::with_options(CollectionOptions {
                deep_hardware_information,
            });
            ctx.anticheat.set(clear.clone()).unwrap();
            let sections = super::collection::collect_with_context(
                &providers,
                only,
                &|_, _| {},
                Duration::from_secs(2),
                ctx,
            );
            let report = full_report(&sections);
            if deep_hardware_information {
                assert_eq!(
                    report
                        .matches("Deep data unavailable on this system.")
                        .count(),
                    1
                );
                assert_eq!(
                    report.replace("Deep data unavailable on this system.\r\n", ""),
                    baseline.as_ref().unwrap().as_str()
                );
            } else {
                baseline = Some(report);
            }
        }
    }
    for (snapshot, expected, ram_skip) in [
        (clear.clone(), "Deep data unavailable on this system.", None),
        (
            Detection {
                detected: vec![dont.clone()],
                ..clear.clone()
            },
            "Deep data unavailable on this system.",
            None,
        ),
        (
            Detection::default(),
            "Cannot read deep hardware data right now: anti-cheat status could not be verified.",
            Some("SPD (driver): Skipped - anti-cheat status unavailable"),
        ),
        (
            Detection {
                detected: vec![dont.clone()],
                failed_layers: vec![failure.clone()],
                ..clear.clone()
            },
            "Cannot read deep hardware data right now: anti-cheat status could not be verified.",
            Some("SPD (driver): Skipped - anti-cheat status unavailable"),
        ),
        (
            Detection {
                detected: vec![must.clone(), should.clone(), dont.clone()],
                failed_layers: vec![failure.clone()],
                ..clear.clone()
            },
            "Cannot read deep hardware data right now: Genshin Impact / miHoYo / HoYoverse Protect, Hyperion / Byfron is running.",
            Some(
                "SPD (driver): Skipped - anti-cheat detected (Genshin Impact / miHoYo / HoYoverse Protect, Hyperion / Byfron)",
            ),
        ),
        (
            Detection {
                detected: vec![should],
                ..clear.clone()
            },
            "Cannot read deep hardware data right now: Hyperion / Byfron is running.",
            Some("SPD (driver): Skipped - anti-cheat detected (Hyperion / Byfron)"),
        ),
    ] {
        for title in ["SYSTEM INFORMATION", "RAM MODULES", "CPU"] {
            let provider = Provider {
                title,
                collect: hardware,
            };
            let off = Ctx::new();
            off.anticheat.set(snapshot.clone()).unwrap();
            let baseline = collect_provider(&provider, &off);
            assert_eq!(
                baseline.body,
                "Serial: SN8D4C2A9\r\n  Écran 😀 table padding  \r\n"
            );
            assert!(off.deep_status.get().is_none());
            let on = Ctx::with_options(CollectionOptions {
                deep_hardware_information: true,
            });
            on.anticheat.set(snapshot.clone()).unwrap();
            if snapshot.gate_decision() != win::anticheat::GateDecision::Clear {
                on.prepare_deep_information();
                assert!(
                    on.deep_spd.get().is_none(),
                    "gate refusal cannot reach the source factory"
                );
            }
            let section = collect_provider(&provider, &on);
            assert!(section.body.starts_with(&baseline.body));
            assert_eq!(section.ids, baseline.ids);
            assert_eq!(section.source, baseline.source);
            assert_eq!(section.failures, baseline.failures);
            if title == "CPU" {
                assert_eq!(section.body.as_bytes(), baseline.body.as_bytes());
                continue;
            }
            assert!(section.body.contains(expected));
            assert_eq!(
                section.body.contains("SPD (driver): Skipped"),
                title == "RAM MODULES" && ram_skip.is_some()
            );
            if title == "RAM MODULES"
                && let Some(skip) = ram_skip
            {
                assert!(section.body.contains(skip));
            }
            assert!(
                !section
                    .body
                    .contains("is running.\r\nDeep data unavailable")
            );
            assert!(!section.body.contains("Valve Anti-Cheat"));
            assert!(!section.body.contains("Honkai: Star Rail"));
            if section.body.contains("Hyperion") {
                assert!(section.body.contains(
                    "Associated games (Hyperion / Byfron; active title unknown): Roblox"
                ));
                assert!(!section.body.contains("Roblox / Hyperion"));
            }
            assert_eq!(
                report::masked(&section)
                    .body
                    .strip_prefix(&report::masked(&baseline).body),
                section.body.strip_prefix(&baseline.body)
            );
            assert!(full_report(std::slice::from_ref(&section)).contains(expected));
            // A cached outcome is reused; no fresh detector or hardware-source call exists here.
            assert!(std::ptr::eq(
                on.deep_information_status().unwrap(),
                on.deep_information_status().unwrap()
            ));
        }
    }
}
