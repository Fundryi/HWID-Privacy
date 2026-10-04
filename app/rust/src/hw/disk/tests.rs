use super::sources::{logical_drive_wmi, parse_unique_ids_json};
use super::*;
use crate::hw::{self, PROVIDERS};
use serde_json::Value;
use std::time::Duration;

#[test]
#[ignore = "shared helper diagnostic queue; run alone with --ignored --test-threads=1"]
fn failsafe_disk_join_diagnostics_are_value_free_and_classified() {
    use super::sources::insert_adapter_serial;
    use std::collections::{HashMap, HashSet};
    let _ = win::take_recorded();
    let mut ids = HashMap::new();
    let mut seen = HashSet::new();
    for serial in ["CTRL-7C29D4", "CTRL-8D3F52"] {
        insert_adapter_serial(
            &mut ids,
            &mut seen,
            Some("7"),
            Some(serial),
            Some("S6PUNF0R812345X"),
            Some(11),
        );
    }
    insert_adapter_serial(
        &mut ids,
        &mut seen,
        Some("8"),
        Some("BAD\nSERIAL"),
        Some("S6PUNF0R812345X"),
        Some(11),
    );
    insert_adapter_serial(
        &mut ids,
        &mut seen,
        Some("9"),
        Some("FFFF"),
        Some("S6PUNF0R812345X"),
        Some(11),
    );
    insert_adapter_serial(&mut ids, &mut seen, Some("10"), None, None, None);
    assert!(
        proven_disk_indices(&[(Some("7".into()), Some(r"\\.\PHYSICALDRIVE9".into()))])[0].is_none()
    );
    let records = win::take_recorded();
    for class in ["ambiguous", "malformed", "placeholder", "absent"] {
        assert!(
            records.iter().any(|e| e.detail.contains(class)),
            "missing class {class}"
        );
    }
    assert!(records.iter().all(|e| !e.detail.contains("S6PUNF0R")
        && !e.detail.contains("CTRL-")
        && !e.detail.contains("BAD")));
}

#[test]
fn unique_id_guid_and_json_fixtures() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wp-01/unique-ids.json"
    ))
    .expect("fixture JSON");
    for case in fixture["cases"].as_array().expect("GUID cases") {
        assert_eq!(
            convert_unique_id_to_hex(case["input"].as_str().expect("input")),
            case["hex"],
            "{}",
            case["name"]
        );
    }
    for case in fixture["json"].as_array().expect("JSON cases") {
        let result = parse_unique_ids_json(case["input"].as_str().expect("JSON input"));
        if case["error"] == true {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(
                serde_json::to_value(result.expect("valid JSON")).expect("map"),
                case["expected"],
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn disk_index_join_rejects_duplicates_and_wrong_paths() {
    let pair = |n: &str, path: &str| (Some(n.into()), Some(path.into()));
    let input = [
        pair("9", r"\\.\PHYSICALDRIVE9"),
        pair("3", r"\\.\PHYSICALDRIVE3"),
        pair("3", r"\\.\PHYSICALDRIVE3"),
        pair("4", r"\\.\PHYSICALDRIVE5"),
        pair("-1", r"\\.\PHYSICALDRIVE0"),
        (None, None),
        pair("0", r"\\.\PHYSICALDRIVE0"),
    ];
    assert_eq!(
        proven_disk_indices(&input),
        [Some(9), None, None, None, None, None, Some(0)]
    );
}

#[test]
fn adapter_join_corroborates_and_never_restores_duplicate_index() {
    use super::sources::insert_adapter_serial;
    use std::collections::{HashMap, HashSet};
    let mut ids = HashMap::new();
    let mut seen = HashSet::new();
    for n in ["0", "1"] {
        insert_adapter_serial(
            &mut ids,
            &mut seen,
            Some(n),
            Some("CTRL-7C29D4"),
            Some("S6PUNF0R812345X"),
            Some(11),
        );
    }
    // Shared controller serial is expected; exact unit serial is still required.
    assert!(ids[&0].matches_disk("S6PUNF0R812345X"));
    assert!(ids[&1].matches_disk("S6PUNF0R812345X"));
    assert!(!ids[&0].matches_disk("S6PUNF0R823456Y"));
    for serial in ["CTRL-8D3F52", "CTRL-7C29D4"] {
        insert_adapter_serial(
            &mut ids,
            &mut seen,
            Some("0"),
            Some(serial),
            Some("S6PUNF0R812345X"),
            Some(11),
        );
        assert!(!ids.contains_key(&0));
    }
    for (n, serial, bus) in [
        ("2", "BAD\nSERIAL", 11),
        ("3", "FFFFFFFF", 11),
        ("4", "CTRL-5E4F2A", 7),
        ("5", "CTRL-5E4F2A", 8),
        ("6", "CTRL-5E4F2A", 16),
    ] {
        insert_adapter_serial(
            &mut ids,
            &mut seen,
            Some(n),
            Some(serial),
            Some("S6PUNF0R812345X"),
            Some(bus),
        );
    }
    assert!(!ids.contains_key(&2));
    assert!(!ids.contains_key(&3));
    for n in [4, 5, 6] {
        assert!(!ids[&n].matches_disk("S6PUNF0R812345X"));
    }
}

#[test]
fn repeated_unit_ids_keep_legacy_output_and_shared_controller_context() {
    let make_disk = |n| DiskInfo {
        device_id: format!(r"\\.\PHYSICALDRIVE{n}"),
        model: "Example SSD".into(),
        serial: format!("S6PUNF0R81234{n}X"),
        nvme_ids: vec![("NVMe Controller Serial", "CTRL-7C29D4".into())],
        firmware: "SVT02B6Q".into(),
        hardware_id: None,
        volumes: vec![],
        details: vec![
            ("Adapter Serial".into(), "CTRL-7C29D4".into(), true),
            ("UniqueId (WMI)".into(), format!("UNIQUE-{n}"), true),
        ],
        failures: vec![],
    };
    let mut disks: Vec<_> = (0..7).map(make_disk).collect();
    let mut baseline = Out::new();
    render_disks(&mut baseline, &disks);
    for disk in &mut disks {
        disk.nvme_ids.push((
            "NVMe Namespace UUID",
            "D197A234-51C8-4E62-9B17-A06432DF8790".into(),
        ));
        disk.details.push((
            "ATA Serial (Identify)".into(),
            "S6PUNF0R899999X".into(),
            true,
        ));
        disk.details
            .push(("ATA WWN".into(), "5002538EA1B2C3D4".into(), true));
    }
    let mut out = Out::new();
    reject_repeated_identities(&mut out, &mut disks);
    render_disks(&mut out, &disks);
    let section = out.finish();
    assert_eq!(section.body, baseline.finish().body);
    assert_eq!(section.failures.len(), 21);
    assert!(
        section
            .failures
            .iter()
            .all(|e| e.contains("implausible") && !e.contains("899999") && !e.contains("D197A234"))
    );
}

#[test]
fn ata_partial_failure_retains_tree_and_records_value_free_diagnostics() {
    let mut disk = DiskInfo {
        device_id: r"\\.\PHYSICALDRIVE7".into(),
        model: "Example SSD".into(),
        serial: "S6PUNF0R812345X".into(),
        nvme_ids: vec![],
        firmware: "SVT02B6Q".into(),
        hardware_id: None,
        volumes: vec![],
        details: vec![],
        failures: vec![],
    };
    let mut out = Out::new();
    let mut sources = vec![];
    collect_ata(
        Ok(storage::AtaIdentity {
            serial: storage::IdentifyOutcome::Failed(Error::msg(
                "ATA Identify",
                "malformed: serial encoding",
            )),
            model: storage::IdentifyOutcome::Ok("Example SSD".into()),
            firmware: storage::IdentifyOutcome::Empty,
            wwn: storage::IdentifyOutcome::Ok(0x5002538EA1B2C3D4),
        }),
        &mut out,
        &mut disk,
        &mut sources,
    );
    assert_eq!(disk.serial, "S6PUNF0R812345X");
    assert_eq!(disk.details.len(), 2);
    assert!(disk.failures[0].contains("Unavailable"));
    render_disks(&mut out, &[disk]);
    let section = out.finish();
    assert!(section.body.contains("ATA WWN: 5002538EA1B2C3D4"));
    assert!(section.failures.iter().any(|e| e.contains("malformed")));
}

#[test]
fn disk_tree_text_and_identifier_records() {
    let disks = [
        DiskInfo {
            device_id: r"\\.\PHYSICALDRIVE7".into(),
            model: "NVMe Example 2TB".into(),
            serial: "S6P8NX0W418725".into(),
            nvme_ids: vec![
                ("NVMe Controller Serial", "S6Z2NS0W847261P".into()),
                ("NVMe Namespace EUI-64", "002538B49C7216D0".into()),
                (
                    "NVMe Namespace NGUID",
                    "6AB193E852CD4F70A4319027DB658CE2".into(),
                ),
            ],
            firmware: "7B2QEXM7".into(),
            hardware_id: Some(("SCSI\\DiskNVMe_Example_2TB".into(), true)),
            failures: vec![],
            volumes: vec![
                ("C".into(), "A1B2C3D4".into()),
                ("F".into(), "79A34D81".into()),
            ],
            details: vec![
                (
                    "UniqueId (IOCTL)".into(),
                    "50:00:C5:02:A9:37:16:B2".into(),
                    true,
                ),
                ("UniqueId (IOCTL) decoded".into(), "<empty>".into(), false),
                (
                    "Partition Style: GPT | Disk GUID".into(),
                    "72C3A490-8B16-4DF2-9A0C-6F21B497E853".into(),
                    true,
                ),
                (
                    "  Partition GUID".into(),
                    "5E7AC148-3B72-4CB4-892A-73A1F624B8D9".into(),
                    true,
                ),
            ],
        },
        DiskInfo {
            device_id: r"\\.\PHYSICALDRIVE9".into(),
            model: "Unknown Model".into(),
            serial: "Unknown Serial".into(),
            nvme_ids: vec![],
            firmware: String::new(),
            hardware_id: None,
            volumes: vec![],
            details: vec![],
            failures: vec![
                "    UniqueId (IOCTL): storage query failed: 0x00000005 fabricated denial".into(),
            ],
        },
    ];
    let mut out = Out::new();
    render_disks(&mut out, &disks);
    let section = out.finish();
    let expected: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/wp-01/disk-tree.json"))
            .expect("tree fixture");
    assert_eq!(section.body, expected["body"]);
    for value in [
        "A1B2C3D4",
        "79A34D81",
        "S6P8NX0W418725",
        "S6Z2NS0W847261P",
        "002538B49C7216D0",
        "6AB193E852CD4F70A4319027DB658CE2",
        "SCSI\\DiskNVMe_Example_2TB",
        "50:00:C5:02:A9:37:16:B2",
        "72C3A490-8B16-4DF2-9A0C-6F21B497E853",
        "5E7AC148-3B72-4CB4-892A-73A1F624B8D9",
    ] {
        assert!(
            section.ids.iter().any(|id| id == value),
            "unrecorded identifier: {value}"
        );
    }
    assert!(!section.ids.iter().any(|id| id == "<empty>"));
}

#[test]
#[ignore = "read-only live capture; prints real identifiers, redirect output to private golden/wp-01"]
fn wp01_capture_nonadmin() {
    use std::{sync::mpsc, thread};
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        capture_nonadmin();
        let _ = send.send(()); // The capture may have timed out.
    });
    receive
        .recv_timeout(Duration::from_secs(60))
        .expect("provider and source-comparison capture deadline");
}

fn capture_nonadmin() {
    use std::{fs, path::Path};
    let folder = Path::new(r"D:\GIT\HWID-Privacy\app\rust\golden\wp-01");
    fs::create_dir_all(folder).expect("private capture folder");
    let section = hw::collect_provider(&PROVIDERS[0], &Ctx::new());
    fs::write(folder.join("rust-nonadmin.txt"), &section.body).expect("private text");
    fs::write(
        folder.join("rust-nonadmin.diag.txt"),
        format!(
            "elevated={}\r\nelapsed_ms={}\r\nsource={}\r\n{}\r\n",
            win::security::is_admin(),
            section.elapsed_ms,
            section.source,
            section.failures.join("\r\n")
        ),
    )
    .expect("private diagnostics");
    println!("{}", section.body);
    assert!(
        !section
            .body
            .contains("Error retrieving DISK DRIVES information:")
    );

    // Independently compare native volume values to the legacy WMI source.
    let mut checks = String::new();
    for letter in storage::drive_letters().expect("drive letters") {
        match storage::volume(letter) {
            Ok(Some(native)) => match logical_drive_wmi(letter) {
                Ok(wmi) => {
                    let same = wmi.iter().any(|v| {
                        v.physical == format!(r"\\.\PHYSICALDRIVE{}", native.disk_number)
                            && v.serial == native.serial
                    });
                    checks.push_str(&format!(
                        "{letter}: native disk={} serial={} WMI equal={same}\r\n",
                        native.disk_number, native.serial
                    ));
                    for row in wmi {
                        checks.push_str(&format!(
                            "  WMI {} {} {}\r\n",
                            row.physical, row.letter, row.serial
                        ));
                    }
                }
                Err(error) => checks.push_str(&format!(
                    "{letter}: WMI comparison unavailable: {error}\r\n"
                )),
            },
            Ok(None) => {}
            Err(error) => checks.push_str(&format!(
                "{letter}: native comparison unavailable: {error}\r\n"
            )),
        }
    }
    let wmi_ids = unique_ids_wmi();
    let ps_ids = unique_ids_powershell();
    checks.push_str(&format!(
        "Storage WMI vs PowerShell UniqueIds equal={}\r\n",
        matches!((&wmi_ids, &ps_ids), (Ok(a), Ok(b)) if a == b)
    ));
    checks.push_str(&format!("WMI: {wmi_ids:?}\r\nPowerShell: {ps_ids:?}\r\n"));
    fs::write(folder.join("source-comparison.txt"), checks).expect("private comparison");
}
