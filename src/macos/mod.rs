//! macOS: events from the session, effects on GCD queues and the system APIs.

mod app;
mod bluetooth;
mod desktop;
mod display;
mod error;
mod keymap;
pub mod notification;
mod scroll;
mod session;
pub mod setup;
pub mod status;
mod synth;
mod system_key;
mod tap;
mod window;

use std::{convert::Infallible, io, os::unix::process::CommandExt, process, sync::Arc};

use arc_swap::ArcSwap;
use objc2_application_services::AXIsProcessTrusted;

use self::desktop::Dispatcher;
pub use self::error::Error;
use crate::{config::Config, event::Verdict};

/// The app's bundle identifier, which also names its log subsystem.
pub const IDENTIFIER: &str = env!("BUNDLE_IDENTIFIER");

/// Where the nix-darwin module installs the signed app.
pub const BUNDLE: &str = env!("BUNDLE_PATH");

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

/// Decides each session event against the live `config`, and applies the effects.
pub fn run(config: Arc<ArcSwap<Config>>) -> Result<Infallible, Error> {
    let dispatcher = Dispatcher::new()?;
    session::run(move |event, front| {
        let decision = config.load().decide(event.clone(), front);
        if decision.verdict != Verdict::Pass || decision.effect.is_some() {
            tracing::debug!(%event, verdict = ?decision.verdict, effect = ?decision.effect);
        }
        if let Some(effect) = decision.effect {
            dispatcher.apply(effect);
        }
        decision.verdict
    })
}
