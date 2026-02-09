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
  - [Usage Instructions](#usage-instructions)
    - [GUI Version](#gui-version)
    - [Command Line Scripts](#command-line-scripts)
  - [Project Structure](#project-structure)
  - [Export Format](#export-format)

## Building the Project

From repository root:

```bash
dotnet publish "HWID-Checkers/Software-Project/source/HWIDChecker.csproj" -c Release
```

Alternative:

```bash
dotnet publish "HWID-Checkers/Software-Project/HWID-CHECKER.sln" -c Release
```

Output:

- Published executable: `HWID-Checkers/Software-Project/source/bin/RELEASE/win-x64/publish/HWIDChecker.exe`
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

Current providers (12):

- Disk drives
- Motherboard
- (SM)BIOS
- System information
- RAM modules
- CPU
- TPM modules
- USB devices
- GPU info
- Monitor information
- Network adapters
- ARP info/cache

### System Services

- Hardware collection orchestration (`HardwareInfoManager`)
- Output formatting (`TextFormattingService`)
- File export (`FileExportService`)
- Device cleaning + whitelist management
- Event log cleaning
- Auto-update check/download for `HWIDChecker.exe` from GitHub (SHA256 hash comparison)

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

### Command Line Scripts

Legacy batch scripts are in `HWID-Checkers/Bats/`:

```bat
HWID CHECK W10.bat
HWID CHECK W11.bat
```

## Project Structure

```text
HWID-Checkers/Software-Project/
├── source/
│   ├── Program.cs
│   ├── HWIDChecker.csproj
│   ├── Hardware/
│   │   ├── IHardwareInfo.cs
│   │   ├── HardwareInfoManager.cs
│   │   └── *Info.cs (12 providers)
│   ├── Services/
│   │   ├── AutoUpdateService.cs
│   │   ├── DeviceCleaningService.cs
│   │   ├── DeviceWhitelistService.cs
│   │   ├── EventLogCleaningService.cs
│   │   ├── FileExportService.cs
│   │   ├── SystemCleaningService.cs
│   │   ├── TextFormattingService.cs
│   │   ├── Models/DeviceDetail.cs
│   │   └── Win32/{SetupApi.cs, StorageDeviceIdQuery.cs}
│   ├── UI/
│   │   ├── Forms/SectionedViewForm.cs            # Active main UI
│   │   ├── Forms/CleanDevicesForm.cs
│   │   ├── Forms/CleanLogsForm.cs
│   │   ├── Forms/WhitelistDevicesForm.cs
│   │   ├── Forms/DeviceRemovalConfirmationForm.cs
│   │   └── Components/{Buttons.cs, ThemeColors.cs}
│   └── Resources/app.ico
├── AI-README.md
├── AUTO-UPDATE-README.md
└── HWID-CHECKER.sln
```

## Export Format

Exported files are plain text and include:

- Main header
- One formatted section per hardware provider
- Provider-specific identifiers and metadata

Filename pattern:

- `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`
