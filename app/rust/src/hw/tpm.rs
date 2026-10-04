//! TPM status and native endorsement-key fields, with legacy fallbacks.

use crate::{
    hw::{Ctx, first_ok},
    report::{Out, eq_ignore_case, trim_net},
    win::{self, Error, firmware, tbs, tpm, wmi},
};

struct Info {
    present: Option<bool>,
    enabled: win::Result<bool>,
    manufacturer: String,
    version: String,
    spec: String,
    failures: Vec<Error>,
    source: &'static str,
}

/// Collects TPM status and EK identifiers, retaining the WMI/PowerShell fallbacks.
pub fn collect(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
    let result = collect_status(out);
    append_firmware(ctx, out);
    result
}

fn collect_status(out: &mut Out) -> win::Result<()> {
    let info = match first_ok(
        out,
        "TPM information",
        &[("WMI", &wmi_info), ("PowerShell", &powershell_info)],
    ) {
        Ok(info) => info,
        Err(error) => {
            out.text(&format!("Unable to retrieve TPM information: {error}"));
            return Ok(());
        }
    };
    for error in &info.failures {
        out.fallback_failed("TPM status", error);
    }
    if info.present.is_none() {
        out.text("Unable to retrieve TPM information");
        return Ok(());
    }
    if info.present == Some(false) {
        // C# parity: Hardware/TpmInfo.cs:224-228. A missing True match means OFF.
        out.text("TPM OFF");
        return Ok(());
    }
    let enabled = match &info.enabled {
        Ok(enabled) => {
            out.info("TPM", if *enabled { "ENABLED" } else { "DISABLED" });
            *enabled
        }
        Err(error) => {
            out.fallback_failed("WMI IsEnabled", error);
            if error.op == "IsEnabled" && error.code != 0 {
                // AD-11: the returned TPM status must never become false/disabled.
                out.text(&format!(
                    "TPM: UNKNOWN (IsEnabled failed: 0x{:08X})",
                    error.code
                ));
            } else {
                // C# parity: Hardware/TpmInfo.cs:154. No Boolean from the method or
                // the initial value means DISABLED; the failure stays in diagnostics.
                out.info("TPM", "DISABLED");
            }
            false
        }
    };
    for (label, value) in [
        ("TPM Manufacturer", &info.manufacturer),
        ("TPM Version", &info.version),
        ("TPM Spec Version", &info.spec),
    ] {
        if !value.is_empty() {
            out.info(label, value);
        }
    }
    if enabled {
        append_ek(out, info.source);
    }
    Ok(())
}

fn append_firmware(ctx: &Ctx, out: &mut Out) {
    let devices = match ctx.smbios_result() {
        Ok(table) => firmware::tpm_devices(table),
        Err(error) => {
            out.fallback_failed("SMBIOS TPM", &error);
            Vec::new()
        }
    };
    let tbs_version = tbs::device_version();
    render_firmware(devices, &tbs_version, out);
}

fn render_firmware(
    mut devices: Vec<firmware::TpmDevice>,
    tbs_version: &win::Result<Option<(u8, u8)>>,
    out: &mut Out,
) {
    devices.retain(|device| {
        if device.spec.is_ok()
            || device.firmware1.is_ok()
            || device.firmware2.is_ok()
            || device.characteristics.is_ok()
        {
            return true;
        }
        super::bios::metadata_record_has_values(
            out,
            "TPM Firmware",
            Some(device.handle),
            &[&device.vendor, &device.description],
        )
    });
    for (index, device) in devices.iter().enumerate() {
        if devices.len() >= 2 {
            out.text(&format!("TPM Firmware #{} (SMBIOS)", index + 1));
        }
        super::bios::write_metadata_field(out, "TPM Vendor (SMBIOS)", false, &device.vendor);
        // Retain the vendor-specific raw words rather than assuming a decimal encoding.
        // Each word remains visible even when the other one cannot be decoded.
        let words =
            [(&device.firmware1, "1"), (&device.firmware2, "2")].map(|(word, n)| match word {
                Ok(value) => format!("0x{value:08X}"),
                Err(error) => {
                    out.fallback_failed(&format!("TPM Firmware Version {n} (SMBIOS)"), error);
                    format!("Unavailable ({error})")
                }
            });
        let fields = [
            (
                "TPM Spec Version (SMBIOS)",
                device
                    .spec
                    .as_ref()
                    .map(|(major, minor)| format!("{major}.{minor}")),
            ),
            ("TPM Firmware Version (SMBIOS)", Ok(words.join(" "))),
            (
                "TPM Characteristics (SMBIOS)",
                device
                    .characteristics
                    .as_ref()
                    .map(|value| decode_characteristics(*value)),
            ),
        ];
        for (label, value) in fields {
            match value {
                Ok(value) => {
                    out.info(label, &value);
                }
                Err(error) => {
                    out.fallback_failed(label, error)
                        .info(label, &format!("Unavailable ({error})"));
                }
            }
        }
        super::bios::write_metadata_field(
            out,
            "TPM Description (SMBIOS)",
            false,
            &device.description,
        );
        match (&device.spec, tbs_version) {
            (Ok((major, minor)), Ok(Some(version))) if (*major, *minor) != *version => {
                out.info(
                    "TPM Version Cross-check",
                    &format!(
                        "SMBIOS {major}.{minor} differs from TBS {}.{}",
                        version.0, version.1
                    ),
                );
            }
            (_, Ok(None)) => {
                out.info(
                    "TPM Version Cross-check",
                    "SMBIOS reports a TPM; TBS did not find one",
                );
            }
            _ => {}
        }
    }
    match tbs_version {
        Ok(Some((major, minor))) => {
            out.info("TPM Spec Version (TBS)", &format!("{major}.{minor}"));
        }
        Ok(None) => {
            out.fallback_failed(
                "TBS",
                &Error::msg("Tbsi_GetDeviceInfo", "absent: TPM not found"),
            );
            out.info("TPM Spec Version (TBS)", "Not found");
        }
        Err(error) => {
            out.fallback_failed("TBS", error)
                .info("TPM Spec Version (TBS)", &format!("Unavailable ({error})"));
        }
    }
}

fn decode_characteristics(value: u64) -> String {
    // DSP0134 7.44: bits 0/1 and 6..63 are reserved; retain them as unknown hex.
    let mut names: Vec<String> = [
        (2, "Not supported"),
        (3, "Configurable via firmware update"),
        (4, "Configurable via platform software"),
        (5, "Configurable via OEM proprietary mechanism"),
    ]
    .into_iter()
    .filter(|(bit, _)| value & (1 << bit) != 0)
    .map(|(_, name)| name.to_owned())
    .collect();
    let unknown = value & !0x3C;
    if unknown != 0 {
        names.push(format!("0x{unknown:016X}"));
    }
    if names.is_empty() {
        "None reported".into()
    } else {
        names.join(", ")
    }
}

fn wmi_info() -> win::Result<Info> {
    // C# parity: Hardware/TpmInfo.cs:84-94. First object only; existence means
    // present. IsActivated is queried but never changes the visible status.
    // The WMI wrapper retains __PATH/__RELPATH for method calls after projection.
    let row = wmi::query(
        wmi::Namespace::MicrosoftTpm,
        "SELECT ManufacturerIdTxt, ManufacturerId, ManufacturerVersion, SpecVersion, IsEnabled_InitialValue, IsActivated_InitialValue FROM Win32_Tpm",
    )?
    .into_iter()
    .next()
    .ok_or_else(|| Error::msg("Win32_Tpm", "no TPM object returned"))?;
    let path = row
        .str("__PATH")
        .filter(|value| !value.is_empty())
        .or_else(|| row.str("__RELPATH").filter(|value| !value.is_empty()));
    let mut failures = Vec::new();
    let enabled = invoke_bool(&row, path.as_deref(), "IsEnabled", &mut failures);
    if let Err(error) = invoke_bool(&row, path.as_deref(), "IsActivated", &mut failures) {
        failures.push(error);
    }
    let text = row.str("ManufacturerIdTxt").unwrap_or_default();
    // C# parity: Hardware/TpmInfo.cs:96-101,157-169. Test whitespace, but retain
    // the original manufacturer text; numeric fallback is eight uppercase digits.
    let manufacturer = if !trim_net(&text).is_empty() {
        text
    } else {
        row.u32("ManufacturerId")
            .map(|id| format!("0x{id:08X}"))
            .or_else(|| row.str("ManufacturerId"))
            .unwrap_or_default()
    };
    Ok(Info {
        present: Some(true),
        enabled,
        manufacturer,
        version: row.str("ManufacturerVersion").unwrap_or_default(),
        spec: row.str("SpecVersion").unwrap_or_default(),
        failures,
        source: "WMI",
    })
}

fn invoke_bool(
    row: &wmi::Row,
    path: Option<&str>,
    method: &'static str,
    failures: &mut Vec<Error>,
) -> win::Result<bool> {
    let result = path
        .ok_or_else(|| Error::msg(method, "WMI object path is missing"))
        .and_then(|path| wmi::call_method(wmi::Namespace::MicrosoftTpm, path, method))
        // C# parity: Hardware/TpmInfo.cs:120-150 (alternate Boolean before initial state).
        .map(|output| {
            bool_value(&output, method).or_else(|| output.first_bool_except("ReturnValue"))
        });
    resolve_bool(
        result,
        bool_value(row, &format!("{method}_InitialValue")),
        method,
        failures,
    )
}

fn resolve_bool(
    result: win::Result<Option<bool>>,
    initial: Option<bool>,
    method: &'static str,
    failures: &mut Vec<Error>,
) -> win::Result<bool> {
    let error = match result {
        Ok(output) => match output {
            Some(value) => return Ok(value),
            None => Error::msg(method, "WMI Boolean output is missing or invalid"),
        },
        Err(error) => {
            // The frozen helper encodes a returned TPM error separately from a
            // COM failure in this detail. Only that case is governed by AD-11.
            if error.op == "WMI ExecMethod"
                && error.detail == format!("{method} returned a nonzero ReturnValue")
            {
                return Err(Error {
                    op: method,
                    ..error
                });
            }
            error
        }
    };
    // C# parity: Hardware/TpmInfo.cs:145-150. Unsupported methods may still have
    // the initial-state property. A successful fallback is diagnostic-only.
    if let Some(value) = initial {
        failures.push(error);
        Ok(value)
    } else {
        Err(error)
    }
}

fn bool_value(row: &wmi::Row, name: &str) -> Option<bool> {
    row.bool(name).or_else(|| {
        let value = row.str(name)?;
        if eq_ignore_case(trim_net(&value), "True") {
            Some(true)
        } else if eq_ignore_case(trim_net(&value), "False") {
            Some(false)
        } else {
            None
        }
    })
}

fn powershell_info() -> win::Result<Info> {
    let output = tpm::powershell_status()?;
    let empty = trim_net(&output.text).is_empty();
    let mut failures: Vec<_> = output.failure.into_iter().collect();
    if empty {
        failures.push(Error::msg("Get-Tpm", "no output returned"));
    }
    Ok(Info {
        present: (!empty).then(|| true_property(&output.text, "TpmPresent")),
        enabled: Ok(true_property(&output.text, "TpmEnabled")),
        // C# parity: Hardware/TpmInfo.cs:215-237. The PowerShell fallback omits
        // manufacturer/version/spec even when Get-Tpm contains these properties.
        manufacturer: String::new(),
        version: String::new(),
        spec: String::new(),
        failures,
        source: "PowerShell",
    })
}

fn true_property(text: &str, name: &str) -> bool {
    // C# parity: Hardware/TpmInfo.cs:224,231. Case-sensitive, column zero,
    // multiline ^Name\s*:\s*True\b; even whitespace across lines is accepted.
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
        .any(|offset| {
            text[offset..]
                .strip_prefix(name)
                .and_then(|rest| rest.trim_start().strip_prefix(':'))
                .and_then(|rest| rest.trim_start().strip_prefix("True"))
                .is_some_and(|rest| {
                    rest.chars()
                        .next()
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
                })
        })
}

fn append_ek(out: &mut Out, status_source: &str) {
    match tpm::native_ek() {
        Ok(fields) => {
            render_ek(out, &fields);
            out.source(&format!("{status_source} (status); NCrypt/crypt32 (EK)"));
            return;
        }
        Err(error) => {
            out.fallback_failed("Native EK", &error);
        }
    }
    match tpm::powershell_ek() {
        Ok(output) => {
            if let Some(error) = &output.failure {
                out.fallback_failed("PowerShell EK", error);
            }
            let fields = parse_ek(&output.text);
            if !render_ek(out, &fields) {
                // stderr (already recorded above) can be multi-line and unrelated
                // to the parse result, so it never goes into the report text.
                let error = Error::msg("Get-TpmEndorsementKeyInfo", "no usable EK fields returned");
                out.fallback_failed("PowerShell EK", &error);
                if trim_net(&output.text).is_empty() {
                    // C# parity: Hardware/TpmInfo.cs:175-178. Keep the empty-output text.
                    out.text("Unable to retrieve detailed TPM information");
                } else {
                    out.text(&format!(
                        "Unable to retrieve detailed TPM information: {error}"
                    ));
                }
            } else {
                // The status source may have been either WMI or PowerShell.
                out.source(&format!("{status_source} (status); PowerShell (EK)"));
            }
        }
        Err(error) => {
            out.fallback_failed("PowerShell EK", &error);
            out.text(&format!("Unable to retrieve TPM information: {error}"));
        }
    }
}

fn parse_ek(output: &str) -> Vec<(String, String)> {
    let mut fields = Vec::<(String, String)>::new();
    let mut pending = None;
    for line in output.split(['\r', '\n']).map(trim_net) {
        if line.is_empty() {
            continue;
        }
        let hash = line
            .strip_prefix("PublicKeyHash")
            .and_then(|rest| rest.trim_start().strip_prefix(':'))
            .filter(|rest| !rest.is_empty());
        let entry = if let Some(hash) = hash {
            Some(("PublicKeyHash".to_owned(), trim_net(hash).to_owned()))
        } else if let Some(section) = line
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .filter(|s| !s.is_empty())
        {
            pending = Some(trim_net(section).to_owned());
            None
        } else {
            pending
                .take()
                .filter(|s| !s.is_empty())
                .map(|s| (s, line.to_owned()))
        };
        if let Some((key, value)) = entry {
            // C# parity: Hardware/TpmInfo.cs:242-268. Keys ignore case; only the
            // next nonempty line is stored. Repeated certificate fields overwrite.
            if let Some((_, old)) = fields.iter_mut().find(|(old, _)| eq_ignore_case(old, &key)) {
                *old = value;
            } else {
                fields.push((key, value));
            }
        }
    }
    fields
}

fn render_ek(out: &mut Out, fields: &[(String, String)]) -> bool {
    let mut rendered = false;
    let field = |key| fields.iter().find(|(name, _)| eq_ignore_case(name, key));
    for (key, label) in [
        ("PublicKeyHash", "Sha256 Hash"),
        ("Serial Number", "Serial Number"),
        ("Thumbprint", "Thumbprint"),
    ] {
        if let Some((_, value)) = field(key) {
            out.id(label, value);
            rendered = true;
        }
    }
    if let Some((_, issuer)) = field("Issuer") {
        // C# parity: Hardware/TpmInfo.cs:197-210. Literal CN=/O= substring and
        // comma splitting (not an X.500 parser), with CN before O.
        let parts: Vec<_> = ["CN=", "O="]
            .into_iter()
            .filter_map(|prefix| {
                let value = trim_net(issuer.split(prefix).nth(1)?.split(',').next()?);
                (!value.is_empty()).then(|| format!("{prefix}{value}"))
            })
            .collect();
        if !parts.is_empty() {
            out.info("Issuer", &parts.join(", "));
            rendered = true;
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tbs_failures_keep_legacy_and_tpm12_firmware_byte_identical() {
        let mut record = firmware::Structure {
            kind: 43,
            handle: 0x4318,
            formatted: vec![0; 0x1B],
            strings: vec!["Discrete TPM".into()],
        };
        record.formatted[4..10].copy_from_slice(&[b'I', b'F', b'X', 0, 1, 2]);
        record.formatted[0x12] = 1;
        let table = firmware::Smbios {
            major: 3,
            minor: 6,
            structures: vec![record],
        };
        let mut baseline = Out::new();
        baseline.info("Legacy", "kept");
        render_firmware(
            firmware::tpm_devices(&table),
            &Ok(Some((1, 2))),
            &mut baseline,
        );
        let baseline = baseline.finish();
        let prefix = baseline
            .body
            .strip_suffix("TPM Spec Version (TBS): 1.2\r\n")
            .unwrap();
        for code in [5, 127, 126, 1460, 0x8028_4002] {
            let mut out = Out::new();
            out.info("Legacy", "kept");
            render_firmware(
                firmware::tpm_devices(&table),
                &Err(Error {
                    op: "Tbsi_GetDeviceInfo",
                    code,
                    detail: "unavailable: fabricated failure".into(),
                }),
                &mut out,
            );
            let failed = out.finish();
            assert!(failed.body.starts_with(prefix));
            assert!(failed.body.contains("TPM Spec Version (SMBIOS): 1.2\r\n"));
            assert!(
                failed
                    .body
                    .contains("TPM Spec Version (TBS): Unavailable (")
            );
            assert_eq!(failed.failures.len(), 1);
        }
    }

    #[test]
    fn firmware_rendering_merges_words_decodes_bits_and_keeps_cross_check_observational() {
        let mut record = firmware::Structure {
            kind: 43,
            handle: 0x4301,
            formatted: vec![0; 0x1B],
            strings: vec!["Firmware TPM".into()],
        };
        record.formatted[4..10].copy_from_slice(&[b'I', b'F', b'X', 0, 2, 0]);
        record.formatted[0x0A..0x0E].copy_from_slice(&0x0007_0055u32.to_le_bytes());
        record.formatted[0x0E..0x12].copy_from_slice(&0x11CB_0000u32.to_le_bytes());
        record.formatted[0x12] = 1;
        record.formatted[0x13..0x1B].copy_from_slice(&0x8000_0000_0000_003Cu64.to_le_bytes());
        let mut table = firmware::Smbios {
            major: 3,
            minor: 6,
            structures: vec![record.clone()],
        };
        let mut out = Out::new();
        render_firmware(firmware::tpm_devices(&table), &Ok(Some((1, 2))), &mut out);
        let single = out.finish();
        assert!(single.body.starts_with("TPM Vendor (SMBIOS): IFX\r\n"));
        assert!(
            single
                .body
                .contains("TPM Firmware Version (SMBIOS): 0x00070055 0x11CB0000\r\n")
        );
        assert!(single.body.contains("Not supported, Configurable via firmware update, Configurable via platform software, Configurable via OEM proprietary mechanism, 0x8000000000000000"));
        assert!(
            single
                .body
                .contains("TPM Version Cross-check: SMBIOS 2.0 differs from TBS 1.2\r\n")
        );
        assert!(single.failures.is_empty());
        assert!(single.ids.is_empty());
        assert!(!single.body.contains("0x4301"));
        record.formatted.truncate(14);
        table.structures.push(record);
        let mut out = Out::new();
        render_firmware(firmware::tpm_devices(&table), &Ok(None), &mut out);
        let multiple = out.finish();
        assert!(multiple.body.starts_with("TPM Firmware #1 (SMBIOS)\r\n"));
        assert!(multiple.body.contains("TPM Firmware #2 (SMBIOS)\r\n"));
        assert!(
            multiple
                .body
                .contains("TPM Firmware Version (SMBIOS): 0x00070055 Unavailable (")
        );
        assert!(
            multiple
                .body
                .ends_with("TPM Spec Version (TBS): Not found\r\n")
        );
        table.structures = vec![firmware::Structure {
            kind: 43,
            handle: 0x4302,
            formatted: vec![0; 4],
            strings: vec![],
        }];
        let mut out = Out::new();
        render_firmware(firmware::tpm_devices(&table), &Ok(Some((2, 0))), &mut out);
        let empty = out.finish();
        assert_eq!(empty.body, "TPM Spec Version (TBS): 2.0\r\n");
        assert_eq!(empty.failures.len(), 1);
        assert_eq!(decode_characteristics(0), "None reported");
        assert_eq!(
            decode_characteristics(0x10),
            "Configurable via platform software"
        );
    }

    #[test]
    fn failed_tpm_return_value_cannot_be_replaced_by_initial_state() {
        for initial in [Some(true), Some(false), None] {
            let mut failures = Vec::new();
            let error = resolve_bool(
                Err(Error {
                    op: "WMI ExecMethod",
                    code: 0x8028_0001,
                    detail: "IsEnabled returned a nonzero ReturnValue".into(),
                }),
                initial,
                "IsEnabled",
                &mut failures,
            )
            .expect_err("a TPM return failure must override initial state");
            assert_eq!(error.op, "IsEnabled");
            assert_eq!(error.code, 0x8028_0001);
            assert!(failures.is_empty());
        }
        let mut failures = Vec::new();
        assert!(
            resolve_bool(
                Err(Error::msg("WMI ExecMethod", "method unavailable")),
                Some(true),
                "IsEnabled",
                &mut failures,
            )
            .expect("unsupported methods may fall back to initial state")
        );
        assert_eq!(failures.len(), 1);
        assert!(resolve_bool(Ok(None), None, "IsEnabled", &mut failures).is_err());
    }

    #[test]
    fn ek_text_keeps_last_certificate_single_line_values_and_identifier_records() {
        // Fabricated CNG RSA public blob. The independently encoded PKCS#1 fixture
        // has a sign-padding zero before its 64-byte modulus; hashing the CNG blob
        // itself, SPKI, or a little-endian modulus must not produce this digest.
        let mut public = Vec::new();
        for word in [0x31415352u32, 512, 3, 64, 0, 0] {
            public.extend(word.to_le_bytes());
        }
        public.extend([1, 0, 1, 0x91]);
        public.extend(1u8..64);
        assert_eq!(
            tpm::rsa_public_hash(&public).expect("fabricated CNG public blob"),
            "6bb76d695bfad220fbfbfd93ac1ca7c67b387d986b42c828f5c506d63ba3107f"
        );
        for length in 0..public.len() {
            assert!(tpm::rsa_public_hash(&public[..length]).is_err());
        }
        for offset in [0, 4, 8, 12, 16, 20] {
            let mut corrupt = public.clone();
            corrupt[offset..offset + 4].fill(0xff);
            assert!(tpm::rsa_public_hash(&corrupt).is_err());
        }
        assert!(tpm::certificate_fields(b"not a certificate").is_err());
        let fields = parse_ek(include_str!(
            "../../tests/fixtures/wp-05/ek-format-list.fixture"
        ));
        let mut out = Out::new();
        assert!(render_ek(&mut out, &fields));
        let section = out.finish();
        assert_eq!(
            section.body,
            include_str!("../../tests/fixtures/wp-05/ek-expected.fixture")
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
        );
        assert_eq!(section.ids.len(), 3);
        assert_eq!(section.ids[1], "0183BA26D5E7904FC218");
        assert!(!section.ids.iter().any(|value| value.contains("CN=")));
    }

    #[test]
    fn ek_parser_preserves_pending_sections_and_rejects_similar_labels() {
        let fields = parse_ek(
            "[Serial Number]\r\nPublicKeyHash : 7fa1\r\n\r\n0195\r\n\
             [Thumbprint]\r\n[]\r\n[Issuer]\r\nOU=Device CA, cn=ignored, O=Vendor, CN=Root, O=Other\r\n\
             publickeyhash: must-not-replace\r\nPublicKeyHashExtra: must-not-replace",
        );
        let mut out = Out::new();
        assert!(render_ek(&mut out, &fields));
        assert_eq!(
            out.finish().body,
            "Sha256 Hash: 7fa1\r\nSerial Number: 0195\r\nThumbprint: []\r\nIssuer: CN=Root, O=Vendor\r\n"
        );
        let mut out = Out::new();
        assert!(!render_ek(&mut out, &parse_ek("[Subject]\r\nunused\r\n")));
        assert!(!render_ek(&mut out, &parse_ek(" \r\n")));
    }

    #[test]
    fn get_tpm_parser_keeps_case_column_and_true_word_boundary_rules() {
        for text in [
            "TpmPresent : True\r\n",
            "ignored\nTpmPresent\t:\r\n True\r\n",
        ] {
            assert!(true_property(text, "TpmPresent"), "{text:?}");
        }
        for text in [
            "TpmPresent : False",
            "TpmPresent : true",
            "tpmpresent : True",
            " TpmPresent : True",
            "TpmPresentExtra : True",
            "TpmPresent : TrueValue",
            "TpmPresent : True_",
            "TpmPresent : True1",
            "TpmEnabled : True",
            "",
        ] {
            assert!(!true_property(text, "TpmPresent"), "{text:?}");
        }
    }

    #[test]
    #[ignore = "reads real TPM state and writes identifiers only to the private golden/wp-05 folder"]
    fn wp05_capture_tpm() {
        use crate::{hw, report};
        use std::{fs, path::Path, sync::mpsc, time::Duration};

        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let section = hw::collect_provider(
                &hw::Provider {
                    title: "TPM MODULES",
                    collect,
                },
                &Ctx::new(),
            );
            sender
                .send(section)
                .expect("capture receiver must remain available");
        });
        let section = receiver
            .recv_timeout(Duration::from_secs(65))
            .expect("TPM capture deadline");
        let directory = Path::new(r"D:\GIT\HWID-Privacy\app\rust\golden\wp-05");
        fs::create_dir_all(directory).expect("private capture directory");
        let text = report::format_section(section.title, &section.body);
        let diagnostic = format!(
            "elapsed_ms={}\r\nsource={}\r\nadmin={}\r\nfailures:\r\n{}\r\n",
            section.elapsed_ms,
            section.source,
            win::security::is_admin(),
            section.failures.join("\r\n")
        );
        fs::write(directory.join("rust-tpm.txt"), &text).expect("write private TPM report");
        fs::write(directory.join("rust-tpm.txt.diag.txt"), &diagnostic)
            .expect("write private diagnostics");
        println!("{text}{diagnostic}");
        assert!(!section.source.is_empty());
    }
}
