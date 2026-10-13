//! Actions, namespaced by what they act on: `keyboard.press`, `window.width`, `media.mute`.
//!
//! An action is written `"namespace.verb"` when the verb takes nothing, and
//! `{ namespace.verb = argument }` when it does.

use std::{fmt, iter, process, str::FromStr, sync::Arc, thread};

use serde::{
    Deserialize, Deserializer,
    de::{self, IntoDeserializer},
};
use serde_with::DeserializeFromStr;

use crate::{combo::Combo, platform::Desktop};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Runs a command with `sh -c`.
    Exec(String),
    Keyboard(Keyboard),
    App(App),
    Window(Window),
    Bluetooth(Bluetooth),
    Media(Media),
    Brightness(Brightness),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Keyboard {
    Press(Combo),
    Type(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum App {
    /// Brings a running application to the front.
    Focus(AppId),
}

/// Resizes the focused window; in split view this moves the divider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Window {
    Width(Px),
    Height(Px),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Bluetooth {
    Connect(Device),
    Disconnect(Device),
    Toggle(Device),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Mute,
    VolumeUp,
    VolumeDown,
    PlayPause,
    Next,
    Previous,
}

/// Steps the built-in display's brightness; external displays follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Brightness {
    Up,
    Down,
}

/// An application: its bundle identifier on macOS, its `app_id` under Wayland.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct AppId(String);

/// A paired Bluetooth device, by name or address.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Device(String);

/// A signed length in pixels, written `+100px` or `-100px`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DeserializeFromStr)]
pub struct Px(pub i32);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid length `{0}`; expected signed pixels such as +100px")]
pub struct ParsePxError(String);

impl Action {
    /// Performs the action: `exec` here, every other namespace on `desktop`.
    pub fn perform<D: Desktop>(&self, desktop: &D) -> Result<(), D::Error> {
        match self {
            Self::Exec(command) => {
                let mut child = process::Command::new("/bin/sh")
                    .args(["-c", command])
                    .spawn()?;
                thread::spawn(move || child.wait());
                Ok(())
            }
            Self::Keyboard(action) => desktop.keyboard(action),
            Self::App(action) => desktop.app(action),
            Self::Window(action) => desktop.window(*action),
            Self::Bluetooth(action) => desktop.bluetooth(action),
            Self::Media(action) => desktop.media(*action),
            Self::Brightness(action) => desktop.brightness(*action),
        }
    }

    /// Whether a held key repeats the action, as it repeats a typed character.
    pub fn repeats(&self) -> bool {
        matches!(
            self,
            Self::Keyboard(_) | Self::Window(_) | Self::Media(_) | Self::Brightness(_)
        )
    }
}

impl FromStr for Px {
    type Err = ParsePxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_suffix("px")
            .filter(|n| n.starts_with(['+', '-']))
            .and_then(|n| n.parse().ok())
            .map(Self)
            .ok_or_else(|| ParsePxError(s.to_owned()))
    }
}

impl fmt::Display for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:+}px", self.0)
    }
}

impl From<String> for AppId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for AppId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl AsRef<str> for AppId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Device {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One action or a sequence, run in order; cloning shares the actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command(Arc<[Action]>);

impl Command {
    /// Whether a held key repeats the command: every action repeats.
    pub fn repeats(&self) -> bool {
        self.0.iter().all(Action::repeats)
    }
}

impl AsRef<[Action]> for Command {
    fn as_ref(&self) -> &[Action] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a Command {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// One action, written `"namespace.verb"` or as a table; the string reads as
/// `{ namespace = "verb" }`.
struct Step(Action);

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Step;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("`namespace.verb` or { namespace.verb = argument }")
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Step, E> {
                let (namespace, verb) = s
                    .split_once('.')
                    .ok_or_else(|| E::custom(format!("expected `namespace.verb`, found `{s}`")))?;
                let entry = de::value::MapDeserializer::new(iter::once((namespace, verb)));
                Action::deserialize(de::value::MapAccessDeserializer::new(entry)).map(Step)
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Step, A::Error> {
                Action::deserialize(de::value::MapAccessDeserializer::new(map)).map(Step)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl<'de> Deserialize<'de> for Command {
    /// One action or a list; errors keep the field they come from.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Command;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an action or a list of actions")
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Command, E> {
                Step::deserialize(s.into_deserializer())
                    .map(|Step(action)| Command(Arc::new([action])))
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Command, A::Error> {
                Step::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(|Step(action)| Command(Arc::new([action])))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Command, A::Error> {
                Vec::<Step>::deserialize(de::value::SeqAccessDeserializer::new(seq))
                    .map(|steps| Command(steps.into_iter().map(|Step(action)| action).collect()))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(toml: &str) -> Result<Vec<Action>, toml::de::Error> {
        #[derive(Deserialize)]
        struct Wrapper {
            command: Command,
        }
        toml::from_str::<Wrapper>(&format!("command = {toml}"))
            .map(|wrapper| wrapper.command.as_ref().to_vec())
    }

    #[test]
    fn verbs_read_as_strings_or_tables() {
        assert_eq!(
            command(r#""media.mute""#).unwrap(),
            [Action::Media(Media::Mute)]
        );
        assert_eq!(
            command(r#"{ window.width = "+100px" }"#).unwrap(),
            [Action::Window(Window::Width(Px(100)))]
        );
        assert_eq!(
            command(r#"["brightness.up", { keyboard.type = "✓" }]"#).unwrap(),
            [
                Action::Brightness(Brightness::Up),
                Action::Keyboard(Keyboard::Type("✓".to_owned()))
            ]
        );
    }

    #[test]
    fn errors_name_the_problem() {
        let error = command(r#""media.louder""#).unwrap_err().to_string();
        assert!(error.contains("louder"), "{error}");
        assert!(command(r#"{ window.width = "100px" }"#).is_err());
    }

    #[test]
    fn pixels_round_trip() {
        for s in ["+100px", "-40px"] {
            assert_eq!(s.parse::<Px>().unwrap().to_string(), s);
        }
    }
}
