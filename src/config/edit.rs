//! Edits a copy of the configuration in $VISUAL or $EDITOR; the copy replaces the file once it
//! parses.

use std::{fs, path::Path};

use inquire::Confirm;

use super::Config;

/// Opens a copy of `path` in $VISUAL or $EDITOR until it is valid or discarded, then writes a valid
/// copy to the file `path` links to.
pub fn edit(path: &Path) -> anyhow::Result<()> {
    let target = fs::canonicalize(path)?;
    let original = fs::read_to_string(&target)?;
    let mut text = original.clone();
    loop {
        text = edit::edit_with_builder(&text, edit::Builder::new().suffix(".toml"))?;
        match text.parse::<Config>() {
            Ok(_) if text == original => {
                println!("unchanged");
                return Ok(());
            }
            Ok(_) => {
                fs::write(&target, &text)?;
                println!("wrote {}", target.display());
                return Ok(());
            }
            Err(error) => {
                eprintln!("{error}");
                if !Confirm::new("Edit again?").with_default(true).prompt()? {
                    println!("discarded");
                    return Ok(());
                }
            }
        }
    }
}
