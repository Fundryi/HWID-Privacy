//! Owned by WP-11: fail-closed whitelist files.

use super::devices::GhostDevice;

/// Loads the whitelist, treating missing files as empty and malformed files as errors.
pub fn load_whitelist() -> Result<Vec<GhostDevice>, String> {
    Err("Whitelist load not ported yet".to_owned())
}
/// Atomically saves a whitelist through the destructive-operation guard.
pub fn save_whitelist(_devices: &[GhostDevice]) -> Result<(), String> {
    Err("Whitelist save not ported yet".to_owned())
}
/// Resets the whitelist through the destructive-operation guard.
pub fn reset_whitelist() -> Result<(), String> {
    Err("Whitelist reset not ported yet".to_owned())
}
