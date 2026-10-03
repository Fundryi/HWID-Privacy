//! Owned by WP-11: ghost scanning and guarded device removal.

use super::whitelist;
use crate::{
    report::{Out, Section},
    win::{
        self, Error,
        process::Cancel,
        setupapi::{self, DevInfoSet, Device},
    },
};
use std::{
    collections::HashSet,
    panic::{AssertUnwindSafe, catch_unwind},
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_CLASS, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME,
    SPDRP_HARDWAREID, SPDRP_INSTALL_STATE,
};

// C# parity: app/src/Services/DeviceCleaningService.cs:110-125. Exact joined-ID equality.
const IGNORED: [&str; 10] = [
    r"SW\{96E080C7-143C-11D1-B40F-00A0C9223196}",
    "ms_pppoeminiport",
    "ms_pptpminiport",
    "ms_agilevpnminiport",
    "ms_ndiswanbh",
    "ms_ndiswanip",
    "ms_sstpminiport",
    "ms_ndiswanipv6",
    "ms_l2tpminiport",
    r"MMDEVAPI\AudioEndpoints",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Absent,
    Unclear,
}
#[derive(Clone, Debug)]
pub struct GhostDevice {
    pub name: String,
    pub description: String,
    pub hardware_id: String,
    pub class: String,
    pub instance_id: String,
    pub presence: Presence,
}
pub struct Scan {
    devices: Vec<GhostDevice>,
    data: Vec<SP_DEVINFO_DATA>,
    diagnostics: Section,
    set: DevInfoSet,
}

/// Scans ghost devices while retaining the native snapshot needed for removal.
pub fn scan() -> Result<Scan, String> {
    match catch_unwind(scan_inner) {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(panic) => Err(panic_message(panic.as_ref()).to_owned()),
    }
}

fn scan_inner() -> win::Result<Scan> {
    let set = DevInfoSet::enum_all()?;
    let mut out = Out::new();
    out.source("SetupAPI");
    let present = setupapi::present_instance_ids();
    if let Err(error) = &present {
        out.fallback_failed("Present device snapshot", error)
            .text(&error.to_string());
    }
    let mut devices = Vec::new();
    let mut data = Vec::new();
    for device in set.devices()? {
        // C# parity: app/src/Services/DeviceCleaningService.cs:59-80.
        // INSTALL_STATE's required size, not its DWORD value, defines the legacy flag.
        let legacy_absent = match device.property_raw(SPDRP_INSTALL_STATE, 1024) {
            Ok(raw) => raw.required_size == 0,
            Err(error) => {
                out.fallback_failed("Legacy INSTALL_STATE (failure means ghost)", &error);
                true
            }
        };
        let instance = device.instance_id();
        let known_present = match (&present, &instance) {
            (Ok(ids), Ok(id)) if !id.is_empty() => Some(ids.contains(&id.to_uppercase())),
            _ => None,
        };
        let Some(presence) = classify(legacy_absent, known_present) else {
            continue;
        };
        let instance_id = match instance {
            Ok(id) => id,
            Err(error) => {
                out.fallback_failed("Device instance ID", &error)
                    .text(&error.to_string());
                String::new()
            }
        };
        let hardware_id = property_text(&device, &[SPDRP_HARDWAREID], &mut out)
            .unwrap_or_default()
            .split('\0')
            .collect::<String>();
        if IGNORED.contains(&hardware_id.as_str()) {
            continue;
        }
        // C# parity: app/src/Services/DeviceCleaningService.cs:97-107. Empty successful
        // descriptions win over friendly names; only missing properties fall back.
        let description = property_text(&device, &[SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME], &mut out)
            .unwrap_or_else(|| "Unknown Device".to_owned());
        let class = property_text(&device, &[SPDRP_CLASS], &mut out).unwrap_or_default();
        // C# parity: app/src/Services/DeviceCleaningService.cs:87-96,124.
        data.push(*device.data());
        devices.push(GhostDevice {
            name: "True".to_owned(),
            description,
            hardware_id,
            class,
            instance_id,
            presence,
        });
    }
    Ok(Scan {
        devices,
        data,
        diagnostics: out.finish(),
        set,
    })
}

fn classify(legacy_absent: bool, present: Option<bool>) -> Option<Presence> {
    // An unknown present check cannot disagree with a legacy "present": skip it like C#
    // instead of listing every present device as Unclear when the snapshot fails.
    match (legacy_absent, present) {
        (false, Some(true) | None) => None,
        (true, Some(false)) => Some(Presence::Absent),
        _ => Some(Presence::Unclear),
    }
}

fn property_text(
    device: &Device<'_>,
    properties: &[SETUP_DI_REGISTRY_PROPERTY],
    out: &mut Out,
) -> Option<String> {
    // C# parity: app/src/Services/DeviceCleaningService.cs:59-69. Do not grow the
    // buffer or use wide::from_wide: embedded NULs and leading NUL trimming matter.
    let mut last_failure = None;
    for &property in properties {
        match device.property_raw(property, 1024) {
            Ok(raw) if raw.required_size == 0 => continue,
            Ok(raw) => {
                let (pairs, tail) = raw.bytes.as_chunks::<2>();
                let units: Vec<_> = pairs.iter().map(|b| u16::from_le_bytes(*b)).collect();
                let mut text = String::from_utf16_lossy(&units);
                if !tail.is_empty() {
                    text.push('\u{FFFD}');
                }
                return Some(text.trim_matches('\0').to_owned());
            }
            Err(error) => {
                out.fallback_failed(&format!("Device property {}", property.0), &error);
                last_failure = Some(error);
            }
        }
    }
    // AD-03: failed sources stay diagnostic-only when another property supplied data.
    if let Some(error) = last_failure {
        out.text(&error.to_string());
    }
    None
}

impl Scan {
    /// Borrows the scanned ghosts and devices with unclear presence.
    pub fn devices(&self) -> &[GhostDevice] {
        &self.devices
    }
    /// Removes selected eligible devices, checking cancellation before each one.
    pub fn remove(self, selected: &[usize], cancel: &Cancel, status: &dyn Fn(&str)) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            if selected.is_empty() || cancel.is_cancelled() {
                return;
            }
            // Reload for each removal operation as well as each UI scan (AD-26/27).
            // A caller cannot bypass a corrupt file by passing a prefiltered selection.
            let whitelist = match whitelist::load_whitelist() {
                Ok(devices) => devices,
                Err(error) => {
                    status(whitelist::READ_FAILURE);
                    status(&format!("Error in Cleaning Process: {error}"));
                    return;
                }
            };
            remove_selected(
                &self.devices,
                selected,
                &whitelist,
                cancel,
                status,
                |index, device| {
                    if setupapi::is_present(&device.instance_id)? {
                        status(&format!(
                            "Skipped: {}. Device is now present (will not be removed).",
                            device.description
                        ));
                        return Ok(false);
                    }
                    if cancel.is_cancelled() {
                        return Ok(false);
                    }
                    let result = super::destructive(
                        &format!("SetupDiRemoveDevice: {}", device.description),
                        || {
                            if cancel.is_cancelled() {
                                return Ok(false);
                            }
                            // C# parity: app/src/Services/DeviceCleaningService.cs:171-175.
                            // Copied SP_DEVINFO_DATA belongs to the retained, uniquely owned set.
                            self.set.remove_device(&self.data[index]).map(|()| true)
                        },
                    );
                    match result {
                        Some(Ok(removed)) => Ok(removed),
                        Some(Err(error)) => {
                            // C# parity: app/src/Services/DeviceCleaningService.cs:182-183.
                            status(&format!(
                                "Failed to remove: {}. Error code: {}",
                                device.description, error.code
                            ));
                            Ok(false)
                        }
                        None => {
                            status(&format!(
                                "[DRY RUN] SetupDiRemoveDevice: {}",
                                device.description
                            ));
                            Ok(false)
                        }
                    }
                },
            );
        }));
        if let Err(panic) = result {
            status(&format!(
                "Error in Cleaning Process: {}",
                panic_message(panic.as_ref())
            ));
        }
        // Dropping self releases the HDEVINFO on success, cancellation and every error.
    }

    /// Exposes scan failures for diagnostics and unresolved error lines for the cleaner UI.
    pub fn diagnostics(&self) -> &Section {
        &self.diagnostics
    }
}

fn remove_selected(
    devices: &[GhostDevice],
    selected: &[usize],
    whitelist: &[GhostDevice],
    cancel: &Cancel,
    status: &dyn Fn(&str),
    mut remove: impl FnMut(usize, &GhostDevice) -> win::Result<bool>,
) {
    // Validate the entire selection before any side effect; indices cannot supply native tokens.
    if selected.iter().any(|&index| index >= devices.len()) {
        status(&format!(
            "Error in Cleaning Process: {}",
            Error::msg("Device selection", "index outside this scan")
        ));
        return;
    }
    let mut seen = HashSet::new();
    let eligible: Vec<_> = selected
        .iter()
        .copied()
        .filter(|&index| {
            seen.insert(index)
                && devices[index].presence == Presence::Absent
                && !whitelist::is_whitelisted(&devices[index], whitelist)
        })
        .collect();
    if eligible.is_empty() || cancel.is_cancelled() {
        return;
    }
    // C# parity: app/src/Services/DeviceCleaningService.cs:164,178,188,192-195.
    status(&format!(
        "\r\nAttempting to remove {} ghost device(s)...\r\n",
        eligible.len()
    ));
    let mut removed = 0;
    for &index in &eligible {
        if cancel.is_cancelled() {
            break;
        }
        let device = &devices[index];
        match catch_unwind(AssertUnwindSafe(|| remove(index, device))) {
            Ok(Ok(true)) => {
                removed += 1;
                status(&format!("Successfully removed: {}", device.description));
            }
            Ok(Ok(false)) => {} // The native boundary reports a skip, failure or dry run.
            Ok(Err(error)) => status(&format!("Error removing {}: {error}", device.description)),
            Err(panic) => status(&format!(
                "Error removing {}: {}",
                device.description,
                panic_message(panic.as_ref())
            )),
        }
    }
    status(&format!("\r\nTotal devices removed: {removed}"));
    if removed < eligible.len() {
        status(&format!(
            "Failed to remove {} device(s)",
            eligible.len() - removed
        ));
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("Unknown panic")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, time::Instant};

    fn device(id: &str, presence: Presence) -> GhostDevice {
        GhostDevice {
            name: "True".to_owned(),
            description: "Fabricated USB receiver".to_owned(),
            hardware_id: id.to_owned(),
            class: "USB".to_owned(),
            instance_id: r"USB\VID_046D&PID_C52B\7&2C615B84&0&3".to_owned(),
            presence,
        }
    }

    #[test]
    fn removal_requires_both_absence_checks() {
        assert_eq!(classify(true, Some(false)), Some(Presence::Absent));
        assert_eq!(classify(true, Some(true)), Some(Presence::Unclear));
        assert_eq!(classify(false, Some(false)), Some(Presence::Unclear));
        assert_eq!(classify(false, Some(true)), None);
        assert_eq!(classify(true, None), Some(Presence::Unclear));
        assert_eq!(classify(false, None), None);
    }

    #[test]
    fn removal_filters_protection_deduplicates_and_stops_on_cancel() {
        let devices = [
            device("protected", Presence::Absent),
            device("unclear", Presence::Unclear),
            device("first", Presence::Absent),
            device("second", Presence::Absent),
        ];
        let cancel = Cancel::new();
        let mut attempts = Vec::new();
        let messages = RefCell::new(Vec::new());
        remove_selected(
            &devices,
            &[0, 1, 2, 2, 3],
            &devices[..1],
            &cancel,
            &|text| messages.borrow_mut().push(text.to_owned()),
            |index, _| {
                attempts.push(index);
                cancel.cancel();
                Ok(true)
            },
        );
        assert_eq!(attempts, [2]);
        assert_eq!(
            messages.into_inner(),
            [
                "\r\nAttempting to remove 2 ghost device(s)...\r\n",
                "Successfully removed: Fabricated USB receiver",
                "\r\nTotal devices removed: 1",
                "Failed to remove 1 device(s)"
            ]
        );
    }

    #[test]
    fn invalid_selection_and_preexisting_cancel_never_call_removal() {
        let devices = [device("candidate", Presence::Absent)];
        let cancel = Cancel::new();
        remove_selected(&devices, &[0, 1], &[], &cancel, &|_| {}, |_, _| {
            panic!("invalid selection removed")
        });
        cancel.cancel();
        remove_selected(&devices, &[0], &[], &cancel, &|_| {}, |_, _| {
            panic!("cancelled selection removed")
        });
    }

    #[test]
    #[ignore = "Read-only local device capture; contains real identifiers. Redirect to private golden/wp-11 only."]
    fn wp11_read_only_scan() {
        let start = Instant::now();
        let scan = scan().expect("read-only scan");
        println!("WP11_GHOSTS_BEGIN");
        for device in scan.devices() {
            print!(
                "{} | {} | {} | {} | {:?}\r\n",
                device.description,
                device.class,
                device.hardware_id,
                device.instance_id,
                device.presence
            );
        }
        println!("WP11_GHOSTS_END");
        println!("Elapsed ms: {}", start.elapsed().as_millis());
        println!("Administrator: {}", win::security::is_admin());
        println!("Visible diagnostics:\n{}", scan.diagnostics().body);
        for failure in &scan.diagnostics().failures {
            println!("Diagnostic: {failure}");
        }
    }
}
