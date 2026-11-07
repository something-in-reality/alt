//! The boundary to the operating system: events come from an `Input`, effects go to an `Output`,
//! and actions run on a `Desktop`.

use std::{convert::Infallible, error::Error, io, sync::Arc};

use arc_swap::ArcSwap;

use crate::{
    action::{BundleId, Device, Screen, Side, Size, Tile},
    combo::Combo,
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

/// What a platform does for each verb; `Action::perform` dispatches to it. Window verbs act on the
/// focused window, or on the launching application's window in `on_launch`.
pub trait Desktop {
    type Error: Error + From<io::Error> + Send + Sync + 'static;

    fn press(&self, combo: Combo) -> Result<(), Self::Error>;
    fn type_text(&self, text: &str) -> Result<(), Self::Error>;
    fn open(&self, app: &BundleId) -> Result<(), Self::Error>;
    fn force_quit(&self, app: &BundleId) -> Result<(), Self::Error>;
    fn full_screen(&self) -> Result<(), Self::Error>;
    fn full_screen_tile(&self, side: Side) -> Result<(), Self::Error>;
    fn tile(&self, tile: Tile) -> Result<(), Self::Error>;
    fn fill(&self) -> Result<(), Self::Error>;
    fn center(&self) -> Result<(), Self::Error>;
    fn move_to(&self, screen: &Screen) -> Result<(), Self::Error>;
    fn resize(&self, size: Size) -> Result<(), Self::Error>;
    fn connect(&self, device: &Device) -> Result<(), Self::Error>;
    fn disconnect(&self, device: &Device) -> Result<(), Self::Error>;
    fn toggle_connection(&self, device: &Device) -> Result<(), Self::Error>;
    /// Shows a notification of `text`.
    fn notify(&self, text: &str) -> Result<(), Self::Error>;
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
