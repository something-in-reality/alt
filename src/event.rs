//! Input events, and how the configuration decides them.

use crate::{
    action::{AppId, Command},
    combo::Combo,
    config::Config,
};

/// Input from the platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key {
        combo: Combo,
        repeat: bool,
    },
    /// Scrolling, which is continuous from trackpads and discrete from mouse wheels.
    Scroll {
        continuous: bool,
    },
    /// A brightness key, after the system has applied it.
    Brightness,
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

/// The platform state a decision may need.
pub trait Context {
    fn front(&self) -> Option<AppId>;
    /// The system scroll direction, which `natural_scroll` is relative to.
    fn natural_scroll(&self) -> bool;
}

impl Decision {
    const PASS: Self = Self {
        verdict: Verdict::Pass,
        effect: None,
    };

    fn run(verdict: Verdict, command: &Command, subject: Option<AppId>) -> Self {
        let command = command.clone();
        Self {
            verdict,
            effect: Some(Effect::Run { command, subject }),
        }
    }
}

impl Config {
    /// Decides an event: a bound key is dropped and runs its command once per press.
    pub fn decide(&self, event: Event, cx: &impl Context) -> Decision {
        match event {
            Event::Key { combo, repeat } => {
                let front = cx.front();
                match self.bindsym(front.as_ref(), &combo) {
                    None => Decision::PASS,
                    Some(_) if repeat => Decision {
                        verdict: Verdict::Drop,
                        effect: None,
                    },
                    Some(command) => Decision::run(Verdict::Drop, command, front),
                }
            }
            Event::Scroll { continuous } => {
                let reverse = !continuous
                    && self
                        .input
                        .pointer
                        .natural_scroll
                        .is_some_and(|natural| natural != cx.natural_scroll());
                Decision {
                    verdict: if reverse {
                        Verdict::Reverse
                    } else {
                        Verdict::Pass
                    },
                    effect: None,
                }
            }
            Event::Brightness if self.brightness.sync => Decision {
                verdict: Verdict::Pass,
                effect: Some(Effect::SyncBrightness),
            },
            Event::Brightness => Decision::PASS,
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

    struct Desk {
        front: Option<&'static str>,
        natural_scroll: bool,
    }

    impl Context for Desk {
        fn front(&self) -> Option<AppId> {
            self.front.map(AppId::from)
        }

        fn natural_scroll(&self) -> bool {
            self.natural_scroll
        }
    }

    const DESK: Desk = Desk {
        front: Some("org.nixos.firefox"),
        natural_scroll: true,
    };

    fn key(combo: &str, repeat: bool) -> Event {
        Event::Key {
            combo: combo.parse().unwrap(),
            repeat,
        }
    }

    #[test]
    fn bound_keys_drop_and_run_once_per_press() {
        let config: Config = EXAMPLE.parse().unwrap();
        let decision = config.decide(key("PageUp", false), &DESK);
        assert_eq!(decision.verdict, Verdict::Drop);
        let Some(Effect::Run { command, subject }) = decision.effect else {
            panic!()
        };
        assert_eq!(
            command.as_ref(),
            [Action::Key("ctrl+shift+Tab".parse().unwrap())]
        );
        assert_eq!(subject, Some(AppId::from("org.nixos.firefox")));
        assert_eq!(
            config.decide(key("PageUp", true), &DESK),
            Decision {
                verdict: Verdict::Drop,
                effect: None
            }
        );
    }

    #[test]
    fn unbound_keys_pass() {
        let config: Config = EXAMPLE.parse().unwrap();
        let desk = Desk {
            front: Some("com.mitchellh.ghostty"),
            ..DESK
        };
        assert_eq!(config.decide(key("PageUp", false), &desk), Decision::PASS);
    }

    #[test]
    fn only_discrete_scrolling_against_the_system_reverses() {
        let config: Config = EXAMPLE.parse().unwrap();
        let scroll = |continuous, natural_scroll| {
            config
                .decide(
                    Event::Scroll { continuous },
                    &Desk {
                        natural_scroll,
                        ..DESK
                    },
                )
                .verdict
        };
        assert_eq!(scroll(false, true), Verdict::Reverse);
        assert_eq!(scroll(true, true), Verdict::Pass);
        assert_eq!(scroll(false, false), Verdict::Pass);
    }

    #[test]
    fn brightness_syncs_when_configured() {
        let config: Config = EXAMPLE.parse().unwrap();
        assert_eq!(
            config.decide(Event::Brightness, &DESK).effect,
            Some(Effect::SyncBrightness)
        );
        assert_eq!(
            Config::default().decide(Event::Brightness, &DESK),
            Decision::PASS
        );
    }

    #[test]
    fn launches_run_window_rules_on_the_launched_app() {
        let config: Config = EXAMPLE.parse().unwrap();
        let music = AppId::from("com.apple.Music");
        let Some(Effect::Run { command, subject }) =
            config.decide(Event::Launched(music.clone()), &DESK).effect
        else {
            panic!()
        };
        assert_eq!(command.as_ref(), [Action::Kill]);
        assert_eq!(subject, Some(music));
    }
}
