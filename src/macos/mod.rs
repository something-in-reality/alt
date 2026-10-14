//! macOS: [`Session`] is the `Input`, [`Dispatcher`] the `Output`, and [`Mac`] the `Desktop`.

mod app;
mod bluetooth;
mod desktop;
mod display;
mod error;
mod keymap;
mod session;
pub mod setup;
pub mod status;
mod synth;
mod system_key;
mod tap;
mod window;

use std::{convert::Infallible, io, os::unix::process::CommandExt, process};

use objc2_application_services::AXIsProcessTrusted;

pub use self::{
    desktop::{Dispatcher, Mac},
    error::Error,
    session::{Apps, Session},
};

/// The environment variable that makes the binary report, by its exit status, whether it may
/// observe and post input.
pub const TRUST_CHECK: &str = "ALT_TRUST_CHECK";

/// The app's bundle identifier, which also names its log subsystem.
pub const IDENTIFIER: &str = "something-in-reality.alt";

/// Streams this app's log, debug messages included, replacing this process.
pub fn log() -> io::Result<Infallible> {
    Err(process::Command::new("/usr/bin/log")
        .args([
            "stream",
            "--level",
            "debug",
            "--style",
            "compact",
            "--predicate",
        ])
        .arg(format!("subsystem == \"{IDENTIFIER}\""))
        .exec())
}

/// Whether this process may observe and post input.
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
