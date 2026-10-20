//! Actions, namespaced by what they act on: `keyboard.press`, `window.resize`, `media.mute`.
//!
//! An action is written `"namespace.verb"` when the verb takes nothing, and
//! `{ namespace.verb = argument }` when it does.

use std::{fmt, io, iter, process, sync::Arc, thread};

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
    /// Shows a notification of a `strftime` template filled with the local time, such as
    /// `%H:%M`; text without fields shows as written.
    Notify(String),
    Keyboard(Keyboard),
    App(App),
    Window(Window),
    Bluetooth(Bluetooth),
    Media(Media),
    Display(Display),
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
    /// Brings an application to the front, launching it when it is not running.
    Focus(AppId),
    /// Force-quits a running application.
    Kill(AppId),
}

/// Acts on the focused window, or in an application's `on_launch` command, on its window. Tiling,
/// filling, centring and display moves press items of the application's Window menu.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Window {
    /// Changes the window's size; in split view this moves the divider.
    Resize(Size),
    /// Puts the window in full screen; a window in full screen stays there.
    Fullscreen,
    /// Puts the window in one half of a full-screen split, through the application's Window menu;
    /// macOS then offers the other windows for the other half.
    FullscreenTile(Side),
    /// Moves and resizes the window to a half or quarter of its display, through Window > Move &
    /// Resize.
    Tile(Tile),
    /// Fills the window's display, through Window > Fill.
    Fill,
    /// Centres the window on its display, through Window > Center.
    Center,
    /// Moves the window to a display, through Window > Move to.
    Display(Screen),
}

/// A half of a display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

/// A half or quarter of a display; the display names the Window > Move & Resize item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, strum::Display)]
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

/// A display of the Window menu's "Move to" items: `builtin`; `next`, the first of them;
/// `previous`, the last; or a display's name.
#[derive(Clone, Debug, PartialEq, Eq, DeserializeFromStr, strum::Display, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Screen {
    Builtin,
    Next,
    Previous,
    #[strum(default)]
    Named(String),
}

/// A signed change in width and height, in points; an omitted side stays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Size {
    pub width: i32,
    pub height: i32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    /// Steps the built-in display's brightness; external displays follow when synced.
    BrightnessUp,
    BrightnessDown,
}

/// An application: its bundle identifier on macOS, its `app_id` under Wayland.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct AppId(Arc<str>);

/// A paired Bluetooth device, by name or address.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Device(String);

impl Action {
    /// Performs the action: `exec` here, every other namespace on `desktop`, to which `notify`
    /// passes its filled template.
    pub fn perform<D: Desktop>(&self, desktop: &D) -> Result<(), D::Error> {
        match self {
            Self::Exec(command) => {
                let mut child = process::Command::new("/bin/sh")
                    .args(["-c", command])
                    .spawn()?;
                thread::spawn(move || child.wait());
                Ok(())
            }
            Self::Notify(template) => desktop.notify(
                &jiff::fmt::strtime::format(template, &jiff::Zoned::now())
                    .map_err(io::Error::other)?,
            ),
            Self::Keyboard(action) => desktop.keyboard(action),
            Self::App(action) => desktop.app(action),
            Self::Window(action) => desktop.window(action),
            Self::Bluetooth(action) => desktop.bluetooth(action),
            Self::Media(action) => desktop.media(*action),
            Self::Display(action) => desktop.display(*action),
        }
    }

    /// Whether a key's autorepeat reruns the action.
    pub fn repeats(&self) -> bool {
        matches!(
            self,
            Self::Keyboard(_) | Self::Window(Window::Resize(_)) | Self::Media(_) | Self::Display(_)
        )
    }
}

impl From<String> for AppId {
    fn from(id: String) -> Self {
        Self(id.into())
    }
}

impl From<&str> for AppId {
    fn from(id: &str) -> Self {
        Self(id.into())
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
    /// Reads one action or a list of actions, with each error at the field it comes from.
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
            command("{ window.resize = { width = -100 } }").unwrap(),
            [Action::Window(Window::Resize(Size {
                width: -100,
                height: 0
            }))]
        );
        assert_eq!(
            command(r#"["display.brightness_up", { keyboard.type = "✓" }]"#).unwrap(),
            [
                Action::Display(Display::BrightnessUp),
                Action::Keyboard(Keyboard::Type("✓".to_owned()))
            ]
        );
    }

    #[test]
    fn window_actions_read_their_targets() {
        assert_eq!(
            command(r#"["window.fullscreen", { window.fullscreen_tile = "right" }, { window.tile = "left" }, { window.tile = "bottom_right" }, "window.fill", "window.center", { window.display = "MSI MP341CQ" }]"#)
                .unwrap(),
            [
                Action::Window(Window::Fullscreen),
                Action::Window(Window::FullscreenTile(Side::Right)),
                Action::Window(Window::Tile(Tile::Left)),
                Action::Window(Window::Tile(Tile::BottomRight)),
                Action::Window(Window::Fill),
                Action::Window(Window::Center),
                Action::Window(Window::Display(Screen::Named("MSI MP341CQ".to_owned()))),
            ]
        );
        assert_eq!(
            command(r#"{ window.display = "builtin" }"#).unwrap(),
            [Action::Window(Window::Display(Screen::Builtin))]
        );
        assert!(!Action::Window(Window::Tile(Tile::Right)).repeats());
        assert_eq!(
            command(r#"{ notify = "%H:%M" }"#).unwrap(),
            [Action::Notify("%H:%M".to_owned())]
        );
        assert_eq!(Tile::TopLeft.to_string(), "Top Left");
        assert_eq!(Tile::Bottom.to_string(), "Bottom");
        for screen in ["builtin", "next", "MSI MP341CQ"] {
            assert_eq!(screen.parse::<Screen>().unwrap().to_string(), screen);
        }
    }

    #[test]
    fn errors_name_the_problem() {
        let error = command(r#""media.louder""#).unwrap_err().to_string();
        assert!(error.contains("louder"), "{error}");
        assert!(command("{ window.resize = { depth = 100 } }").is_err());
    }
}
