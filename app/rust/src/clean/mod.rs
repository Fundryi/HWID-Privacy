//! Cleaner contracts and the shared destructive-operation guard.

pub mod devices;
pub mod eventlog;
pub mod whitelist;

/// Frozen dry-run guard contract; the cleaner fill-in implements debug authorization.
pub fn destructive<T>(what: &str, _f: impl FnOnce() -> T) -> Option<T> {
    eprintln!("[NOT PORTED] {what}");
    None
}
