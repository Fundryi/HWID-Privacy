//! Owned by WP-13: download once, validate, then install the retained bytes.

use crate::{
    clean, report,
    win::{self, hash, http, process},
};
use std::{
    fs,
    os::windows::ffi::OsStrExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// The unchanged channel consumed by deployed C# and Rust copies.
pub const UPDATE_URL: &str = "https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe";
/// A checked download whose private owner preserves its bytes and cleans up on decline.
pub struct Downloaded {
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

/// Downloads once and compares SHA-256, retaining the size-checked bytes when available.
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
    let sha256 = hash::sha256_bytes(&response.bytes).map_err(remote)?;
    // A local hash failure is returned instead of silently treating it as a mismatch.
    let local = hash::sha256(executable).map_err(text)?;
    // C# parity: Services/AutoUpdateService.cs:66-77 (hash equality, never version ordering).
    if report::eq_ignore_case(&local, &sha256) {
        return Ok(UpdateCheck::UpToDate);
    }
    Ok(UpdateCheck::Available(Downloaded {
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
    if current != d.executable || d.size != d.response.size {
        return Err(win::Error::msg(
            "Update install",
            "retained download metadata changed",
        ));
    }
    checked_path(&current)?;
    let size = d.response.bytes.len() as u64;
    if size > http::MAX_DOWNLOAD
        || size != d.size
        || d.response
            .content_length
            .is_some_and(|expected| expected != size)
    {
        return Err(win::Error::msg(
            "Download size",
            "retained size differs from Content-Length or checked size",
        ));
    }
    if !report::eq_ignore_case(&hash::sha256_bytes(&d.response.bytes)?, &d.sha256) {
        return Err(win::Error::msg(
            "Update SHA256",
            "retained download hash changed",
        ));
    }
    const WHAT: &str = "install update and restart";
    match clean::destructive(WHAT, || {
        // The locked snapshot lets the existing parser inspect exactly the in-memory
        // bytes without changing hash.rs or introducing a second PE implementation.
        http::validate_x64_pe(&d.response.bytes)?;
        swap_and_restart(&current, &d.response.bytes, |path| {
            let child = Command::new(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| hash::io_error("Update restart", e))?;
            drop(child); // The restarted application owns its lifetime.
            Ok(())
        })
    }) {
        Some(result) => result?,
        // The guard's contract: the caller shows `[DRY RUN] {what}` (stderr is lost in the GUI).
        None => {
            return Err(win::Error::msg(
                "Update install",
                format!("[DRY RUN] {WHAT}"),
            ));
        }
    }
    // C# parity: Services/AutoUpdateService.cs:272-273 (exit only after the new executable starts).
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

/// Kept for the startup UI; in-process failures return to the current update dialog.
pub fn take_pending_error() -> Result<Option<String>, String> {
    Ok(None)
}

fn swap_and_restart(
    executable: &Path,
    bytes: &[u8],
    restart: impl FnOnce(&Path) -> win::Result<()>,
) -> win::Result<()> {
    let mut swap = http::ExecutableSwap::begin(executable)?;
    let result = swap.write(bytes).and_then(|()| restart(executable));
    if let Err(error) = result {
        if let Err(rollback) = swap.rollback() {
            let detail = format!("{error}; rollback failed: {rollback}");
            win::record(rollback);
            return Err(win::Error::msg("Update install", detail));
        }
        return Err(error);
    }
    swap.commit();
    Ok(())
}

/// Removes previous images at startup, recording every inspection or deletion failure.
pub fn cleanup_old_executables() {
    let result = caught("Update cleanup", || {
        let executable = std::env::current_exe()
            .map_err(|e| text(hash::io_error("Locate current executable", e)))?;
        cleanup_old_siblings(&executable, |path| {
            let what = format!("delete old update executable {}", path.display());
            clean::destructive(&what, || http::delete_old_executable(path)).unwrap_or_else(|| {
                Err(win::Error::msg(
                    "Update cleanup",
                    format!("[DRY RUN] {what}"),
                ))
            })
        })
        .map_err(text)
    });
    if let Err(error) = result {
        win::record(win::Error::msg("Update cleanup", error));
    }
}

fn cleanup_old_siblings(
    executable: &Path,
    mut delete: impl FnMut(&Path) -> win::Result<()>,
) -> win::Result<()> {
    let parent = executable
        .parent()
        .ok_or_else(|| win::Error::msg("Update cleanup", "missing executable parent"))?;
    let mut prefix = executable
        .file_name()
        .ok_or_else(|| win::Error::msg("Update cleanup", "missing executable name"))?
        .encode_wide()
        .collect::<Vec<_>>();
    prefix.extend(".old-".encode_utf16());
    let siblings = fs::read_dir(parent).map_err(|e| hash::io_error("Read update directory", e))?;
    for sibling in siblings {
        match sibling {
            Ok(entry)
                if entry
                    .file_name()
                    .encode_wide()
                    .collect::<Vec<_>>()
                    .starts_with(&prefix) =>
            {
                if let Err(error) = delete(&entry.path()) {
                    win::record(error);
                }
            }
            Ok(_) => {}
            Err(error) => win::record(hash::io_error("Read old update entry", error)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "HWIDChecker_swap_test_{}",
                hash::random_name().expect("test random name")
            ));
            fs::create_dir(&path).expect("exclusively create scratch directory");
            Self(path)
        }

        fn executable(&self) -> PathBuf {
            self.0.join("HWIDChecker.exe")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove only owned scratch directory");
        }
    }

    #[test]
    fn swap_renames_and_writes_only_a_temp_copy() {
        let scratch = Scratch::new();
        let executable = scratch.executable();
        fs::write(&executable, b"old image").expect("fixture image");
        // Exercise the same transaction as production, solely on a fabricated temp image.
        swap_and_restart(&executable, b"checked new bytes", |path| {
            assert!(path.is_absolute());
            assert_eq!(
                fs::read(path).expect("closed new image"),
                b"checked new bytes"
            );
            let old = fs::read_dir(&scratch.0)
                .expect("scratch siblings")
                .map(|entry| entry.expect("entry").path())
                .find(|path| {
                    path.file_name()
                        .expect("name")
                        .to_string_lossy()
                        .starts_with("HWIDChecker.exe.old-")
                })
                .expect("old image sibling");
            assert_eq!(fs::read(old).expect("old bytes"), b"old image");
            Ok(()) // Never launch either fixture.
        })
        .expect("fixture swap");
        assert_eq!(
            fs::read(executable).expect("installed bytes"),
            b"checked new bytes"
        );
    }

    #[test]
    fn create_new_refuses_existing_file_and_directory_without_deleting_them() {
        for directory in [false, true] {
            let scratch = Scratch::new();
            let executable = scratch.executable();
            fs::write(&executable, b"old image").expect("fixture image");
            let mut swap = http::ExecutableSwap::begin(&executable).expect("rename fixture");
            if directory {
                fs::create_dir(&executable).expect("interfering directory");
                fs::write(executable.join("unrelated.txt"), b"keep me")
                    .expect("directory contents");
            } else {
                fs::write(&executable, b"unrelated file").expect("interfering file");
            }
            assert!(swap.write(b"new bytes").is_err());
            assert!(
                swap.rollback().is_err(),
                "rollback must not replace the unowned path"
            );
            if directory {
                assert_eq!(
                    fs::read(executable.join("unrelated.txt")).expect("preserved contents"),
                    b"keep me"
                );
                fs::remove_file(executable.join("unrelated.txt")).expect("fixture cleanup");
                fs::remove_dir(&executable).expect("fixture directory cleanup");
            } else {
                assert_eq!(
                    fs::read(&executable).expect("preserved file"),
                    b"unrelated file"
                );
                fs::remove_file(&executable).expect("fixture file cleanup");
            }
            swap.rollback()
                .expect("restore after fixture interference removed");
            assert_eq!(fs::read(executable).expect("restored bytes"), b"old image");
        }
    }

    #[test]
    fn restart_failure_and_unfinished_swap_restore_the_old_temp_file() {
        let scratch = Scratch::new();
        let executable = scratch.executable();
        fs::write(&executable, b"old image").expect("fixture image");
        let error = swap_and_restart(&executable, b"new bytes", |_| {
            Err(win::Error::msg(
                "Update restart",
                "fabricated start failure",
            ))
        })
        .expect_err("restart failure");
        assert_eq!(error.op, "Update restart");
        assert_eq!(fs::read(&executable).expect("restored bytes"), b"old image");
        let mut swap = http::ExecutableSwap::begin(&executable).expect("rename fixture");
        swap.write(b"unfinished bytes").expect("fixture write");
        drop(swap);
        assert_eq!(
            fs::read(executable).expect("RAII restored bytes"),
            b"old image"
        );
        assert_eq!(
            fs::read_dir(&scratch.0).expect("scratch siblings").count(),
            1
        );
    }

    #[test]
    fn startup_cleanup_skips_directories_and_unrelated_files() {
        let scratch = Scratch::new();
        let executable = scratch.executable();
        fs::write(&executable, b"current image").expect("fixture current");
        let old = scratch.0.join("HWIDChecker.exe.old-fabricated");
        fs::write(&old, b"old image").expect("fixture old");
        let directory = scratch.0.join("HWIDChecker.exe.old-directory");
        fs::create_dir(&directory).expect("fixture directory");
        fs::write(directory.join("keep.txt"), b"keep me").expect("fixture contents");
        let unrelated = scratch.0.join("Other.exe.old-fabricated");
        fs::write(&unrelated, b"unrelated image").expect("fixture unrelated");
        cleanup_old_siblings(&executable, http::delete_old_executable).expect("fixture cleanup");
        assert!(!old.exists());
        assert_eq!(
            fs::read(directory.join("keep.txt")).expect("preserved contents"),
            b"keep me"
        );
        assert_eq!(
            fs::read(unrelated).expect("preserved unrelated"),
            b"unrelated image"
        );
        assert_eq!(
            fs::read(executable).expect("preserved current"),
            b"current image"
        );
    }

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
                http::validate_x64_pe(&d.response.bytes).expect("production x64 PE");
                println!(
                    "Available: different SHA256; valid x64 PE; size={}, Content-Length={:?}; SHA256={}",
                    d.size,
                    d.content_length(),
                    d.sha256
                );
                println!("UserDeclined: retained bytes discarded; no install requested");
                drop(d);
            }
            Err(error) => panic!("Error checking for updates: {error}"),
        }
        println!("Elapsed: {} ms", start.elapsed().as_millis());
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
        assert_eq!(d.response.bytes, changed);
        drop(d);
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
            (&b"not a PE"[..], "[DRY RUN] install update and restart"),
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
            assert_eq!(
                http::validate_x64_pe(&d.response.bytes).is_ok(),
                body == bytes
            );
            let error = install_and_restart(d).expect_err("must not install");
            assert!(error.contains(expected), "{error}");
            requests(server, 1);
        }
        println!("PE snapshot validation: passed; install: dry-run refusal; no second GET");

        assert_eq!(take_pending_error().expect("no next-start path"), None);
    }
}
