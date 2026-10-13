//! Errors of the macOS platform.

use crate::{
    action::{AppId, Device},
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
    #[error("{0} has no Mac key code")]
    Unmapped(Key),
    #[error("could not create {0}")]
    Unavailable(&'static str),
    #[error("{0} is not paired")]
    NotPaired(Device),
    #[error("Bluetooth error {1:#x} with {0}")]
    Bluetooth(Device, i32),
    #[error("accessibility error {0}")]
    Accessibility(i32),
    #[error("no built-in display")]
    NoBuiltinDisplay,
    #[error("DisplayServices error {0}")]
    DisplayServices(i32),
    #[error("loading DisplayServices")]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
