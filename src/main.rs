use std::{io::IsTerminal, path::PathBuf, sync::Arc};

use accessibility::{
    config::Config,
    event::{Context, Event, Verdict},
    macos::{Desktop, Session},
    platform::{self, Input},
    watch::watch,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::{Parser, Subcommand};
use etcetera::BaseStrategy;
use tracing_subscriber::EnvFilter;

/// Keyboard, pointer, window and display behaviour outside the terminal.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Defaults to $XDG_CONFIG_HOME/accessibility/config.toml.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Default)]
enum Commands {
    /// Applies the configuration, reloading it when it changes.
    #[default]
    Run,
    /// Checks the configuration.
    Check,
    /// Prints key combinations and the front application as they happen.
    Events,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let cli = Cli::parse();
    let path = match cli.config {
        Some(path) => path,
        None => etcetera::choose_base_strategy()?
            .config_dir()
            .join("accessibility/config.toml"),
    };
    match cli.command.unwrap_or_default() {
        Commands::Check => {
            Config::load(&path)?;
        }
        Commands::Run => {
            let config = Arc::new(ArcSwap::from_pointee(Config::load(&path)?));
            let _watcher = watch(path, config.clone()).context("watching the configuration")?;
            platform::run(Session, Desktop::new()?, config)?;
        }
        Commands::Events => {
            Session.run(|event, cx| {
                if let Event::Press { combo, .. } = event {
                    println!(
                        "{combo}\t{}",
                        cx.front().map(|app| app.to_string()).unwrap_or_default()
                    );
                }
                Verdict::Pass
            })?;
        }
    }
    Ok(())
}
