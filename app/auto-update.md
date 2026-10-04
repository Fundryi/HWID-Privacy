# HWID Checker Auto-Update

The native Rust app and deployed legacy C# clients use the same update channel:

`https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe`

This URL must stay unchanged. Checks use a cache-busting query and compare SHA-256 of the downloaded bytes with the running executable. Different hashes offer an update; version ordering is not used. Replacing the channel binary with the saved C# exe therefore also offers a rollback.

## Rust client

The Updates button runs `app/rust/src/update.rs` on a worker. `win/http.rs` downloads through WinHTTP with a 100-second budget and a 256 MiB limit, reports progress, and checks Content-Length when supplied. The checked bytes are retained for installation, avoiding a second download.

After confirmation, installation rechecks size and hash and validates a Windows x64 PE. It renames the running image to a unique `.old-` sibling, writes the new exe at the original path, and starts it directly. The old process exits only after process creation succeeds. Write/restart failures attempt to restore the original exe; rollback failures are reported. Startup cleans up old image siblings. Paths containing `%` are refused. Debug builds guard update installation unless explicitly enabled for an authorized child process.

## Legacy C# client hand-over

`app/src/Services/AutoUpdateService.cs` downloads the same channel to compare hashes, asks for confirmation, then downloads again into a fixed temp path. A temporary batch script waits two seconds, copies over the installed exe, deletes the temp file, and starts the replacement.

The C# client does not require a managed assembly, a particular version, or a fixed size. It can therefore receive the Rust exe. Both applications require administrator rights on launch. The C# batch script has no copy/restart checks or retry loop: a slow exit, another process holding the file, permissions, or antivirus quarantine can prevent replacement or restart. Its second download is not rechecked against the first hash. Paths containing batch metacharacters and concurrent updates using the fixed temp names are additional legacy risks. The `.bat` is passed directly to `Process.Start` with `UseShellExecute=false`, without an explicit `cmd.exe /c`; successful batch launch is not established by this code review. These are existing client behaviors.

## Local build and channel staging

From the repository root:

```powershell
pwsh -NoProfile -File app/rust/check.ps1
dotnet msbuild app/rust/HWIDChecker.Rust.proj -t:Publish
```

Publish runs `app/rust/release.ps1`, builds `target/dist/HWIDChecker.exe`, validates its PE/imports/manifest, and copies it to the repository root through the Rust Publish target. Ordinary solution/Rust Build leaves the root exe unchanged. Never manually copy the shipped exe. The legacy C# project remains buildable and has no root-exe copy target.

Local staging does not change GitHub. The orchestrator reviews the source and root binary together, then commits, merges, and pushes after owner approval. Installed copies see the new payload on their next user-initiated update check after `main` changes.

The last C# exe and its SHA-256 are preserved locally at `app/rust/golden/cutover/` in the main checkout for a later rollback release asset. Uploading that asset is separate from this local cutover preparation.
