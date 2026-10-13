//! macOS: one event tap on the main run loop; actions run in order on a worker thread.

mod app;
mod display;
mod keymap;
mod synth;
mod tap;
mod window;

use std::{
    convert::Infallible,
    process,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use anyhow::Context as _;
use arc_swap::ArcSwap;
use objc2_app_kit::{NSEvent, NSEventType};
use objc2_application_services::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRunLoop};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventType};
use objc2_foundation::{NSString, NSUserDefaults};

use self::{
    display::Brightness,
    synth::Keyboard,
    tap::{Tap, Verdict},
};
use crate::{
    action::{Action, AppId, Command, Direction},
    combo::{Combo, Key, Modifiers},
    config::Config,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not running")]
    NotRunning(AppId),
    #[error("{0} has no Mac key code")]
    Unmapped(Key),
    #[error("could not create {0}")]
    Unavailable(&'static str),
    #[error("accessibility error {0}")]
    Accessibility(i32),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// NX_SYSDEFINED, which carries media and brightness keys.
const SYSTEM_DEFINED: CGEventType = CGEventType(14);
const BRIGHTNESS_UP: isize = 2;
const BRIGHTNESS_DOWN: isize = 3;

/// A command and the application it applies to; `None` is the front application.
struct Job {
    command: Command,
    subject: Option<AppId>,
}

/// Runs the configuration until the process ends.
pub fn run(config: Arc<ArcSwap<Config>>) -> anyhow::Result<Infallible> {
    trust();
    let (jobs, rx) = mpsc::channel();
    thread::spawn(move || work(rx));
    let brightness = Brightness::spawn();

    let _observer = app::on_launch({
        let (config, jobs) = (config.clone(), jobs.clone());
        move |app| {
            if let Some(command) = config.load().for_window.get(&app) {
                let _ = jobs.send(Job {
                    command: command.clone(),
                    subject: Some(app),
                });
            }
        }
    });
    let _tap = Tap::new(
        &[
            CGEventType::KeyDown,
            CGEventType::ScrollWheel,
            SYSTEM_DEFINED,
        ],
        move |kind, event| handle(&config.load(), &jobs, &brightness, kind, event),
    )
    .context("creating the event tap")?;
    CFRunLoop::run();
    anyhow::bail!("the run loop ended")
}

/// Prints each key combination with the front application, and each brightness key.
pub fn events() -> anyhow::Result<Infallible> {
    trust();
    let _tap = Tap::new(&[CGEventType::KeyDown, SYSTEM_DEFINED], |kind, event| {
        if let Some(combo) = combo(event) {
            println!(
                "{combo}\t{}",
                app::front().map(|app| app.0).unwrap_or_default()
            );
        } else if let Some(direction) = brightness_key(kind, event) {
            println!("brightness {direction}");
        }
        Verdict::Pass
    })
    .context("creating the event tap")?;
    CFRunLoop::run();
    anyhow::bail!("the run loop ended")
}

/// Prompts for Accessibility permission and waits until it is granted.
fn trust() {
    let prompt = CFDictionary::from_slices(
        &[unsafe { kAXTrustedCheckOptionPrompt }],
        &[CFBoolean::new(true)],
    );
    if !unsafe { AXIsProcessTrustedWithOptions(Some(prompt.as_opaque())) } {
        tracing::info!("waiting for Accessibility permission");
        while !unsafe { AXIsProcessTrusted() } {
            thread::sleep(Duration::from_secs(2));
        }
    }
}

fn handle(
    config: &Config,
    jobs: &Sender<Job>,
    brightness: &Brightness,
    kind: CGEventType,
    event: &CGEvent,
) -> Verdict {
    if synth::is_synthetic(event) {
        return Verdict::Pass;
    }
    match kind {
        CGEventType::KeyDown => {
            let Some(combo) = combo(event) else {
                return Verdict::Pass;
            };
            let front = config.is_app_bound(&combo).then(app::front).flatten();
            let Some(command) = config.bindsym(front.as_ref(), &combo) else {
                return Verdict::Pass;
            };
            if CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventAutorepeat) == 0
            {
                let _ = jobs.send(Job {
                    command: command.clone(),
                    subject: front,
                });
            }
            Verdict::Drop
        }
        CGEventType::ScrollWheel => {
            reverse_wheel(config, event);
            Verdict::Pass
        }
        _ => {
            if config.brightness.sync && brightness_key(kind, event).is_some() {
                brightness.sync();
            }
            Verdict::Pass
        }
    }
}

fn combo(event: &CGEvent) -> Option<Combo> {
    let code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    Some(Combo {
        modifiers: keymap::modifiers(CGEvent::flags(Some(event))),
        key: keymap::key(u16::try_from(code).ok()?)?,
    })
}

/// The direction of a brightness key press, if `event` is one.
fn brightness_key(kind: CGEventType, event: &CGEvent) -> Option<&'static str> {
    if kind != SYSTEM_DEFINED {
        return None;
    }
    let event = NSEvent::eventWithCGEvent(event)?;
    if event.r#type() != NSEventType::SystemDefined || event.subtype().0 != 8 {
        return None;
    }
    let data = event.data1();
    let down = (data >> 8) & 0xff == 0xa;
    match data >> 16 {
        BRIGHTNESS_UP if down => Some("up"),
        BRIGHTNESS_DOWN if down => Some("down"),
        _ => None,
    }
}

/// Reverses discrete scrolling, from mouse wheels, when its configured direction differs from the system's.
fn reverse_wheel(config: &Config, event: &CGEvent) {
    let Some(natural) = config.input.pointer.natural_scroll else {
        return;
    };
    let continuous =
        CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventIsContinuous) != 0;
    if continuous || natural == system_natural_scroll() {
        return;
    }
    for field in [
        CGEventField::ScrollWheelEventDeltaAxis1,
        CGEventField::ScrollWheelEventPointDeltaAxis1,
    ] {
        let delta = CGEvent::integer_value_field(Some(event), field);
        CGEvent::set_integer_value_field(Some(event), field, -delta);
    }
    let field = CGEventField::ScrollWheelEventFixedPtDeltaAxis1;
    let delta = CGEvent::double_value_field(Some(event), field);
    CGEvent::set_double_value_field(Some(event), field, -delta);
}

/// The system scroll direction, natural unless set otherwise.
fn system_natural_scroll() -> bool {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str("com.apple.swipescrolldirection");
    defaults.objectForKey(&key).is_none() || defaults.boolForKey(&key)
}

fn work(jobs: Receiver<Job>) {
    let keyboard = match Keyboard::new() {
        Ok(keyboard) => keyboard,
        Err(error) => return tracing::error!(%error, "starting the worker"),
    };
    for job in jobs {
        for action in &job.command {
            if let Err(error) = perform(&keyboard, action, job.subject.as_ref()) {
                tracing::warn!(%error, ?action, "performing");
            }
        }
    }
}

fn perform(keyboard: &Keyboard, action: &Action, subject: Option<&AppId>) -> Result<(), Error> {
    match action {
        Action::Exec(command) => {
            let mut child = process::Command::new("/bin/sh")
                .args(["-c", command])
                .spawn()?;
            thread::spawn(move || child.wait());
            Ok(())
        }
        Action::Key(combo) => keyboard.press(combo),
        Action::Type(text) => keyboard.type_text(text),
        Action::Focus(app) => app::focus(app),
        Action::Kill => match subject.cloned().or_else(app::front) {
            Some(app) => app::kill(&app),
            None => Err(Error::Unavailable("the front application")),
        },
        Action::Resize(resize) => window::resize(*resize),
        Action::Workspace(direction) => {
            let arrow = match direction {
                Direction::Prev => "ArrowLeft",
                Direction::Next => "ArrowRight",
            };
            keyboard.press(&Combo {
                modifiers: Modifiers::CTRL,
                key: arrow.parse().expect("W3C code"),
            })
        }
    }
}
