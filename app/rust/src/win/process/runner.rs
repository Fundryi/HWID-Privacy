//! Process startup, command quoting and cancellable wait.

use super::{
    Cancel, Output, POLL,
    capture::{Reader, pipe},
    inheritable,
    job::{self, Attributes, terminate},
    own,
};
use crate::win::{Error, OwnedHandle, record, wide::to_wide};
use std::{
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::Threading::{
            CREATE_NO_WINDOW, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
    core::{PCWSTR, PWSTR},
};

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
        record(Error::msg(
            "CreateProcessW",
            "absolute path and NUL-free arguments required",
        ));
        return Err(start_text(exe));
    }
    let mut command = quote(&application);
    for arg in args {
        command.push(b' ' as u16);
        command.extend(quote(&arg.encode_utf16().collect::<Vec<_>>()));
    }
    if command.len() >= 32767 {
        record(Error::msg(
            "CreateProcessW",
            "command line exceeds 32767 UTF-16 units",
        ));
        return Err(start_text(exe));
    }
    command.push(0);
    let mut application = application;
    application.push(0);

    let job = job::create()?;
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
        record(Error::from_win("CreateProcessW", error));
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
                record(Error::msg("stdout cleanup", cleanup));
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
        record(Error::msg("process cleanup", error));
    }
    // Close before joining: descendants may still own pipe writers after root exit.
    drop(job);
    let stdout = stdout.finish(cleanup_started);
    let stderr = stderr.finish(cleanup_started);
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            for failure in [stdout, stderr].into_iter().filter_map(Result::err) {
                record(Error::msg("pipe cleanup", failure));
            }
            return Err(error);
        }
    };
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(error) => {
            if let Err(other) = stderr {
                record(Error::msg("stderr cleanup", other));
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
