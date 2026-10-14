//! Passes the bundle identifier from Cargo.toml's `package.metadata.bundle` to the crate, and
//! writes shell completions into $COMPLETIONS when it is set.

use std::{env, fs, io, path::Path};

use clap::CommandFactory;
use clap_complete::Shell;

#[path = "src/cli.rs"]
mod cli;

fn main() -> io::Result<()> {
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-changed=src/cli.rs");
    println!("cargo::rerun-if-env-changed=COMPLETIONS");
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default();
    let manifest: toml::Table = fs::read_to_string(Path::new(&manifest_dir).join("Cargo.toml"))?
        .parse()
        .map_err(io::Error::other)?;
    let identifier = manifest["package"]["metadata"]["bundle"]["identifier"]
        .as_str()
        .ok_or_else(|| io::Error::other("package.metadata.bundle.identifier is a string"))?;
    println!("cargo::rustc-env=BUNDLE_IDENTIFIER={identifier}");

    let Some(dir) = env::var_os("COMPLETIONS") else {
        return Ok(());
    };
    let dir = Path::new(&manifest_dir).join(dir);
    fs::create_dir_all(&dir)?;
    let mut command = cli::Cli::command();
    for shell in [Shell::Bash, Shell::Zsh] {
        clap_complete::generate_to(shell, &mut command, env!("CARGO_PKG_NAME"), &dir)?;
    }
    Ok(())
}
