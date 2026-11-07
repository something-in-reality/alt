//! Key combinations: modifiers and a physical key joined by `-` in lower case, with the modifiers
//! in Apple's order: `ctrl-alt-shift-cmd-key`. Keys are W3C `KeyboardEvent.code`s in snake_case,
//! letters and digits by their character: `b`, `1`, `page_up`, `f3`, `backquote`.

use std::{borrow::Cow, fmt, str::FromStr};

use bitflags::bitflags;
use keycode::KeyMappingCode;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde_with::DeserializeFromStr;

bitflags! {
    /// Modifier keys, in Apple's order: Control, Option, Shift, Command.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Modifiers: u8 {
        const CTRL = 1;
        const ALT = 1 << 1;
        const SHIFT = 1 << 2;
        const CMD = 1 << 3;
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

/// The modifiers of a combination's pattern, each at most once and in order.
const MODIFIERS_PATTERN: &str = "(ctrl-)?(alt-)?(shift-)?(cmd-)?";

impl JsonSchema for Combo {
    fn schema_name() -> Cow<'static, str> {
        "Combo".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Modifiers (ctrl, alt, shift, cmd, in that order) and a key, joined by `-`. Keys are W3C KeyboardEvent.code names in snake case, letters and digits by character: b, 1, page_up, f3, backquote.",
            "pattern": format!("^{MODIFIERS_PATTERN}[a-z0-9_]+$"),
            "examples": ["f6", "cmd-home", "ctrl-shift-tab"]
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseComboError {
    #[error("unknown modifier `{0}`; expected ctrl, alt, shift or cmd in lower case")]
    Modifier(String),
    #[error(
        "unknown key `{0}`; expected a lower case key such as a, 1, home, page_up or backquote"
    )]
    Key(String),
    #[error("modifiers go in the order ctrl, alt, shift, cmd: write `{0}`")]
    Order(Combo),
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
        let (names, key) = s
            .rsplit_once('-')
            .map_or((None, s), |(names, key)| (Some(names), key));
        let mut modifiers = Modifiers::empty();
        let mut ordered = true;
        for name in names.into_iter().flat_map(|names| names.split('-')) {
            let modifier: Modifiers = name.parse()?;
            ordered &= modifiers.bits() < modifier.bits();
            modifiers |= modifier;
        }
        let combo = Self {
            modifiers,
            key: key.parse()?,
        };
        if ordered {
            Ok(combo)
        } else {
            Err(ParseComboError::Order(combo))
        }
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, _) in self.modifiers.iter_names() {
            write!(f, "{}-", name.to_ascii_lowercase())?;
        }
        self.key.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANONICAL: [&str; 9] = [
        "home",
        "ctrl-shift-tab",
        "cmd-backquote",
        "alt-b",
        "1",
        "page_up",
        "audio_volume_mute",
        "dictation",
        "f10",
    ];

    #[test]
    fn round_trips() {
        for s in CANONICAL.into_iter().chain(["ctrl-alt-shift-cmd-a"]) {
            assert_eq!(s.parse::<Combo>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn modifiers_parse_in_apple_order_only() {
        let valid = ["ctrl-alt-shift-cmd-a", "alt-cmd-a", "ctrl-shift-tab"];
        let invalid = [
            "shift-ctrl-tab",
            "cmd-ctrl-a",
            "ctrl-ctrl-a",
            "ctrl-alt-ctrl-a",
        ];
        for s in valid {
            assert!(s.parse::<Combo>().is_ok(), "{s}");
        }
        for s in invalid {
            assert!(
                matches!(s.parse::<Combo>(), Err(ParseComboError::Order(_))),
                "{s}"
            );
        }
        assert_eq!(
            "shift-ctrl-tab".parse::<Combo>().unwrap_err().to_string(),
            "modifiers go in the order ctrl, alt, shift, cmd: write `ctrl-shift-tab`"
        );
    }

    #[test]
    fn parser_and_schema_agree_on_syntax() {
        let schema = schemars::schema_for!(Combo).to_value();
        let combo = jsonschema::validator_for(&schema).unwrap();
        for (s, valid) in [
            ("a", true),
            ("ctrl-a", true),
            ("ctrl-alt-shift-cmd-page_up", true),
            ("alt-cmd-1", true),
            ("-a", false),
            ("ctrl--a", false),
            ("ctrl-", false),
            ("-", false),
            ("", false),
            ("shift-ctrl-a", false),
            ("ctrl-ctrl-a", false),
            ("hyper-a", false),
            ("super-a", false),
            ("Ctrl-a", false),
            ("ctrl-A", false),
            ("ctrl+a", false),
            ("ctrl a", false),
        ] {
            assert_eq!(s.parse::<Combo>().is_ok(), valid, "combo parser: {s:?}");
            assert_eq!(combo.is_valid(&s.into()), valid, "combo schema: {s:?}");
        }
        for s in CANONICAL {
            assert!(combo.is_valid(&s.into()), "{s}");
        }
    }

    #[test]
    fn rejects_upper_case() {
        for s in [
            "Shift-CTRL-tab",
            "ctrl-Tab",
            "ctrl-TAB",
            "ctrl-Page_Up",
            "A",
        ] {
            assert!(s.parse::<Combo>().is_err(), "{s}");
        }
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(matches!(
            "hyper-KeyA".parse::<Combo>(),
            Err(ParseComboError::Modifier(_))
        ));
        let error = "ctrl-tee".parse::<Combo>().unwrap_err();
        assert_eq!(error, ParseComboError::Key("tee".to_owned()));
        assert_eq!(
            error.to_string(),
            "unknown key `tee`; expected a lower case key such as a, 1, home, page_up or backquote"
        );
    }
}
