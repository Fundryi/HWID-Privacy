# AI-README: HWID Checker Architecture Index

> **READ THIS FIRST** - This file is the authoritative guide for AI-assisted code modifications.
> WE USE THIS BUILD COMMAND ALWAYS: dotnet publish "app/src/HWIDChecker.csproj" -c Release

---

## Status

- Legacy `MainForm*` and `UI/DataHandlers/*` paths were removed in Phase 2 cleanup.
- Active UI entry point is `UI/Forms/SectionedViewForm.cs`.

---

## Quick Reference: Modification Patterns

| Task | Files to Modify | Key Classes |
|------|----------------|-------------|
| Add new hardware info type | `Hardware/*Info.cs`, `HardwareInfoManager.cs` | `IHardwareInfo` |
| Add property to existing hardware | `Hardware/*Info.cs` | Specific info class |
| Change text formatting | `TextFormattingService.cs` | `TextFormattingService` |
| Add UI button/handler | `SectionedViewForm.cs` | `SectionedViewForm` (buttons created inline) |
| Add export format | `FileExportService.cs` | `FileExportService` |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Program.cs                                 │
│                           (Entry Point)                              │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     SectionedViewForm.cs                            │
│              (MAIN UI - Sidebar + Content + Buttons)                │
└──────┬──────────────────────────────────────────────────┬───────────┘
       │                                                  │
       ▼                                                  ▼
┌─────────────────────┐                        ┌─────────────────────┐
│ HardwareInfoManager │                        │   Services Layer    │
│  (Orchestrator)     │                        │                     │
└──────┬──────────────┘                        │ • TextFormatting    │
       │                                       │ • FileExport        │
       │                                       │ • DeviceCleaning    │
       │                                       │ • EventLogCleaning  │
       │                                       │ • AutoUpdate        │
       ▼                                       └─────────────────────┘
┌─────────────────────────────────────────────────────────────────────┐
│                     Hardware Providers                               │
│              (Implement IHardwareInfo)                               │
│                                                                     │
│ DiskDriveInfo, CpuInfo, GpuInfo, BiosInfo,                         │
│ MotherboardInfo, RamInfo, TpmInfo, UsbInfo,                        │
│ MonitorInfo, NetworkInfo, ArpInfo                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Core Contracts

### IHardwareInfo Interface
**Location:** `Hardware/IHardwareInfo.cs`

All hardware info classes must implement:
```csharp
string GetInformation();     // Returns raw data string
string SectionTitle { get; } // Display title for UI
```

---

## File Responsibilities

### Hardware Layer (`Hardware/`)

| File | Purpose | Dependencies |
|------|---------|-------------|
| `HardwareInfoManager.cs` | **ORCHESTRATOR** - Combines all hardware providers, runs parallel collection | All `*Info.cs`, `TextFormattingService` |
| `IHardwareInfo.cs` | Contract for all hardware providers | None |
| `DiskDriveInfo.cs` | Collects disk info (device ID, model, serial, firmware, volume serial, WWN/UniqueId) | `TextFormattingService`, WMI, PowerShell |
| `CpuInfo.cs` | Collects CPU info (name, processor ID, serial) | `TextFormattingService`, WMI |
| `BiosInfo.cs` | Collects BIOS/SMBIOS info (manufacturer, version, UUID) | `TextFormattingService`, WMI |
| `MotherboardInfo.cs` | Collects motherboard info (product, serial, manufacturer) | `TextFormattingService`, WMI |
| `RamInfo.cs` | Collects RAM info (capacity, speed, device locator) | `TextFormattingService`, WMI |
| `GpuInfo.cs` | Collects GPU info (name, device ID, driver version) | `TextFormattingService`, WMI |
| `TpmInfo.cs` | Collects TPM state + EK details (hash/serial/thumbprint/issuer) | `TextFormattingService`, PowerShell |
| `UsbInfo.cs` | Collects USB device info (device ID, description) | `TextFormattingService`, WMI |
| `MonitorInfo.cs` | Collects monitor info (name, serial, product code) | `TextFormattingService`, WMI |
| `NetworkInfo.cs` | Collects network adapter info (MAC, adapter type, driver) | `TextFormattingService`, WMI |
| `ArpInfo.cs` | Collects dynamic ARP entries (IP + MAC) | `TextFormattingService`, `arp.exe` |

### Services Layer (`Services/`)

| File | Purpose | Dependencies |
|------|---------|-------------|
| `TextFormattingService.cs` | **FORMATTER** - Formats all output text (headers, sections, separators) | None |
| `FileExportService.cs` | Exports hardware data to timestamped TXT files | None |
| `DeviceCleaningService.cs` | Scans and removes ghost (non-present) devices | `SetupApi` (Win32) |
| `SystemCleaningService.cs` | Wrapper for async device cleaning operations | `DeviceCleaningService` |
| `DeviceWhitelistService.cs` | Manages device whitelist for cleaning | None |
| `EventLogCleaningService.cs` | Cleans Windows event logs | None |
| `AutoUpdateService.cs` | Checks GitHub-hosted exe via SHA256 hash diff and auto-updates local exe | `HttpClient`, hashing, WinForms |

### Services - Subdirectories

| Subdir | Purpose |
|--------|---------|
| `Models/` | Data models (`DeviceDetail`) |
| `Win32/` | Native Windows API P/Invoke declarations (`SetupApi`) |

### UI Layer (`UI/`)

| File | Purpose | Dependencies |
|------|---------|-------------|
| `Forms/SectionedViewForm.cs` | **MAIN UI** - Sidebar navigation + content display + all buttons | `HardwareInfoManager`, `FileExportService`, `AutoUpdateService` |
| `Forms/CleanDevicesForm.cs` | Ghost device cleaning UI | `SystemCleaningService`, `DeviceWhitelistService` |
| `Forms/CleanLogsForm.cs` | Event log cleaning UI | `EventLogCleaningService` |
| `Forms/WhitelistDevicesForm.cs` | Device whitelist management UI | `DeviceWhitelistService` |
| `Forms/DeviceRemovalConfirmationForm.cs` | Confirmation dialog for device removal | None |
| `Components/Buttons.cs` | Button styling utilities | None |
| `Components/ThemeColors.cs` | Color constants for dark theme | None |

---

## Critical Workflows

### Adding a New Hardware Info Type

**Example: Adding `SoundCardInfo.cs`**

1. **Create the provider class** (`Hardware/SoundCardInfo.cs`):
   - Implement `IHardwareInfo`
   - Set `SectionTitle => "SOUND CARDS"`
   - Use WMI to query `Win32_SoundDevice`
   - Inject `TextFormattingService` in constructor
   - Return formatted string from `GetInformation()`

2. **Register in HardwareInfoManager** (`Hardware/HardwareInfoManager.cs`):
   - Add to `InitializeProviders()` list:
   ```csharp
   new SoundCardInfo(textFormatter)
   ```

### Adding a New Property to Existing Hardware

**Example: Adding "Temperature" to CPU info**

1. **Modify the info class** (`Hardware/CpuInfo.cs`):
   - Add WMI query for temperature (if available)
   - Append to output string

### Modifying Text Formatting

**Location:** `Services/TextFormattingService.cs`

- `FormatHeader()` - Main title formatting
- `FormatSection()` - Section header + content wrapper
- `AppendInfoLine()` - Single line: `Label: Value`
- `AppendCombinedInfoLine()` - Multi-column with ` | ` separator
- `AppendDeviceGroup()` - Multiple devices with separators

Constants at top:
- `LINE_WIDTH = 93` - Main separator width
- `ITEM_SEPARATOR_WIDTH = 40` - Item separator width

## Device Cleaning System

**Flow:**
1. `CleanDevicesForm` → `SystemCleaningService.ScanForGhostDevicesAsync()`
2. `SystemCleaningService` → `DeviceCleaningService.ScanForGhostDevices()`
3. `DeviceCleaningService` uses `SetupApi` (Win32) to enumerate devices
4. Filters out whitelisted devices (`DeviceWhitelistService`)
5. Shows confirmation dialog
6. Removes via `SetupDiRemoveDevice()`

**Win32 Integration:**
- `Services/Win32/SetupApi.cs` - P/Invoke declarations
- `SP_DEVINFO_DATA` struct - Device info structure
- `SetupDiGetClassDevs()` - Get device list
- `SetupDiEnumDeviceInfo()` - Enumerate devices
- `SetupDiGetDeviceRegistryProperty()` - Get device properties
- `SetupDiRemoveDevice()` - Remove device

---

## Auto-Update System

**Location:** `Services/AutoUpdateService.cs`

**Flow:**
1. Download GitHub `HWIDChecker.exe` (raw URL with cache-busting query)
2. Compute SHA256 of GitHub file
3. Compute SHA256 of current local executable
4. If hashes differ, download update to temp file
5. Create batch script to replace exe and restart app

**Dependency:** `HttpClient` + hashing (`System.Security.Cryptography`)

---

## Data Flow Diagram

```mermaid
graph TD
    A[User Clicks Scan] --> B[SectionedViewForm]
    B --> C[HardwareInfoManager.GetAllHardwareInfo]
    C --> D[Parallel: GetInformation from each *Info]
    D --> E[TextFormattingService.FormatSection]
    E --> F[Combine all sections]
    F --> G[Parse into sections]
    G --> H[Create sidebar buttons]
    H --> I[Display in content panel]
    
    J[User Clicks Export] --> K[FileExportService.ExportHardwareInfo]
    K --> L[Save as HWID-EXPORT-DD.MM.YY-HH;mm;ss.txt]
```

---

## Important Constants & Config

| Location | Constant | Value | Purpose |
|----------|----------|-------|---------|
| `TextFormattingService.cs` | `LINE_WIDTH` | 93 | Main separator width |
| `TextFormattingService.cs` | `ITEM_SEPARATOR_WIDTH` | 40 | Item separator width |
| `SectionedViewForm.cs` | `ENABLE_DEBUG_BUTTON` | `true` | Show/hide debug button |
| `Program.cs` | Main form | `SectionedViewForm` | Entry point UI |

---

## When to Update This File

**Update architecture.md when:**

1. Adding/removing hardware info types
2. Changing service layer contracts
3. Adding new UI forms or major UI restructuring
4. Modifying the data flow between layers
5. Adding new Win32 integrations or native API calls
6. Changing the auto-update mechanism

**DO NOT update for:**
- Bug fixes in existing code
- Minor UI tweaks
- Performance optimizations
- Code refactoring that doesn't change architecture

---

## Quick Start for AI

**Before making ANY changes:**

1. Read this file (`architecture.md`)
2. Identify the modification pattern from the Quick Reference table
3. Read the relevant files listed in that pattern
4. Make changes to ALL files listed in the pattern
5. Test that the change works end-to-end

**Example Prompt:**
> "Read AI-README.md, then add a new property 'PowerState' to DiskDriveInfo. Update all necessary files according to the 'Add property to existing hardware' pattern."

---

## File Structure Summary

```
source/
├── Program.cs                          # Entry point
├── HWIDChecker.csproj                  # Project file
├── Hardware/
│   ├── IHardwareInfo.cs                # Interface
│   ├── HardwareInfoManager.cs          # ORCHESTRATOR
│   └── *Info.cs                        # 12 hardware providers
├── Services/
│   ├── TextFormattingService.cs        # FORMATTER
│   ├── FileExportService.cs            # Export to TXT
│   ├── DeviceCleaningService.cs        # Ghost device removal
│   ├── SystemCleaningService.cs        # Async wrapper
│   ├── DeviceWhitelistService.cs       # Whitelist management
│   ├── EventLogCleaningService.cs      # Log cleaning
│   ├── AutoUpdateService.cs            # GitHub updates
│   ├── Models/                         # Data models
│   └── Win32/                          # Native Windows API
└── UI/
    ├── Forms/
    │   ├── SectionedViewForm.cs         # MAIN UI
    │   ├── CleanDevicesForm.cs         # Device cleaning
    │   ├── CleanLogsForm.cs            # Log cleaning
    │   ├── WhitelistDevicesForm.cs     # Whitelist UI
    │   └── DeviceRemovalConfirmationForm.cs
    ├── Components/                     # Reusable UI components
    │   ├── Buttons.cs
    │   └── ThemeColors.cs
```

---

**Last Updated:** 2026-02-09
