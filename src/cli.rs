//! The command line; `build.rs` includes this module to generate shell completions.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Keyboard, pointer, window and display behaviour outside the terminal.
///
/// Without a command, checks the configuration and reports the app: its process, signature and
/// Accessibility permission.
#[derive(Parser)]
#[command(name = "alt", version)]
pub struct Cli {
    /// Defaults to $XDG_CONFIG_HOME/alt/config.toml.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Applies the configuration in the foreground and reloads it when it changes; launchd runs
    /// it at login.
    Run,
    /// Opens a copy of the configuration in $VISUAL or $EDITOR, and writes it into place once it
    /// is valid.
    Edit,
    /// Prints the configuration's JSON Schema. An editor with a TOML language server, such as taplo,
    /// validates and completes config.toml against it from a first line `#:schema <path or url>`.
    Schema,
    /// Streams the running app's log: each decided event, action and error.
    Log,
    /// Shows the machine's one-time steps, the Accessibility permission and the Glove80's
    /// firmware, and runs those selected.
    Setup,
}
