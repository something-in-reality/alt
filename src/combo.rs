//! Key combinations: modifiers and a physical key, written `ctrl+shift+Tab`.

use std::{fmt, str::FromStr};

use bitflags::bitflags;
use keycode::KeyMappingCode;
use serde_with::DeserializeFromStr;

bitflags! {
    /// Modifier keys, named as in sway.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Modifiers: u8 {
        const CTRL = 1;
        const ALT = 1 << 1;
        const SHIFT = 1 << 2;
        const SUPER = 1 << 3;
    }
}

/// A physical key, by its W3C `KeyboardEvent.code` name, which is independent of the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Code(KeyMappingCode),
    /// The dictation key, which has no W3C code.
    Dictation,
}

/// A key pressed with a set of modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, DeserializeFromStr)]
pub struct Combo {
    pub modifiers: Modifiers,
    pub key: Key,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseComboError {
    #[error("unknown modifier `{0}`; expected ctrl, alt, shift or super")]
    Modifier(String),
    #[error("unknown key `{0}`; expected a W3C key code such as KeyA, Home or Backquote")]
    Key(String),
}

impl FromStr for Key {
    type Err = ParseComboError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dictation" => Ok(Self::Dictation),
            _ => s
                .parse()
                .map(Self::Code)
                .map_err(|_| ParseComboError::Key(s.to_owned())),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(code) => code.fmt(f),
            Self::Dictation => f.write_str("Dictation"),
        }
    }
}

impl FromStr for Modifiers {
    type Err = ParseComboError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_name(&s.to_ascii_uppercase())
            .ok_or_else(|| ParseComboError::Modifier(s.to_owned()))
    }
}

impl FromStr for Combo {
    type Err = ParseComboError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (modifiers, key) = s.rsplit_once('+').unwrap_or(("", s));
        Ok(Self {
            modifiers: modifiers
                .split('+')
                .filter(|name| !name.is_empty())
                .map(str::parse)
                .collect::<Result<_, _>>()?,
            key: key.parse()?,
        })
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, _) in self.modifiers.iter_names() {
            write!(f, "{}+", name.to_ascii_lowercase())?;
        }
        self.key.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for s in [
            "Home",
            "ctrl+shift+Tab",
            "super+Backquote",
            "alt+KeyB",
            "Dictation",
            "F3",
        ] {
            assert_eq!(s.parse::<Combo>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn modifiers_are_case_insensitive_and_canonical() {
        let combo: Combo = "Shift+CTRL+Tab".parse().unwrap();
        assert_eq!(combo.to_string(), "ctrl+shift+Tab");
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(matches!(
            "hyper+KeyA".parse::<Combo>(),
            Err(ParseComboError::Modifier(_))
        ));
        assert!(matches!(
            "ctrl+t".parse::<Combo>(),
            Err(ParseComboError::Key(_))
        ));
    }
}
