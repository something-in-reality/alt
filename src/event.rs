//! Input events, and how the configuration decides them.

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
    Launched(AppId),
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
    /// Runs `command`; `subject` is the application `kill` quits.
    Run {
        command: Command,
        subject: Option<AppId>,
    },
    /// External displays follow the built-in display's brightness.
    SyncBrightness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub effect: Option<Effect>,
}

/// The platform's own settings, which the configuration is relative to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct System {
    pub natural_scroll: bool,
}

/// Live platform state, queried only when a decision needs it.
pub trait Context {
    fn front(&self) -> Option<AppId>;
}

impl Decision {
    const PASS: Self = Self::verdict(Verdict::Pass);

    const fn verdict(verdict: Verdict) -> Self {
        Self {
            verdict,
            effect: None,
        }
    }

    fn run(verdict: Verdict, command: &Command, subject: Option<AppId>) -> Self {
        Self {
            verdict,
            effect: Some(Effect::Run {
                command: command.clone(),
                subject,
            }),
        }
    }
}

impl Config {
    /// Decides an event: a bound key is dropped and runs its command once per press.
    pub fn decide(&self, event: Event, system: &System, cx: &impl Context) -> Decision {
        match event {
            Event::Press { combo, repeat } => {
                let front = cx.front();
                match self.bindsym(front.as_ref(), &combo) {
                    None => Decision::PASS,
                    Some(_) if repeat => Decision::verdict(Verdict::Drop),
                    Some(command) => Decision::run(Verdict::Drop, command, front),
                }
            }
            Event::Scroll { continuous: false } => match self.input.pointer.natural_scroll {
                Some(wanted) if wanted != system.natural_scroll => {
                    Decision::verdict(Verdict::Reverse)
                }
                _ => Decision::PASS,
            },
            Event::Scroll { continuous: true } => Decision::PASS,
            Event::BrightnessKey if self.brightness.sync => Decision {
                verdict: Verdict::Pass,
                effect: Some(Effect::SyncBrightness),
            },
            Event::BrightnessKey => Decision::PASS,
            Event::Launched(app) => match self.for_window.get(&app) {
                Some(command) => Decision::run(Verdict::Pass, command, Some(app)),
                None => Decision::PASS,
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
            (GHOSTTY, "F1", r#"{ key = "Dictation" }"#),
            (GHOSTTY, "F3", r#"{ workspace = "prev" }"#),
            (GHOSTTY, "F4", r#"{ exec = "open -a 'Mission Control'" }"#),
            (FIREFOX, "F5", r#"{ workspace = "next" }"#),
            (GHOSTTY, "F6", r#"{ key = "AudioVolumeMute" }"#),
            (GHOSTTY, "ctrl+KeyT", r#"{ type = "✓" }"#),
            (GHOSTTY, "Home", r#"{ focus = "org.nixos.firefox" }"#),
            (FIREFOX, "End", r#"{ focus = "com.mitchellh.ghostty" }"#),
            (
                GHOSTTY,
                "super+Home",
                r#"{ resize = { grow = { width = "100px" } } }"#,
            ),
            (
                GHOSTTY,
                "super+End",
                r#"{ resize = { shrink = { width = "100px" } } }"#,
            ),
            (
                FIREFOX,
                "super+Home",
                r#"{ resize = { shrink = { width = "100px" } } }"#,
            ),
            (
                FIREFOX,
                "super+End",
                r#"{ resize = { grow = { width = "100px" } } }"#,
            ),
            (FIREFOX, "PageUp", r#"{ key = "ctrl+shift+Tab" }"#),
            (FIREFOX, "PageDown", r#"{ key = "ctrl+Tab" }"#),
            (FIREFOX, "Backquote", r#"{ key = "ctrl+KeyZ" }"#),
            (FIREFOX, "ctrl+Backquote", r#"{ key = "ctrl+KeyZ" }"#),
            (
                FIREFOX,
                "super+Backquote",
                r#"[{ key = "super+KeyL" }, { key = "alt+Enter" }]"#,
            ),
        ];
        for (front, combo, expected) in triggers {
            let decision = press(front, combo, false);
            assert_eq!(decision.verdict, Verdict::Drop, "{combo} in {front}");
            let Some(Effect::Run { command, subject }) = decision.effect else {
                panic!("{combo} in {front} runs nothing");
            };
            assert_eq!(command.as_ref(), actions(expected), "{combo} in {front}");
            assert_eq!(subject, Some(AppId::from(front)));
        }
    }

    #[test]
    fn repeats_drop_without_running() {
        assert_eq!(
            press(FIREFOX, "PageUp", true),
            Decision::verdict(Verdict::Drop)
        );
    }

    #[test]
    fn unbound_keys_pass() {
        assert_eq!(press(GHOSTTY, "PageUp", false), Decision::PASS);
        assert_eq!(press(FIREFOX, "KeyA", false), Decision::PASS);
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
    fn brightness_syncs_when_configured() {
        let decide = |config: Config| config.decide(Event::BrightnessKey, &NATURAL, &Front(None));
        assert_eq!(decide(config()).effect, Some(Effect::SyncBrightness));
        assert_eq!(decide(Config::default()), Decision::PASS);
    }

    #[test]
    fn launches_run_window_rules_on_the_launched_app() {
        let music = AppId::from("com.apple.Music");
        let decision = config().decide(Event::Launched(music.clone()), &NATURAL, &Front(None));
        let Some(Effect::Run { command, subject }) = decision.effect else {
            panic!("Music's launch runs nothing");
        };
        assert_eq!(command.as_ref(), [Action::Kill]);
        assert_eq!(subject, Some(music));
    }
}
