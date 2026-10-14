//! The app's files under the XDG base directories, named after the package.

use std::path::PathBuf;

use etcetera::{BaseStrategy, HomeDirError};

const NAME: &str = env!("CARGO_PKG_NAME");

/// $XDG_CONFIG_HOME/alt/config.toml.
pub fn config_file() -> Result<PathBuf, HomeDirError> {
    Ok(etcetera::choose_base_strategy()?
        .config_dir()
        .join(NAME)
        .join("config.toml"))
}

/// $XDG_STATE_HOME/alt, or the data directory on a platform without a state directory.
pub fn state_dir() -> Result<PathBuf, HomeDirError> {
    let strategy = etcetera::choose_base_strategy()?;
    Ok(strategy
        .state_dir()
        .unwrap_or_else(|| strategy.data_dir())
        .join(NAME))
}

/// $XDG_DATA_HOME/alt.
pub fn data_dir() -> Result<PathBuf, HomeDirError> {
    Ok(etcetera::choose_base_strategy()?.data_dir().join(NAME))
}
