//! Cleaner contracts and the shared destructive-operation guard.

pub mod devices;
pub mod eventlog;
pub mod whitelist;

/// Runs destructive work only in release builds or with exact debug authorization.
///
/// `None` means skipped by the dry-run guard. The caller must show `[DRY RUN] {what}`
/// through its own status output, since stderr is lost in the GUI executable.
pub fn destructive<T>(what: &str, _f: impl FnOnce() -> T) -> Option<T> {
    let _ = what; // The caller owns the status output and operation description.
    if cfg!(debug_assertions)
        && std::env::var_os("HWID_ALLOW_DESTRUCTIVE").as_deref() != Some(std::ffi::OsStr::new("1"))
    {
        return None;
    }
    Some(_f())
}

#[cfg(test)]
mod tests {
    use super::destructive;

    #[test]
    fn destructive_skips_unauthorized_debug_work() {
        // Never change process-wide destructive authorization, even for a test.
        if cfg!(debug_assertions) {
            if std::env::var_os("HWID_ALLOW_DESTRUCTIVE").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                assert_eq!(destructive("test only", || 42), Some(42));
                return;
            }
            assert_eq!(
                destructive("test only", || panic!("guard ran the work")),
                None::<()>
            );
        } else {
            assert_eq!(destructive("test only", || 42), Some(42));
        }
    }
}
