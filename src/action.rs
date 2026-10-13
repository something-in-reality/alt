//! Actions, in sway's vocabulary where sway has a word for them.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, de};
use serde_with::DeserializeFromStr;

use crate::combo::Combo;

/// An application: its bundle identifier on macOS, its `app_id` under Wayland.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct AppId(pub String);

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Runs a command with `sh -c`.
    Exec(String),
    /// Presses a key combination.
    Key(Combo),
    /// Types text.
    Type(String),
    /// Brings a running application to the front.
    Focus(AppId),
    /// Quits the application the command applies to.
    Kill,
    /// Resizes the focused window.
    Resize(Resize),
    /// Moves to the previous or next workspace.
    Workspace(Direction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Resize {
    Grow(Dimension),
    Shrink(Dimension),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Dimension {
    Width(Px),
    Height(Px),
}

/// A length in pixels, written `100px`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DeserializeFromStr)]
pub struct Px(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Prev,
    Next,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid length `{0}`; expected pixels such as 100px")]
pub struct ParsePxError(String);

impl FromStr for Px {
    type Err = ParsePxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_suffix("px")
            .and_then(|n| n.parse().ok())
            .map(Self)
            .ok_or_else(|| ParsePxError(s.to_owned()))
    }
}

impl fmt::Display for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}px", self.0)
    }
}

/// One action or a sequence, run in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command(pub Vec<Action>);

impl<'de> Deserialize<'de> for Command {
    /// Keeps the inner error, which an untagged enum would replace.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Command;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an action or a list of actions")
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Command, E> {
                Action::deserialize(de::value::StrDeserializer::new(s))
                    .map(|action| Command(vec![action]))
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Command, A::Error> {
                Action::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(|action| Command(vec![action]))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Command, A::Error> {
                Vec::deserialize(de::value::SeqAccessDeserializer::new(seq)).map(Command)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl<'a> IntoIterator for &'a Command {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_round_trip() {
        assert_eq!("100px".parse::<Px>().unwrap().to_string(), "100px");
        assert!("100".parse::<Px>().is_err());
    }
}
