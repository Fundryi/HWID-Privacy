//! Native UI entry points frozen before work packages fill in the windows.

pub mod clean_devices;
pub mod clean_logs;
pub mod compare;
pub mod confirm_removal;
pub mod controls;
pub mod dpi;
pub mod layout;
pub mod main_window;
pub mod msgbox;
pub mod raw_view;
#[cfg(test)]
mod spike;
pub mod theme;
pub mod update_progress;
pub mod whitelist;
pub mod window;

/// Runs the main UI and returns after it closes.
pub fn run() {
    main_window::run();
}
