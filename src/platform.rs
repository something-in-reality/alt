//! The boundary to the operating system: events come from an `Input`, effects go to an `Output`,
//! and actions run on a `Desktop`.

use std::{convert::Infallible, error::Error, io, sync::Arc};

use arc_swap::ArcSwap;

use crate::{
    action::{App, Bluetooth, Display, Keyboard, Media, Window},
    config::Config,
    event::{Context, Effect, Event, System, Verdict},
};

/// A source of events, which runs until the process ends.
pub trait Input {
    type Context: Context;
    type Error: Error + Send + Sync + 'static;

    /// The platform's settings, read once.
    fn system(&self) -> System;

    fn run(
        self,
        handle: impl Fn(Event, &Self::Context) -> Verdict + 'static,
    ) -> Result<Infallible, Self::Error>;
}

/// What a platform does for each namespace of actions; `Action::perform` dispatches to it.
pub trait Desktop {
    type Error: Error + From<io::Error> + Send + Sync + 'static;

    fn keyboard(&self, action: &Keyboard) -> Result<(), Self::Error>;
    fn app(&self, action: &App) -> Result<(), Self::Error>;
    fn window(&self, action: Window) -> Result<(), Self::Error>;
    fn bluetooth(&self, action: &Bluetooth) -> Result<(), Self::Error>;
    fn media(&self, action: Media) -> Result<(), Self::Error>;
    fn display(&self, action: Display) -> Result<(), Self::Error>;
}

/// A sink for effects; it applies them asynchronously, in order, and logs failures.
pub trait Output {
    fn apply(&self, effect: Effect);
}

/// Decides each event from `input` against the live `config`, and applies the effects to `output`.
pub fn run<I: Input>(
    input: I,
    output: impl Output + 'static,
    config: Arc<ArcSwap<Config>>,
) -> Result<Infallible, I::Error> {
    let system = input.system();
    input.run(move |event, cx| {
        let decision = config.load().decide(event.clone(), &system, cx);
        if decision.verdict != Verdict::Pass || decision.effect.is_some() {
            tracing::debug!(%event, verdict = ?decision.verdict, effect = ?decision.effect);
        }
        if let Some(effect) = decision.effect {
            output.apply(effect);
        }
        decision.verdict
    })
}
