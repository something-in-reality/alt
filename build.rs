//! Writes shell completions into $COMPLETIONS when it is set.

use std::{env, fs, io, path::Path};

use clap::CommandFactory;
use clap_complete::Shell;

#[path = "src/cli.rs"]
mod cli;

fn main() -> io::Result<()> {
    println!("cargo::rerun-if-changed=src/cli.rs");
    println!("cargo::rerun-if-env-changed=COMPLETIONS");
    let Some(dir) = env::var_os("COMPLETIONS") else {
        return Ok(());
    };
    let dir = Path::new(&env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default()).join(dir);
    fs::create_dir_all(&dir)?;
    let mut command = cli::Cli::command();
    for shell in [Shell::Bash, Shell::Fish, Shell::Zsh] {
        clap_complete::generate_to(shell, &mut command, "accessibility", &dir)?;
    }
    Ok(())
}
