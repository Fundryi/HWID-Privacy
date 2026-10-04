//! Standard channel lists and read-only additional channel discovery.

use super::{PROCESS_TIMEOUT, Status, checkpoint, workers::parallel};
use crate::{
    report,
    win::{self, Error, Result, evt, process::Cancel},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// C# parity: EventLogCleaningService.cs:192-300 (order and spellings are deliberate).
pub(super) const STANDARD: &str = "Windows PowerShell
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
pub(super) const UNCLEARABLE: &str = "Microsoft-Windows-Kernel-PnP/Device Management
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

pub(super) fn unique<'a>(names: impl IntoIterator<Item = &'a str>) -> (Vec<String>, usize) {
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
pub(super) fn contains(names: &[String], name: &str) -> bool {
    names.iter().any(|n| report::eq_ignore_case(n, name))
}
pub(super) fn discover(known: Vec<String>, cancel: &Cancel, emit: &Status) -> Result<Vec<String>> {
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
