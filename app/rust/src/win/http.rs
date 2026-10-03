//! Owned by WP-13: bounded WinHTTP downloads.

use super::{Error, Result, hash, process::Cancel, record, wide};
use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    Win32::Networking::WinHttp::*,
    core::{PCWSTR, w},
};

// C# parity: Services/AutoUpdateService.cs:28 (HttpClient's default 100 s budget).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(100);
// Far above any HWIDChecker.exe build; stops a broken or hostile reply filling %TEMP%.
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

fn too_large() -> Error {
    Error::msg("Download size", format!("exceeds {MAX_DOWNLOAD} bytes"))
}

struct Internet(*mut c_void);

impl Internet {
    fn new(handle: *mut c_void, op: &'static str) -> Result<Self> {
        if handle.is_null() {
            Err(Error::last(op))
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for Internet {
    fn drop(&mut self) {
        // SAFETY: This guard owns a valid WinHTTP handle, not a kernel HANDLE.
        if let Err(error) = unsafe { WinHttpCloseHandle(self.0) } {
            record(Error::from_win("WinHttpCloseHandle", error));
        }
    }
}

/// An exclusively created temporary file, deleted on drop unless handed to the installer.
pub struct TempFile {
    path: PathBuf,
    file: Option<File>,
    keep: bool,
}

impl TempFile {
    /// Creates an unpredictable updater file and denies other writers or deleters.
    pub fn create(extension: &str) -> Result<Self> {
        if !matches!(extension, "exe" | "bat") {
            return Err(Error::msg(
                "Create update temp file",
                "unsupported extension",
            ));
        }
        for _ in 0..16 {
            let path = std::env::temp_dir().join(format!(
                "HWIDChecker_update_{}.{extension}",
                hash::random_name()?
            ));
            // FILE_SHARE_READ keeps the retained bytes immutable until the helper handoff.
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(1)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        keep: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(hash::io_error("Create update temp file", error)),
            }
        }
        Err(Error::msg(
            "Create update temp file",
            "unique-name retries exhausted",
        ))
    }

    /// Borrows the absolute path without transferring cleanup ownership.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Borrows the held file so hashing and validation inspect the downloaded bytes.
    pub fn file(&self) -> Result<&File> {
        self.file
            .as_ref()
            .ok_or_else(|| Error::msg("Update temp file", "file is closed"))
    }

    /// Hands cleanup ownership to a successfully launched replacement helper.
    pub fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Close first: the retained handle deliberately denies deletion while held.
        drop(self.file.take());
        if !self.keep
            && let Err(error) = fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            record(hash::io_error("Delete update temp file", error));
        }
    }
}

/// The retained response, including its independently measured and advertised sizes.
pub struct Download {
    pub temp: TempFile,
    pub size: u64,
    pub content_length: Option<u64>,
}

fn component(url: &[u16], ptr: *const u16, length: u32) -> Result<Vec<u16>> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let start = ptr as usize;
    let base = url.as_ptr() as usize;
    let bytes = length as usize * 2;
    if start < base
        || start - base > url.len() * 2
        || bytes > url.len() * 2 - (start - base)
        || !(start - base).is_multiple_of(2)
    {
        return Err(Error::msg(
            "WinHttpCrackUrl",
            "invalid URL component bounds",
        ));
    }
    let offset = (start - base) / 2;
    Ok(url[offset..offset + length as usize].to_vec())
}

fn timeouts(handle: &Internet, start: Instant, budget: Duration, cancel: &Cancel) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::msg("WinHTTP download", "cancelled"));
    }
    let remaining = budget
        .checked_sub(start.elapsed())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| {
            Error::msg(
                "WinHTTP download",
                format!("timed out after {}ms", budget.as_millis()),
            )
        })?;
    let ms = remaining.as_millis().clamp(1, 30_000) as i32;
    // SAFETY: The live synchronous handle accepts finite, positive millisecond timeouts.
    unsafe { WinHttpSetTimeouts(handle.0, ms.min(10_000), ms.min(10_000), ms, ms) }
        .map_err(|e| Error::from_win("WinHttpSetTimeouts", e))
}

fn content_length(request: &Internet) -> Result<Option<u64>> {
    let mut buffer = [0_u16; 64];
    let mut length = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: The header buffer is writable for length bytes; no header name/index is needed.
    match unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_CONTENT_LENGTH,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr().cast()),
            &mut length,
            std::ptr::null_mut(),
        )
    } {
        Ok(()) => {
            let text = wide::from_wide(&buffer);
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Error::msg("Content-Length", "invalid decimal length"));
            }
            text.parse::<u64>()
                .map(Some)
                .map_err(|e| Error::msg("Content-Length", e.to_string()))
        }
        Err(error) if error.code().0 as u32 & 0xFFFF == ERROR_WINHTTP_HEADER_NOT_FOUND => Ok(None),
        Err(error) => Err(Error::from_win(
            "WinHttpQueryHeaders(Content-Length)",
            error,
        )),
    }
}

/// Downloads with the OS proxy/certificate policy, bounded waits, cancellation, and progress.
pub fn download(
    url: &str,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Download> {
    download_with_timeout(url, cancel, progress, DOWNLOAD_TIMEOUT)
}

/// Uses the same bounded transport with a shorter budget for read-only failure checks.
pub(crate) fn download_with_timeout(
    url: &str,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
    budget: Duration,
) -> Result<Download> {
    if url.contains('\0') || url.encode_utf16().count() > 32767 {
        return Err(Error::msg("WinHttpCrackUrl", "invalid URL length or NUL"));
    }
    let encoded = wide::to_wide(url);
    let mut parts = URL_COMPONENTS {
        dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
        dwHostNameLength: u32::MAX,
        dwUrlPathLength: u32::MAX,
        dwExtraInfoLength: u32::MAX,
        dwUserNameLength: u32::MAX,
        dwPasswordLength: u32::MAX,
        ..Default::default()
    };
    // SAFETY: The URL slice and writable components live through all component copies.
    unsafe { WinHttpCrackUrl(&encoded[..encoded.len() - 1], 0, &mut parts) }
        .map_err(|e| Error::from_win("WinHttpCrackUrl", e))?;
    if !matches!(
        parts.nScheme,
        WINHTTP_INTERNET_SCHEME_HTTP | WINHTTP_INTERNET_SCHEME_HTTPS
    ) || parts.dwHostNameLength == 0
        || parts.dwUserNameLength != 0
        || parts.dwPasswordLength != 0
    {
        return Err(Error::msg(
            "WinHttpCrackUrl",
            "an HTTP(S) URL without credentials is required",
        ));
    }
    let mut host = component(&encoded, parts.lpszHostName.0, parts.dwHostNameLength)?;
    host.push(0);
    let mut object = component(&encoded, parts.lpszUrlPath.0, parts.dwUrlPathLength)?;
    if object.is_empty() {
        object.push(b'/' as u16);
    }
    object.extend(component(
        &encoded,
        parts.lpszExtraInfo.0,
        parts.dwExtraInfoLength,
    )?);
    if let Some(index) = object.iter().position(|&ch| ch == b'#' as u16) {
        object.truncate(index);
    }
    object.push(0);
    let start = Instant::now();
    // C# parity: Services/AutoUpdateService.cs:29 (the exact User-Agent).
    // SAFETY: Static UTF-16 strings and null proxy pointers are valid; the guard closes it.
    let session = Internet::new(
        unsafe {
            WinHttpOpen(
                w!("HWID-Checker-AutoUpdater"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        },
        "WinHttpOpen",
    )?;
    timeouts(&session, start, budget, cancel)?;
    // SAFETY: The host string is NUL-terminated and session remains alive.
    let connection = Internet::new(
        unsafe { WinHttpConnect(session.0, PCWSTR(host.as_ptr()), parts.nPort, 0) },
        "WinHttpConnect",
    )?;
    let flags = if parts.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    // SAFETY: The connection stays live; GET and object are terminated, optional pointers null.
    let request = Internet::new(
        unsafe {
            WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                PCWSTR(object.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                flags,
            )
        },
        "WinHttpOpenRequest",
    )?;
    // Follow redirects, retaining the default refusal of HTTPS-to-HTTP downgrade.
    let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP.to_ne_bytes();
    // SAFETY: The option takes a DWORD and the byte slice has precisely that size.
    unsafe {
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&policy),
        )
    }
    .map_err(|e| Error::from_win("WinHttpSetOption(redirects)", e))?;
    // C# parity: Services/AutoUpdateService.cs:32-38 (cache-busting request headers).
    let headers =
        wide::to_wide("Cache-Control: no-cache, no-store, must-revalidate\r\nPragma: no-cache\r\n");
    timeouts(&request, start, budget, cancel)?;
    // SAFETY: The live request gets a correctly sized UTF-16 header slice and no request body.
    unsafe {
        WinHttpSendRequest(
            request.0,
            Some(&headers[..headers.len() - 1]),
            None,
            0,
            0,
            0,
        )
    }
    .map_err(|e| Error::from_win("WinHttpSendRequest", e))?;
    timeouts(&request, start, budget, cancel)?;
    // SAFETY: The synchronous request was sent; the reserved argument is null.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|e| Error::from_win("WinHttpReceiveResponse", e))?;
    let mut code = 0_u32;
    let mut length = std::mem::size_of_val(&code) as u32;
    // SAFETY: The numeric status buffer is a writable DWORD of the advertised size.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut code as *mut u32).cast()),
            &mut length,
            std::ptr::null_mut(),
        )
    }
    .map_err(|e| Error::from_win("WinHttpQueryHeaders(status)", e))?;
    // C# parity: Services/AutoUpdateService.cs:97,200 (EnsureSuccessStatusCode).
    if !(200..300).contains(&code) {
        return Err(Error::msg("HTTP GET", format!("HTTP status {code}")));
    }
    let content_length = content_length(&request)?;
    if content_length.is_some_and(|n| n > MAX_DOWNLOAD) {
        return Err(too_large());
    }
    let temp = TempFile::create("exe")?;
    let mut file = temp.file()?;
    let mut size = 0_u64;
    let mut buffer = [0_u8; 8192];
    progress(0, content_length);
    loop {
        timeouts(&request, start, budget, cancel)?;
        let mut count = 0_u32;
        // SAFETY: buffer is writable for 8192 bytes, count is writable, and request is live.
        unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut count,
            )
        }
        .map_err(|e| Error::from_win("WinHttpReadData", e))?;
        if count == 0 {
            break;
        }
        if size + count as u64 > MAX_DOWNLOAD {
            return Err(too_large());
        }
        file.write_all(&buffer[..count as usize])
            .map_err(|e| hash::io_error("Write update file", e))?;
        size += count as u64;
        progress(size, content_length);
    }
    timeouts(&request, start, budget, cancel)?;
    file.sync_all()
        .map_err(|e| hash::io_error("Flush update file", e))?;
    if let Some(expected) = content_length
        && size != expected
    {
        return Err(Error::msg(
            "Download size",
            format!("Content-Length is {expected}, downloaded {size}"),
        ));
    }
    Ok(Download {
        temp,
        size,
        content_length,
    })
}

#[cfg(test)]
/// Bounded loopback helpers for read-only update and transport verification.
pub(crate) mod tests {
    use super::*;
    use std::{
        io::Read,
        net::TcpListener,
        thread::{self, JoinHandle},
    };

    /// Serves a finite list of fabricated HTTP replies and records request headers.
    pub(crate) fn serve(replies: Vec<(Duration, Vec<u8>)>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
        let address = listener.local_addr().expect("loopback address");
        listener.set_nonblocking(true).expect("bounded accept");
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (delay, response) in replies {
                let start = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            assert!(
                                start.elapsed() < Duration::from_secs(5),
                                "loopback accept timed out"
                            );
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("loopback accept: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .expect("write timeout");
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut buffer = [0_u8; 1024];
                    let count = stream.read(&mut buffer).expect("read request");
                    assert!(
                        count > 0 && request.len() < 32768,
                        "invalid loopback request"
                    );
                    request.extend_from_slice(&buffer[..count]);
                }
                requests.push(String::from_utf8(request).expect("HTTP request ASCII"));
                thread::sleep(delay);
                // A timeout check intentionally closes before its late reply is written.
                if let Err(error) = stream.write_all(&response) {
                    assert!(!delay.is_zero(), "loopback response: {error}");
                }
            }
            requests
        });
        (format!("http://{address}/payload?cb=1234567890"), handle)
    }

    #[test]
    fn retained_file_blocks_writes_and_deletes_then_cleans_up() {
        let temp = TempFile::create("exe").expect("unique file");
        let path = temp.path().to_owned();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        assert!(File::open(&path).is_ok());
        drop(temp);
        assert!(!path.exists());
    }

    #[test]
    #[ignore = "read-only real WinHTTP timeout/cancellation/offline checks"]
    fn wp13_transport_failures() {
        let (url, server) = serve(vec![(
            Duration::from_millis(600),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )]);
        let start = Instant::now();
        let result = download_with_timeout(
            &url,
            &Cancel::new(),
            &mut |_, _| {},
            Duration::from_millis(200),
        );
        let error = result.err().expect("stalled headers must fail");
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(error.to_string().contains("failed: 0x"));
        server.join().expect("bounded loopback server");
        let listener = TcpListener::bind("127.0.0.1:0").expect("offline port");
        let url = format!("http://{}/payload", listener.local_addr().expect("address"));
        drop(listener);
        assert!(
            download_with_timeout(
                &url,
                &Cancel::new(),
                &mut |_, _| {},
                Duration::from_millis(200)
            )
            .is_err()
        );
        let cancel = Cancel::new();
        cancel.cancel();
        let error = download(&url, &cancel, &mut |_, _| {})
            .err()
            .expect("cancelled");
        assert_eq!(error.detail, "cancelled");
        println!("WinHTTP stalled headers, closed loopback port, and cancellation: passed");
    }
}
