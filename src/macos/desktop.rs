//! The `Output` and `Desktop`: effects run on GCD queues, and actions on the macOS APIs.

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::autoreleasepool;

use super::{
    Error, app, bluetooth, display::Displays, notification, synth::Source, system_key::SystemKey,
    window,
};
use crate::{
    action::{App, AppId, Bluetooth, Command, Display, Keyboard, Media, Window},
    event::Effect,
    platform::{Desktop, Output},
};

/// Applies effects: commands in order on one serial queue, launched applications' commands on
/// another, as they wait for their windows, and brightness on a third.
pub struct Dispatcher {
    actions: DispatchRetained<DispatchQueue>,
    launches: DispatchRetained<DispatchQueue>,
    displays: Displays,
}

/// Performs actions, posting input from a private event source; window actions act on `app`'s
/// window, or without one on the focused window.
pub struct Mac {
    source: Source,
    app: Option<AppId>,
}

impl Dispatcher {
    pub fn new() -> Result<Self, Error> {
        if let Err(error) = notification::authorize() {
            tracing::warn!(%error, "requesting notification authorization");
        }
        Ok(Self {
            actions: DispatchQueue::new("alt.actions", DispatchQueueAttr::SERIAL),
            launches: DispatchQueue::new("alt.launches", DispatchQueueAttr::SERIAL),
            displays: Displays::new()?,
        })
    }
}

impl Output for Dispatcher {
    fn apply(&self, effect: Effect) {
        match effect {
            Effect::Run(command) => self
                .actions
                .exec_async(move || autoreleasepool(|_| execute(&command, None))),
            Effect::Launch { app, command } => self
                .launches
                .exec_async(move || autoreleasepool(|_| execute(&command, Some(app)))),
            Effect::SyncBrightness => self.displays.sync(),
        }
    }
}

/// Runs each action of `command`, logging those that fail.
fn execute(command: &Command, app: Option<AppId>) {
    let mac = match Source::new() {
        Ok(source) => Mac { source, app },
        Err(error) => return tracing::warn!(%error, "creating an event source"),
    };
    for action in command {
        if let Err(error) = action.perform(&mac) {
            tracing::warn!(%error, ?action, "performing");
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

    fn window(&self, action: &Window) -> Result<(), Error> {
        window::perform(action, self.app.as_ref())
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

    fn notify(&self, text: &str) -> Result<(), Error> {
        notification::post(text)
    }
}
