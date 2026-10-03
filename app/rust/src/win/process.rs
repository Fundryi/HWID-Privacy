//! Bounded, cancellable child processes; executable paths must be absolute.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

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
    _exe: &Path,
    _args: &[&str],
    _timeout: Duration,
    _cancel: &Cancel,
) -> std::result::Result<Output, String> {
    Err("Process runner not ported yet".to_owned())
}
/// Resolves a System32 child executable without searching PATH or the executable folder.
pub fn system32(_name: &str) -> PathBuf {
    PathBuf::from("System32 resolution not ported yet")
}
