//! The configuration file: `[shortcuts]`, `[mouse]`, `[displays]`, and a table for each application
//! by bundle identifier.

pub mod edit;
pub mod watch;

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt, fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings, json_schema};
use serde::{
    Deserialize, Deserializer,
    de::{self, IntoDeserializer, MapAccess},
};
use serde_untagged::UntaggedEnumVisitor;
use serde_with::DeserializeFromStr;

use crate::{
    action::{BundleId, Command},
    combo::{Combo, Shortcut},
};

/// `$XDG_CONFIG_HOME/alt/config.toml`: shortcuts and settings for every application, and a table per application by bundle identifier.
#[derive(Debug, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Shortcuts in every application.
    pub shortcuts: Shortcuts,
    /// System Settings > Mouse.
    pub mouse: Mouse,
    /// System Settings > Displays.
    pub displays: Displays,
    /// Applications by bundle identifier, which the schema describes as pattern properties.
    #[serde(flatten)]
    pub apps: BTreeMap<BundleId, App>,
}

impl<'de> Deserialize<'de> for Config {
    /// Reads sections by name and applications by bundle identifier: section names have no dot,
    /// and bundle identifiers always do.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Config;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("sections and applications' tables")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Config, A::Error> {
                let mut config = Config::default();
                while let Some(section) = map.next_key()? {
                    match section {
                        Section::Shortcuts => config.shortcuts = map.next_value()?,
                        Section::Mouse => config.mouse = map.next_value()?,
                        Section::Displays => config.displays = map.next_value()?,
                        Section::App(app) => _ = config.apps.insert(app, map.next_value()?),
                    }
                }
                Ok(config)
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

/// A root key: a section by name, or an application by bundle identifier.
#[derive(DeserializeFromStr)]
enum Section {
    Shortcuts,
    Mouse,
    Displays,
    App(BundleId),
}

#[derive(Debug, thiserror::Error)]
#[error(
    "unknown section `{0}`; expected shortcuts, mouse, displays, or a quoted bundle identifier such as [\"com.apple.Safari\"]"
)]
struct UnknownSection(String);

impl FromStr for Section {
    type Err = UnknownSection;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "shortcuts" => Ok(Self::Shortcuts),
            "mouse" => Ok(Self::Mouse),
            "displays" => Ok(Self::Displays),
            other => other
                .parse()
                .map(Self::App)
                .map_err(|_| UnknownSection(other.to_owned())),
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Mouse {
    /// Natural scrolling for mouse wheels; trackpads and continuous scrolling follow System Settings. Unset follows System Settings.
    pub natural_scrolling: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Displays {
    /// External displays follow the built-in display's brightness, over DDC/CI.
    pub sync_brightness: bool,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct App {
    /// Runs when the application launches or is running when alt starts; window actions act on its main window.
    pub on_launch: Option<Command>,
    /// Shortcuts while the application is in front, over the global ones.
    pub shortcuts: Shortcuts,
}

/// Bindings by shortcut. A shortcut has one spelling, so TOML's duplicate key error covers a
/// repeated shortcut.
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Shortcuts(HashMap<Shortcut, Binding>);

impl Shortcuts {
    pub fn get(&self, combo: &Combo) -> Option<&Binding> {
        self.0.get(combo)
    }
}

impl JsonSchema for Shortcuts {
    fn schema_name() -> Cow<'static, str> {
        "Shortcuts".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "Bindings by shortcut.",
            "type": "object",
            "propertyNames": generator.subschema_for::<Shortcut>(),
            "additionalProperties": generator.subschema_for::<Binding>()
        })
    }
}

/// A shortcut's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    /// Runs the command and drops the key; `[]` runs nothing.
    Run(Command),
    /// Written `false`: the key reaches the application.
    Unbound,
}

impl JsonSchema for Binding {
    fn schema_name() -> Cow<'static, str> {
        "Binding".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A command, which runs and drops the key, or false, which passes the key to the application.",
            "anyOf": [
                generator.subschema_for::<Command>(),
                { "type": "boolean", "enum": [false] }
            ]
        })
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const EXPECTED: &str = "an action, a list of actions, or false";
        UntaggedEnumVisitor::new()
            .expecting(EXPECTED)
            .bool(|bound| match bound {
                false => Ok(Self::Unbound),
                true => Err(de::Error::custom(format_args!("expected {EXPECTED}"))),
            })
            .string(|verb| Command::deserialize(verb.into_deserializer()).map(Self::Run))
            .map(|action| action.deserialize().map(Self::Run))
            .seq(|actions| actions.deserialize().map(Self::Run))
            .deserialize(deserializer)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {}: {error}", path.display())]
    Read { path: PathBuf, error: io::Error },
    #[error("{}: {error}", path.display())]
    Parse {
        path: PathBuf,
        error: toml::de::Error,
    },
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|error| Error::Read {
            path: path.to_owned(),
            error,
        })?;
        text.parse().map_err(|error| Error::Parse {
            path: path.to_owned(),
            error,
        })
    }

    /// The JSON Schema of the configuration file, draft-07 for editors' TOML language servers.
    pub fn schema() -> Schema {
        SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Self>()
    }

    /// The command that `combo` runs while `front` is in front: the application's binding, else
    /// the global one; `None` passes the key.
    pub fn shortcut(&self, front: Option<&BundleId>, combo: &Combo) -> Option<&Command> {
        front
            .and_then(|app| self.apps.get(app))
            .and_then(|app| app.shortcuts.get(combo))
            .or_else(|| self.shortcuts.get(combo))
            .and_then(|binding| match binding {
                Binding::Run(command) => Some(command),
                Binding::Unbound => None,
            })
    }
}

impl FromStr for Config {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s)
    }
}
