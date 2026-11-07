//! The `Output` and `Desktop`: effects run on GCD queues, and actions on the macOS APIs.

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::autoreleasepool;

use super::{
    Error, app, bluetooth, display::Displays, notification, synth::Source, window::Window,
};
use crate::{
    action::{BundleId, Command, Device, Screen, Side, Size, Tile},
    combo::Combo,
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

/// Performs actions, posting input from a private event source; window verbs act on `app`'s
/// window, or without one on the focused window.
pub struct Mac {
    source: Source,
    app: Option<BundleId>,
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
fn execute(command: &Command, app: Option<BundleId>) {
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

impl Mac {
    fn window(&self) -> Result<Window, Error> {
        Window::new(self.app.as_ref())
    }
}

impl Desktop for Mac {
    type Error = Error;

    fn press(&self, combo: Combo) -> Result<(), Error> {
        self.source.press(combo)
    }

    fn type_text(&self, text: &str) -> Result<(), Error> {
        self.source.type_text(text)
    }

    fn open(&self, app: &BundleId) -> Result<(), Error> {
        app::open(app)
    }

    fn force_quit(&self, app: &BundleId) -> Result<(), Error> {
        app::force_quit(app)
    }

    fn full_screen(&self) -> Result<(), Error> {
        self.window()?.full_screen()
    }

    fn full_screen_tile(&self, side: Side) -> Result<(), Error> {
        self.window()?.full_screen_tile(side)
    }

    fn tile(&self, tile: Tile) -> Result<(), Error> {
        self.window()?.tile(tile)
    }

    fn fill(&self) -> Result<(), Error> {
        self.window()?.fill()
    }

    fn center(&self) -> Result<(), Error> {
        self.window()?.center()
    }

    fn move_to(&self, screen: &Screen) -> Result<(), Error> {
        self.window()?.move_to(screen)
    }

    fn resize(&self, size: Size) -> Result<(), Error> {
        self.window()?.resize(size)
    }

    fn connect(&self, device: &Device) -> Result<(), Error> {
        bluetooth::set(device, Some(true))
    }

    fn disconnect(&self, device: &Device) -> Result<(), Error> {
        bluetooth::set(device, Some(false))
    }

    fn toggle_connection(&self, device: &Device) -> Result<(), Error> {
        bluetooth::set(device, None)
    }

    fn notify(&self, text: &str) -> Result<(), Error> {
        notification::post(text)
    }
}
