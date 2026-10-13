//! The configuration file, in sway's section names.

use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::Path,
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
    pub brightness: Brightness,
    /// Bindings in every application.
    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
    /// Bindings that apply while an application is in front, over those in `bindsym`.
    pub app_id: BTreeMap<AppId, App>,
    /// Commands run when an application launches.
    pub for_window: BTreeMap<AppId, Command>,
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
pub struct Brightness {
    /// External displays follow the built-in display's brightness.
    pub sync: bool,
}

#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct App {
    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub bindsym: HashMap<Combo, Command>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}")]
    Read { path: String, source: io::Error },
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.display().to_string(),
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
    use crate::action::{Action, Dimension, Direction, Px, Resize};

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
        assert!(config.brightness.sync);
    }

    #[test]
    fn front_application_bindings_win() {
        let config: Config = EXAMPLE.parse().unwrap();
        let grow = vec![Action::Resize(Resize::Grow(Dimension::Width(Px(100))))];
        let shrink = vec![Action::Resize(Resize::Shrink(Dimension::Width(Px(100))))];
        assert_eq!(command(&config, None, "super+Home"), Some(grow.clone()));
        assert_eq!(
            command(&config, Some("com.mitchellh.ghostty"), "super+Home"),
            Some(grow)
        );
        assert_eq!(
            command(&config, Some("org.nixos.firefox"), "super+Home"),
            Some(shrink)
        );
    }

    #[test]
    fn application_bindings_stay_in_their_application() {
        let config: Config = EXAMPLE.parse().unwrap();
        assert_eq!(command(&config, None, "PageUp"), None);
        assert_eq!(
            command(&config, Some("org.nixos.firefox"), "super+Backquote"),
            Some(vec![
                Action::Press("super+KeyL".parse().unwrap()),
                Action::Press("alt+Enter".parse().unwrap()),
            ])
        );
        assert_eq!(
            command(&config, Some("org.nixos.firefox"), "F3"),
            Some(vec![Action::Workspace(Direction::Prev)])
        );
    }

    #[test]
    fn rejects_one_combo_bound_twice() {
        let config = "[bindsym]\n\"ctrl+shift+Tab\" = \"kill\"\n\"shift+ctrl+Tab\" = \"kill\"";
        assert!(config.parse::<Config>().is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!("[input.pointer]\nnatural = true".parse::<Config>().is_err());
    }
}
