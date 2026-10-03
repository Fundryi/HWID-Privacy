//! Owned by WP-13: download once, validate, then install the retained file.

use std::path::PathBuf;

pub const UPDATE_URL: &str = "https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe";
pub struct Downloaded {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}
pub enum UpdateCheck {
    UpToDate,
    Available(Downloaded),
}

/// Downloads and validates an update once, retaining its file when available.
pub fn check() -> Result<UpdateCheck, String> {
    Err("Update check not ported yet".to_owned())
}
/// Installs the retained download and restarts, returning errors before setup completes.
pub fn install_and_restart(_d: Downloaded) -> Result<(), String> {
    Err("Update install not ported yet".to_owned())
}
