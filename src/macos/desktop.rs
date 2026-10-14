//! The `Output` and `Desktop`: effects run on GCD queues, and actions on the macOS APIs.

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::autoreleasepool;

use super::{
    Error, app, bluetooth, display::Displays, synth::Source, system_key::SystemKey, window,
};
use crate::{
    action::{App, Bluetooth, Command, Display, Keyboard, Media, Window},
    event::Effect,
    platform::{Desktop, Output},
};

/// Applies effects: commands in order on one serial queue, brightness on another.
pub struct Dispatcher {
    actions: DispatchRetained<DispatchQueue>,
    displays: Displays,
}

/// Performs actions, posting input from a private event source.
pub struct Mac {
    source: Source,
}

impl Dispatcher {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            actions: DispatchQueue::new("alt.actions", DispatchQueueAttr::SERIAL),
            displays: Displays::new()?,
        })
    }
}

impl Output for Dispatcher {
    fn apply(&self, effect: Effect) {
        match effect {
            Effect::Run(command) => self
                .actions
                .exec_async(move || autoreleasepool(|_| execute(&command))),
            Effect::SyncBrightness => self.displays.sync(),
        }
    }
}

/// Runs each action of `command`, logging those that fail.
fn execute(command: &Command) {
    let mac = match Source::new() {
        Ok(source) => Mac { source },
        Err(error) => return tracing::warn!(%error, "creating an event source"),
    };
    for action in command {
        if let Err(error) = action.perform(&mac) {
            tracing::warn!(error = %format_args!("{:#}", anyhow::Error::from(error)), ?action, "performing");
        }
    }
}

impl Desktop for Mac {
    type Error = Error;

    fn keyboard(&self, action: &Keyboard) -> Result<(), Error> {
        match action {
            Keyboard::Press(combo) => self.source.press(*combo),
            Keyboard::Type(text) => self.source.type_text(text),
        }
    }

    fn app(&self, action: &App) -> Result<(), Error> {
        match action {
            App::Focus(app) => app::focus(app),
            App::Kill(app) => app::kill(app),
        }
    }

    fn window(&self, action: Window) -> Result<(), Error> {
        match action {
            Window::Resize(change) => window::resize(change),
        }
    }

    fn bluetooth(&self, action: &Bluetooth) -> Result<(), Error> {
        bluetooth::perform(action)
    }

    fn media(&self, action: Media) -> Result<(), Error> {
        SystemKey::from(action).press()
    }

    fn display(&self, action: Display) -> Result<(), Error> {
        SystemKey::from(action).press()
    }
}
