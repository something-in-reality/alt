mod cli;

use std::{
    io::{self, IsTerminal, Write},
    process::ExitCode,
    sync::Arc,
};

use alt::{
    config::{Config, edit::edit, watch::watch},
    macos::{self, status::Status},
    paths,
};
use anyhow::Context as _;
use arc_swap::ArcSwap;
use clap::Parser;
use tracing_oslog::OsLogger;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use self::cli::{Cli, Commands};

fn main() -> anyhow::Result<ExitCode> {
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
    let path = || {
        cli.config
            .clone()
            .or_else(paths::config_file)
            .context("no home directory for the config file")
    };
    match cli.command {
        None => {
            let path = path()?;
            let loaded = Config::load(&path);
            let config = match &loaded {
                Ok(_) => path.display().to_string(),
                Err(error) => error.to_string(),
            };
            // A reader that closes early, such as `head`, ends the report.
            match writeln!(
                io::stdout().lock(),
                "config:        {config}\n{}",
                Status::read()
            ) {
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {}
                written => written?,
            }
            if loaded.is_err() {
                return Ok(ExitCode::FAILURE);
            }
        }
        Some(Commands::Run) => {
            let path = path()?;
            let config = Arc::<ArcSwap<Config>>::default();
            let _watcher = watch(path, config.clone()).context("watching the configuration")?;
            macos::run(config)?;
        }
        Some(Commands::Edit) => edit(&path()?)?,
        Some(Commands::Schema) => {
            let mut stdout = io::stdout().lock();
            serde_json::to_writer_pretty(&mut stdout, &Config::schema())?;
            writeln!(stdout)?;
        }
        Some(Commands::Log) => match macos::log()? {},
        Some(Commands::Setup) => macos::setup::setup()?,
        Some(Commands::Notifications) => println!("{}", macos::notification::authorization()?),
        Some(Commands::Trusted) => {
            return Ok(if macos::trusted() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}
