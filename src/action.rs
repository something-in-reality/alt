//! Actions: a verb written `"fill"`, or `{ tile = "left" }` when the verb takes an argument. A
//! command is one action or a list of actions that run in order.

use std::{borrow::Cow, str::FromStr, sync::Arc};

use derive_more::{AsRef, Display, From};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, de::IntoDeserializer};
use serde_untagged::UntaggedEnumVisitor;
use serde_with::DeserializeFromStr;

use crate::combo::Combo;

/// An action: a verb, or `{ verb = argument }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Presses a combination as a keyboard does; media and brightness keys post the system key.
    Press(Combo),
    /// Types text.
    Type(String),
    /// Opens an application: launches it, or brings it to the front.
    Open(BundleId),
    /// Force-quits a running application.
    ForceQuit(BundleId),
    /// Window > Enter Full Screen.
    FullScreen,
    /// Window > Full Screen Tile: puts the window in one half of a full-screen split.
    FullScreenTile(Side),
    /// Window > Move & Resize: moves and resizes the window to a half or quarter of its display.
    Tile(Tile),
    /// Window > Fill: fills the window's display.
    Fill,
    /// Window > Center: centres the window on its display.
    Center,
    /// Window > Move to: moves the window to a display.
    MoveTo(Screen),
    /// Changes the window's size by a signed amount in points; in split view, moves the divider.
    Resize(Size),
    /// Connects a paired Bluetooth device.
    Connect(Device),
    /// Disconnects a paired Bluetooth device.
    Disconnect(Device),
    /// Connects a paired Bluetooth device when disconnected, otherwise disconnects it.
    ToggleConnection(Device),
    /// Runs a command with `sh -c`.
    Exec(String),
    /// Shows a notification of a `strftime` template filled with the local time.
    Notify(Template),
}

/// A half of a display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

/// A half or quarter of a display, as titled in Window > Move & Resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "title_case")]
pub enum Tile {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A display of the Window menu's "Move to" items: `built_in`; `next`, the first of them;
/// `previous`, the last; or a display's name.
#[derive(Clone, Debug, PartialEq, Eq, DeserializeFromStr, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Screen {
    BuiltIn,
    Next,
    Previous,
    #[strum(default)]
    Named(String),
}

impl JsonSchema for Screen {
    fn schema_name() -> Cow<'static, str> {
        "Screen".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A display of the Window menu's \"Move to\" items: `built_in`, `next` (the first), `previous` (the last), or a display's name.",
            "anyOf": [
                { "enum": ["built_in", "next", "previous"] },
                { "type": "string" }
            ]
        })
    }
}

/// A signed change in width and height, in points; an omitted side stays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

/// A paired Bluetooth device, by name or address.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema, AsRef, Display)]
#[serde(transparent)]
pub struct Device(#[as_ref(str)] String);

/// An application's bundle identifier, in reverse-DNS form: `org.mozilla.firefox`.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, DeserializeFromStr, AsRef, Display,
)]
pub struct BundleId(#[as_ref(str)] Arc<str>);

/// The text of a malformed bundle identifier.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("expected a bundle identifier such as org.mozilla.firefox")]
pub struct ParseBundleIdError;

impl FromStr for BundleId {
    type Err = ParseBundleIdError;

    /// Accepts two or more dot-separated parts of letters, digits and `-`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let part = |part: &str| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        };
        if s.contains('.') && s.split('.').all(part) {
            Ok(Self(s.into()))
        } else {
            Err(ParseBundleIdError)
        }
    }
}

impl JsonSchema for BundleId {
    fn schema_name() -> Cow<'static, str> {
        "BundleId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "An application's bundle identifier, in reverse-DNS form.",
            "pattern": r"^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$",
            "examples": ["org.mozilla.firefox", "com.apple.Safari"]
        })
    }
}

/// A `strftime` template, such as `%H:%M`; text without fields shows as written.
#[derive(Clone, Debug, PartialEq, Eq, DeserializeFromStr, JsonSchema, AsRef)]
pub struct Template(#[as_ref(str)] String);

impl Template {
    /// The template filled with `time`.
    pub fn format(&self, time: &Zoned) -> Result<String, jiff::Error> {
        jiff::fmt::strtime::format(&self.0, time)
    }
}

impl FromStr for Template {
    type Err = jiff::Error;

    /// Accepts a template that formats the Unix epoch in UTC.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let template = Self(s.to_owned());
        template.format(&Timestamp::UNIX_EPOCH.to_zoned(TimeZone::UTC))?;
        Ok(template)
    }
}

impl Action {
    /// Whether a key's autorepeat reruns the action.
    pub fn repeats(&self) -> bool {
        matches!(self, Self::Press(_) | Self::Type(_) | Self::Resize(_))
    }
}

/// One action or a sequence, run in order; cloning shares the actions.
#[derive(Clone, Debug, PartialEq, Eq, AsRef, From)]
#[from(forward)]
pub struct Command(#[as_ref([Action])] Arc<[Action]>);

impl Command {
    /// Whether a held key repeats the command: every action repeats.
    pub fn repeats(&self) -> bool {
        self.0.iter().all(Action::repeats)
    }
}

impl<'a> IntoIterator for &'a Command {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl JsonSchema for Command {
    fn schema_name() -> Cow<'static, str> {
        "Command".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let action = generator.subschema_for::<Action>();
        json_schema!({
            "description": "An action, or a list of actions run in order.",
            "anyOf": [
                action,
                { "type": "array", "items": action }
            ]
        })
    }
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UntaggedEnumVisitor::new()
            .expecting("an action or a list of actions")
            .string(|verb| Action::deserialize(verb.into_deserializer()).map(|a| Self::from([a])))
            .map(|action| action.deserialize::<Action>().map(|a| Self::from([a])))
            .seq(|actions| actions.deserialize::<Vec<Action>>().map(Self::from))
            .deserialize(deserializer)
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

    fn error(toml: &str) -> String {
        command(toml).unwrap_err().message().to_owned()
    }

    #[test]
    fn verbs_read_as_strings_or_tables() {
        assert_eq!(command(r#""fill""#).unwrap(), [Action::Fill]);
        assert_eq!(
            command("{ resize = { width = -100 } }").unwrap(),
            [Action::Resize(Size {
                width: -100,
                height: 0
            })]
        );
        assert_eq!(
            command(r#"["full_screen", { type = "✓" }]"#).unwrap(),
            [Action::FullScreen, Action::Type("✓".to_owned())]
        );
        assert_eq!(command("[]").unwrap(), []);
    }

    #[test]
    fn arguments_read_their_targets() {
        assert_eq!(
            command(r#"[{ full_screen_tile = "right" }, { tile = "left" }, { tile = "bottom_right" }, "center", { move_to = "MSI MP341CQ" }, { move_to = "built_in" }]"#)
                .unwrap(),
            [
                Action::FullScreenTile(Side::Right),
                Action::Tile(Tile::Left),
                Action::Tile(Tile::BottomRight),
                Action::Center,
                Action::MoveTo(Screen::Named("MSI MP341CQ".to_owned())),
                Action::MoveTo(Screen::BuiltIn),
            ]
        );
        assert_eq!(
            command(r#"[{ press = "ctrl-shift-tab" }, { open = "org.mozilla.firefox" }, { force_quit = "com.apple.Music" }, { toggle_connection = "f8-8f-07-40-54-34" }]"#)
                .unwrap()
                .len(),
            4
        );
        assert_eq!(Tile::TopLeft.to_string(), "Top Left");
        assert_eq!(Tile::Bottom.to_string(), "Bottom");
        for screen in ["built_in", "next", "MSI MP341CQ"] {
            assert_eq!(screen.parse::<Screen>().unwrap().to_string(), screen);
        }
    }

    #[test]
    fn only_incremental_actions_repeat() {
        let repeats = |toml| command(toml).unwrap().iter().all(Action::repeats);
        assert!(repeats(r#"{ press = "a" }"#));
        assert!(repeats(r#"{ type = "a" }"#));
        assert!(repeats("{ resize = { width = 1 } }"));
        assert!(!repeats(r#"{ tile = "right" }"#));
        assert!(!repeats(r#"{ open = "org.mozilla.firefox" }"#));
        assert!(Command::from([Action::Press("a".parse().unwrap())]).repeats());
        assert!(!Command::from([Action::Press("a".parse().unwrap()), Action::Fill]).repeats());
    }

    #[test]
    fn notify_templates_are_checked_when_parsed() {
        assert_eq!(
            command(r#"{ notify = "%H:%M" }"#).unwrap(),
            [Action::Notify("%H:%M".parse().unwrap())]
        );
        for valid in ["%H:%M · %a %-d %b", "%Z %z", "no fields", ""] {
            assert!(
                command(&format!("{{ notify = {valid:?} }}")).is_ok(),
                "{valid}"
            );
        }
        for invalid in ["%", "100%", "%H:%", "%!", "%-"] {
            let message = error(&format!("{{ notify = {invalid:?} }}"));
            assert!(message.contains("strftime"), "{invalid}: {message}");
        }
    }

    #[test]
    fn errors_name_the_problem() {
        assert!(
            error(r#""louder""#)
                .contains("unknown variant `louder`, expected one of `press`, `type`, `open`")
        );
        assert!(error(r#"{ tlie = "left" }"#).contains("unknown variant `tlie`"));
        assert!(
            error(r#"{ tile = "middle" }"#)
                .contains("unknown variant `middle`, expected one of `left`")
        );
        assert!(
            error(r#"{ open = "Firefox" }"#)
                .contains("expected a bundle identifier such as org.mozilla.firefox")
        );
        assert!(error("{ resize = { depth = 100 } }").contains("unknown field `depth`"));
        assert!(error("1").contains("expected an action or a list of actions"));
        assert!(error(r#""tile""#).contains("expected newtype variant"));
        assert!(error(r#"{ fill = "left" }"#).contains("expected unit"));
    }

    #[test]
    fn bundle_identifiers_are_reverse_dns() {
        for valid in [
            "org.mozilla.firefox",
            "com.apple.Safari",
            "a.b",
            "my-org.app-1",
        ] {
            assert!(valid.parse::<BundleId>().is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "Firefox",
            "org",
            ".org.app",
            "org..app",
            "org.app.",
            "org.app!",
            "org. app",
            "org.mozilla.fire_fox",
        ] {
            assert!(invalid.parse::<BundleId>().is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn bundle_identifiers_agree_with_the_schema() {
        let schema = BundleId::json_schema(&mut SchemaGenerator::default()).to_value();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for s in [
            "org.mozilla.firefox",
            "com.apple.Safari",
            "a.b",
            "my-org.app-1",
            "",
            "Firefox",
            "org",
            ".org.app",
            "org..app",
            "org.app.",
            "org.app!",
            "org. app",
            "org.fire_fox",
            "org.app\n",
        ] {
            assert_eq!(
                s.parse::<BundleId>().is_ok(),
                validator.is_valid(&s.into()),
                "{s:?}"
            );
        }
    }
}
