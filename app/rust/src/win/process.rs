//! Bounded, cancellable child processes; executable paths must be absolute.

use std::{
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::{OsStrExt, OsStringExt},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            ERROR_BROKEN_PIPE, ERROR_INSUFFICIENT_BUFFER, GENERIC_READ, HANDLE,
            HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation, WAIT_FAILED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            ReadFile,
        },
        System::{
            JobObjects::{
                CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
                QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
            },
            Pipes::CreatePipe,
            SystemInformation::GetSystemDirectoryW,
            Threading::{
                CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
                EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
                PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
                UpdateProcThreadAttribute, WaitForSingleObject,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

use super::{Error, OwnedHandle, wide::to_wide};

// Cargo's frozen feature list omits Win32_Globalization. Keep the one required
// Kernel32 declaration here instead of changing the orchestrator-owned manifest.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        bytes: *const u8,
        byte_count: i32,
        wide: *mut u16,
        wide_count: i32,
    ) -> i32;
}
const CP_OEMCP: u32 = 1;
const POLL: Duration = Duration::from_millis(10);
const CLEANUP: Duration = Duration::from_secs(1);

#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Cancel {
    /// Creates an uncancelled token shared by clones.
    pub fn new() -> Self {
        Self::default()
    }
    /// Signals cancellation to every clone of this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    /// Reports whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Runs an absolute executable with bounded lifetime and C# parity error texts.
pub fn run(
    exe: &Path,
    args: &[&str],
    timeout: Duration,
    cancel: &Cancel,
) -> std::result::Result<Output, String> {
    if cancel.is_cancelled() {
        return Err(cancel_text(exe, args));
    }
    let application: Vec<u16> = exe.as_os_str().encode_wide().collect();
    if !exe.is_absolute() || application.contains(&0) || args.iter().any(|arg| arg.contains('\0')) {
        eprintln!(
            "{}",
            Error::msg(
                "CreateProcessW",
                "absolute path and NUL-free arguments required"
            )
        );
        return Err(start_text(exe));
    }
    let mut command = quote(&application);
    for arg in args {
        command.push(b' ' as u16);
        command.extend(quote(&arg.encode_utf16().collect::<Vec<_>>()));
    }
    if command.len() >= 32767 {
        eprintln!(
            "{}",
            Error::msg("CreateProcessW", "command line exceeds 32767 UTF-16 units")
        );
        return Err(start_text(exe));
    }
    command.push(0);
    let mut application = application;
    application.push(0);

    // SAFETY: No security attributes or name; the returned job has a unique owner.
    let job = own(
        unsafe { CreateJobObjectW(None, PCWSTR::null()) },
        "CreateJobObjectW",
    )?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: The live job and complete, correctly sized SDK structure are borrowed.
    unsafe {
        SetInformationJobObject(
            job.as_raw(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|e| Error::from_win("SetInformationJobObject", e).to_string())?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    let security = inheritable();
    let nul = to_wide("NUL");
    // SAFETY: NUL and security are live; the returned handle is uniquely owned.
    let stdin = own(
        unsafe {
            CreateFileW(
                PCWSTR(nul.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                Some(&security),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        },
        "CreateFileW(NUL)",
    )?;

    // Attribute values must outlive the list, including its destructor.
    let jobs = [job.as_raw()];
    let inherited = [stdin.as_raw(), stdout_write.as_raw(), stderr_write.as_raw()];
    let attributes = Attributes::new()?;
    // SAFETY: Both arrays remain live until attributes is deleted below; sizes match.
    unsafe {
        UpdateProcThreadAttribute(
            attributes.raw(),
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            Some(jobs.as_ptr().cast()),
            size_of_val(&jobs),
            None,
            None,
        )
    }
    .map_err(|e| Error::from_win("UpdateProcThreadAttribute(JOB_LIST)", e).to_string())?;
    // Restrict inheritance so parallel runners cannot keep each other's pipes open.
    // SAFETY: Every listed handle is inheritable, non-pseudo and alive through creation.
    unsafe {
        UpdateProcThreadAttribute(
            attributes.raw(),
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(inherited.as_ptr().cast()),
            size_of_val(&inherited),
            None,
            None,
        )
    }
    .map_err(|e| Error::from_win("UpdateProcThreadAttribute(HANDLE_LIST)", e).to_string())?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = stdin.as_raw();
    startup.StartupInfo.hStdOutput = stdout_write.as_raw();
    startup.StartupInfo.hStdError = stderr_write.as_raw();
    startup.lpAttributeList = attributes.raw();
    let mut process = PROCESS_INFORMATION::default();
    let started = Instant::now();
    // C# parity: EventLogCleaningService.cs:315-318
    // SAFETY: Absolute application, writable NUL-terminated command, live extended
    // startup and attributes; output storage is complete. Assignment to the job is
    // atomic with process creation, so no child can run outside its lifetime bound.
    let created = unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR::null(),
            &startup.StartupInfo,
            &mut process,
        )
    };
    drop(attributes);
    if let Err(error) = created {
        eprintln!("{}", Error::from_win("CreateProcessW", error));
        return Err(start_text(exe));
    }
    // SAFETY: Successful CreateProcessW transferred these two unique kernel handles.
    let process_handle =
        unsafe { OwnedHandle::from_raw(process.hProcess) }.map_err(|e| e.to_string())?;
    // SAFETY: hThread is the other unique handle returned by CreateProcessW.
    let thread_handle =
        unsafe { OwnedHandle::from_raw(process.hThread) }.map_err(|e| e.to_string())?;
    drop(thread_handle);
    drop(stdin);
    drop(stdout_write);
    drop(stderr_write);

    // C# parity: EventLogCleaningService.cs:329-330
    let stdout = Reader::start(stdout_read, "stdout")?;
    let stderr = match Reader::start(stderr_read, "stderr") {
        Ok(reader) => reader,
        Err(error) => {
            drop(job);
            if let Err(cleanup) = stdout.finish(Instant::now()) {
                eprintln!("{cleanup}");
            }
            return Err(error);
        }
    };
    let result = wait(
        &process_handle,
        &stdout,
        &stderr,
        started,
        timeout,
        cancel,
        exe,
        args,
    );
    let cleanup_started = Instant::now();
    if result.is_err()
        && let Err(error) = terminate(&job, &process_handle, cleanup_started)
    {
        eprintln!("{error}");
    }
    // Close before joining: descendants may still own pipe writers after root exit.
    drop(job);
    let stdout = stdout.finish(cleanup_started);
    let stderr = stderr.finish(cleanup_started);
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            for failure in [stdout, stderr].into_iter().filter_map(Result::err) {
                eprintln!("{failure}");
            }
            return Err(error);
        }
    };
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(error) => {
            if let Err(other) = stderr {
                eprintln!("{other}");
            }
            return Err(error);
        }
    };
    let mut stderr = stderr?;
    // C# parity: EventLogCleaningService.cs:372-374
    if code != 0 && stderr.trim().is_empty() {
        stderr = format!("Process exited with code {code}.");
    }
    Ok(Output {
        code,
        stdout,
        stderr,
    })
}
/// Resolves a System32 child executable without searching PATH or the executable folder.
pub fn system32(name: &str) -> PathBuf {
    match system_path(name) {
        Ok(path) => path,
        Err(error) => {
            // The frozen infallible API must record failures and fail closed: run
            // rejects this empty path, rather than searching PATH or a fallback directory.
            eprintln!("{error}");
            PathBuf::new()
        }
    }
}

fn system_path(name: &str) -> super::Result<PathBuf> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':', '\0']) {
        return Err(Error::msg(
            "system32",
            "a single executable filename is required",
        ));
    }
    let mut buffer = vec![0_u16; 260];
    loop {
        // SAFETY: The slice is writable for its declared capacity.
        let count = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
        if count == 0 {
            return Err(Error::last("GetSystemDirectoryW"));
        }
        if count >= buffer.len() {
            if count > 32768 {
                return Err(Error::msg(
                    "GetSystemDirectoryW",
                    "system directory is too long",
                ));
            }
            buffer.resize(count + 1, 0);
            continue;
        }
        let path = PathBuf::from(OsString::from_wide(&buffer[..count]));
        if !path.is_absolute() {
            return Err(Error::msg(
                "GetSystemDirectoryW",
                "system directory is not absolute",
            ));
        }
        return Ok(path.join(name));
    }
}

fn start_text(exe: &Path) -> String {
    // C# parity: EventLogCleaningService.cs:326
    format!("Failed to start process: {}", exe.display())
}

fn cancel_text(exe: &Path, args: &[&str]) -> String {
    // C# parity: EventLogCleaningService.cs:351
    format!("Process canceled: {} {}", exe.display(), args.join(" "))
}

fn quote(value: &[u16]) -> Vec<u16> {
    if !value.is_empty()
        && !value
            .iter()
            .any(|unit| matches!(unit, 9 | 10 | 13 | 32 | 34))
    {
        return value.to_vec();
    }
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for &unit in value {
        if unit == b'\\' as u16 {
            slashes += 1;
        } else {
            result.extend(std::iter::repeat_n(
                b'\\' as u16,
                if unit == b'"' as u16 {
                    slashes * 2 + 1
                } else {
                    slashes
                },
            ));
            result.push(unit);
            slashes = 0;
        }
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    result
}

fn own(handle: windows::core::Result<HANDLE>, op: &'static str) -> Result<OwnedHandle, String> {
    let handle = handle.map_err(|e| Error::from_win(op, e).to_string())?;
    // SAFETY: Callers pass freshly acquired, uniquely owned non-pseudo kernel handles.
    unsafe { OwnedHandle::from_raw(handle) }.map_err(|e| e.to_string())
}

fn inheritable() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    }
}

fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    let security = inheritable();
    // SAFETY: Both handle outputs and the complete security descriptor are live.
    unsafe { CreatePipe(&mut read, &mut write, Some(&security), 0) }
        .map_err(|e| Error::from_win("CreatePipe", e).to_string())?;
    // SAFETY: CreatePipe succeeded; each handle is uniquely owned by its guard.
    let read = unsafe { OwnedHandle::from_raw(read) }.map_err(|e| e.to_string())?;
    // SAFETY: The second successful CreatePipe output has no other closing owner.
    let write = unsafe { OwnedHandle::from_raw(write) }.map_err(|e| e.to_string())?;
    // SAFETY: The read handle is live; only its inheritance flag is cleared.
    unsafe { SetHandleInformation(read.as_raw(), HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) }
        .map_err(|e| Error::from_win("SetHandleInformation", e).to_string())?;
    Ok((read, write))
}

struct Attributes(Vec<usize>);
impl Attributes {
    fn new() -> Result<Self, String> {
        let mut bytes = 0;
        // SAFETY: Null list is the documented sizing call; bytes is writable.
        match unsafe { InitializeProcThreadAttributeList(None, 2, None, &mut bytes) } {
            Err(error) if error.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() && bytes != 0 => {}
            Err(error) => {
                return Err(Error::from_win("InitializeProcThreadAttributeList", error).to_string());
            }
            Ok(()) => {
                return Err(Error::msg(
                    "InitializeProcThreadAttributeList",
                    "unexpected sizing success",
                )
                .to_string());
            }
        }
        let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
        let raw = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        // SAFETY: Allocation is pointer-aligned and large enough for both attributes.
        unsafe { InitializeProcThreadAttributeList(Some(raw), 2, None, &mut bytes) }
            .map_err(|e| Error::from_win("InitializeProcThreadAttributeList", e).to_string())?;
        Ok(Self(storage))
    }

    fn raw(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.0.as_ptr().cast_mut().cast())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: The list was initialized and storage and attribute values are still live.
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

struct Reader(JoinHandle<Result<String, String>>);
impl Reader {
    fn start(pipe: OwnedHandle, stream: &'static str) -> Result<Self, String> {
        thread::Builder::new()
            .name(format!("process-{stream}"))
            .spawn(move || {
                let result = match catch_unwind(AssertUnwindSafe(|| read_pipe(pipe))) {
                    Ok(result) => result,
                    Err(_) => {
                        Err(Error::msg("ReadFile", format!("{stream} reader panicked")).to_string())
                    }
                };
                // Also record errors if bounded cleanup has already detached this reader.
                if let Err(error) = &result {
                    eprintln!("{error}");
                }
                result
            })
            .map(Self)
            .map_err(|e| Error::msg("spawn pipe reader", e.to_string()).to_string())
    }

    fn finish(self, cleanup_started: Instant) -> Result<String, String> {
        while !self.0.is_finished() {
            if cleanup_started.elapsed() >= CLEANUP {
                // Dropping JoinHandle detaches, but the thread still owns and closes
                // its pipe. Never hang the caller on an unexpected OS teardown delay.
                return Err(
                    Error::msg("ReadFile", "pipe reader did not stop within 1000ms").to_string(),
                );
            }
            thread::sleep(POLL);
        }
        self.0
            .join()
            .map_err(|_| Error::msg("join pipe reader", "reader panicked").to_string())?
    }
}

fn read_pipe(pipe: OwnedHandle) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 16384];
    loop {
        let mut read = 0;
        // SAFETY: The live synchronous pipe, writable buffer and byte count are borrowed.
        match unsafe { ReadFile(pipe.as_raw(), Some(&mut buffer), Some(&mut read), None) } {
            Ok(()) if read == 0 => break,
            Ok(()) => bytes.extend_from_slice(&buffer[..read as usize]),
            // A broken pipe is the documented EOF after all writers have closed.
            Err(error) if error.code() == ERROR_BROKEN_PIPE.to_hresult() => break,
            Err(error) => return Err(Error::from_win("ReadFile", error).to_string()),
        }
    }
    decode_oem(&bytes)
}

fn decode_oem(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let count = i32::try_from(bytes.len())
        .map_err(|e| Error::msg("MultiByteToWideChar", e.to_string()).to_string())?;
    // SAFETY: The explicit byte count fits the live input; null output requests size.
    let needed =
        unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), count, std::ptr::null_mut(), 0) };
    if needed == 0 {
        return Err(Error::last("MultiByteToWideChar").to_string());
    }
    let mut wide = vec![0_u16; needed as usize];
    // SAFETY: Input is unchanged and output has the exact capacity returned above.
    let written = unsafe {
        MultiByteToWideChar(
            CP_OEMCP,
            0,
            bytes.as_ptr(),
            count,
            wide.as_mut_ptr(),
            needed,
        )
    };
    if written == 0 {
        return Err(Error::last("MultiByteToWideChar").to_string());
    }
    // Output can contain embedded NULs; this is not a NUL-terminated API string.
    Ok(String::from_utf16_lossy(&wide[..written as usize]))
}

fn terminate(job: &OwnedHandle, process: &OwnedHandle, started: Instant) -> Result<(), String> {
    // SAFETY: The job is live and owned here; termination includes every descendant.
    unsafe { TerminateJobObject(job.as_raw(), 1) }
        .map_err(|e| Error::from_win("TerminateJobObject", e).to_string())?;
    loop {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: The live job and correctly sized, writable SDK structure are borrowed.
        unsafe {
            QueryInformationJobObject(
                Some(job.as_raw()),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
        }
        .map_err(|e| Error::from_win("QueryInformationJobObject", e).to_string())?;
        // SAFETY: This live handle retains the root even after its job membership ends.
        let status = unsafe { WaitForSingleObject(process.as_raw(), 0) };
        if status == WAIT_FAILED {
            return Err(Error::last("WaitForSingleObject").to_string());
        }
        if info.ActiveProcesses == 0 && status == WAIT_OBJECT_0 {
            return Ok(());
        }
        if started.elapsed() >= CLEANUP {
            return Err(Error::msg(
                "TerminateJobObject",
                "process tree did not stop within 1000ms",
            )
            .to_string());
        }
        thread::sleep(POLL);
    }
}

#[allow(clippy::too_many_arguments)] // Keeps wait state local to this single process run.
fn wait(
    process: &OwnedHandle,
    stdout: &Reader,
    stderr: &Reader,
    started: Instant,
    timeout: Duration,
    cancel: &Cancel,
    exe: &Path,
    args: &[&str],
) -> Result<i32, String> {
    loop {
        if cancel.is_cancelled() {
            return Err(cancel_text(exe, args));
        }
        // SAFETY: process is live; a zero-duration wait only queries its signalled state.
        let status = unsafe { WaitForSingleObject(process.as_raw(), 0) };
        if status == WAIT_OBJECT_0 && stdout.0.is_finished() && stderr.0.is_finished() {
            let mut code = 0;
            // SAFETY: A live, exited process and writable DWORD output are provided.
            unsafe { GetExitCodeProcess(process.as_raw(), &mut code) }
                .map_err(|e| Error::from_win("GetExitCodeProcess", e).to_string())?;
            // C# parity: EventLogCleaningService.cs:372-374 (ExitCode is signed Int32).
            return Ok(code as i32);
        }
        if status == WAIT_FAILED {
            return Err(Error::last("WaitForSingleObject").to_string());
        }
        if status != WAIT_OBJECT_0 && status != WAIT_TIMEOUT {
            return Err(Error::msg(
                "WaitForSingleObject",
                format!("unexpected status {}", status.0),
            )
            .to_string());
        }
        if started.elapsed() >= timeout {
            // C# parity: EventLogCleaningService.cs:366
            return Err(format!(
                "Process timed out after {}ms: {} {}",
                timeout.as_millis(),
                exe.display(),
                args.join(" ")
            ));
        }
        thread::sleep(POLL.min(timeout.saturating_sub(started.elapsed())));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::{fs, io, sync::atomic::AtomicUsize};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};

    fn powershell() -> PathBuf {
        system32("cmd.exe")
            .parent()
            .unwrap()
            .join("WindowsPowerShell/v1.0/powershell.exe")
    }

    fn ps(script: &str) -> Output {
        run(
            &powershell(),
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ],
            Duration::from_secs(15),
            &Cancel::new(),
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn normal_run_captures_both_streams_and_quotes() {
        let output = ps(
            r#"[Console]::Out.Write("hello `"quoted`" C:\folder\"); [Console]::Error.Write('warning')"#,
        );
        assert_eq!(output.code, 0);
        assert_eq!(output.stdout, "hello \"quoted\" C:\\folder\\");
        assert_eq!(output.stderr, "warning");
    }

    #[test]
    fn reads_more_than_one_megabyte_from_each_pipe() {
        let output =
            ps("[Console]::Out.Write(('x' * 1100000)); [Console]::Error.Write(('y' * 1100000))");
        assert_eq!(output.code, 0);
        assert_eq!(output.stdout, "x".repeat(1_100_000));
        assert_eq!(output.stderr, "y".repeat(1_100_000));
    }

    #[test]
    fn missing_and_relative_executables_have_exact_start_text() {
        for exe in [
            system32("__HWIDChecker_missing_process_test.exe"),
            PathBuf::from("cmd.exe"),
        ] {
            assert!(!exe.exists());
            let error = run(&exe, &[], Duration::from_secs(2), &Cancel::new())
                .err()
                .unwrap();
            assert_eq!(error, format!("Failed to start process: {}", exe.display()));
        }
    }

    #[test]
    fn nonzero_exit_synthesizes_empty_or_whitespace_stderr() {
        let output = run(
            &system32("cmd.exe"),
            &["/d", "/s", "/c", "exit /b 13"],
            Duration::from_secs(5),
            &Cancel::new(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.code, 13);
        assert_eq!(output.stderr, "Process exited with code 13.");
        let output = ps("[Console]::Error.Write(\" `t`r`n\"); exit 13");
        assert_eq!(output.code, 13);
        assert_eq!(output.stderr, "Process exited with code 13.");
    }

    #[test]
    fn nonzero_exit_preserves_stderr_and_signed_code() {
        let output = ps("[Console]::Error.Write('failure detail'); exit -1");
        assert_eq!(output.code, -1);
        assert_eq!(output.stderr, "failure detail");
    }

    #[test]
    fn oem_decoding_preserves_embedded_nul() {
        assert_eq!(decode_oem(b"first\0last\r\n").unwrap(), "first\0last\r\n");
        assert_eq!(decode_oem(b"").unwrap(), "");
    }

    #[test]
    fn system32_is_absolute_and_rejects_path_escape() {
        let exe = system32("cmd.exe");
        assert!(exe.is_absolute());
        assert!(exe.is_file());
        assert_eq!(exe.file_name().unwrap(), "cmd.exe");
        for name in [
            "",
            ".",
            "..",
            "../cmd.exe",
            "sub\\cmd.exe",
            "C:\\cmd.exe",
            "cmd.exe\0",
        ] {
            assert!(system32(name).as_os_str().is_empty());
        }
    }

    #[test]
    fn cloned_cancel_prevents_start_with_exact_csharp_text() {
        let cancel = Cancel::new();
        let shared = cancel.clone();
        assert!(!shared.is_cancelled());
        cancel.cancel();
        assert!(shared.is_cancelled());
        let exe = system32("cmd.exe");
        let error = run(&exe, &["/c", "exit 0"], Duration::from_secs(5), &shared)
            .err()
            .unwrap();
        assert_eq!(
            error,
            format!("Process canceled: {} /c exit 0", exe.display())
        );
    }

    // Private temporary markers contain only subprocess PIDs, never hardware IDs.
    struct TreeProbe {
        marker: PathBuf,
        cancel: Cancel,
    }
    impl TreeProbe {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self {
                marker: std::env::temp_dir().join(format!(
                    "hwid-process-test-{}-{}.txt",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                )),
                cancel: Cancel::new(),
            }
        }

        fn script(&self, exit_parent: bool) -> String {
            let ping = system32("ping.exe")
                .display()
                .to_string()
                .replace('\'', "''");
            let marker = self.marker.display().to_string().replace('\'', "''");
            format!(
                "$child = Start-Process -FilePath '{ping}' -ArgumentList '-n 60 127.0.0.1' -NoNewWindow -PassThru; \
                [IO.File]::WriteAllText('{marker}', ('{{0}} {{1}}' -f $PID,$child.Id)); {}",
                if exit_parent {
                    "exit 0"
                } else {
                    "Start-Sleep -Seconds 60"
                }
            )
        }

        fn handles(&self, exit_parent: bool) -> Vec<OwnedHandle> {
            let started = Instant::now();
            loop {
                match fs::read_to_string(&self.marker) {
                    Ok(text) => {
                        let pids: Vec<u32> = text
                            .split_whitespace()
                            .map(|pid| pid.parse().unwrap())
                            .collect();
                        if pids.len() == 2 {
                            return pids
                                .into_iter()
                                .skip(usize::from(exit_parent))
                                .map(|pid| {
                                    // SAFETY: Only a test-created PID is opened, with synchronization access.
                                    own(
                                        unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) },
                                        "OpenProcess(test)",
                                    )
                                    .unwrap()
                                })
                                .collect();
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => panic!("read test marker: {error}"),
                }
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "process tree never became ready"
                );
                thread::sleep(POLL);
            }
        }
    }
    impl Drop for TreeProbe {
        fn drop(&mut self) {
            self.cancel.cancel();
            match fs::remove_file(&self.marker) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => eprintln!("remove test marker: {error}"),
            }
        }
    }

    fn check_tree(cancelled: bool, exit_parent: bool) {
        let probe = TreeProbe::new();
        let script = probe.script(exit_parent);
        let args = [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ];
        let exe = powershell();
        let cancel = probe.cancel.clone();
        let timeout = Duration::from_secs(if cancelled { 30 } else { 5 });
        thread::scope(|scope| {
            let started = Instant::now();
            let runner = scope.spawn(|| run(&exe, &args, timeout, &cancel));
            let handles = probe.handles(exit_parent);
            let cancel_started = Instant::now();
            if cancelled {
                probe.cancel.cancel();
            }
            let error = runner.join().unwrap().err().unwrap();
            let elapsed = if cancelled {
                cancel_started.elapsed()
            } else {
                started.elapsed().saturating_sub(timeout)
            };
            assert!(
                elapsed < Duration::from_millis(1250),
                "cleanup took {elapsed:?}"
            );
            let expected = if cancelled {
                format!("Process canceled: {} {}", exe.display(), args.join(" "))
            } else {
                format!(
                    "Process timed out after 5000ms: {} {}",
                    exe.display(),
                    args.join(" ")
                )
            };
            assert_eq!(error, expected);
            for handle in handles {
                // SAFETY: This retained handle belongs to a test-created process.
                let status = unsafe { WaitForSingleObject(handle.as_raw(), 0) };
                assert_eq!(status, WAIT_OBJECT_0, "a process survived job termination");
            }
        });
    }

    #[test]
    fn timeout_kills_parent_and_grandchild_holding_pipes() {
        check_tree(false, false);
    }

    #[test]
    fn cancel_kills_parent_and_grandchild_holding_pipes() {
        check_tree(true, false);
    }

    #[test]
    fn exited_parent_does_not_bypass_timeout_on_grandchild_pipes() {
        check_tree(false, true);
    }
}
