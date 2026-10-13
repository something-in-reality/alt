//! The configuration file, in sway's section names.

pub mod edit;
pub mod watch;

use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;
use serde_with::{MapPreventDuplicates, serde_as};

use crate::{
    action::{AppId, Command},
    combo::Combo,
};

#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub input: Input,
    pub display: Display,
    /// Bindings in every application.
    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
    /// Bindings that apply while an application is in front, over those in `bindsym`.
    pub app_id: BTreeMap<AppId, App>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub pointer: Pointer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pointer {
    /// Scroll direction of mice; unset follows the system setting.
    pub natural_scroll: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    /// External displays follow the built-in display's brightness.
    pub sync_brightness: bool,
}

#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct App {
    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
    /// Runs when the application launches.
    pub on_launch: Option<Command>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
        Ok(text.parse()?)
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

    #[test]
    fn example_parses() {
        let config: Config = EXAMPLE.parse().unwrap();
        assert_eq!(config.input.pointer.natural_scroll, Some(false));
        assert!(config.display.sync_brightness);
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
