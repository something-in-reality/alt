use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    sync::Arc,
};

use accessibility::{
    config::Config,
    macos::{self, Desktop, Session, setup, status::Status},
    platform,
    watch::watch,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use etcetera::BaseStrategy;
use tracing_oslog::OsLogger;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Keyboard, pointer, window and display behaviour outside the terminal.
///
/// Without a command, applies the configuration in the foreground and reloads it when it changes;
/// launchd runs it so at login.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Defaults to $XDG_CONFIG_HOME/accessibility/config.toml.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Checks the configuration and reports the running app and its signature.
    Status,
    /// Streams the running app's log: each decided event, action and error.
    Log,
    /// Sets up this machine once: a code-signing certificate that signs the app, usable without
    /// prompts.
    Setup {
        /// The certificate's name, which the home-manager module's `certificate` names too.
        #[arg(long)]
        certificate: String,
    },
    /// Prints shell completions.
    #[command(hide = true)]
    Completions { shell: Shell },
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
            io::stderr()
                .is_terminal()
                .then(|| tracing_subscriber::fmt::layer().with_writer(io::stderr)),
        )
        .init();
    let cli = Cli::parse();
    let path = match cli.config {
        Some(path) => path,
        None => etcetera::choose_base_strategy()?
            .config_dir()
            .join("accessibility/config.toml"),
    };
    match cli.command {
        None => {
            let config = Arc::new(ArcSwap::from_pointee(Config::load(&path)?));
            let _watcher = watch(path, config.clone()).context("watching the configuration")?;
            platform::run(Session, Desktop::new()?, config)?;
        }
        Some(Commands::Status) => {
            match Config::load(&path) {
                Ok(_) => println!("config:    {}", path.display()),
                Err(error) => println!("config:    {}: {error}", path.display()),
            }
            println!("{}", Status::read());
        }
        Some(Commands::Log) => return Err(macos::log().into()),
        Some(Commands::Setup { certificate }) => {
            setup::setup(&[
                &setup::Certificate {
                    name: certificate.clone(),
                },
                &setup::KeyAccess { name: certificate },
            ])?;
        }
        Some(Commands::Completions { shell }) => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "accessibility",
                &mut io::stdout(),
            );
        }
        Some(Commands::Trusted) => {
            if !macos::trusted() {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
