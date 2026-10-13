//! Keyboard, pointer, window and display behaviour outside the terminal.
//!
//! An [`Input`](platform::Input) produces [`Event`](event::Event)s, the [`Config`](config::Config)
//! decides each, and an [`Output`](platform::Output) applies the effects.

pub mod action;
pub mod combo;
pub mod config;
pub mod event;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod platform;
pub mod watch;
