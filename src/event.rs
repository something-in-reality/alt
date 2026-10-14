//! Input events, and how the configuration decides them.

use std::fmt;

use crate::{
    action::{AppId, Command},
    combo::Combo,
    config::Config,
};

/// Input from the platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Press {
        combo: Combo,
        repeat: bool,
    },
    /// Scrolling, which is continuous from trackpads and discrete from mouse wheels.
    Scroll {
        continuous: bool,
    },
    /// A brightness key, after the system has applied it.
    BrightnessKey,
    /// An application is launching, or was running when input began.
    Launch(AppId),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Press {
                combo,
                repeat: false,
            } => combo.fmt(f),
            Self::Press {
                combo,
                repeat: true,
            } => write!(f, "{combo} (repeat)"),
            Self::Scroll { continuous: true } => f.write_str("scroll (continuous)"),
            Self::Scroll { continuous: false } => f.write_str("scroll (discrete)"),
            Self::BrightnessKey => f.write_str("brightness key"),
            Self::Launch(app) => write!(f, "launch {app}"),
        }
    }
}

/// What happens to the event itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Drop,
    /// Pass the scroll with its direction reversed.
    Reverse,
}

/// Work the platform does in response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Runs a bound command; window actions act on the focused window.
    Run(Command),
    /// Runs a launching application's command; window actions act on its window once it opens.
    Launch { app: AppId, command: Command },
    /// External displays follow the built-in display's brightness.
    SyncBrightness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub effect: Option<Effect>,
}

/// The platform's settings that configuration values are compared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct System {
    pub natural_scroll: bool,
}

/// Live platform state, queried only when a decision needs it.
pub trait Context {
    fn front(&self) -> Option<AppId>;
}

impl Decision {
    const PASS: Self = Self {
        verdict: Verdict::Pass,
        effect: None,
    };
}

impl From<Verdict> for Decision {
    fn from(verdict: Verdict) -> Self {
        Self {
            verdict,
            effect: None,
        }
    }
}

impl Config {
    /// Decides an event: a bound press drops the key and runs its command, and an autorepeat
    /// reruns commands that repeat.
    pub fn decide(&self, event: Event, system: &System, cx: &impl Context) -> Decision {
        match event {
            Event::Press { combo, repeat } => match self.bindsym(cx.front().as_ref(), &combo) {
                None => Decision::PASS,
                Some(command) => Decision {
                    verdict: Verdict::Drop,
                    effect: (!repeat || command.repeats()).then(|| Effect::Run(command.clone())),
                },
            },
            Event::Scroll { continuous } => match self.input.pointer.natural_scroll {
                Some(wanted) if !continuous && wanted != system.natural_scroll => {
                    Verdict::Reverse.into()
                }
                _ => Decision::PASS,
            },
            Event::BrightnessKey => Decision {
                effect: self
                    .display
                    .sync_brightness
                    .then_some(Effect::SyncBrightness),
                ..Decision::PASS
            },
            Event::Launch(app) => Decision {
                effect: self
                    .app_id
                    .get(&app)
                    .and_then(|config| config.on_launch.clone())
                    .map(|command| Effect::Launch { app, command }),
                ..Decision::PASS
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    const EXAMPLE: &str = include_str!("../config.toml");
    const NATURAL: System = System {
        natural_scroll: true,
    };
    const FIREFOX: &str = "org.nixos.firefox";
    const GHOSTTY: &str = "com.mitchellh.ghostty";

    /// The front application.
    struct Front(Option<&'static str>);

    impl Context for Front {
        fn front(&self) -> Option<AppId> {
            self.0.map(AppId::from)
        }
    }

    fn config() -> Config {
        EXAMPLE.parse().unwrap()
    }

    fn press(front: &'static str, combo: &str, repeat: bool) -> Decision {
        let event = Event::Press {
            combo: combo.parse().unwrap(),
            repeat,
        };
        config().decide(event, &NATURAL, &Front(Some(front)))
    }

    fn actions(toml: &str) -> Vec<Action> {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            command: Command,
        }
        let wrapper: Wrapper = toml::from_str(&format!("command = {toml}")).unwrap();
        wrapper.command.as_ref().to_vec()
    }

    #[test]
    fn every_trigger_runs_its_command_in_its_application() {
        let triggers = [
            (GHOSTTY, "f1", r#"{ keyboard.press = "dictation" }"#),
            (GHOSTTY, "f4", r#"{ exec = "open -a 'Mission Control'" }"#),
            (GHOSTTY, "f6", r#""media.mute""#),
            (GHOSTTY, "f7", r#""media.volume_down""#),
            (GHOSTTY, "f8", r#""media.volume_up""#),
            (FIREFOX, "f9", r#""display.brightness_down""#),
            (FIREFOX, "f10", r#""display.brightness_up""#),
            (GHOSTTY, "ctrl+t", r#"{ keyboard.type = "✓" }"#),
            (GHOSTTY, "home", r#"{ app.focus = "org.nixos.firefox" }"#),
            (FIREFOX, "end", r#"{ app.focus = "com.mitchellh.ghostty" }"#),
            (
                GHOSTTY,
                "alt+b",
                r#"{ bluetooth.toggle = "f8-8f-07-40-54-34" }"#,
            ),
            (GHOSTTY, "super+home", "{ window.resize = { width = 100 } }"),
            (GHOSTTY, "super+end", "{ window.resize = { width = -100 } }"),
            (
                FIREFOX,
                "super+home",
                "{ window.resize = { width = -100 } }",
            ),
            (FIREFOX, "super+end", "{ window.resize = { width = 100 } }"),
            (
                FIREFOX,
                "page_up",
                r#"{ keyboard.press = "ctrl+shift+tab" }"#,
            ),
            (FIREFOX, "page_down", r#"{ keyboard.press = "ctrl+tab" }"#),
            (FIREFOX, "backquote", r#"{ keyboard.press = "ctrl+z" }"#),
            (
                FIREFOX,
                "ctrl+backquote",
                r#"{ keyboard.press = "ctrl+z" }"#,
            ),
        ];
        for (front, combo, expected) in triggers {
            let decision = press(front, combo, false);
            assert_eq!(decision.verdict, Verdict::Drop, "{combo} in {front}");
            let Some(Effect::Run(command)) = decision.effect else {
                panic!("{combo} in {front} runs nothing");
            };
            assert_eq!(command.as_ref(), actions(expected), "{combo} in {front}");
        }
    }

    #[test]
    fn held_keys_repeat_only_incremental_commands() {
        let resize = press(GHOSTTY, "super+home", true);
        assert!(matches!(resize.effect, Some(Effect::Run(_))));
        assert_eq!(press(GHOSTTY, "home", true), Decision::from(Verdict::Drop));
        assert_eq!(press(GHOSTTY, "alt+b", true), Decision::from(Verdict::Drop));
    }

    #[test]
    fn unbound_keys_pass() {
        assert_eq!(press(GHOSTTY, "page_up", false), Decision::PASS);
        assert_eq!(press(FIREFOX, "a", false), Decision::PASS);
    }

    #[test]
    fn only_discrete_scrolling_against_the_system_reverses() {
        let scroll = |continuous, natural_scroll| {
            config()
                .decide(
                    Event::Scroll { continuous },
                    &System { natural_scroll },
                    &Front(None),
                )
                .verdict
        };
        assert_eq!(scroll(false, true), Verdict::Reverse);
        assert_eq!(scroll(true, true), Verdict::Pass);
        assert_eq!(scroll(false, false), Verdict::Pass);
    }

    #[test]
    fn launches_run_their_application_command() {
        let launch = |app| config().decide(Event::Launch(AppId::from(app)), &NATURAL, &Front(None));
        let Some(Effect::Launch { app, command }) = launch("com.apple.Music").effect else {
            panic!("launching Music runs nothing");
        };
        assert_eq!(
            command.as_ref(),
            actions(r#"{ app.kill = "com.apple.Music" }"#)
        );
        assert_eq!(app, AppId::from("com.apple.Music"));
        let Some(Effect::Launch { app, command }) = launch(FIREFOX).effect else {
            panic!("launching Firefox runs nothing");
        };
        assert_eq!(command.as_ref(), actions(r#""window.fullscreen""#));
        assert_eq!(app, AppId::from(FIREFOX));
        assert_eq!(launch("com.apple.TextEdit"), Decision::PASS);
    }

    #[test]
    fn brightness_syncs_when_configured() {
        let decide = |config: Config| config.decide(Event::BrightnessKey, &NATURAL, &Front(None));
        assert_eq!(decide(config()).effect, Some(Effect::SyncBrightness));
        assert_eq!(decide(Config::default()), Decision::PASS);
    }
}
