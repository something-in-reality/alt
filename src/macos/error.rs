//! Errors of the macOS platform.

use crate::{
    action::{AppId, Device, Screen},
    combo::Key,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("creating the event tap failed")]
    Tap,
    #[error("the run loop ended")]
    RunLoopEnded,
    #[error("{0} is not running")]
    NotRunning(AppId),
    #[error("{0} is not installed")]
    NotInstalled(AppId),
    #[error("{0} has no Mac key code")]
    Unmapped(Key),
    #[error("could not create {0}")]
    Unavailable(&'static str),
    #[error("{0} is not paired")]
    NotPaired(Device),
    #[error("Bluetooth error {1:#x} with {0}")]
    Bluetooth(Device, i32),
    #[error("Accessibility API error {code} on {name}")]
    Accessibility { name: String, code: i32 },
    #[error("no display {0}")]
    NoDisplay(Screen),
    #[error("no menu item {0}")]
    NoMenuItem(String),
    #[error("the window is in full screen")]
    Fullscreen,
    #[error("no built-in display")]
    NoBuiltinDisplay,
    #[error("DisplayServices error {0}")]
    DisplayServices(i32),
    #[error("showing a notification: {0}")]
    Notification(#[from] notify_rust::error::Error),
    #[error("loading DisplayServices: {0}")]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
