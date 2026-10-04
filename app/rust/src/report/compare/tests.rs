use super::*;

#[test]
fn new_identity_labels_and_placeholders_have_text_export_verdicts() {
    for label in [
        "NVMe Namespace UUID",
        "NVMe Namespace NGUID",
        "Storage ID (port, vendor ID, ASCII)",
        "Serial Number",
        "PDI",
        "Board Part Number",
        "Instance ID",
        "Endpoint ID",
        "Endpoint Container ID",
        "Stable ID",
        "Battery Serial",
        "Battery Unique ID",
        "Battery Serial (SMBIOS)",
        "Serial (SMBIOS)",
        "Asset Tag (SMBIOS)",
        "Socket Asset Tag (SMBIOS)",
    ] {
        let diff = difference(
            "IDENTITY",
            &format!("{label}: ID-4A37"),
            &format!("{label}: ID-4A38"),
        );
        let field = &diff.entities[0].fields[0];
        assert!(field.identifier && field.kind == Kind::Changed, "{label}");
        for placeholder in [
            "NA",
            "To be filled by OEM",
            "Unavailable (probe failed)",
            "11111111",
            "GPU-00000000-0000-0000-0000-000000000000",
            "{0.0.1.00000000}.{FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF}",
        ] {
            let body = format!("{label}: {placeholder}");
            let diff = difference("IDENTITY", &body, &body);
            assert!(
                diff.entities[0].fields[0].not_unique(),
                "{label}: {placeholder}"
            );
        }
    }
    for label in [
        "Hardware IDs",
        "Component ID (SMBIOS)",
        "Socket Part Number (SMBIOS)",
        "TPM Firmware Version (SMBIOS)",
        "VBIOS Version",
    ] {
        assert!(
            !identifier_label(label),
            "context, not unit identity: {label}"
        );
    }
    assert!(!generic_value("0000_0000_0000_0001."));
}

#[test]
fn audio_adapters_and_endpoints_pair_independently_when_reordered() {
    let adapter = "Adapter: USB Audio Device\nInstance ID: USB\\VID_046D&PID_0A9F\\A7C28E41\nContainer ID: {52B18C39-7D64-4AF0-963E-826A19DB4507}\n";
    let mic = "Endpoint: Microphone\nDirection: Capture\nEndpoint ID: {0.0.1.00000000}.{941d59a2-72ce-4536-a71a-66af60dd2788}\nStable ID: microphone-A7C28E41\n";
    let speaker = "Endpoint: Speakers\nDirection: Render\nEndpoint ID: {0.0.0.00000000}.{6d73e082-684f-4130-a0cb-fb538d1c3279}\n";
    let before = format!("{adapter}{mic}{speaker}");
    let after = format!("{adapter}{speaker}{mic}");
    let diff = difference("AUDIO DEVICES", &before, &after);
    assert_eq!(diff.entities.len(), 3);
    assert_eq!(diff.entities[0].kind, Kind::Same);
    assert!(
        diff.entities[1..]
            .iter()
            .all(|e| e.kind == Kind::Moved && e.fields.iter().all(|f| f.kind == Kind::Same))
    );
    let changed = after
        .replace("microphone-A7C28E41", "microphone-B8D39F52")
        .replace(
            "941d59a2-72ce-4536-a71a-66af60dd2788",
            "a52e6ab3-83df-4647-b82b-77bf71ee3899",
        );
    let diff = difference("AUDIO DEVICES", &before, &changed);
    assert_eq!(
        diff.entities
            .iter()
            .flat_map(|e| &e.fields)
            .filter(|f| f.identifier && f.kind == Kind::Changed)
            .count(),
        2
    );
    let twins = format!("{adapter}{speaker}{speaker}");
    let diff = difference("AUDIO DEVICES", &twins, &twins);
    assert!(diff.entities.iter().all(|e| e.kind == Kind::Same));
}

#[test]
fn battery_sources_stay_separate_and_numbered_positions_are_not_identity() {
    let pack = |n, serial| {
        format!(
            "Battery: #{n}\nBattery Name: L18M3P73\nBattery Manufacturer: SMP\nBattery Serial: {serial}\nBattery Unique ID: SMP-{serial}\n"
        )
    };
    let firmware =
        "SMBIOS Battery: #1\nBattery Name (SMBIOS): L18M3P73\nBattery Serial (SMBIOS): 4A37\n";
    let before = format!(
        "{}----------------------------------------\n{}----------------------------------------\n{firmware}",
        pack(1, "BAT2402A7381"),
        pack(2, "BAT2402A7382")
    );
    let after = format!(
        "{}----------------------------------------\n{}----------------------------------------\n{firmware}",
        pack(1, "BAT2402A7382"),
        pack(2, "BAT2402A7381")
    );
    let diff = difference("BATTERY", &before, &after);
    assert_eq!(diff.entities.len(), 3);
    assert!(
        diff.entities[..2]
            .iter()
            .all(|e| e.kind == Kind::Moved && e.fields.iter().all(|f| f.kind == Kind::Same))
    );
    assert_eq!(diff.entities[2].kind, Kind::Same);
    let diff = difference(
        "BATTERY",
        &pack(1, "BAT2402A7381"),
        &pack(1, "BAT2402A8391"),
    );
    assert_eq!(diff.entities.len(), 1);
    assert_eq!(diff.entities[0].kind, Kind::Changed);
    let twins = format!(
        "{}----------------------------------------\n{}",
        pack(1, "0000"),
        pack(2, "0000")
    );
    let diff = difference("BATTERY", &twins, &twins);
    assert!(diff.entities.iter().all(|e| e.kind == Kind::Same));
    let diff = difference("BATTERY", &pack(1, "BAT2402A7381"), firmware);
    assert_eq!(
        diff.entities.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::Removed, Kind::Added]
    );
}

#[test]
fn firmware_lists_keep_legacy_headers_and_pair_singletons_twins_and_moves() {
    for (section, prefix, model, unit) in [
        ("CHASSIS", "Power Supply", "Model/Part", "Serial"),
        (
            "(SM)BIOS",
            "Firmware Component",
            "Component Name",
            "Component ID",
        ),
        (
            "CPU",
            "CPU Socket",
            "Socket Part Number",
            "Socket Asset Tag",
        ),
        (
            "TPM MODULES",
            "TPM Firmware",
            "TPM Description",
            "TPM Firmware Version",
        ),
    ] {
        let base = "Legacy: unchanged\n";
        let fields = |name, id| format!("{model} (SMBIOS): {name}\n{unit} (SMBIOS): {id}\n");
        let one = fields("Model A", "UNIT-4A37");
        let two = fields("Model B", "UNIT-5B48");
        let before = format!("{base}{prefix} #1 (SMBIOS)\n{one}{prefix} #2 (SMBIOS)\n{two}");
        let after = format!("{base}{prefix} #1 (SMBIOS)\n{two}{prefix} #2 (SMBIOS)\n{one}");
        let diff = difference(section, &before, &after);
        assert_eq!(diff.entities.len(), 3, "{section}");
        assert!(diff.entities[0].is_header);
        assert!(
            diff.entities[1..].iter().all(|e| e.kind == Kind::Moved),
            "{section}"
        );
        let diff = difference(section, &format!("{base}{one}"), &before);
        assert_eq!(diff.entities[0].kind, Kind::Same);
        assert_eq!(
            diff.entities[1].kind,
            Kind::Same,
            "singleton gained heading: {section}"
        );
        assert_eq!(diff.entities[2].kind, Kind::Added);
        let diff = difference(
            section,
            &format!("{base}{one}"),
            &format!("{base}{}", one.replace("UNIT-4A37", "UNIT-6C59")),
        );
        assert_eq!(
            diff.entities[1].kind,
            Kind::Changed,
            "unique model: {section}"
        );
        let twins = before.replace(&two, &one);
        let diff = difference(section, &twins, &twins);
        assert!(
            diff.entities.iter().all(|e| e.kind == Kind::Same),
            "twins: {section}"
        );
        let diff = difference(section, base, &format!("{base}{one}"));
        assert_eq!(
            diff.entities[0].kind,
            Kind::Same,
            "old header preserved: {section}"
        );
        assert_eq!(diff.entities[1].kind, Kind::Added);
    }
}

#[test]
fn new_gpu_details_and_storage_ids_remain_with_their_device() {
    let gpu = "GPU 0\n└── NVIDIA GeForce RTX 3070\n    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504c0\n\nSerial Number: 032482719630\nPDI: 08F47A2196BC3D50\nBoard Part Number: 900-1G141-2530-000\nVBIOS Version: 94.04.3A.00.71\nGPU 1\n└── Intel UHD Graphics 770\n";
    let parsed = parse(&format!("===== GPU INFO =====\n{gpu}"), false).unwrap();
    assert_eq!(parsed.sections[0].entities[0].rows.len(), 6);
    assert_eq!(parsed.sections[0].entities[1].rows.len(), 1);
    let qualified = gpu
        .replace("\nPDI:", "\nGPU 0 PDI:")
        .replace("\nSerial Number:", "\nGPU 0 Serial Number:");
    assert!(
        difference("GPU INFO", gpu, &qualified)
            .entities
            .iter()
            .all(|e| e.kind == Kind::Same)
    );
    let disk = |n, uuid| {
        format!(
            "└── PHYSICALDRIVE{n}\n    ├── Model: Samsung SSD 980\n    ├── NVMe Namespace UUID: {uuid}\n    └── Storage ID (port, vendor ID, ASCII): Port_4A729C1E\n"
        )
    };
    let diff = difference(
        "DISK DRIVES",
        &disk(0, "71A25E94-D6B8-4C02-9F31-826C50A7B493"),
        &disk(2, "71A25E94-D6B8-4C02-9F31-826C50A7B494"),
    );
    assert_eq!(diff.entities.len(), 1);
    assert!(
        diff.entities[0]
            .fields
            .iter()
            .any(|f| f.label == "NVMe Namespace UUID" && f.identifier && f.kind == Kind::Changed)
    );
}

#[test]
fn diagnostics_are_rejected_even_if_a_helper_contains_export_like_text() {
    let sections = [super::super::Section {
        title: "BATTERY",
        source: "Battery IOCTL".into(),
        elapsed_ms: 3,
        ..Default::default()
    }];
    let text = super::super::diagnostics(&sections, &[], false);
    assert!(parse(&text, false).unwrap().is_empty());
    let injected = format!("{text}===== BATTERY =====\nBattery Serial: BAT2402A7381\n");
    assert!(parse(&injected, false).unwrap().is_empty());
    // Extension rejection precedes file IO and also covers masked/case variants.
    for path in ["not-present.diag.txt", "HWID-EXPORT-MASKED.DIAG.TXT"] {
        assert!(matches!(read(Path::new(path)), Err(ReadError::Empty(_))));
    }
}

#[test]
fn vanished_virtual_nic_does_not_change_survivors() {
    let before = parse("===== NETWORK ADAPTERS (NIC's) =====\nName: Virtual\nMAC Address: 02:41:67:93:A8:2C\n----------------------------------------\nName: Ethernet\nMAC Address: 3C:FD:FE:72:19:A6\n----------------------------------------\nName: Wi-Fi\nMAC Address: 00:1B:21:73:95:C4", false).unwrap();
    let after = parse("===== NETWORK ADAPTERS (NIC's) =====\nName: Ethernet\nMAC Address: 3C:FD:FE:72:19:A6\n----------------------------------------\nName: Wi-Fi\nMAC Address: 00:1B:21:73:95:C4", false).unwrap();
    let diff = compare(&before, &after, Path::new("a"), Path::new("b"));
    assert!(
        diff.summary
            .starts_with("Changed 0 · Added 0 · Removed 2 · Same 4"),
        "{}",
        diff.summary
    );
    assert_eq!(
        diff.entities.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::Removed, Kind::Moved, Kind::Moved]
    );
    assert!(
        diff.entities[1..]
            .iter()
            .flat_map(|e| &e.fields)
            .all(|f| f.kind == Kind::Same)
    );
    assert!(diff.entities[0].fields.iter().all(|f| !f.identifier));
    assert!(
        diff.entities[1..]
            .iter()
            .flat_map(|e| &e.fields)
            .filter(|f| f.label == "MAC Address")
            .all(|f| f.identifier)
    );
}

#[test]
fn identifier_verdicts_distinguish_generic_pairs_and_real_changes() {
    let values = [
        ("Serial", "S6B2NJ0R414C7E", "S6B2NJ0R409Q3M", true),
        (
            "MAC Address",
            "3C:FD:FE:72:19:A6",
            "3C:FD:FE:72:19:A6",
            true,
        ),
        ("EUI-64", "002538B731804A29", "002538B731804A38", true),
        ("WWN", "5002538B731804A2", "5002538B731804A2", true),
        ("PDI", "0000000000004817", "0000000000004918", true),
        (
            "UUID",
            "80D942C7-651B-4D30-AE82-17C9B6435F28",
            "80D942C7-651B-4D30-AE82-17C9B6435F29",
            true,
        ),
        (
            "Partition GUID",
            "4E67A291-AB36-4F82-976D-C51840B79E23",
            "4E67A291-AB36-4F82-976D-C51840B79E23",
            true,
        ),
        ("UniqueId", "5002538B731804A2", "5002538B731804B3", true),
        (
            "Container ID",
            "80D942C7-651B-4D30-AE82-17C9B6435F28",
            "80D942C7-651B-4D30-AE82-17C9B6435F28",
            true,
        ),
        ("Storage ID", "5002538B731804A2", "5002538B731804B3", true),
        (
            "Hardware ID",
            "PCI\\VEN_8086&DEV_15F3",
            "PCI\\VEN_8086&DEV_15F3",
            false,
        ),
        (
            "Device ID",
            "PCI\\VEN_8086&DEV_15F3",
            "PCI\\VEN_8086&DEV_15F3",
            false,
        ),
        ("Firmware", "5B2QGXA7", "5B2QGXA8", false),
        ("Serial", "To be filled by O.E.M.", "S6B2NJ0R409Q3M", true),
        (
            "Serial",
            "S6B2NJ0R409Q3M",
            "Unavailable (read failed)",
            true,
        ),
        ("Serial", "0000-0000", "S6B2NJ0R409Q3M", true),
        ("Serial", "", "S6B2NJ0R409Q3M", true),
        ("Serial", "XXXXXXXX", "XXXXXXXX", true),
    ];
    for (label, old, new, expected) in values {
        let left = Entity {
            rows: vec![(label.into(), old.into())],
            ..Default::default()
        };
        let right = Entity {
            rows: vec![(label.into(), new.into())],
            ..Default::default()
        };
        let paired = fields(Some(&left), Some(&right));
        assert_eq!(paired[0].identifier, expected, "{label}: {old} -> {new}");
        assert_eq!(
            paired[0].kind,
            if old == new {
                Kind::Same
            } else {
                Kind::Changed
            }
        );
        assert!(!fields(Some(&left), None)[0].identifier);
        assert!(!fields(None, Some(&right))[0].identifier);
    }
    // Either provider's ID list can mark an otherwise unrecognized field label.
    let mut left = Entity {
        rows: vec![("Opaque token".into(), "02A791B8".into())],
        ..Default::default()
    };
    let mut right = Entity {
        rows: vec![("Opaque token".into(), "02A791C9".into())],
        ..Default::default()
    };
    left.ids.push("02A791B8".into());
    assert!(fields(Some(&left), Some(&right))[0].identifier);
    left.ids.clear();
    right.ids.push("02A791C9".into());
    assert!(fields(Some(&left), Some(&right))[0].identifier);
    left.rows[0].1 = "00000000".into();
    assert!(fields(Some(&left), Some(&right))[0].identifier);

    for generic in [
        "",
        "  ",
        "null",
        "None",
        "N/A",
        "Not Available",
        "Unknown",
        "<empty>",
        "00000000",
        "0000000000000",
        "0000_0000_0000_0000.",
        "0x00000000",
        "00000000-0000-0000-0000-000000000000",
        "00:00:00:00:00:00",
        "FFFFFFFF",
        "FF:FF:FF:FF:FF:FF",
        "XXXXXXXX",
        "xx-xx",
        "0xFFFFFFFF",
        "Default string",
        "To Be Filled By O.E.M.",
        "To be filled by OEM",
        "System Serial Number",
        "System Product Name",
        "Not Specified",
        "Not Applicable",
        "OEM",
        "0123456789",
        "1234567890",
        "Chassis Serial Number",
        "Base Board Serial Number",
        "Baseboard Serial Number",
    ] {
        let variant = format!("  {}  ", generic.to_uppercase());
        assert!(generic_value(&variant), "{generic}");
        for (old, new, neutral) in [
            (generic, generic, true),
            (generic, "Default string", true),
            (generic, "S6B2NJ0R409Q3M", false),
            ("S6B2NJ0R409Q3M", generic, false),
        ] {
            let diff = difference("CPU", &format!("Serial: {old}"), &format!("Serial: {new}"));
            assert!(
                diff.warning.is_none(),
                "X placeholders are not mask metadata"
            );
            let field = &diff.entities[0].fields[0];
            assert!(field.identifier, "{old} -> {new}");
            assert_eq!(field.not_unique(), neutral, "{old} -> {new}");
            assert_eq!(
                field.kind,
                if old == new {
                    Kind::Same
                } else {
                    Kind::Changed
                }
            );
        }
        let entity = Entity {
            rows: vec![("Serial".into(), variant)],
            ..Default::default()
        };
        assert_eq!(
            score(&entity, &entity),
            0,
            "{generic} must not match devices"
        );
        assert!(fields(Some(&entity), None)[0].not_unique());
        assert!(fields(None, Some(&entity))[0].not_unique());
    }
    for real in [
        "0000_0000_0000_0001.",
        "00000001",
        "00:00:00:00:00:01",
        "FFFF01",
        "XXX1",
    ] {
        assert!(!generic_value(real), "{real}");
        let diff = difference(
            "CPU",
            &format!("Serial: {real}"),
            &format!("Serial: {real}"),
        );
        assert!(diff.entities[0].fields[0].identifier);
        assert!(!diff.entities[0].fields[0].not_unique());
    }

    // Export trims a final empty value's trailing space; retain that identifier row.
    let empty = super::super::export_text(&[super::super::Section {
        title: "CPU",
        body: "Serial: \r\n".into(),
        ..Default::default()
    }]);
    let empty = parse(&empty, false).unwrap();
    let diff = compare(&empty, &empty, Path::new("a"), Path::new("b"));
    assert!(diff.entities[0].fields[0].not_unique());

    // TXT has no provider ID metadata: every collected identity label must stand alone.
    for label in [
        "Adapter Serial",
        "ATA Serial (Identify)",
        "Permanent MAC (OID)",
        "Container ID",
        "Thumbprint",
        "Sha256 Hash",
        "Serial Number (Product ID)",
        "Machine GUID",
        "Hardware Profile GUID",
        "Volume-SN",
        "Disk GUID",
        "Partition GUID",
        "Disk Signature",
        "UniqueId (IOCTL)",
        "UniqueId (IOCTL) decoded",
        "UniqueId (WMI)",
        "UniqueId (WMI) decoded",
        "NVMe Controller Serial",
        "NVMe Namespace EUI-64",
        "NVMe Namespace NGUID",
        "NVMe Namespace UUID",
        "UUID",
        "ProcessorId",
        "Serial",
        "SerialNumber",
        "IdentifyingNumber",
        "Asset Tag",
        "SKU",
        "System Serial",
        "System SKU",
        "OEM String (0x0011, 1)",
        "Windows Product Key",
        "CPUID Serial Number",
        "Serial (device)",
        "Board Serial Number",
        "EDID Serial (numeric)",
        "EDID Serial (text, registry)",
        "EDID Serial (block 2, DisplayID)",
        "MAC Address",
        "MAC Address (Overridden)",
        "Permanent MAC",
        "MAC",
        "ATA WWN",
        "PDI",
        "Storage ID (device / logical unit, EUI-64, binary)",
        "Instance ID",
        "Endpoint ID",
        "Endpoint Container ID",
        "Stable ID",
        "Battery Serial",
        "Battery Unique ID",
        "Battery Serial (SMBIOS)",
        "Serial (SMBIOS)",
        "Socket Asset Tag (SMBIOS)",
        "Board Part Number",
    ] {
        for (new, kind) in [("7D29A4B8", Kind::Same), ("7D29A4B9", Kind::Changed)] {
            let diff = difference(
                "FIXTURE",
                &format!("{label}: 7D29A4B8"),
                &format!("{label}: {new}"),
            );
            let field = &diff.entities[0].fields[0];
            assert!(field.identifier, "{label}");
            assert!(!field.not_unique(), "{label}");
            assert_eq!(field.kind, kind, "{label}");
        }
    }
}

fn difference(title: &str, before: &str, after: &str) -> Comparison {
    let before = parse(&format!("===== {title} =====\n{before}"), false).unwrap();
    let after = parse(&format!("===== {title} =====\n{after}"), false).unwrap();
    compare(&before, &after, Path::new("before"), Path::new("after"))
}

fn monitor(serial: &str) -> String {
    format!("Manufacturer: DEL\nModel: DELL U2723QE\nSerial Number: {serial}\n")
}

#[test]
fn monitor_moves_from_first_to_third_without_field_changes() {
    let a = monitor("8VJ6M42");
    let b = monitor("8VJ6M43");
    let c = monitor("8VJ6M44");
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("Count: 3 monitor(s) found:\n{a}----\n{b}----\n{c}"),
        &format!("Count: 3 monitor(s) found:\n{b}----\n{c}----\n{a}"),
    );
    assert!(diff.entities[0].is_header);
    assert_eq!(diff.entities[0].kind, Kind::Same);
    assert_eq!(diff.entities[1].after_position, Some(2));
    assert!(
        diff.entities[1..]
            .iter()
            .all(|e| e.kind == Kind::Moved && e.fields.iter().all(|f| f.kind == Kind::Same))
    );
}

#[test]
fn disk_indices_never_match_identity_and_partition_guids_are_a_multiset() {
    let a = "Model: Samsung SSD 980 PRO\nSerial: S5GXNF0R913742K\nPartition GUID: 80D942C7-651B-4D30-AE82-17C9B6435F28\nPartition GUID: 4E67A291-AB36-4F82-976D-C51840B79E23\n";
    let reordered = "Model: Samsung SSD 980 PRO\nSerial: S5GXNF0R913742K\nPartition GUID: 4E67A291-AB36-4F82-976D-C51840B79E23\nPartition GUID: 80D942C7-651B-4D30-AE82-17C9B6435F28\n";
    let b = "Model: WDC Blue\nSerial: WD-WX21A7391842\n";
    let diff = difference(
        "DISK DRIVES",
        &format!("└── PHYSICALDRIVE0\n{a}└── PHYSICALDRIVE1\n{b}"),
        &format!("└── PHYSICALDRIVE0\n{b}└── PHYSICALDRIVE1\n{reordered}"),
    );
    assert_eq!(diff.entities.len(), 2);
    assert!(
        diff.entities
            .iter()
            .all(|e| e.kind == Kind::Moved && e.fields.iter().all(|f| f.kind == Kind::Same))
    );
    let diff = difference(
        "DISK DRIVES",
        &format!("└── PHYSICALDRIVE0\n{a}"),
        &format!(
            "└── PHYSICALDRIVE0\n{reordered}Partition GUID: 80D942C7-651B-4D30-AE82-17C9B6435F28\n"
        ),
    );
    assert_eq!(
        diff.entities[0]
            .fields
            .iter()
            .filter(|f| f.kind == Kind::Added)
            .count(),
        1
    );
}

#[test]
fn serial_changes_with_corroboration_and_moves_can_also_have_changed_fields() {
    let a = monitor("8VJ6M42");
    let changed = monitor("8VJ6M49");
    let diff = difference("MONITOR INFORMATION", &a, &changed);
    assert_eq!(diff.entities.len(), 1);
    assert_eq!(diff.entities[0].kind, Kind::Changed);
    let serial = diff.entities[0]
        .fields
        .iter()
        .find(|f| f.label == "Serial Number")
        .unwrap();
    assert_eq!(serial.kind, Kind::Changed);
    assert_eq!(serial.before.as_deref(), Some("8VJ6M42"));
    assert_eq!(serial.after.as_deref(), Some("8VJ6M49"));
    let other = monitor("8VJ6M43");
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("{a}----\n{other}"),
        &format!("{other}----\n{changed}"),
    );
    assert_eq!(diff.entities[0].kind, Kind::Moved);
    assert!(
        diff.entities[0]
            .fields
            .iter()
            .any(|f| f.kind == Kind::Changed)
    );
}

#[test]
fn unrelated_devices_are_removed_and_added_even_at_the_same_index() {
    let diff = difference(
        "DISK DRIVES",
        "└── PHYSICALDRIVE0\nModel: Samsung SSD 980 PRO\nSerial: S5GXNF0R913742K",
        "└── PHYSICALDRIVE0\nModel: WDC Blue\nSerial: WD-WX21A7391842",
    );
    assert_eq!(
        diff.entities.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::Removed, Kind::Added]
    );
    assert!(
        diff.entities[0]
            .fields
            .iter()
            .all(|f| f.kind == Kind::Removed)
    );
    assert!(
        diff.entities[1]
            .fields
            .iter()
            .all(|f| f.kind == Kind::Added)
    );
}

#[test]
fn identical_models_use_serials_and_unresolved_ties_do_not_use_position() {
    let a = monitor("8VJ6M42");
    let b = monitor("8VJ6M43");
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("{a}----\n{b}"),
        &format!("{b}----\n{a}"),
    );
    assert_eq!(diff.entities[0].after_position, Some(1));
    assert_eq!(diff.entities[1].after_position, Some(0));
    assert!(diff.entities.iter().all(|e| e.kind == Kind::Moved));
    let unknown = monitor("Unavailable");
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("{a}----\n{b}"),
        &format!("{unknown}----\n{unknown}"),
    );
    assert_eq!(
        diff.entities.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::Removed, Kind::Removed, Kind::Added, Kind::Added]
    );
    let diff = difference(
        "DISK DRIVES",
        "└── PHYSICALDRIVE0\nModel: Samsung SSD 980 PRO\nSerial: Unavailable",
        "└── PHYSICALDRIVE0\nModel: Samsung SSD 980 PRO\nSerial: Unavailable",
    );
    assert_eq!(
        diff.entities.len(),
        1,
        "Identical complete content pairs without scoring placeholders"
    );
    assert_eq!(diff.entities[0].kind, Kind::Same);
}

#[test]
fn gpu_roots_and_detached_details_follow_the_device() {
    let before = "GPU 0\n└── NVIDIA Model A\n    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504ce\nGPU 1\n└── NVIDIA Model B\n    └── UUID: GPU-2d415b68-812e-49ab-b372-67e9ca312406\nGPU 0 Board Serial Number: 032482719635\nGPU 1 Board Serial Number: 032482719636";
    let after = "GPU 0\n└── NVIDIA Model B\n    └── UUID: GPU-2d415b68-812e-49ab-b372-67e9ca312406\nGPU 1\n└── NVIDIA Model A\n    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504ce\nGPU 0 Board Serial Number: 032482719636\nGPU 1 Board Serial Number: 032482719635";
    let diff = difference("GPU INFO", before, after);
    assert_eq!(diff.entities.len(), 2);
    assert!(diff.entities.iter().all(|e| e.kind == Kind::Moved
        && e.fields.len() == 3
        && e.fields.iter().all(|f| f.kind == Kind::Same)));
    let legacy = parse(
        &format!("===== GPU INFO =====\n{before}\nBoard Serial Number: 032482719637"),
        false,
    )
    .unwrap();
    assert!(
        legacy.sections[0].entities[0]
            .rows
            .contains(&("Board Serial Number".into(), "032482719637".into()))
    );
}

#[test]
fn live_sections_and_json_share_model_and_mask_mismatch_is_visible() {
    let section = super::super::Section {
        title: "MONITOR INFORMATION",
        body: monitor("8VJ6M42").replace('\n', "\r\n"),
        ids: vec!["8VJ6M42".into()],
        ..Default::default()
    };
    let live = from_sections(std::slice::from_ref(&section), false);
    let json = r#"{"app":"HWIDChecker","version":"2.0.0","exported":"2026-10-04","masked":false,"sections":[{"title":"MONITOR INFORMATION","lines":["Manufacturer: DEL","Model: DELL U2723QE","Serial Number: 8VJ6M42"],"ids":["8VJ6M42"]}]}"#;
    assert_eq!(live, parse(json, true).unwrap());
    // Exercise the public file path too: BOM, JSON detection and text mask naming.
    let dir = std::env::temp_dir().join(format!(
        "hwid-compare-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("fixture.json");
    std::fs::write(&file, format!("\u{feff}{json}")).unwrap();
    assert_eq!(live, read(&file).unwrap());
    let text_file = dir.join("fixture-MASKED.txt");
    std::fs::write(&text_file, "===== CPU =====\r\nName: Fabricated CPU\r\n").unwrap();
    assert!(read(&text_file).unwrap().masked);
    std::fs::write(&file, "{bad json").unwrap();
    assert!(matches!(read(&file), Err(ReadError::Read(_))));
    std::fs::write(&text_file, "===== CPU =====\r\nNo data available").unwrap();
    assert!(matches!(read(&text_file), Err(ReadError::Empty(_))));
    std::fs::remove_file(file).unwrap();
    std::fs::remove_file(text_file).unwrap();
    std::fs::remove_dir(dir).unwrap();
    let hidden = from_sections(std::slice::from_ref(&section), true);
    let hidden_json = json
        .replace("8VJ6M42", "XXXXXXX")
        .replace("\"masked\":false", "\"masked\":true");
    assert_eq!(hidden, parse(&hidden_json, true).unwrap());
    let diff = compare(&live, &hidden, Path::new("export"), Path::new("current"));
    assert!(diff.entities.is_empty());
    assert!(diff.text.contains("Cannot compare masked and unmasked"));
    assert!(diff.warning.is_some());
    let diff = compare(&hidden, &hidden, Path::new("a"), Path::new("b"));
    assert!(diff.warning.is_some());
    let text_hidden = parse("===== USB DEVICES =====\nSerial: XXXX-XXXX", false).unwrap();
    assert!(!text_hidden.masked);
    let diff = compare(&text_hidden, &text_hidden, Path::new("a"), Path::new("b"));
    assert_eq!(
        diff.entities.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::Same]
    );
    assert!(diff.entities[0].fields[0].not_unique());
}

#[test]
fn exact_twins_pair_in_order_before_unique_model_fallback() {
    let twin = monitor("0000000000000");
    let other = "Manufacturer: ACR\nModel: Acer XV272U\nSerial Number: 0000000000000\n";
    for (left_count, right_count) in [(2, 2), (3, 2), (2, 3)] {
        let before = format!(
            "{other}----\n{}",
            vec![twin.as_str(); left_count].join("----\n")
        );
        let after = format!(
            "{}----\n{other}",
            vec![twin.as_str(); right_count].join("----\n")
        );
        let diff = difference("MONITOR INFORMATION", &before, &after);
        assert_eq!(diff.entities[0].after_position, Some(right_count));
        for i in 0..left_count.min(right_count) {
            let entity = &diff.entities[i + 1];
            assert_eq!(entity.after_position, Some(i));
            assert!(entity.fields.iter().all(|f| f.kind == Kind::Same));
            assert!(
                entity
                    .fields
                    .iter()
                    .find(|f| f.label == "Serial Number")
                    .unwrap()
                    .not_unique()
            );
        }
        assert_eq!(
            diff.entities
                .iter()
                .filter(|e| e.kind == Kind::Removed)
                .count(),
            left_count.saturating_sub(right_count)
        );
        assert_eq!(
            diff.entities
                .iter()
                .filter(|e| e.kind == Kind::Added)
                .count(),
            right_count.saturating_sub(left_count)
        );
    }
    // Field ordering is immaterial, but duplicate occurrences and extra fields matter.
    let before = "Model: Display\nSerial: 0000\nPort: HDMI\nPort: HDMI\n";
    let reordered = "Port: HDMI\nSerial: 0000\nPort: HDMI\nModel: Display\n";
    let other = "Model: Display\nSerial: 0000\nPort: DP\n";
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("{before}----\n{other}"),
        &format!("{other}----\n{reordered}"),
    );
    assert_eq!(diff.entities[0].after_position, Some(1));
    assert!(
        diff.entities
            .iter()
            .all(|e| e.fields.iter().all(|f| f.kind == Kind::Same))
    );
    let diff = difference(
        "MONITOR INFORMATION",
        &format!("{before}----\n{before}"),
        &format!(
            "{reordered}----\n{}",
            reordered.replacen("Port: HDMI\n", "", 1)
        ),
    );
    assert_eq!(diff.entities[0].after_position, Some(0));
    assert_eq!(diff.entities[1].kind, Kind::Changed);
}

#[test]
fn unique_models_pair_changed_ids_but_ambiguous_models_stay_unmatched() {
    let before =
        "GPU 0\n└── NVIDIA GeForce RTX 5080\nUUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504ce\n";
    let after = before.replace(
        "9e521d74-03ba-4c68-a27f-81d639b504ce",
        "2d415b68-812e-49ab-b372-67e9ca312406",
    );
    let diff = difference("GPU INFO", before, &after);
    assert_eq!(diff.entities.len(), 1);
    assert_eq!(diff.entities[0].kind, Kind::Changed);
    let uuid = &diff.entities[0].fields[1];
    assert!(uuid.identifier && !uuid.not_unique());
    assert_eq!(uuid.kind, Kind::Changed);
    // Name is the model-level field when there is no explicit Model label.
    let diff = difference(
        "AUDIO DEVICES",
        "Name: USB Audio DAC\nManufacturer: N/A\nEndpoint ID: endpoint-a",
        "Name: USB Audio DAC\nManufacturer: N/A\nEndpoint ID: endpoint-b",
    );
    assert_eq!(diff.entities.len(), 1);
    assert_eq!(diff.entities[0].kind, Kind::Changed);
    for (left_count, right_count) in [(2, 2), (1, 2), (2, 1)] {
        let disks = |count, serial| {
            (0..count).map(|i| format!("└── PHYSICALDRIVE{i}\nModel: Samsung SSD 980 PRO\nManufacturer: Samsung\nSerial: S5GXNF0R{serial}{i}\nDisk GUID: {serial}-{i}\n")).collect::<String>()
        };
        let diff = difference(
            "DISK DRIVES",
            &disks(left_count, "913742"),
            &disks(right_count, "924853"),
        );
        assert_eq!(
            diff.entities
                .iter()
                .filter(|e| e.kind == Kind::Removed)
                .count(),
            left_count
        );
        assert_eq!(
            diff.entities
                .iter()
                .filter(|e| e.kind == Kind::Added)
                .count(),
            right_count
        );
    }
    let diff = difference(
        "USB DEVICES",
        "Product: Audio DAC\nManufacturer: Vendor A\nSerial: 8C31D7A2",
        "Product: Audio DAC\nManufacturer: Vendor B\nSerial: 8C31D7B9",
    );
    assert_eq!(diff.entities.len(), 2);
}

#[test]
fn ram_table_keeps_cells_and_tracks_serials_across_slots() {
    let row = |values: [&str; 5]| {
        values
            .into_iter()
            .zip([15, 12, 16, 8, 12])
            .map(|(v, w)| pad_right_utf16(v, w))
            .collect::<Vec<_>>()
            .join(" ")
            + "\n"
    };
    let header = row([
        "DeviceLocator",
        "Manufacturer",
        "PartNumber",
        "Capacity",
        "SerialNumber",
    ]);
    let before = format!(
        "{header}-------------------------------------------------------------------\n{}{}",
        row(["DIMM_A2", "Kingston", "KF432C16BB1/16", "16 GB", "8C31D7A2"]),
        row(["DIMM_B2", "Kingston", "KF432C16BB1/16", "16 GB", "8C31D7B9"])
    );
    let after = format!(
        "{header}-------------------------------------------------------------------\n{}{}",
        row(["DIMM_A2", "Kingston", "KF432C16BB1/16", "16 GB", "8C31D7B9"]),
        row(["DIMM_B2", "Kingston", "KF432C16BB1/16", "16 GB", "8C31D7A2"])
    );
    let diff = difference("RAM MODULES", &before, &after);
    assert_eq!(diff.entities.len(), 2);
    assert!(
        diff.entities
            .iter()
            .all(|e| e.kind == Kind::Moved && e.fields.len() == 5)
    );
    assert!(
        diff.entities
            .iter()
            .flat_map(|e| &e.fields)
            .filter(|f| f.kind == Kind::Changed)
            .all(|f| f.label == "DeviceLocator")
    );
    assert!(
        diff.entities
            .iter()
            .flat_map(|e| &e.fields)
            .any(|f| f.after.as_deref() == Some("16 GB"))
    );
    let generic = before
        .replace("8C31D7A2", "00000000")
        .replace("8C31D7B9", "00000000");
    let diff = difference("RAM MODULES", &generic, &generic);
    assert_eq!(diff.entities.len(), 2);
    assert!(diff.entities.iter().all(|e| e.kind == Kind::Same));
    assert!(
        diff.entities
            .iter()
            .flat_map(|e| &e.fields)
            .filter(|f| f.label == "SerialNumber")
            .all(|f| f.not_unique())
    );
}

#[test]
fn external_formats_and_multiset_matching() {
    let before = parse(
        "ignored: value\r\n===== DISK DRIVES =====\r\n  ├─ Serial: S1 | Model: Drive\r\n│ └─ Serial: S2\r\nSize: 2: 4\r\nGone: old\r\n123: ignored\r\n===== BOARD =====\r\nVendor: Acme\r\n",
        false,
    ).unwrap();
    let after = parse(
        r#"{"app":"HWIDChecker","version":"2.0.0","exported":"2026-10-04T09:30:00","masked":false,"sections":[{"title":"disk drives","lines":["Serial: S2 | Model: Drive","Serial: S1","Size: 2: 4","New: XXXX"],"ids":["S2","S1"]},{"title":"BOARD","lines":["Vendor: Acme"],"ids":[]},{"title":"USB","lines":["Name: Device"],"ids":[]}] }"#,
        true,
    ).unwrap();
    let diff = compare(
        &before,
        &after,
        Path::new("before.txt"),
        Path::new("after.json"),
    );
    assert_eq!(diff.summary, "Changed 0 · Added 2 · Removed 1 · Same 5");
    assert_eq!(
        diff.text,
        concat!(
            "Before: before.txt\r\nAfter:  after.json\r\n\r\n",
            "DISK DRIVES\r\n",
            "  removed Gone    old\r\n  added   New     XXXX\r\n",
            "  same    Serial  S1\r\n  same    Model   Drive\r\n  same    Serial  S2\r\n  same    Size    2: 4\r\n\r\n",
            "BOARD\r\n  same     1 values, none changed\r\n\r\n",
            "USB\r\n  added   Name  Device\r\n\r\n"
        )
    );
    let same = compare(&before, &before, Path::new("a"), Path::new("a"));
    assert_eq!(same.summary, "Changed 0 · Added 0 · Removed 0 · Same 6");
    let removed = compare(&after, &before, Path::new("a"), Path::new("b"));
    assert!(removed.text.contains("USB\r\n  removed Name  Device"));
}

#[test]
fn rejects_raw_report_and_malformed_json_and_preserves_exact_labels() {
    let raw = super::super::format_section("DISK DRIVES", "Serial: S1\r\n");
    assert!(parse(&raw, false).unwrap().sections.is_empty());
    assert!(
        parse(
            " ===== DISK =====\nSerial: S1\n===== DISK ===== \nSerial: S2",
            false
        )
        .unwrap()
        .sections
        .is_empty()
    );
    for invalid in [
        "{",
        "{}",
        r#"{"sections":[{"title":"CPU","lines":"Serial: x"}]}"#,
    ] {
        assert!(parse(invalid, true).is_err());
    }
    let empty = parse("===== Empty =====\r\nNo data available\r\n123: x", false).unwrap();
    assert!(empty.sections[0].entities.is_empty());
    let left = parse("===== CPU =====\nSerial: AbC\nLabel: \n", false).unwrap();
    let right = parse("===== cpu =====\nserial: AbC\nLabel: X\n", false).unwrap();
    assert_eq!(
        compare(&left, &right, Path::new("a"), Path::new("b")).summary,
        "Changed 1 · Added 1 · Removed 1 · Same 0"
    );
    let right = parse("===== cpu =====\nSerial: abc\nLabel: \n", false).unwrap();
    assert_eq!(
        compare(&left, &right, Path::new("a"), Path::new("b")).summary,
        "Changed 1 · Added 0 · Removed 0 · Same 1"
    );
}
