# HWID Checker Architecture

HWIDChecker is a native Rust Windows x64 inspector, cleaner, and updater. The shipped executable comes from `app/rust/`. It uses native Win32 controls and a static CRT and needs neither .NET nor the VC++ redistributable. Every launch requires administrator rights. The C# WinForms implementation in `app/src/` remains buildable for reference and rollback; it is no longer shipped.

## Build and publish

From the repository root:

```powershell
pwsh -NoProfile -File app/rust/check.ps1
dotnet build app/HWID-CHECKER.sln -c Release -p:Platform=x64
dotnet msbuild app/rust/HWIDChecker.Rust.proj -t:Publish
```

Build produces the Rust release exe under `app/rust/target/release/` and leaves the root exe unchanged. Publish runs `app/rust/release.ps1`: the base checks, the Cargo `dist` build, and dist PE/import/manifest validation. Only after success does the Rust Publish target copy `app/rust/target/dist/HWIDChecker.exe` to the root. C# has no PostPublish copy target. The Rust release script performs no version bump, commit, tag, push, or upload.

## Source layout and contracts

| Path under `app/rust/src/` | Responsibility |
|---|---|
| `main.rs` | GUI entrypoint and read-only CLI modes |
| `hw/` | Fourteen providers, shared collection context, parallel collection and deadlines |
| `win/` | Windows API wrappers, RAII handles, WMI, SMBIOS/EDID/storage parsers, bounded child processes, HTTP and hashing |
| `clean/` | Ghost-device removal, whitelist JSON, event-log cleaning, destructive-operation guard |
| `ui/` | Main window, raw view, cleaner/whitelist/confirmation/update windows, layout and drawing |
| `update.rs` | Hash comparison, retained download, executable replacement, rollback and restart |
| `report.rs` | Section model, output builder, text formatting and export serialization |

`hw/mod.rs` defines the provider table and context. Providers return `report::Section`; `report::Out` builds report text and diagnostics. A collection shares cached SetupAPI and SMBIOS data. WMI connections belong to the calling thread. Providers run in parallel and report progress; a provider exceeding 60 seconds becomes a timeout section.

The fourteen sections are disk, motherboard, BIOS, chassis, system, RAM, CPU, TPM, USB, GPU, Bluetooth, monitors, network, and ARP. Sources include WMI, SetupAPI, SMBIOS, storage IOCTLs, registry data, IP Helper, TPM APIs, and optional GPU/Bluetooth DLLs. Optional DLLs are loaded dynamically; missing APIs use fallback paths.

`ui` may call `hw`, `clean`, `update`, and `report`. Hardware, cleaning, and update logic may call `win` and `report`; `win` calls the OS. Worker failures are returned or recorded. Provider/worker panics are caught. Windows API safety and handle ownership live in the native wrappers and UI kit. The binding UI design is [Rust DESIGN.md](rust/DESIGN.md).

## Collection and export

The main window starts collection, receives sections and progress from workers, and displays the selected section. Refresh starts a new collection. Export serializes the complete report to `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`. Output preserves CRLF line endings, UTF-16 width semantics, the 93-character main separator and 40-character item separator. Data and diagnostics are separate.

## Device and event-log cleaning

`clean/devices.rs` scans through SetupAPI, keeps devices with unclear presence, and removes only confirmed non-present devices after whitelist filtering and confirmation. `clean/whitelist.rs` stores the whitelist in the temp directory. The UI provides details, review output, and whitelist management.

`clean/eventlog/` processes standard and discovered event channels, requests security/backup privileges, skips missing/disabled/duplicate and known OS-locked channels, and uses native event-log APIs with bounded fallback processes. Analytic/Debug channels are restored after the disable-clear-re-enable cycle. The cleaner reports live progress and final totals. Log clearing removes event records; device cleaning is a separate action.

Debug builds guard destructive operations unless `HWID_ALLOW_DESTRUCTIVE=1` is set for an authorized child process. Release and dist builds perform confirmed actions normally.

## Updates

`update.rs` uses the unchanged channel `https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe` through `win/http.rs`. It compares SHA-256 of the download and running exe, independent of version ordering. It retains the checked bytes, validates size/hash and x64 PE before installation, renames the running image to a unique sibling, creates the replacement, and starts it before exiting. Write/restart errors attempt rollback; startup removes old siblings. See [auto-update.md](auto-update.md).

Deployed C# clients in `app/src/Services/AutoUpdateService.cs` use the same URL and hash comparison. They download to a temp exe and use a batch file to copy and restart. The payload has no managed-assembly requirement, so the native exe can replace the C# exe.

## Editing routes

| Change | Read and edit |
|---|---|
| Provider data | Existing `hw/` provider, `hw/mod.rs` contract, relevant `win/` wrapper |
| Shared text/export | `report.rs`, export handler in `ui/main_window.rs` |
| UI | Relevant `ui/` window, UI kit, `rust/DESIGN.md` |
| Device/whitelist | `clean/devices.rs`, `clean/whitelist.rs`, related UI and SetupAPI wrappers |
| Event logs | `clean/eventlog/`, `win/evt.rs`, `ui/clean_logs.rs` |
| Updater | `update.rs`, `win/http.rs`, `ui/update_progress.rs` |
| Build/staging | `HWIDChecker.Rust.proj`, `release.ps1`, `check.ps1`, Cargo config and resource manifest |

Read `AGENTS.md` first, preserve public contracts and established report text, then run `check.ps1` and check the actual changed behavior. Update this document when module ownership, contracts, data flow, or deployment changes.
