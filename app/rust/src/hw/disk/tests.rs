use super::sources::{logical_drive_wmi, parse_unique_ids_json};
use super::*;
use crate::hw::{self, PROVIDERS};
use serde_json::Value;
use std::time::Duration;

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
