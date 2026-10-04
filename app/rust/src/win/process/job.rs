//! Job-object ownership, process attributes and bounded tree termination.

use super::{CLEANUP, POLL, own};
use crate::win::{Error, OwnedHandle};
use std::{mem::size_of, thread, time::Instant};
use windows::{
    Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, WAIT_FAILED, WAIT_OBJECT_0},
        System::{
            JobObjects::{
                CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
                QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
            },
            Threading::{
                DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
                LPPROC_THREAD_ATTRIBUTE_LIST, WaitForSingleObject,
            },
        },
    },
    core::PCWSTR,
};

pub(super) fn create() -> Result<OwnedHandle, String> {
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
    Ok(job)
}

pub(super) struct Attributes {
    _storage: Vec<usize>,
    raw: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    pub(super) fn new() -> Result<Self, String> {
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
        Ok(Self {
            _storage: storage,
            raw,
        })
    }

    pub(super) fn raw(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.raw
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: The list was initialized and storage and attribute values are still live.
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}
pub(super) fn terminate(
    job: &OwnedHandle,
    process: &OwnedHandle,
    started: Instant,
) -> Result<(), String> {
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
