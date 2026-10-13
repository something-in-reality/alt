use std::{io::IsTerminal, path::PathBuf, sync::Arc};

use accessibility::{
    config::Config,
    event::{Context, Verdict},
    macos::{self, Desktop, Session, setup},
    platform::{self, Input},
    watch::watch,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::{Parser, Subcommand};
use etcetera::BaseStrategy;
use tracing_oslog::OsLogger;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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
    /// Prints each event and the front application as they happen, without affecting them.
    Events,
    /// Sets up this machine once: a code-signing certificate that signs the app.
    Setup {
        /// The certificate's name, which the home-manager module's `certificate` names too.
        #[arg(long)]
        certificate: String,
    },
    /// Exits successfully when this process may observe and post input.
    #[command(hide = true)]
    Trusted,
}

fn main() -> anyhow::Result<()> {
    // The unified log, as the app's subsystem; a terminal also reads it on stderr.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(OsLogger::new(macos::IDENTIFIER, "default"))
        .with(
            std::io::stderr()
                .is_terminal()
                .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr)),
        )
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
            platform::run(Session::default(), Desktop::new()?, config)?;
        }
        Commands::Events => {
            Session::listen_only().run(|event, cx| {
                let front = cx.front().map(|app| app.to_string()).unwrap_or_default();
                println!("{event}\t{front}");
                Verdict::Pass
            })?;
        }
        Commands::Setup { certificate } => {
            setup::setup(&[&setup::Certificate { name: certificate }])?;
        }
        Commands::Trusted => {
            if !macos::trusted() {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
