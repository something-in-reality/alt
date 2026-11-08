//! Keyboard, pointer, window and display behaviour outside the terminal.
//!
//! The platform produces [`Event`](event::Event)s, the [`Config`](config::Config) decides each, and
//! the platform applies the effects.

pub mod action;
pub mod combo;
pub mod config;
pub mod event;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod paths;
