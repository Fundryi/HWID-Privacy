//! Owned by WP-10b: the main hardware window.

/// Shows the main window stub and returns after its message box closes.
pub fn run() {
    super::window::show_info(Default::default(), "UI not ported yet", "HWID Checker");
}
