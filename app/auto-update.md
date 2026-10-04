# HWID Checker Auto-Update

The native Rust app and deployed legacy C# clients use the same update channel:

`https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe`

This URL and the repository-root `HWIDChecker.exe` path and name must stay unchanged. Checks use a cache-busting query and compare SHA-256 of the downloaded bytes with the running executable. Different hashes offer an update; version ordering is not used. Replacing the channel binary with the saved C# exe therefore also offers a rollback.

## Rust client

The sidebar **Auto Update** toggle enables only a startup check and an `Update available` footer notice; installation still needs confirmation. Its leading Sync icon has a check badge when on and an X badge when off. The `check_updates_on_start` setting defaults to false and is saved in `%LOCALAPPDATA%\HWIDChecker\settings.json` (the directory is created as needed). If that file is absent on startup, the app copies the legacy `HWIDChecker.settings.json` beside the exe to the new location before deleting the old file. Existing new settings take precedence; migration failures are recorded without crashing. Unknown JSON keys survive saves.

The Updates button runs `app/rust/src/update.rs` on a worker. `win/http.rs` downloads through WinHTTP with a 100-second budget and a 256 MiB limit, reports progress, and checks Content-Length when supplied. The checked bytes are retained for installation, avoiding a second download.

After confirmation, installation rechecks size and hash and validates a Windows x64 PE. It renames the running image to a unique `.old-` sibling, writes the new exe at the original path, and starts it directly. The old process exits only after process creation succeeds. Write/restart failures attempt to restore the original exe; rollback failures are reported. Startup cleans up old image siblings. Paths containing `%` are refused. Debug builds treat update installation as a dry run unless the process has `HWID_ALLOW_DESTRUCTIVE=1`.

## Legacy C# client hand-over

The [historical C# updater](https://github.com/Fundryi/HWID-Privacy/blob/3768ddc9c21c9e64f8ada067d8466c7e9f7460e3/app/src/Services/AutoUpdateService.cs) downloads the same channel to compare hashes, asks for confirmation, then downloads again into a fixed temp path. A temporary batch script waits two seconds, copies over the installed exe, deletes the temp file, and starts the replacement. This describes deployed legacy clients; their source is retired from the current tree.

The C# client does not require a managed assembly, a particular version, or a fixed size. It can therefore receive the Rust exe. Both applications require administrator rights on launch. The C# batch script has no copy/restart checks or retry loop: a slow exit, another process holding the file, permissions, or antivirus quarantine can prevent replacement or restart. Its second download is not rechecked against the first hash. Paths containing batch metacharacters and concurrent updates using the fixed temp names are additional legacy risks. The `.bat` is passed directly to `Process.Start` with `UseShellExecute=false`, without an explicit `cmd.exe /c`; successful batch launch is not established by this code review. These are existing client behaviors.

## Building and releasing

From the repository root in PowerShell 7, run [.\app\rust\scripts\build-test.ps1](rust/scripts/build-test.ps1) for a test exe; add `-Safe -Run` to open a debug build with destructive actions guarded as dry runs. The shipped root exe stays unchanged.

Run [.\app\rust\scripts\release.ps1](rust/scripts/release.ps1) to release the next patch (`-Minor`, `-Major`, or `-Version X.Y.Z` for a higher version). It reads the version from Cargo, invokes the checked `build-dist.ps1`, copies exactly `app/rust/target/dist/HWIDChecker.exe` to `HWIDChecker.exe` at the root, verifies its version and hash, and asks for `YES` before committing, tagging, pushing and creating the GitHub release. `-DryRun` builds and restores the local version files and root exe without publishing. Installed copies get the pushed root exe on their next update check.

See [Building the Project](readme.md#building-the-project) for the build commands and rollback options. The [csharp-last GitHub release](https://github.com/Fundryi/HWID-Privacy/releases/tag/csharp-last) holds the last C# exe.
