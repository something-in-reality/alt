//! The app's files under the XDG base directories, named after the package.

use std::path::PathBuf;

use xdg::BaseDirectories;

const NAME: &str = env!("CARGO_PKG_NAME");

/// $XDG_CONFIG_HOME/alt/config.toml.
pub fn config_file() -> Option<PathBuf> {
    BaseDirectories::with_prefix(NAME).get_config_file("config.toml")
}

/// The first alt/glove80.uf2 in $XDG_DATA_HOME, then $XDG_DATA_DIRS.
pub fn firmware() -> Option<PathBuf> {
    BaseDirectories::with_prefix(NAME).find_data_file("glove80.uf2")
}
