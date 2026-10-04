# HWID Checker Project

> Native Rust Windows desktop tool for inspecting hardware identifiers, viewing them in a sectioned UI, and exporting results to text files. The C# app remains buildable in `app/src/` for legacy reference; the shipped exe comes from `app/rust/`.

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
pwsh -NoProfile -File app/rust/check.ps1
dotnet msbuild app/rust/HWIDChecker.Rust.proj -t:Publish
```

To build both Rust and the legacy C# app without changing the root exe:

```bash
dotnet build "app/HWID-CHECKER.sln" -c Release -p:Platform=x64
```

Output:

- Published executable: `app/rust/target/dist/HWIDChecker.exe`
- The Rust Publish target runs `release.ps1` (base checks, dist build, PE/import/manifest checks), then copies the dist exe to repository root (`HWIDChecker.exe`).
- Ordinary Build leaves the root exe unchanged. The legacy C# PostPublish copy target has been removed.

## Requirements

- Windows 10/11 (x64)
- Administrator privileges on every launch
- No .NET runtime or VC++ redistributable required to run the shipped exe
- To build: pinned Rust toolchain, Visual Studio C++ build tools/Windows SDK, PowerShell 7; .NET 10 SDK for the MSBuild wrapper and legacy solution project

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

- Parallel hardware collection (`src/hw/`)
- Output formatting and export serialization (`src/report.rs`)
- File export from the main window (`src/ui/main_window.rs`)
- Device cleaning + whitelist management
- Event log cleaning (native Windows API discovery, privilege elevation, OS-locked log skipping)
- Admin check and native Windows helpers (`src/win/`)
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
- Discovers additional active channels via native Wevtapi.dll calls (zero process spawns for discovery).
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
2. Wait for the initial scan in the native main window.
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
├── rust/
│   ├── HWIDChecker.Rust.proj                 # MSBuild Build/Publish wrapper
│   ├── Cargo.toml
│   ├── check.ps1                             # Formatting, lint, tests, release checks
│   ├── release.ps1                           # Checked dist build
│   └── src/
│       ├── main.rs                           # GUI and read-only CLI entrypoint
│       ├── hw/                               # 14 providers and collection
│       ├── win/                              # Native OS wrappers and parsers
│       ├── clean/                            # Devices, whitelist, event logs
│       ├── ui/                               # Native Win32 windows and controls
│       ├── update.rs                         # SHA-256 update/install/restart
│       └── report.rs                         # Text and export formatting
├── src/                                      # Legacy C# WinForms, still buildable
├── scripts/                                  # Legacy read/export batch scripts
└── HWID-CHECKER.sln
```

## Export Format

Exported files are plain text and include:

- Main header
- One formatted section per hardware provider
- Provider-specific identifiers and metadata

Filename pattern:

- `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`
