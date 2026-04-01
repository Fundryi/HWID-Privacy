# HWID-Privacy

## Project Overview

Hardware identification (HWID) privacy guide and tooling. The repo contains:
- **Guides** (`guides/`, `README.md`): Hardware spoofing/privacy documentation (MAC, SSD, motherboard, TPM, etc.)
- **HWIDChecker** (`app/`): A C# WinForms app that enumerates and displays hardware identifiers
- **Batch scripts** (`app/scripts/`): Quick HWID check scripts for Windows 10/11
- **Published binary** (`HWIDChecker.exe`): Pre-built executable at repo root

## Absolute Rules

> **RULE 1: Always verify build after code changes.**
> Run `dotnet publish -c Release` from `app/src/` and confirm it succeeds before committing.
> A broken build = a broken repo.

> **RULE 2: Never modify AutoUpdateService URLs.**
> The auto-update mechanism in `Services/AutoUpdateService.cs` points to a live endpoint.
> Changing it breaks updates for all deployed copies.

> **RULE 3: Never commit real hardware identifiers in guide examples.**
> Use realistic but fabricated identifiers -- real OUI prefixes, plausible serial formats,
> and manufacturer-consistent patterns. Change the device-specific portion, not the format.
> The result should look like a different real device, not obviously fake like `AA:BB:CC:DD:EE:FF`.

> **RULE 4: Never commit HWIDChecker.exe without rebuilding from source.**
> The repo-root binary must always match committed source. Run `dotnet publish -c Release`
> first -- the MSBuild PostPublish target copies it to repo root automatically.

> **RULE 5: Don't manually copy the published binary.**
> The PostPublish target in `app/src/HWIDChecker.csproj` handles copying to repo root.
> Manual copies risk version mismatch between source and binary.

> **RULE 6: Flag potentially unauthorized tool links in guides.**
> When adding tool or download links, don't silently add or remove questionable links.
> Flag them to the user and let them decide.

> **RULE 7: Guide content must be technically verified.**
> Hardware spoofing instructions affect real systems. Test procedures on actual hardware
> before documenting. Mark untested steps clearly with a warning.

## Tech Stack

- **Language**: C# (.NET 10, Windows Forms)
- **Target**: `win-x64`, single-file publish, framework-dependent
- **Solution**: `app/HWID-CHECKER.sln`
- **Project**: `app/src/HWIDChecker.csproj`
- **Dependencies**: `System.Management` (WMI queries)

## Build & Publish

```bash
# From app/src/
dotnet publish -c Release
# Output: bin/RELEASE/win-x64/publish/HWIDChecker.exe
# Post-publish copies to repo root automatically (MSBuild PostPublish target)
```

Only `Release|x64` configuration exists. No Debug config.

## Architecture

### Source Layout (`app/src/`)

```
Hardware/           # Per-component hardware info providers
  IHardwareInfo.cs  # Interface: GetInformation() + SectionTitle
  HardwareInfoManager.cs  # Orchestrator: runs all providers in parallel
  NetworkInfo.cs    # NIC/MAC enumeration (WMI Win32_NetworkAdapter)
  DiskDriveInfo.cs  # Storage devices
  MotherboardInfo.cs, BiosInfo.cs, SystemInfo.cs, CpuInfo.cs
  GpuInfo.cs, RamInfo.cs, TpmInfo.cs, UsbInfo.cs
  MonitorInfo.cs, ArpInfo.cs

Services/           # Business logic
  TextFormattingService.cs   # Output formatting
  FileExportService.cs       # Export results
  DeviceWhitelistService.cs  # Device whitelisting
  DeviceCleaningService.cs   # Device cleanup
  SystemCleaningService.cs   # System-level cleaning
  EventLogCleaningService.cs # Event log cleaning
  AutoUpdateService.cs       # Auto-update mechanism
  Models/DeviceDetail.cs     # Shared model
  Win32/SetupApi.cs          # P/Invoke for SetupAPI
  Win32/StorageDeviceIdQuery.cs  # Low-level storage ID queries

UI/                 # Windows Forms UI
  Forms/SectionedViewForm.cs     # Main window (sectioned hardware view)
  Forms/CleanDevicesForm.cs      # Device cleaning UI
  Forms/CleanLogsForm.cs         # Log cleaning UI
  Forms/WhitelistDevicesForm.cs  # Whitelist management
  Forms/DeviceRemovalConfirmationForm.cs
  Components/ThemeColors.cs      # Unified theme
  Components/Buttons.cs          # Shared button styles
```

### Key Pattern

All hardware providers implement `IHardwareInfo` (GetInformation + SectionTitle). `HardwareInfoManager` runs them all in parallel via `Task.WhenAll` and reports progress per-provider.

## Conventions

- One class per file, filename matches class name
- Namespace follows folder structure: `HWIDChecker.Hardware`, `HWIDChecker.Services`, `HWIDChecker.UI.Forms`
- WMI queries use `System.Management.ManagementObjectSearcher`
- Low-level Win32 access via P/Invoke in `Services/Win32/`
- UI uses a dark theme defined in `ThemeColors.cs`

## Read Routing Table

| Task | Read |
|---|---|
| Any code change | `CLAUDE.md` (conventions + architecture) |
| Tool/MCP selection | `AI_TOOLS.md` |
| Agent entry (Codex) | `AGENTS.md` -> `CLAUDE.md` |
| UI modernization work | `docs/plans/ui-modernization-plan.md` |
| Hardware provider changes | `app/src/Hardware/IHardwareInfo.cs` (interface contract) |
| WMI query patterns | Existing providers (`NetworkInfo.cs`, `DiskDriveInfo.cs`) |
| Guide content changes | `guides/` (mac-spoofing, ssd-spoofing, etc.) + `README.md` |
| Batch script changes | `app/scripts/` + `CLAUDE.md` conventions |
| Build/publish workflow | `CLAUDE.md` Build & Publish section |
| Project restructure status | `RESTRUCTURE-PLAN.md` (temporary artifact -- remove when archived) |

## Known Issues

- Mellanox ConnectX-3 PCIe NICs are filtered out by `NetworkInfo.IsRealNetworkAdapter()` — the `AdapterType` check rejects enterprise NICs that don't report "Ethernet 802.3" via WMI. Root cause identified, fix pending.

## Active Work

- Branch `feature/ui-modernization`: UI overhaul (see `docs/plans/ui-modernization-plan.md`)

## Project Gotchas

| Topic | Gotcha |
|---|---|
| WMI AdapterType filtering | `NetworkInfo.IsRealNetworkAdapter()` rejects NICs that don't report "Ethernet 802.3" -- enterprise/PCIe NICs (Mellanox ConnectX, etc.) get filtered out |
| WMI query permissions | Some WMI classes require admin elevation -- app should handle `ManagementException` gracefully |
| Single-file publish | `PublishSingleFile` bundles everything -- P/Invoke DLLs must be marked for extraction if needed |
| PostPublish copy path | `DestinationFolder="../.."` in the csproj is relative to `app/src/` and correctly resolves to repo root. If source directory depth changes, this path breaks silently |
| Parallel provider failures | `HardwareInfoManager` runs all providers via `Task.WhenAll` -- one provider throwing can surface as `AggregateException` |
| SetupAPI P/Invoke | `Services/Win32/SetupApi.cs` uses direct Win32 calls -- signature mismatches crash the entire app, not just the feature |
| Dark theme assumption | All UI components assume `ThemeColors.cs` dark palette -- adding new forms without applying the theme creates visual inconsistency |
| Auto-update URL | Hardcoded in `AutoUpdateService.cs` -- changing it breaks all deployed copies (see Absolute Rule 2) |
| .NET version targeting | Project targets .NET 10 -- build machines need the matching SDK. Old `obj/` artifacts from prior SDK versions (v8, v9) are harmless leftovers |
| Framework-dependent deploy | Published binary requires .NET runtime on target machine -- not self-contained |
