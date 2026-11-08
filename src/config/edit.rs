//! Edits a copy of the configuration in $VISUAL or $EDITOR; the copy replaces the file once it
//! parses.

use std::{fs, io::Write, path::Path};

use inquire::Confirm;
use tempfile::NamedTempFile;

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
                write(&target, &text)?;
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

/// Replaces the file `target` with `text` atomically, keeping its permissions: the text is written
/// to a temporary file in the same directory, which is renamed over `target`.
fn write(target: &Path, text: &str) -> anyhow::Result<()> {
    let directory = target.parent().unwrap_or(Path::new("."));
    let mut file = NamedTempFile::new_in(directory)?;
    file.write_all(text.as_bytes())?;
    file.as_file()
        .set_permissions(fs::metadata(target)?.permissions())?;
    file.persist(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn writing_replaces_the_file_in_place_with_its_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        write(&target, "new").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
