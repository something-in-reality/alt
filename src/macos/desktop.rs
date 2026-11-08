//! Effects run on GCD queues, and actions on the macOS APIs.

use std::{
    io, process,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use jiff::Zoned;
use objc2::rc::autoreleasepool;

use super::{
    Error, app, bluetooth, display::Displays, notification, synth::Source, window::Window,
};
use crate::{
    action::{Action, BundleId, Command},
    event::Effect,
};

/// Applies effects: commands in order on one serial queue, launched applications' commands on
/// another, as they wait for their windows, and brightness on a third. A held key's autorepeat
/// runs only when no command is queued or running.
pub(super) struct Dispatcher {
    actions: DispatchRetained<DispatchQueue>,
    pending: Arc<AtomicUsize>,
    launches: DispatchRetained<DispatchQueue>,
    displays: Displays,
}

/// Performs actions, posting input from a private event source; window verbs act on `app`'s
/// window, or without one on the focused window.
struct Mac {
    source: Source,
    app: Option<BundleId>,
}

impl Dispatcher {
    pub(super) fn new() -> Result<Self, Error> {
        if let Err(error) = notification::authorize() {
            tracing::warn!(%error, "requesting notification authorization");
        }
        Ok(Self {
            actions: DispatchQueue::new("alt.actions", DispatchQueueAttr::SERIAL),
            pending: Arc::default(),
            launches: DispatchQueue::new("alt.launches", DispatchQueueAttr::SERIAL),
            displays: Displays::new()?,
        })
    }

    /// Applies `effect` asynchronously, in order per queue.
    pub(super) fn apply(&self, effect: Effect) {
        match effect {
            Effect::Run { command, repeat } => {
                if repeat && self.pending.load(Ordering::Acquire) > 0 {
                    return;
                }
                self.pending.fetch_add(1, Ordering::AcqRel);
                let pending = self.pending.clone();
                self.actions.exec_async(move || {
                    autoreleasepool(|_| execute(&command, None));
                    pending.fetch_sub(1, Ordering::AcqRel);
                });
            }
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
        if let Err(error) = mac.perform(action) {
            tracing::warn!(%error, ?action, "performing");
        }
    }
}

impl Mac {
    fn window(&self) -> Result<Window, Error> {
        Window::new(self.app.as_ref())
    }

    /// Performs `action`: `exec` spawns `sh -c`, `notify` fills its template with the local time.
    fn perform(&self, action: &Action) -> Result<(), Error> {
        match action {
            Action::Press(combo) => self.source.press(*combo),
            Action::Type(text) => self.source.type_text(text),
            Action::Open(app) => app::open(app),
            Action::ForceQuit(app) => app::force_quit(app),
            Action::FullScreen => self.window()?.full_screen(),
            Action::FullScreenTile(side) => self.window()?.full_screen_tile(*side),
            Action::Tile(tile) => self.window()?.tile(*tile),
            Action::Fill => self.window()?.fill(),
            Action::Center => self.window()?.center(),
            Action::MoveTo(screen) => self.window()?.move_to(screen),
            Action::Resize(size) => self.window()?.resize(*size),
            Action::Connect(device) => bluetooth::set(device, Some(true)),
            Action::Disconnect(device) => bluetooth::set(device, Some(false)),
            Action::ToggleConnection(device) => bluetooth::set(device, None),
            Action::Exec(command) => {
                let mut child = process::Command::new("/bin/sh")
                    .args(["-c", command])
                    .spawn()?;
                thread::spawn(move || child.wait());
                Ok(())
            }
            Action::Notify(template) => {
                notification::post(&template.format(&Zoned::now()).map_err(io::Error::other)?)
            }
        }
    }
}
