//! The configuration file, in sway's section names.

pub mod edit;
pub mod watch;

use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use schemars::{JsonSchema, Schema, generate::SchemaSettings};
use serde::Deserialize;
use serde_with::{MapPreventDuplicates, serde_as};

use crate::{
    action::{AppId, Command},
    combo::Combo,
};

/// The configuration, `$XDG_CONFIG_HOME/alt/config.toml`.
#[serde_as]
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Input devices.
    pub input: Input,
    /// Display settings.
    pub display: Display,
    /// Bindings in every application.
    #[serde_as(deserialize_as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
    /// Bindings that apply while an application is in front, over those in `bindsym`.
    pub app_id: BTreeMap<AppId, App>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    /// Mice.
    pub pointer: Pointer,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Pointer {
    /// Scroll direction of mice; unset follows the system setting.
    pub natural_scroll: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(rename = "DisplayConfig")]
pub struct Display {
    /// External displays follow the built-in display's brightness.
    pub sync_brightness: bool,
}

#[serde_as]
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(rename = "AppConfig")]
pub struct App {
    #[serde_as(deserialize_as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
    /// Runs when the application launches.
    pub on_launch: Option<Command>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {}: {error}", path.display())]
    Read { path: PathBuf, error: io::Error },
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|error| Error::Read {
            path: path.to_owned(),
            error,
        })?;
        Ok(text.parse()?)
    }

    /// The JSON Schema of the configuration file, draft-07 for editors' TOML language servers.
    pub fn schema() -> Schema {
        SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Self>()
    }

    /// The command bound to `combo`, preferring the front application's binding.
    pub fn bindsym(&self, front: Option<&AppId>, combo: &Combo) -> Option<&Command> {
        front
            .and_then(|app| self.app_id.get(app))
            .and_then(|app| app.bindsym.get(combo))
            .or_else(|| self.bindsym.get(combo))
    }
}

impl FromStr for Config {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, Keyboard, Size, Window};

    const EXAMPLE: &str = include_str!("../config.toml");

    fn command(config: &Config, front: Option<&str>, combo: &str) -> Option<Vec<Action>> {
        let front = front.map(AppId::from);
        config
            .bindsym(front.as_ref(), &combo.parse().unwrap())
            .map(|command| command.as_ref().to_vec())
    }

    fn validates(toml: &str) -> bool {
        let schema = Config::schema().to_value();
        let instance: serde_json::Value = toml::from_str(toml).unwrap();
        jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&instance)
    }

    #[test]
    fn example_matches_the_schema() {
        assert!(validates(EXAMPLE));
        let schema = Config::schema().to_value();
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        for section in ["input", "display", "bindsym", "app_id"] {
            assert!(schema["properties"][section].is_object(), "{section}");
        }
    }

    #[test]
    fn schema_rejects_what_the_parser_rejects() {
        for invalid in [
            "[input.pointer]\nnatural = true",
            "[bindsym]\nf1 = \"media.louder\"",
            "[bindsym]\nf1 = { window.tile = \"middle\" }",
            "[bindsym]\nf1 = { window.resize = { depth = 1 } }",
            "[bindsym]\n\"hyper+a\" = \"media.mute\"",
            "[app_id.\"com.apple.Music\"]\non_launch = 1",
        ] {
            assert!(invalid.parse::<Config>().is_err(), "{invalid}");
            assert!(!validates(invalid), "{invalid}");
        }
        for valid in [
            "[bindsym]\nf1 = []",
            "[bindsym]\nf1 = [\"window.fill\", { window.display = \"Studio Display\" }]",
            "[bindsym]\nf1 = { media = \"mute\" }",
        ] {
            assert!(valid.parse::<Config>().is_ok(), "{valid}");
            assert!(validates(valid), "{valid}");
        }
    }

    #[test]
    fn example_parses() {
        let config: Config = EXAMPLE.parse().unwrap();
        assert_eq!(config.input.pointer.natural_scroll, Some(false));
        assert!(config.display.sync_brightness);
        assert_eq!(
            command(&config, None, "ctrl+alt+enter"),
            Some(vec![Action::Window(Window::Fill)])
        );
    }

    #[test]
    fn front_application_bindings_win() {
        let config: Config = EXAMPLE.parse().unwrap();
        let width = |width| vec![Action::Window(Window::Resize(Size { width, height: 0 }))];
        let (grow, shrink) = (width(100), width(-100));
        assert_eq!(command(&config, None, "super+home"), Some(grow.clone()));
        assert_eq!(
            command(&config, Some("com.mitchellh.ghostty"), "super+home"),
            Some(grow)
        );
        assert_eq!(
            command(&config, Some("org.nixos.firefox"), "super+home"),
            Some(shrink)
        );
    }

    #[test]
    fn application_bindings_stay_in_their_application() {
        let config: Config = EXAMPLE.parse().unwrap();
        assert_eq!(command(&config, None, "page_up"), None);
        assert_eq!(
            command(&config, Some("org.nixos.firefox"), "backquote"),
            Some(vec![Action::Keyboard(Keyboard::Press(
                "ctrl+z".parse().unwrap()
            ))])
        );
    }

    #[test]
    fn sequences_run_in_order() {
        let config: Config =
            "[bindsym]\nf1 = [{ keyboard.press = \"super+l\" }, { keyboard.type = \"x\" }]"
                .parse()
                .unwrap();
        assert_eq!(
            command(&config, None, "f1"),
            Some(vec![
                Action::Keyboard(Keyboard::Press("super+l".parse().unwrap())),
                Action::Keyboard(Keyboard::Type("x".to_owned())),
            ])
        );
    }

    #[test]
    fn rejects_one_combo_bound_twice() {
        let config =
            "[bindsym]\n\"ctrl+shift+tab\" = \"media.mute\"\n\"shift+ctrl+tab\" = \"media.mute\"";
        assert!(config.parse::<Config>().is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!("[input.pointer]\nnatural = true".parse::<Config>().is_err());
    }
}
