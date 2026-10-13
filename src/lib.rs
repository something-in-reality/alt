//! Keyboard, pointer, window and display behaviour outside the terminal.

pub mod action;
pub mod combo;
pub mod config;
pub mod watch;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{events, run};
