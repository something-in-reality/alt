//! The daemon's Accessibility permission, as it last recorded it for `setup` and the status report.

use std::{fmt, fs, io, path::PathBuf, str::FromStr};

use crate::paths;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Waiting,
    Granted,
}

impl Permission {
    fn path() -> io::Result<PathBuf> {
        Ok(paths::state_dir()
            .map_err(io::Error::other)?
            .join("permission"))
    }

    pub fn read() -> Option<Self> {
        fs::read_to_string(Self::path().ok()?)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Records the permission; a failure only logs.
    pub fn record(self) {
        let written = Self::path().and_then(|path| {
            fs::create_dir_all(path.parent().unwrap_or(&path))?;
            fs::write(path, self.to_string())
        });
        if let Err(error) = written {
            tracing::warn!(%error, "recording the permission");
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Waiting => "waiting",
            Self::Granted => "granted",
        })
    }
}

impl FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "waiting" => Ok(Self::Waiting),
            "granted" => Ok(Self::Granted),
            _ => Err(format!("unknown permission `{s}`")),
        }
    }
}
