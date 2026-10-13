mod cli;

use std::{
    env,
    io::{self, IsTerminal},
    process::ExitCode,
    sync::Arc,
};

use accessibility::{
    config::{Config, edit::edit, watch::watch},
    macos::{self, Dispatcher, Session, status::Status},
    platform,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::Parser;
use etcetera::BaseStrategy;
use tracing_oslog::OsLogger;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use self::cli::{Cli, Commands};

fn main() -> anyhow::Result<ExitCode> {
    if env::var_os(macos::TRUST_CHECK).is_some() {
        return Ok(if macos::trusted() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
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
            match Config::load(&path) {
                Ok(_) => println!("config:     {}", path.display()),
                Err(error) => println!(
                    "config:     {}: {:#}",
                    path.display(),
                    anyhow::Error::from(error)
                ),
            }
            println!("{}", Status::read());
        }
        Some(Commands::Run) => {
            let config = Arc::new(ArcSwap::from_pointee(Config::load(&path)?));
            let _watcher = watch(path, config.clone()).context("watching the configuration")?;
            platform::run(Session, Dispatcher::new()?, config)?;
        }
        Some(Commands::Edit) => edit(&path)?,
        Some(Commands::Log) => match macos::log()? {},
        Some(Commands::Setup) => macos::setup::setup()?,
    }
    Ok(ExitCode::SUCCESS)
}
