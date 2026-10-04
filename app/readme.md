# HWID Checker Project

> Native Rust Windows desktop tool for inspecting hardware identifiers, viewing them in a sectioned UI, and exporting results to text files. The C# app remains buildable in `app/src/` for legacy reference; the shipped exe comes from `app/rust/`.

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
    - [Command Line Scripts](#command-line-scripts)
  - [Project Structure](#project-structure)
  - [Export Format](#export-format)

## Building the Project

Run these commands from the repository root in PowerShell 7.

### Test build

```powershell
.\test-build.ps1
```

Builds a release test exe and prints its path without changing the shipped root exe. Add `-Safe` for a debug build with destructive actions guarded as dry runs; add `-Run` to open it (`-Safe -Run` clears the destructive-action override for that launch).

Visual Studio: open `app/HWID-CHECKER.sln`, select `Release | x64`, then **Build Solution**; the root exe stays unchanged.

### Release a new version

```powershell
.\release.ps1
```

Releases the next patch from a clean `main`; use `-Minor` for new features, `-Major` for big changes, or `-Version X.Y.Z` for an exact higher version. It updates Cargo's version and lockfile, checks and builds the exe, shows its hash and pending commits, then asks for `YES` before committing, tagging, pushing and creating the GitHub release. GitHub CLI (`gh`) must be signed in.

Add `-DryRun` to check and build without going live; it restores the version files and root exe afterward.

### Roll back

The [csharp-last GitHub release](https://github.com/Fundryi/HWID-Privacy/releases/tag/csharp-last) holds the last C# exe. To roll back the Rust app, revert and commit the code, then run `release.ps1` with a version higher than the current release; or restore an older root exe from Git history, commit it and push `main`. Installed copies offer it on their next update check because updates compare hashes.

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
