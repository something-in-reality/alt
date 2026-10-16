mod cli;

use std::{
    env,
    io::{self, IsTerminal, Write},
    process::ExitCode,
    sync::Arc,
};

use alt::{
    config::{Config, edit::edit, watch::watch},
    macos::{self, Dispatcher, Session, status::Status},
    paths, platform,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::Parser;
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
        None => paths::config_file()?,
    };
    match cli.command {
        None => {
            let config = match Config::load(&path) {
                Ok(_) => path.display().to_string(),
                Err(error) => format!("{}: {error}", path.display()),
            };
            // A reader that closes early, such as `head`, ends the report.
            match writeln!(
                io::stdout().lock(),
                "config:     {config}\n{}",
                Status::read()
            ) {
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {}
                written => written?,
            }
        }
        Some(Commands::Run) => {
            let config = Arc::new(ArcSwap::from_pointee(Config::load(&path)?));
            let hidpi = macos::HiDpi::new(config.clone())?;
            let _watcher = watch(path, config.clone(), move || hidpi.reconcile())
                .context("watching the configuration")?;
            platform::run(Session, Dispatcher::new()?, config)?;
        }
        Some(Commands::Edit) => edit(&path)?,
        Some(Commands::Log) => match macos::log()? {},
        Some(Commands::Setup) => macos::setup::setup()?,
    }
    Ok(ExitCode::SUCCESS)
}
