//! Shared port modules; library tests deliberately receive no elevated manifest.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clean;
pub mod hw;
pub mod report;
pub mod settings;
pub mod ui;
pub mod update;
pub mod win;
