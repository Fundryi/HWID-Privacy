# HWID Checker Architecture

HWIDChecker is a native Rust Windows x64 inspector, cleaner, and updater. The shipped executable comes from `app/rust/`. It uses native Win32 controls and a static CRT, with no additional runtime installation. Every launch requires administrator rights.

## Build and publish

From the repository root:

```powershell
pwsh -NoProfile -File app/rust/scripts/check.ps1
.\app\rust\scripts\build-test.ps1
.\app\rust\scripts\release.ps1 -DryRun
```

`build-test.ps1` produces the release test exe under `app/rust/target/release/`; `-Safe` builds debug and `-Run` launches the selected build. It leaves the root exe unchanged. `build-dist.ps1` runs `check.ps1`, builds Cargo `dist`, validates PE/imports/manifest, and prints its size, version, and hash without staging or publishing.

The owner `scripts/release.ps1` requires clean `main`, refreshes Cargo's version and lockfile, invokes `build-dist.ps1`, verifies the dist version, and copies exactly `app/rust/target/dist/HWIDChecker.exe` to the root. It verifies the copied hash and version. `-DryRun` restores both version files and the root exe; a real release requires `YES` and race checks before commit, tag, atomic push, and GitHub release. Supported version switches are `-Minor`, `-Major`, and `-Version X.Y.Z`. Scripts derive the Rust root from their own directory and the repository root from the parent of `app`, so the caller's working directory does not matter. Cargo target overrides are refused for staging.

## Source layout and contracts

Rust comments marked `C# parity` record historical formatting and behavior decisions. Their C# filenames and line numbers refer to the [pre-retirement source snapshot](https://github.com/Fundryi/HWID-Privacy/tree/3768ddc9c21c9e64f8ada067d8466c7e9f7460e3/app/src), not files needed to build or test Rust. Fabricated compatibility fixtures remain active.

| Path under `app/rust/src/` | Responsibility |
|---|---|
| `main.rs` | GUI entrypoint and read-only CLI modes |
| `hw/` | Sixteen providers, shared collection context, parallel collection and deadlines |
| `win/` | Windows API wrappers, RAII handles, WMI, SMBIOS/EDID/storage parsers, bounded child processes, HTTP and hashing |
| `clean/` | Ghost-device removal, whitelist JSON, event-log cleaning, destructive-operation guard |
| `ui/` | Main window, raw view, cleaner/whitelist/confirmation/update windows, layout and drawing |
| `update.rs` | Hash comparison, retained download, executable replacement, rollback and restart |
| `report.rs` | Section model, output builder, text formatting and export serialization |

`hw/mod.rs` defines the provider table and context. Providers return `report::Section`; `report::Out` builds report text and diagnostics. A collection shares cached SetupAPI and SMBIOS data. WMI connections belong to the calling thread. Providers run in parallel and report progress; a provider exceeding 60 seconds becomes a timeout section.

The sixteen sections are disk, motherboard, BIOS, chassis, system, RAM, CPU, TPM, USB, GPU, Bluetooth, monitors, network, audio, battery, and ARP. Sources include WMI, SetupAPI, SMBIOS, storage IOCTLs, registry data, IP Helper, TPM and TBS APIs, MMDevice audio endpoints, battery and NDIS IOCTLs, and optional GPU/Bluetooth DLLs. Optional DLLs are loaded dynamically; missing APIs use fallback paths.

`ui` may call `hw`, `clean`, `update`, and `report`. Hardware, cleaning, and update logic may call `win` and `report`; `win` calls the OS. Worker failures are returned or recorded. Provider/worker panics are caught. Windows API safety and handle ownership live in the native wrappers and UI kit. The binding UI design is [Rust DESIGN.md](rust/DESIGN.md).

## Collection and export

The main window starts collection, receives sections and progress from workers, and displays the selected section. Refresh starts a new collection. Export serializes the complete report to `HWID-EXPORT-dd.MM.yyyy-HH;mm;ss.txt`. Output preserves CRLF line endings, UTF-16 width semantics, the 93-character main separator and 40-character item separator. Data and diagnostics are separate.

## Device and event-log cleaning

`clean/devices.rs` scans through SetupAPI, keeps devices with unclear presence, and removes only confirmed non-present devices after whitelist filtering and confirmation. `clean/whitelist.rs` stores the whitelist in the temp directory. The UI provides details, review output, and whitelist management.

`clean/eventlog/` processes standard and discovered event channels, requests security/backup privileges, skips missing/disabled/duplicate and known OS-locked channels, and uses native event-log APIs with bounded fallback processes. Analytic/Debug channels are restored after the disable-clear-re-enable cycle. The cleaner reports live progress and final totals. Log clearing removes event records; device cleaning is a separate action.

Debug builds guard destructive operations unless `HWID_ALLOW_DESTRUCTIVE=1` is set for an authorized child process. Release and dist builds perform confirmed actions normally.

## Updates

`update.rs` uses the unchanged channel `https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe` through `win/http.rs`. It compares SHA-256 of the download and running exe, independent of version ordering. It retains the checked bytes, validates size/hash and x64 PE before installation, renames the running image to a unique sibling, creates the replacement, and starts it before exiting. Write/restart errors attempt rollback; startup removes old siblings. See [auto-update.md](auto-update.md).

Deployed C# clients use the same URL and hash comparison. Their historical download/copy/restart protocol and the retained `csharp-last` fallback are documented in [auto-update.md](auto-update.md). The root `HWIDChecker.exe` path and name are part of that installed-client contract and must never change.

## Editing routes

| Change | Read and edit |
|---|---|
| Provider data | Existing `hw/` provider, `hw/mod.rs` contract, relevant `win/` wrapper |
| Shared text/export | `report.rs`, export handler in `ui/main_window.rs` |
| UI | Relevant `ui/` window, UI kit, `rust/DESIGN.md` |
| Device/whitelist | `clean/devices.rs`, `clean/whitelist.rs`, related UI and SetupAPI wrappers |
| Event logs | `clean/eventlog/`, `win/evt.rs`, `ui/clean_logs.rs` |
| Updater | `update.rs`, `win/http.rs`, `ui/update_progress.rs` |
| Build/staging | `rust/scripts/`, Cargo config, `rust/app.rc`, `rust/assets/app.ico`, and resource manifest |

Read local `AGENTS.md` when present, preserve public contracts and established report text, then run `app/rust/scripts/check.ps1` and check the actual changed behavior. Update this document when module ownership, contracts, data flow, or deployment changes.
