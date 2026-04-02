# HWID Checker Project

> Windows desktop tool for inspecting hardware identifiers, viewing them in a sectioned UI, and exporting results to text files.

## Table of Contents

- [HWID Checker Project](#hwid-checker-project)
  - [Table of Contents](#table-of-contents)
  - [Building the Project](#building-the-project)
  - [Requirements](#requirements)
  - [Features](#features)
    - [Core Functionality](#core-functionality)
    - [Hardware Providers](#hardware-providers)
    - [System Services](#system-services)
    - [Cleaning Actions](#cleaning-actions)
  - [Usage Instructions](#usage-instructions)
    - [GUI Version](#gui-version)
    - [Command Line Scripts](#command-line-scripts)
  - [Project Structure](#project-structure)
  - [Export Format](#export-format)

## Building the Project

From repository root:

```bash
dotnet publish "app/src/HWIDChecker.csproj" -c Release
```

Alternative:

```bash
dotnet publish "app/HWID-CHECKER.sln" -c Release
```

Output:

- Published executable: `app/src/bin/RELEASE/win-x64/publish/HWIDChecker.exe`
- Post-publish copy target: repository root (`HWIDChecker.exe`)

## Requirements

- .NET 10.0 SDK
- Windows 10/11 (x64)
- Administrator privileges only for cleaning features (device/log cleaning)

## Features

### Core Functionality

- Collects and displays hardware identifiers from local system sources
- Sectioned UI with per-section navigation
- Refresh scan results in-app
- Export full scan output to timestamped `.txt` files

### Hardware Providers

Current providers (14):

- Disk drives
- Motherboard
- (SM)BIOS
- Chassis (SMBIOS Type 3)
- System information
- RAM modules
- CPU
- TPM modules
- USB devices
- GPU info
- Bluetooth devices
- Monitor information
- Network adapters
- ARP info/cache

### System Services

- Hardware collection orchestration (`HardwareInfoManager`)
- Output formatting (`TextFormattingService`)
- File export (`FileExportService`)
- Device cleaning + whitelist management
- Event log cleaning (P/Invoke-based discovery, privilege elevation, OS-locked log skipping)
- Admin check helper (`SecurityHelper`)
- Auto-update check/download for `HWIDChecker.exe` from GitHub (SHA256 hash comparison)

### Cleaning Actions

`🧹 Clean Devices`:
- Scans for non-present (ghost) devices only.
- Shows full device details before removal (name, description, hardware ID, class).
- Applies whitelist filtering before removal.
- Removes only non-whitelisted ghost devices after confirmation.
- Supports whitelist management (`Manage Whitelist`) and per-run review output.

`📝 Clean Logs`:
- Clears a curated standard set of Windows event channels first.
- Discovers additional active channels via Wevtapi.dll P/Invoke (zero process spawns).
- Elevates `SeSecurityPrivilege` and `SeBackupPrivilege` for protected log access.
- Handles Analytic/Debug channels with a disable-clear-re-enable cycle.
- Skips 23 OS-locked channels (kernel/driver/service-held) to avoid wasted fallback attempts.
- Uses deduplicated channel sets (case-insensitive) to avoid double processing.
- Skips channels that are missing/disabled on the current system.
- Shows live progress and a final summary block with:
  - collected standard/additional totals
  - attempted/cleared counts
  - skipped (not found/disabled/duplicate)
  - skipped unclearable (OS-locked)
  - failed count
- Window remains open after completion for manual review.

Operational note:
- Log cleaning is history-destructive by design (it removes event log records).
- It does not modify hardware state; device cleaning and log cleaning are separate actions.

## Usage Instructions

### GUI Version

1. Run `HWIDChecker.exe`.
2. Wait for initial scan (main window loads into `SectionedViewForm`).
3. Use section buttons to inspect specific hardware outputs.
4. Use:
   - `↻ Refresh` to rescan
   - `💾 Export` to export all section data
   - `🧹 Clean Devices` / `📝 Clean Logs` for maintenance tasks (admin required)
   - `⟳ Updates` to check/download updates
5. For cleaning:
   - `Clean Devices` opens a device-focused cleanup flow with whitelist support.
   - `Clean Logs` opens a log-focused cleanup flow with live progress and end-of-run overview.

### Command Line Scripts

Legacy batch scripts are in `app/scripts/`:

```bat
hwid-check-w10.bat
hwid-check-w11.bat
```

## Project Structure

```text
app/
├── src/
│   ├── Program.cs
│   ├── HWIDChecker.csproj
│   ├── Hardware/
│   │   ├── IHardwareInfo.cs
│   │   ├── HardwareInfoManager.cs
│   │   └── *Info.cs (14 providers)
│   ├── Services/
│   │   ├── AutoUpdateService.cs
│   │   ├── DeviceCleaningService.cs
│   │   ├── DeviceWhitelistService.cs
│   │   ├── EventLogCleaningService.cs
│   │   ├── FileExportService.cs
│   │   ├── SecurityHelper.cs
│   │   ├── SystemCleaningService.cs
│   │   ├── TextFormattingService.cs
│   │   ├── Models/DeviceDetail.cs
│   │   └── Win32/{EventLogApi.cs, FirmwareTable.cs, IpHlpApi.cs, SetupApi.cs, StorageDeviceIdQuery.cs}
│   ├── UI/
│   │   ├── Forms/SectionedViewForm.cs            # Active main UI
│   │   ├── Forms/CleanDevicesForm.cs
│   │   ├── Forms/CleanLogsForm.cs
│   │   ├── Forms/WhitelistDevicesForm.cs
│   │   ├── Forms/DeviceRemovalConfirmationForm.cs
│   │   └── Components/{Buttons.cs, ThemeColors.cs}
│   └── Resources/app.ico
└── HWID-CHECKER.sln
```

## Export Format

Exported files are plain text and include:

- Main header
- One formatted section per hardware provider
- Provider-specific identifiers and metadata

Filename pattern:

- `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`
