//! Bounded, cancellable child processes; executable paths must be absolute.

mod capture;
mod job;
mod paths;
mod runner;

pub use paths::{powershell, system32};
pub use runner::run;

use super::{Error, OwnedHandle};
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use windows::Win32::{Foundation::HANDLE, Security::SECURITY_ATTRIBUTES};

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
