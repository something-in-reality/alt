mod cli;

use std::{
    env,
    io::{self, IsTerminal},
    process::ExitCode,
    sync::Arc,
};

use alt::{
    config::{Config, edit::edit, watch::watch},
    macos::{self, Dispatcher, Session, status::Status},
    platform,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::Parser;
use etcetera::BaseStrategy;
use tracing_oslog::OsLogger;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use self::cli::{Cli, Commands};

fn main() -> anyhow::Result<ExitCode> {
    if env::var_os(macos::TRUST_CHECK).is_some() {
        return Ok(if macos::trusted() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    // The unified log receives debug messages, which `alt log` streams; a terminal reads info and
    // above on stderr. RUST_LOG overrides both.
    let filter =
        |default: &str| EnvFilter::try_from_default_env().unwrap_or_else(|_| default.into());
    tracing_subscriber::registry()
        .with(OsLogger::new(macos::IDENTIFIER, "default").with_filter(filter("debug")))
        .with(io::stderr().is_terminal().then(|| {
            tracing_subscriber::fmt::layer()
                .with_writer(io::stderr)
                .with_filter(filter("info"))
        }))
        .init();
    let cli = Cli::parse();
    let path = match cli.config {
        Some(path) => path,
        None => etcetera::choose_base_strategy()?
            .config_dir()
            .join("alt/config.toml"),
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
