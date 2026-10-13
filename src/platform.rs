//! The boundary to the operating system: events come from an `Input`, effects go to an `Output`.

use std::{convert::Infallible, error::Error, sync::Arc};

use arc_swap::ArcSwap;

use crate::{
    config::Config,
    event::{Context, Effect, Event, Verdict},
};

/// A source of events, which runs until the process ends.
pub trait Input {
    type Context: Context;
    type Error: Error + Send + Sync + 'static;

    fn run(
        self,
        handle: impl Fn(Event, &Self::Context) -> Verdict + 'static,
    ) -> Result<Infallible, Self::Error>;
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
    input.run(move |event, cx| {
        let decision = config.load().decide(event, cx);
        if let Some(effect) = decision.effect {
            output.apply(effect);
        }
        decision.verdict
    })
}
