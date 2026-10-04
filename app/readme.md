# HWID Checker Project

> Native Rust Windows desktop tool for inspecting hardware identifiers in 16 sections and exporting results to text files. [Download HWIDChecker.exe](../HWIDChecker.exe) and run it as administrator.

## Table of Contents

- [HWID Checker Project](#hwid-checker-project)
  - [Table of Contents](#table-of-contents)
  - [Building the Project](#building-the-project)
    - [Test build](#test-build)
    - [Release a new version](#release-a-new-version)
    - [Roll back](#roll-back)
  - [Requirements](#requirements)
  - [Features](#features)
    - [Core Functionality](#core-functionality)
    - [Hardware Providers](#hardware-providers)
    - [System Services](#system-services)
    - [Cleaning Actions](#cleaning-actions)
  - [Usage Instructions](#usage-instructions)
    - [GUI Version](#gui-version)
  - [Project Structure](#project-structure)
  - [Export Format](#export-format)

## Building the Project

Run these commands from the repository root in PowerShell 7.

### Test build

Build: `.\app\rust\scripts\build-test.ps1` (release test exe; `-Safe` for debug, `-Run` to open it, `-Safe -Run` to launch with destructive actions guarded as dry runs; root exe unchanged).

### Release a new version

Release: `.\app\rust\scripts\release.ps1` (next patch; `-Minor`, `-Major`, or `-Version X.Y.Z` for a higher version; `-DryRun` builds and restores version files/root exe; clean `main` and signed-in `gh` required; `YES` gates commit, tag, atomic push, and GitHub release; previews the title `HWID Checker vX.Y.Z` and Changes/Download/SHA-256 body, with unique feat/fix/perf subjects since the previous `v*` tag).

The release script runs `build-dist.ps1`, which runs `check.ps1` and validates the dist executable, then copies exactly `app/rust/target/dist/HWIDChecker.exe` to the root after checking its version. The copied hash must match. Run `pwsh -NoProfile -File app/rust/scripts/check.ps1` for the standalone gate. Every script anchors paths to its own location and also works when invoked by absolute path from another folder.

### Roll back

The [csharp-last GitHub release](https://github.com/Fundryi/HWID-Privacy/releases/tag/csharp-last) holds the last C# exe as the historical fallback. To roll back Rust source, revert and commit the code, then run `.\app\rust\scripts\release.ps1` with a version higher than the current release. Installed copies compare hashes at the fixed root-executable URL documented in [auto-update.md](auto-update.md); keep that path and filename unchanged.

## Requirements

- Windows 10/11 (x64)
- Administrator privileges on every launch
- Standalone executable with a static CRT; no additional runtime installation
- To build: pinned Rust toolchain, Visual Studio C++ build tools/Windows SDK, PowerShell 7
- To release: Git and authenticated GitHub CLI (`gh`)

## Features

### Core Functionality

- Collects and displays hardware identifiers from local system sources
- Sectioned UI with per-section navigation
- Refresh scan results in-app
- Export full scan output to timestamped `.txt` files

### Hardware Providers

Current sections (16):

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
- Audio devices
- Battery
- ARP info/cache

### System Services

- Parallel hardware collection (`rust/src/hw/`)
- Output formatting (`rust/src/report.rs`)
- Text export and adjacent diagnostics from the main window (`rust/src/ui/main_window.rs`)
- Device cleaning + whitelist management
- Event log cleaning (native Windows API discovery, privilege elevation, OS-locked log skipping)
- Admin check and native Windows helpers (`rust/src/win/`)
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
   - `Refresh` to rescan
   - `Export` to export all section data and diagnostics
   - `Mask IDs` to hide identifiers in the view, Copy, and Export
   - `Compare now` / `Compare files` for before/after reports
   - `Clean Devices` / `Clean Logs` for maintenance tasks
   - `Updates` to check/download updates; `Auto Update` enables the optional startup check
5. For cleaning:
   - `Clean Devices` opens a device-focused cleanup flow with whitelist support.
   - `Clean Logs` opens a log-focused cleanup flow with live progress and end-of-run overview.

## Project Structure

```text
app/
├── rust/
│   ├── Cargo.toml
│   ├── assets/                               # Icon and embedded fonts/license
│   ├── scripts/
│   │   ├── check.ps1                         # Formatting, lint, tests, release checks
│   │   ├── build-test.ps1                    # Local build and optional launch
│   │   ├── build-dist.ps1                    # Checked dist build
│   │   └── release.ps1                       # Version, controlled staging, publication
│   ├── tests/fixtures/                       # Fabricated Rust regression data
│   └── src/
│       ├── main.rs                           # GUI and read-only CLI entrypoint
│       ├── hw/                               # 16 providers and collection
│       ├── win/                              # Native OS wrappers and parsers
│       ├── clean/                            # Devices, whitelist, event logs
│       ├── ui/                               # Native Win32 windows and controls
│       ├── update.rs                         # SHA-256 update/install/restart
│       └── report.rs                         # Text and export formatting
├── architecture.md
├── auto-update.md
└── readme.md
```

## Export Format

Exported files are plain text and include:

- Main header
- One formatted section per hardware provider
- Provider-specific identifiers and metadata

Filename pattern:

- `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`
