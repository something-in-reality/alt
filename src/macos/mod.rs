//! macOS: a session event tap is the `Input`; GCD queues apply the `Output`.

mod app;
mod display;
mod keymap;
mod synth;
mod tap;
mod window;

use std::{convert::Infallible, process, thread};

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use keycode::KeyMappingCode;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSEvent, NSEventType};
use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRunLoop};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventType};
use objc2_foundation::{NSString, NSUserDefaults};

use self::{display::Displays, synth::Keyboard, tap::Tap};
use crate::{
    action::{Action, AppId, Command, Direction},
    combo::{Combo, Key, Modifiers},
    event::{Context, Effect, Event, System, Verdict},
    platform::{Input, Output},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Accessibility permission is missing; grant it, then restart")]
    Untrusted,
    #[error("creating the event tap failed")]
    Tap,
    #[error("the run loop ended")]
    RunLoopEnded,
    #[error("{0} is not running")]
    NotRunning(AppId),
    #[error("{0} has no Mac key code")]
    Unmapped(Key),
    #[error("could not create {0}")]
    Unavailable(&'static str),
    #[error("accessibility error {0}")]
    Accessibility(i32),
    #[error("no built-in display")]
    NoBuiltinDisplay,
    #[error("DisplayServices error {0}")]
    DisplayServices(i32),
    #[error("loading DisplayServices")]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// NX_SYSDEFINED, which carries media and brightness keys.
const SYSTEM_DEFINED: CGEventType = CGEventType(14);

/// The login session's keys, scrolling and brightness keys, from an event tap.
pub struct Session;

/// Running applications, through NSWorkspace.
pub struct Apps;

/// Applies effects: actions in order on one serial queue, brightness on another.
pub struct Desktop {
    actions: DispatchRetained<DispatchQueue>,
    displays: Displays,
}

impl Input for Session {
    type Context = Apps;
    type Error = Error;

    fn system(&self) -> System {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str("com.apple.swipescrolldirection");
        System {
            natural_scroll: defaults.objectForKey(&key).is_none() || defaults.boolForKey(&key),
        }
    }

    fn run(self, handle: impl Fn(Event, &Apps) -> Verdict + 'static) -> Result<Infallible, Error> {
        trust()?;
        let _tap = Tap::new(
            &[
                CGEventType::KeyDown,
                CGEventType::ScrollWheel,
                SYSTEM_DEFINED,
            ],
            move |kind, event| {
                if synth::is_synthetic(event) {
                    return true;
                }
                let Some(input) = read(kind, event) else {
                    return true;
                };
                match handle(input, &Apps) {
                    Verdict::Pass => true,
                    Verdict::Drop => false,
                    Verdict::Reverse => {
                        reverse_scroll(event);
                        true
                    }
                }
            },
        )
        .ok_or(Error::Tap)?;
        tracing::info!("running");
        CFRunLoop::run();
        Err(Error::RunLoopEnded)
    }
}

impl Context for Apps {
    fn front(&self) -> Option<AppId> {
        app::front()
    }
}

impl Desktop {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            actions: DispatchQueue::new("accessibility.actions", DispatchQueueAttr::SERIAL),
            displays: Displays::new()?,
        })
    }
}

impl Output for Desktop {
    fn apply(&self, effect: Effect) {
        match effect {
            Effect::Run(command) => self
                .actions
                .exec_async(move || autoreleasepool(|_| execute(&command))),
            Effect::SyncBrightness => self.displays.sync(),
        }
    }
}

/// Prompts for Accessibility permission when the process lacks it.
/// A process sees a grant only after it restarts, so an untrusted process prompts and exits.
fn trust() -> Result<(), Error> {
    let prompt = CFDictionary::from_slices(
        &[unsafe { kAXTrustedCheckOptionPrompt }],
        &[CFBoolean::new(true)],
    );
    if unsafe { AXIsProcessTrustedWithOptions(Some(prompt.as_opaque())) } {
        Ok(())
    } else {
        Err(Error::Untrusted)
    }
}

/// The event a tapped `CGEvent` is, if it is one this crate decides.
fn read(kind: CGEventType, event: &CGEvent) -> Option<Event> {
    let field = |field| CGEvent::integer_value_field(Some(event), field);
    match kind {
        CGEventType::KeyDown => Some(Event::Press {
            combo: Combo {
                modifiers: keymap::modifiers(CGEvent::flags(Some(event))),
                key: keymap::key(u16::try_from(field(CGEventField::KeyboardEventKeycode)).ok()?)?,
            },
            repeat: field(CGEventField::KeyboardEventAutorepeat) != 0,
        }),
        CGEventType::ScrollWheel => Some(Event::Scroll {
            continuous: field(CGEventField::ScrollWheelEventIsContinuous) != 0,
        }),
        SYSTEM_DEFINED => is_brightness_key(event).then_some(Event::BrightnessKey),
        _ => None,
    }
}

/// Whether `event` is a brightness key going down.
fn is_brightness_key(event: &CGEvent) -> bool {
    const BRIGHTNESS_UP: isize = 2;
    const BRIGHTNESS_DOWN: isize = 3;
    NSEvent::eventWithCGEvent(event).is_some_and(|event| {
        let data = event.data1();
        event.r#type() == NSEventType::SystemDefined
            && event.subtype().0 == 8
            && matches!(data >> 16, BRIGHTNESS_UP | BRIGHTNESS_DOWN)
            && (data >> 8) & 0xff == 0xa
    })
}

/// Reverses both axes of a scroll event.
fn reverse_scroll(event: &CGEvent) {
    for field in [
        CGEventField::ScrollWheelEventDeltaAxis1,
        CGEventField::ScrollWheelEventDeltaAxis2,
        CGEventField::ScrollWheelEventPointDeltaAxis1,
        CGEventField::ScrollWheelEventPointDeltaAxis2,
    ] {
        let delta = CGEvent::integer_value_field(Some(event), field);
        CGEvent::set_integer_value_field(Some(event), field, -delta);
    }
    for field in [
        CGEventField::ScrollWheelEventFixedPtDeltaAxis1,
        CGEventField::ScrollWheelEventFixedPtDeltaAxis2,
    ] {
        let delta = CGEvent::double_value_field(Some(event), field);
        CGEvent::set_double_value_field(Some(event), field, -delta);
    }
}

/// Runs each action of `command`, logging those that fail.
fn execute(command: &Command) {
    let keyboard = match Keyboard::new() {
        Ok(keyboard) => keyboard,
        Err(error) => return tracing::warn!(%error, "creating a keyboard"),
    };
    for action in command.as_ref() {
        if let Err(error) = perform(&keyboard, action) {
            tracing::warn!(%error, ?action, "performing");
        }
    }
}

fn perform(keyboard: &Keyboard, action: &Action) -> Result<(), Error> {
    match action {
        Action::Exec(command) => {
            let mut child = process::Command::new("/bin/sh")
                .args(["-c", command])
                .spawn()?;
            thread::spawn(move || child.wait());
            Ok(())
        }
        Action::Press(combo) => keyboard.press(*combo),
        Action::Type(text) => keyboard.type_text(text),
        Action::Focus(app) => app::focus(app),
        Action::Resize(resize) => window::resize(*resize),
        Action::Workspace(direction) => keyboard.press(Combo {
            modifiers: Modifiers::CTRL,
            key: Key::Code(match direction {
                Direction::Prev => KeyMappingCode::ArrowLeft,
                Direction::Next => KeyMappingCode::ArrowRight,
            }),
        }),
    }
}
