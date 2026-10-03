//! Every worker thread and every wndproc body must use `catch_panic` so a caught
//! panic follows the caller's error/status path without an unexpected-error dialog.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
};

thread_local! {
    static GUARDED: Cell<bool> = const { Cell::new(false) };
}

struct Guard(bool);

impl Drop for Guard {
    fn drop(&mut self) {
        GUARDED.with(|guarded| guarded.set(self.0));
    }
}

/// Runs a panic boundary, suppressing the GUI panic hook and returning its message.
/// Every worker thread and every wndproc body must run inside this boundary.
pub fn catch_panic<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    let guard = Guard(GUARDED.with(|guarded| guarded.replace(true)));
    let result = catch_unwind(AssertUnwindSafe(f));
    drop(guard);
    result.map_err(|panic| {
        panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic payload")
            .to_owned()
    })
}

/// Reports whether this thread's panic hook is inside a shared panic boundary.
pub fn is_guarded() -> bool {
    GUARDED.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_boundaries_restore_state_after_success_and_panic() {
        assert!(!is_guarded());
        assert_eq!(
            catch_panic(|| {
                assert!(is_guarded());
                assert!(!std::thread::spawn(is_guarded).join().unwrap());
                assert_eq!(
                    catch_panic(|| panic!("caught inner")),
                    Err::<(), _>("caught inner".into())
                );
                assert!(is_guarded());
                panic!("caught outer");
            }),
            Err::<(), _>("caught outer".into())
        );
        assert!(!is_guarded());
        assert_eq!(catch_panic(|| 42), Ok(42));
        assert!(!is_guarded());
        assert_eq!(
            catch_panic(|| std::panic::panic_any(42)),
            Err::<(), _>("non-string panic payload".into())
        );
        assert!(!is_guarded());
    }
}
