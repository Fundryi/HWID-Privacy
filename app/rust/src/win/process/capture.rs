//! Inheritance-isolated pipes and bounded OEM output capture.

use super::{CLEANUP, POLL, inheritable};
use crate::win::{Error, OwnedHandle, catch_panic, record};
use std::{
    thread::{self, JoinHandle},
    time::Instant,
};
use windows::Win32::{
    Foundation::{
        ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation,
    },
    Storage::FileSystem::ReadFile,
    System::Pipes::CreatePipe,
};

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
pub(super) fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
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
pub(super) struct Reader(pub(super) JoinHandle<Result<String, String>>);
impl Reader {
    pub(super) fn start(pipe: OwnedHandle, stream: &'static str) -> Result<Self, String> {
        thread::Builder::new()
            .name(format!("process-{stream}"))
            .spawn(move || {
                let result = match catch_panic(|| read_pipe(pipe)) {
                    Ok(result) => result,
                    Err(message) => Err(Error::msg(
                        "ReadFile",
                        format!("{stream} reader panicked: {message}"),
                    )
                    .to_string()),
                };
                // Also record errors if bounded cleanup has already detached this reader.
                if let Err(error) = &result {
                    record(Error::msg("pipe reader", error.clone()));
                }
                result
            })
            .map(Self)
            .map_err(|e| Error::msg("spawn pipe reader", e.to_string()).to_string())
    }

    pub(super) fn finish(self, cleanup_started: Instant) -> Result<String, String> {
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

pub(super) fn decode_oem(bytes: &[u8]) -> Result<String, String> {
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
