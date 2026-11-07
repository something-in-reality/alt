//! Errors of the macOS platform.

use crate::{
    action::{BundleId, Device, Screen},
    combo::Key,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("creating the event tap failed")]
    Tap,
    #[error("the run loop ended")]
    RunLoopEnded,
    #[error("{0} is not running")]
    NotRunning(BundleId),
    #[error("{0} is not installed")]
    NotInstalled(BundleId),
    #[error("{0} has no Mac key code")]
    Unmapped(Key),
    #[error("could not create {0}")]
    Unavailable(&'static str),
    #[error("the process runs outside an app bundle")]
    NoBundle,
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
    #[error("menu item {0} is disabled")]
    DisabledMenuItem(String),
    #[error("no built-in display")]
    NoBuiltinDisplay,
    #[error("DisplayServices error {0}")]
    DisplayServices(i32),
    #[error("loading DisplayServices: {0}")]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
