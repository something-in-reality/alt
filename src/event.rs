//! Input events, and how the configuration decides them.

use std::fmt;

use crate::{
    action::{BundleId, Command},
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
    /// An application is launching.
    Launch(BundleId),
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
    /// Runs a bound command; window actions act on the focused window. `repeat` marks a held key's
    /// autorepeat.
    Run { command: Command, repeat: bool },
    /// Runs a launching application's command; window actions act on its window once it opens.
    Launch { app: BundleId, command: Command },
    /// External displays follow the built-in display's brightness.
    SyncBrightness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub effect: Option<Effect>,
}

/// Live platform state, queried only when a decision needs it.
pub trait Context {
    fn front(&self) -> Option<BundleId>;

    /// Whether System Settings has natural scrolling on.
    fn natural_scrolling(&self) -> bool;
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
    pub fn decide(&self, event: Event, cx: &impl Context) -> Decision {
        match event {
            Event::Press { combo, repeat } => match self.shortcut(cx.front().as_ref(), &combo) {
                None => Decision::PASS,
                Some(command) => Decision {
                    verdict: Verdict::Drop,
                    effect: (!repeat || command.repeats()).then(|| Effect::Run {
                        command: command.clone(),
                        repeat,
                    }),
                },
            },
            Event::Scroll { continuous } => match self.mouse.natural_scrolling {
                Some(wanted) if !continuous && wanted != cx.natural_scrolling() => {
                    Verdict::Reverse.into()
                }
                _ => Decision::PASS,
            },
            Event::BrightnessKey => Decision {
                effect: self
                    .displays
                    .sync_brightness
                    .then_some(Effect::SyncBrightness),
                ..Decision::PASS
            },
            Event::Launch(app) => Decision {
                effect: self
                    .apps
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
    const FIREFOX: &str = "org.mozilla.firefox";
    const GHOSTTY: &str = "com.mitchellh.ghostty";

    /// The front application, and System Settings' natural scrolling.
    struct Cx {
        front: Option<&'static str>,
        natural_scrolling: bool,
    }

    impl Cx {
        const fn new(front: Option<&'static str>) -> Self {
            Self {
                front,
                natural_scrolling: true,
            }
        }
    }

    impl Context for Cx {
        fn front(&self) -> Option<BundleId> {
            self.front.map(|front| front.parse().unwrap())
        }

        fn natural_scrolling(&self) -> bool {
            self.natural_scrolling
        }
    }

    fn config() -> Config {
        EXAMPLE.parse().unwrap()
    }

    fn decide(config: &Config, front: &'static str, combo: &str, repeat: bool) -> Decision {
        let event = Event::Press {
            combo: combo.parse().unwrap(),
            repeat,
        };
        config.decide(event, &Cx::new(Some(front)))
    }

    fn press(front: &'static str, combo: &str, repeat: bool) -> Decision {
        decide(&config(), front, combo, repeat)
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
    fn every_shortcut_runs_its_command_in_its_application() {
        let shortcuts = [
            (GHOSTTY, "f1", r#"{ press = "dictation" }"#),
            (GHOSTTY, "f4", r#"{ exec = "open -a 'Mission Control'" }"#),
            (GHOSTTY, "f6", r#"{ press = "audio_volume_mute" }"#),
            (GHOSTTY, "f7", r#"{ press = "audio_volume_down" }"#),
            (GHOSTTY, "f8", r#"{ press = "audio_volume_up" }"#),
            (FIREFOX, "f9", r#"{ press = "brightness_down" }"#),
            (FIREFOX, "f10", r#"{ press = "brightness_up" }"#),
            (GHOSTTY, "ctrl-t", r#"{ type = "✓" }"#),
            (GHOSTTY, "home", r#"{ open = "org.mozilla.firefox" }"#),
            (FIREFOX, "end", r#"{ open = "com.mitchellh.ghostty" }"#),
            (
                GHOSTTY,
                "alt-b",
                r#"{ toggle_connection = "f8-8f-07-40-54-34" }"#,
            ),
            (GHOSTTY, "cmd-home", "{ resize = { width = 100 } }"),
            (GHOSTTY, "cmd-end", "{ resize = { width = -100 } }"),
            (FIREFOX, "cmd-home", "{ resize = { width = -100 } }"),
            (FIREFOX, "cmd-end", "{ resize = { width = 100 } }"),
            (FIREFOX, "page_up", r#"{ press = "ctrl-shift-tab" }"#),
            (FIREFOX, "page_down", r#"{ press = "ctrl-tab" }"#),
            (FIREFOX, "backquote", r#"{ press = "ctrl-z" }"#),
            (FIREFOX, "ctrl-backquote", r#"{ press = "ctrl-z" }"#),
            (GHOSTTY, "ctrl-alt-arrow_left", r#"{ tile = "left" }"#),
            (GHOSTTY, "ctrl-alt-arrow_up", r#""full_screen""#),
            (GHOSTTY, "ctrl-alt-arrow_down", r#"{ move_to = "next" }"#),
            (GHOSTTY, "ctrl-alt-enter", r#""fill""#),
            (GHOSTTY, "ctrl-alt-c", r#""center""#),
        ];
        for (front, combo, expected) in shortcuts {
            let decision = press(front, combo, false);
            assert_eq!(decision.verdict, Verdict::Drop, "{combo} in {front}");
            let Some(Effect::Run { command, .. }) = decision.effect else {
                panic!("{combo} in {front} runs nothing");
            };
            assert_eq!(command.as_ref(), actions(expected), "{combo} in {front}");
        }
    }

    #[test]
    fn held_keys_repeat_only_incremental_commands() {
        let resize = press(GHOSTTY, "cmd-home", true);
        assert!(matches!(
            resize.effect,
            Some(Effect::Run { repeat: true, .. })
        ));
        assert_eq!(press(GHOSTTY, "home", true), Decision::from(Verdict::Drop));
        assert_eq!(press(GHOSTTY, "alt-b", true), Decision::from(Verdict::Drop));
    }

    #[test]
    fn unbound_keys_pass() {
        assert_eq!(press(GHOSTTY, "page_up", false), Decision::PASS);
        assert_eq!(press(FIREFOX, "a", false), Decision::PASS);
    }

    #[test]
    fn an_empty_command_drops_the_key() {
        let decision = press(FIREFOX, "cmd-h", false);
        assert_eq!(decision.verdict, Verdict::Drop);
        let Some(Effect::Run { command, .. }) = decision.effect else {
            panic!("an empty command is still a command");
        };
        assert!(command.as_ref().is_empty());
    }

    #[test]
    fn false_passes_the_key_to_the_application() {
        assert_eq!(press(GHOSTTY, "cmd-h", false), Decision::PASS);
        assert_eq!(press(GHOSTTY, "cmd-h", true), Decision::PASS);
    }

    #[test]
    fn false_at_the_root_is_an_absent_shortcut() {
        let config: Config = "[shortcuts]\nf1 = false\nf2 = []".parse().unwrap();
        assert_eq!(decide(&config, FIREFOX, "f1", false), Decision::PASS);
        assert_eq!(decide(&config, FIREFOX, "f3", false), Decision::PASS);
        assert_eq!(decide(&config, FIREFOX, "f2", false).verdict, Verdict::Drop);
    }

    #[test]
    fn false_in_an_application_ends_the_lookup() {
        let config: Config =
            format!("[shortcuts]\nf1 = \"fill\"\n[\"{GHOSTTY}\"]\nshortcuts.f1 = false")
                .parse()
                .unwrap();
        assert_eq!(decide(&config, GHOSTTY, "f1", false), Decision::PASS);
        assert_eq!(decide(&config, FIREFOX, "f1", false).verdict, Verdict::Drop);
    }

    #[test]
    fn an_empty_command_in_an_application_drops_over_a_global_command() {
        let config: Config =
            format!("[shortcuts]\nf1 = \"fill\"\n[\"{GHOSTTY}\"]\nshortcuts.f1 = []")
                .parse()
                .unwrap();
        let decision = decide(&config, GHOSTTY, "f1", false);
        assert_eq!(decision.verdict, Verdict::Drop);
        assert!(
            matches!(decision.effect, Some(Effect::Run { command, .. }) if command.as_ref().is_empty())
        );
    }

    #[test]
    fn only_discrete_scrolling_against_the_system_reverses() {
        let scroll = |continuous, natural_scrolling| {
            let cx = Cx {
                natural_scrolling,
                ..Cx::new(None)
            };
            config().decide(Event::Scroll { continuous }, &cx).verdict
        };
        assert_eq!(scroll(false, true), Verdict::Reverse);
        assert_eq!(scroll(true, true), Verdict::Pass);
        assert_eq!(scroll(false, false), Verdict::Pass);
    }

    #[test]
    fn launches_run_their_application_command() {
        let launch =
            |app: &str| config().decide(Event::Launch(app.parse().unwrap()), &Cx::new(None));
        let Some(Effect::Launch { app, command }) = launch("com.apple.Music").effect else {
            panic!("launching Music runs nothing");
        };
        assert_eq!(
            command.as_ref(),
            actions(r#"{ force_quit = "com.apple.Music" }"#)
        );
        assert_eq!(app, "com.apple.Music".parse().unwrap());
        let Some(Effect::Launch { app, command }) = launch(FIREFOX).effect else {
            panic!("launching Firefox runs nothing");
        };
        assert_eq!(command.as_ref(), actions(r#""full_screen""#));
        assert_eq!(app, FIREFOX.parse().unwrap());
        assert_eq!(launch("com.apple.TextEdit"), Decision::PASS);
    }

    #[test]
    fn brightness_syncs_when_configured() {
        let decide = |config: Config| config.decide(Event::BrightnessKey, &Cx::new(None));
        assert_eq!(decide(config()).effect, Some(Effect::SyncBrightness));
        assert_eq!(decide(Config::default()), Decision::PASS);
    }
}
