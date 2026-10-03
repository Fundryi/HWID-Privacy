//! Owned by WP-13: download once, validate, then install the retained file.

use crate::{
    clean, report,
    win::{self, hash, http, process},
};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, process::CommandExt},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// The unchanged channel consumed by deployed C# and Rust copies.
pub const UPDATE_URL: &str = "https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe";
/// A checked download whose private owner preserves its bytes and cleans up on decline.
pub struct Downloaded {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    response: http::Download,
    executable: PathBuf,
}
/// The C# hash comparison result, with the single download retained when different.
pub enum UpdateCheck {
    UpToDate,
    Available(Downloaded),
}

/// Downloads once and compares SHA-256, retaining the size-checked file when available.
pub fn check() -> Result<UpdateCheck, String> {
    check_with_progress(&process::Cancel::new(), &mut |_, _| {})
}

/// Checks the same channel while forwarding measured download progress and cancellation.
pub fn check_with_progress(
    cancel: &process::Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<UpdateCheck, String> {
    caught("Update check", || {
        let executable = std::env::current_exe()
            .map_err(|e| text(hash::io_error("Locate current executable", e)))?;
        let url = channel_url().map_err(text)?;
        check_url(&url, &executable, cancel, progress)
    })
}

fn channel_url() -> win::Result<String> {
    #[cfg(debug_assertions)]
    let base = match std::env::var("HWID_UPDATE_URL") {
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => UPDATE_URL.to_owned(),
        Err(error) => return Err(win::Error::msg("HWID_UPDATE_URL", error.to_string())),
    };
    #[cfg(not(debug_assertions))]
    let base = UPDATE_URL.to_owned();
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| win::Error::msg("Update timestamp", e.to_string()))?
        .as_secs();
    // C# parity: Services/AutoUpdateService.cs:93 (Unix seconds, not a version endpoint).
    Ok(format!("{base}?cb={seconds}"))
}

fn check_url(
    url: &str,
    executable: &Path,
    cancel: &process::Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<UpdateCheck, String> {
    // C# parity: Services/AutoUpdateService.cs:106 (fixed prefix before the download or
    // remote-hash error, so the UI line is `Error checking for updates: Failed to get ...`).
    let remote =
        |e: win::Error| format!("Failed to get GitHub file SHA256 for HWIDChecker.exe: {e}");
    let response = http::download(url, cancel, progress).map_err(remote)?;
    let sha256 = response
        .temp
        .file()
        .and_then(hash::sha256_file)
        .map_err(remote)?;
    // A local hash failure is returned instead of silently treating it as a mismatch.
    let local = hash::sha256(executable).map_err(text)?;
    // C# parity: Services/AutoUpdateService.cs:66-77 (hash equality, never version ordering).
    if report::eq_ignore_case(&local, &sha256) {
        return Ok(UpdateCheck::UpToDate);
    }
    Ok(UpdateCheck::Available(Downloaded {
        path: response.temp.path().to_owned(),
        size: response.size,
        sha256,
        response,
        executable: executable.to_owned(),
    }))
}

impl Downloaded {
    /// Returns the retained response length for the progress UI, including unknown length.
    pub fn content_length(&self) -> Option<u64> {
        self.response.content_length
    }
}

/// Installs the retained download and restarts, returning errors before setup completes.
pub fn install_and_restart(mut d: Downloaded) -> Result<(), String> {
    caught("Update install", || install(&mut d).map_err(text))
}

fn install(d: &mut Downloaded) -> win::Result<()> {
    let current =
        std::env::current_exe().map_err(|e| hash::io_error("Locate current executable", e))?;
    if current != d.executable || d.path != d.response.temp.path() || d.size != d.response.size {
        return Err(win::Error::msg(
            "Update install",
            "retained download metadata changed",
        ));
    }
    checked_path(&current)?;
    checked_path(&d.path)?;
    let file = d.response.temp.file()?;
    let size = file
        .metadata()
        .map_err(|e| hash::io_error("Read update size", e))?
        .len();
    if size != d.size
        || d.response
            .content_length
            .is_some_and(|expected| expected != size)
    {
        return Err(win::Error::msg(
            "Download size",
            "retained size differs from Content-Length or checked size",
        ));
    }
    hash::validate_x64_pe(file)?;
    if !report::eq_ignore_case(&hash::sha256_file(file)?, &d.sha256) {
        return Err(win::Error::msg(
            "Update SHA256",
            "retained download hash changed",
        ));
    }
    const WHAT: &str = "install update and restart";
    match clean::destructive(WHAT, || launch_installer(d)) {
        Some(result) => result?,
        // The guard's contract: the caller shows `[DRY RUN] {what}` (stderr is lost in the GUI).
        None => {
            return Err(win::Error::msg(
                "Update install",
                format!("[DRY RUN] {WHAT}"),
            ));
        }
    }
    // C# parity: Services/AutoUpdateService.cs:272-273 (exit only after helper launch).
    std::process::exit(0)
}

/// AD-34: a `%` path is shown as exactly `Update failed: path contains %`.
const PERCENT: &str = "path contains %";

fn text(error: win::Error) -> String {
    if error.detail == PERCENT {
        error.detail
    } else {
        error.to_string()
    }
}

fn caught<T>(op: &'static str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            Err(win::Error::msg(op, format!("panicked: {message}")).to_string())
        }
    }
}

fn checked_path(path: &Path) -> win::Result<()> {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&(b'%' as u16)) {
        return Err(win::Error::msg("Update install", PERCENT));
    }
    if !path.is_absolute() || units.iter().any(|&ch| matches!(ch, 0 | 10 | 13 | 34)) {
        return Err(win::Error::msg(
            "Update install",
            "invalid absolute update path",
        ));
    }
    Ok(())
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn pending_error_path(executable: &Path) -> win::Result<PathBuf> {
    let bytes: Vec<u8> = executable
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect();
    Ok(std::env::temp_dir().join(format!(
        "HWIDChecker_update_error_{}.txt",
        hash::sha256_bytes(&bytes)?
    )))
}

/// Reads and consumes a replacement failure for the startup UI's Update Error message box.
pub fn take_pending_error() -> Result<Option<String>, String> {
    caught("Read update error", || pending_error().map_err(text))
}

fn pending_error() -> win::Result<Option<String>> {
    let executable =
        std::env::current_exe().map_err(|e| hash::io_error("Locate current executable", e))?;
    let path = pending_error_path(&executable)?;
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(hash::io_error("Open update error", error)),
    };
    let mut message = String::new();
    (&file)
        .take(4096)
        .read_to_string(&mut message)
        .map_err(|e| hash::io_error("Read update error", e))?;
    drop(file);
    // An empty reserved marker is not a failed replacement; leave it for its helper.
    if message.is_empty() {
        return Ok(None);
    }
    fs::remove_file(&path).map_err(|e| hash::io_error("Delete update error", e))?;
    Ok(Some(format!("Update failed: {}", message.trim_end())))
}

struct ErrorMarker {
    path: PathBuf,
    keep: bool,
}
impl Drop for ErrorMarker {
    fn drop(&mut self) {
        if !self.keep
            && let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            win::record(hash::io_error("Delete update error", error));
        }
    }
}

fn launch_installer(d: &mut Downloaded) -> win::Result<()> {
    let mut script = http::TempFile::create("bat")?;
    let new = appended(&d.executable, ".new");
    let lock = appended(&d.executable, ".update-lock");
    let error = pending_error_path(&d.executable)?;
    let cmd = process::system32("cmd.exe");
    let ping = process::system32("ping.exe");
    for path in [script.path(), &new, &lock, &error, &cmd, &ping] {
        checked_path(path)?;
    }
    // Reserve this installation's failure marker; never overwrite an unread old failure.
    drop(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&error)
            .map_err(|e| hash::io_error("Reserve update error", e))?,
    );
    let mut marker = ErrorMarker {
        path: error,
        keep: false,
    };
    let mut file = script.file()?;
    file.write_all(INSTALL_SCRIPT.as_bytes())
        .map_err(|e| hash::io_error("Write update batch", e))?;
    file.sync_all()
        .map_err(|e| hash::io_error("Flush update batch", e))?;
    // The batch is ASCII. UTF-16 environment values preserve Unicode paths without
    // a console/code-page dependency; delayed expansion is off, so ! remains literal.
    let mut argument = OsString::from("\"\"");
    argument.push(script.path());
    argument.push("\"\"");
    let child = Command::new(cmd)
        .args(["/d", "/s", "/c"])
        .raw_arg(argument)
        .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("HWID_U_SOURCE", &d.path)
        .env("HWID_U_EXE", &d.executable)
        .env("HWID_U_NEW", new)
        .env("HWID_U_LOCK", lock)
        .env("HWID_U_ERROR", &marker.path)
        .env("HWID_U_SCRIPT", script.path())
        .env("HWID_U_PING", ping)
        .spawn()
        .map_err(|e| hash::io_error("Start update batch", e))?;
    // Child owns its OS handles; dropping them leaves the detached helper running.
    drop(child);
    script.keep();
    marker.keep = true;
    d.response.temp.keep();
    Ok(())
}

// Each wait is a two-packet loopback ping (finite 1000 ms replies). timeout.exe
// needs console input and is unsuitable under CREATE_NO_WINDOW. Move retries also
// wait for the old executable's image lock to be released. The directory lock
// serializes two installers sharing the prescribed <exe>.new staging filename.
// C# parity: Services/AutoUpdateService.cs:253 (START's empty title and quoted exe).
const INSTALL_SCRIPT: &str = concat!(
    "@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\n",
    "set /a HWID_U_TRIES=0\r\n:lock\r\n",
    "mkdir \"%HWID_U_LOCK%\" >nul 2>&1\r\nif not errorlevel 1 goto stage\r\n",
    "set /a HWID_U_TRIES+=1\r\nif %HWID_U_TRIES% GEQ 30 goto lock_failed\r\n",
    "\"%HWID_U_PING%\" -n 2 -w 1000 127.0.0.1 >nul 2>&1\r\ngoto lock\r\n",
    ":stage\r\ncopy /B /Y \"%HWID_U_SOURCE%\" \"%HWID_U_NEW%\" >nul 2>&1\r\n",
    "if errorlevel 1 goto stage_failed\r\nset /a HWID_U_TRIES=0\r\n:replace\r\n",
    "move /Y \"%HWID_U_NEW%\" \"%HWID_U_EXE%\" >nul 2>&1\r\nif not errorlevel 1 goto installed\r\n",
    "set /a HWID_U_TRIES+=1\r\nif %HWID_U_TRIES% GEQ 30 goto replace_failed\r\n",
    "\"%HWID_U_PING%\" -n 2 -w 1000 127.0.0.1 >nul 2>&1\r\ngoto replace\r\n",
    ":installed\r\ndel /Q \"%HWID_U_ERROR%\" >nul 2>&1\r\ngoto restart\r\n",
    ":stage_failed\r\n>\"%HWID_U_ERROR%\" echo Update install failed: 0x00000000 could not copy the retained update to the staging file; the previous version was restarted.\r\ngoto failed\r\n",
    ":replace_failed\r\n>\"%HWID_U_ERROR%\" echo Update install failed: 0x00000000 could not replace the executable after 30 attempts; the previous version was restarted.\r\n",
    ":failed\r\ndel /Q \"%HWID_U_NEW%\" >nul 2>&1\r\n",
    ":restart\r\nrmdir \"%HWID_U_LOCK%\" >nul 2>&1\r\n",
    "start \"\" \"%HWID_U_EXE%\"\r\nif not errorlevel 1 goto cleanup\r\n",
    ">\"%HWID_U_ERROR%\" echo Update restart failed: 0x00000000 could not start the executable; start HWID Checker manually to read this error.\r\ngoto cleanup\r\n",
    ":lock_failed\r\n>\"%HWID_U_ERROR%\" echo Update install failed: 0x00000000 could not acquire the installation lock after 30 attempts; the previous version was restarted.\r\n",
    "start \"\" \"%HWID_U_EXE%\"\r\n",
    ":cleanup\r\ndel /Q \"%HWID_U_SOURCE%\" >nul 2>&1\r\n",
    "del /Q \"%HWID_U_SCRIPT%\" >nul 2>&1 & exit /b\r\n",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_paths_refuse_percent_and_preserve_other_shell_characters() {
        assert_eq!(
            checked_path(Path::new(r"C:\apps\100%\HWIDChecker.exe"))
                .expect_err("percent")
                .detail,
            "path contains %"
        );
        checked_path(Path::new(r"C:\apps\Größe & (test)!^\HWIDChecker.exe"))
            .expect("quoted UTF-16 paths");
        assert!(checked_path(Path::new("relative.exe")).is_err());
        assert!(checked_path(Path::new("C:\\apps\\newline\n.exe")).is_err());
    }

    #[test]
    #[ignore = "read-only production URL check; never installs or starts an executable"]
    fn wp13_real_url_check_decline() {
        assert!(
            std::env::var_os("HWID_UPDATE_URL").is_none(),
            "unset the debug URL for this production check"
        );
        let start = std::time::Instant::now();
        match check() {
            Ok(UpdateCheck::UpToDate) => println!("NoUpdateAvailable: hashes match"),
            Ok(UpdateCheck::Available(d)) => {
                hash::validate_x64_pe(d.response.temp.file().expect("retained file"))
                    .expect("production x64 PE");
                println!(
                    "Available: different SHA256; valid x64 PE; size={}, Content-Length={:?}; SHA256={}",
                    d.size,
                    d.content_length(),
                    d.sha256
                );
                let path = d.path.clone();
                println!("UserDeclined: retained file will be discarded; no install requested");
                drop(d);
                assert!(!path.exists(), "decline must remove its unique download");
                println!("Decline cleanup: passed");
            }
            Err(error) => panic!("Error checking for updates: {error}"),
        }
        println!("Elapsed: {} ms", start.elapsed().as_millis());
    }

    #[test]
    #[ignore = "read-only cmd.exe Unicode/metacharacter launch check; no install script is run"]
    fn wp13_hidden_cmd_paths() {
        let script = http::TempFile::create("bat").expect("temp batch");
        let output = http::TempFile::create("exe").expect("temp output reservation");
        let path = appended(output.path(), " Größe & (test)!^.txt");
        drop(output);
        let mut file = script.file().expect("batch file");
        file.write_all(b"@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\n>\"%HWID_U_OUTPUT%\" echo fabricated fixture\r\n").expect("write batch");
        let mut argument = OsString::from("\"\"");
        argument.push(script.path());
        argument.push("\"\"");
        let mut child = Command::new(process::system32("cmd.exe"))
            .args(["/d", "/s", "/c"])
            .raw_arg(argument)
            .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("HWID_U_OUTPUT", &path)
            .spawn()
            .expect("hidden cmd");
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("wait cmd") {
                assert!(status.success());
                break;
            }
            if start.elapsed() > std::time::Duration::from_secs(5) {
                child.kill().expect("kill timed-out read-only helper");
                panic!("read-only cmd timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Read the native output bytes: the value was passed through UTF-16 environment
        // expansion, so the Unicode path is used for file opening regardless of stdout CP.
        assert!(path.exists());
        fs::remove_file(path).expect("remove harmless output");
        println!("Hidden cmd /d /s /c: passed; no installer was executed");
    }

    #[test]
    #[ignore = "read-only local update matrix; destructive guard must remain disabled"]
    fn wp13_local_update_matrix() {
        use std::time::Duration;
        assert!(
            std::env::var_os("HWID_ALLOW_DESTRUCTIVE").is_none(),
            "destructive authorization must be absent"
        );
        fn fixture() -> Vec<u8> {
            let hex: String = include_str!("../tests/fixtures/wp-13/x64-pe.hex")
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            hex.as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16).expect("hex")
                })
                .collect()
        }
        fn reply(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
            let mut response =
                format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n").into_bytes();
            response.extend_from_slice(body);
            response
        }
        fn requests(server: std::thread::JoinHandle<Vec<String>>, count: usize) {
            let requests = server.join().expect("bounded server");
            assert_eq!(requests.len(), count);
            for request in requests {
                let lower = request.to_ascii_lowercase();
                assert!(lower.contains("user-agent: hwid-checker-autoupdater\r\n"));
                assert!(lower.contains("cache-control: no-cache, no-store, must-revalidate\r\n"));
                assert!(lower.contains("pragma: no-cache\r\n"));
            }
        }
        let bytes = fixture();
        let local = http::TempFile::create("exe").expect("local fixture");
        let mut file = local.file().expect("fixture file");
        file.write_all(&bytes).expect("fixture write");
        let (url, server) = http::tests::serve(vec![(
            Duration::ZERO,
            reply("200 OK", "Content-Length: 1024\r\n", &bytes),
        )]);
        assert!(matches!(
            check_url(&url, local.path(), &process::Cancel::new(), &mut |_, _| {})
                .expect("same hash"),
            UpdateCheck::UpToDate
        ));
        requests(server, 1);
        println!("Same hash: NoUpdateAvailable; one GET; exact updater headers");

        let mut changed = bytes.clone();
        changed[513] = 0x90;
        let (url, server) =
            http::tests::serve(vec![(Duration::ZERO, reply("200 OK", "", &changed))]);
        let UpdateCheck::Available(d) =
            check_url(&url, local.path(), &process::Cancel::new(), &mut |_, _| {})
                .expect("unknown length")
        else {
            panic!("different bytes");
        };
        assert_eq!(d.content_length(), None);
        assert_eq!(d.size, 1024);
        let path = d.path.clone();
        drop(d);
        assert!(!path.exists());
        requests(server, 1);
        println!("Different hash, unknown Content-Length, decline cleanup: passed");

        let (url, server) = http::tests::serve(vec![
            (
                Duration::ZERO,
                reply(
                    "302 Found",
                    "Location: /final\r\nContent-Length: 0\r\n",
                    b"",
                ),
            ),
            (
                Duration::ZERO,
                reply("200 OK", "Content-Length: 1024\r\n", &bytes),
            ),
        ]);
        let mut progress = Vec::new();
        let d = http::download(&url, &process::Cancel::new(), &mut |size, total| {
            progress.push((size, total))
        })
        .expect("redirect");
        assert_eq!(d.size, 1024);
        assert_eq!(progress.last(), Some(&(1024, Some(1024))));
        drop(d);
        requests(server, 2);
        println!("HTTP redirect and measured progress: passed");

        for response in [
            reply("404 Not Found", "Content-Length: 0\r\n", b""),
            reply("200 OK", "Content-Length: 2048\r\n", &bytes),
            reply("200 OK", "Content-Length: nonsense\r\n", &bytes),
            reply("200 OK", "Content-Length: 268435457\r\n", &bytes),
        ] {
            let (url, server) = http::tests::serve(vec![(Duration::ZERO, response)]);
            assert!(http::download(&url, &process::Cancel::new(), &mut |_, _| {}).is_err());
            requests(server, 1);
        }
        println!("HTTP failure, short body, invalid or oversized Content-Length: refused");

        let (url, server) = http::tests::serve(vec![(
            Duration::ZERO,
            reply("404 Not Found", "Content-Length: 0\r\n", b""),
        )]);
        let error = check_url(&url, local.path(), &process::Cancel::new(), &mut |_, _| {})
            .err()
            .expect("HTTP failure");
        // C# shows `Error checking for updates: Failed to get GitHub file SHA256 ...: {inner}`.
        assert_eq!(
            error,
            "Failed to get GitHub file SHA256 for HWIDChecker.exe: HTTP GET failed: 0x00000000 HTTP status 404"
        );
        requests(server, 1);
        println!("Check error text keeps the C# prefix: passed");

        // Use the actual library-test path so install's current-exe identity check is real.
        let current = std::env::current_exe().expect("test exe");
        for (body, expected) in [
            (&bytes[..], "[DRY RUN] install update and restart"),
            (&b"not a PE"[..], "not a valid x64 PE"),
        ] {
            let (url, server) = http::tests::serve(vec![(
                Duration::ZERO,
                reply(
                    "200 OK",
                    &format!("Content-Length: {}\r\n", body.len()),
                    body,
                ),
            )]);
            let UpdateCheck::Available(d) =
                check_url(&url, &current, &process::Cancel::new(), &mut |_, _| {})
                    .expect("different update")
            else {
                panic!("different fixture");
            };
            let path = d.path.clone();
            let error = install_and_restart(d).expect_err("must not install");
            assert!(error.contains(expected), "{error}");
            assert!(!path.exists());
            requests(server, 1);
        }
        println!(
            "Valid install: dry-run refusal; invalid PE: refused before install; no second GET"
        );

        let path = pending_error_path(&current).expect("pending path");
        let marker = ErrorMarker { path, keep: false };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker.path)
            .expect("private test marker");
        file.write_all(b"Update install failed: 0x00000000 fabricated replacement failure.\r\n")
            .expect("marker write");
        drop(file);
        assert_eq!(
            take_pending_error().expect("pending error").as_deref(),
            Some(
                "Update failed: Update install failed: 0x00000000 fabricated replacement failure."
            )
        );
        assert!(!marker.path.exists());
        println!("Next-start failure read and consume: passed (UI wiring remains separate)");
    }
}
