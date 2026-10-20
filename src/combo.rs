//! Key combinations: modifiers and a physical key, written `ctrl+shift+tab`, in lower case. Keys
//! are W3C `KeyboardEvent.code`s in snake_case, letters and digits by their character: `b`, `1`,
//! `page_up`, `f3`, `backquote`.

use std::{borrow::Cow, fmt, str::FromStr};

use bitflags::bitflags;
use keycode::KeyMappingCode;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
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

/// A physical key, independent of the layout.
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

impl JsonSchema for Combo {
    fn schema_name() -> Cow<'static, str> {
        "Combo".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Modifiers (ctrl, alt, shift, super) and a key, joined by `+`. Keys are W3C KeyboardEvent.code names in snake case, letters and digits by character: b, 1, page_up, f3, backquote.",
            "pattern": r"^((ctrl|alt|shift|super)\+)*[a-z0-9_]+$",
            "examples": ["f6", "super+home", "ctrl+shift+tab"]
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseComboError {
    #[error("unknown modifier `{0}`; expected ctrl, alt, shift or super in lower case")]
    Modifier(String),
    #[error(
        "unknown key `{0}`; expected a lower case key such as a, 1, home, page_up or backquote"
    )]
    Key(String),
}

impl FromStr for Key {
    type Err = ParseComboError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let code = match s.as_bytes() {
            b"dictation" => return Ok(Self::Dictation),
            [letter] if letter.is_ascii_lowercase() => {
                format!("Key{}", letter.to_ascii_uppercase() as char)
            }
            [digit] if digit.is_ascii_digit() => format!("Digit{}", *digit as char),
            _ => s.split('_').map(capitalised).collect(),
        };
        code.parse()
            .map(Self::Code)
            .ok()
            .filter(|key| key.to_string() == s)
            .ok_or_else(|| ParseComboError::Key(s.to_owned()))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Code(code) = self else {
            return f.write_str("dictation");
        };
        let code = code.to_string();
        match (code.strip_prefix("Key"), code.strip_prefix("Digit")) {
            (Some(letter), _) if letter.len() == 1 => f.write_str(&letter.to_ascii_lowercase()),
            (_, Some(digit)) if digit.len() == 1 => f.write_str(digit),
            _ => code.chars().enumerate().try_for_each(|(i, c)| {
                if c.is_ascii_uppercase() && i > 0 {
                    f.write_str("_")?;
                }
                write!(f, "{}", c.to_ascii_lowercase())
            }),
        }
    }
}

/// `word` with its first letter in upper case.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl FromStr for Modifiers {
    type Err = ParseComboError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_name(&s.to_ascii_uppercase())
            .filter(|_| !s.bytes().any(|byte| byte.is_ascii_uppercase()))
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

    const CANONICAL: [&str; 9] = [
        "home",
        "ctrl+shift+tab",
        "super+backquote",
        "alt+b",
        "1",
        "page_up",
        "audio_volume_mute",
        "dictation",
        "f10",
    ];

    #[test]
    fn round_trips() {
        for s in CANONICAL {
            assert_eq!(s.parse::<Combo>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn canonical_combos_match_the_schema() {
        let schema = schemars::schema_for!(Combo).to_value();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for s in CANONICAL {
            assert!(validator.is_valid(&s.into()), "{s}");
        }
        assert!(!validator.is_valid(&"ctrl+".into()));
    }

    #[test]
    fn rejects_upper_case() {
        for s in [
            "Shift+CTRL+tab",
            "ctrl+Tab",
            "ctrl+TAB",
            "ctrl+Page_Up",
            "A",
        ] {
            assert!(s.parse::<Combo>().is_err(), "{s}");
        }
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(matches!(
            "hyper+KeyA".parse::<Combo>(),
            Err(ParseComboError::Modifier(_))
        ));
        assert!(matches!(
            "ctrl+tee".parse::<Combo>(),
            Err(ParseComboError::Key(_))
        ));
    }
}
