//! The `Input`: the login session's keys, scrolling and system keys, from an event tap, and
//! application launches, from NSWorkspace.

use std::{
    cell::RefCell,
    convert::Infallible,
    env,
    os::unix::process::CommandExt,
    process,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use dispatch2::DispatchQueue;
use objc2_app_kit::{
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceWillLaunchApplicationNotification,
};
use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRunLoop};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventType};
use objc2_foundation::{NSString, NSUserDefaults};

use super::{
    Error, app, keymap,
    setup::Permission,
    synth,
    system_key::{SYSTEM_DEFINED, SystemKey},
    tap::Tap,
};
use crate::{
    action::AppId,
    combo::Combo,
    event::{Context, Event, System, Verdict},
    platform::Input,
};

/// The login session.
pub struct Session;

/// Running applications; NSWorkspace's notifications keep the front one current, so a decision
/// reads it without a call to another process.
pub struct Apps {
    front: RefCell<Option<AppId>>,
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
        let apps = Rc::new(Apps {
            front: RefCell::new(app::front()),
        });
        let handle = Rc::new(handle);
        let _activate = app::observe(unsafe { NSWorkspaceDidActivateApplicationNotification }, {
            let apps = apps.clone();
            move |front| *apps.front.borrow_mut() = Some(front)
        });
        let launch = {
            let (apps, handle) = (apps.clone(), handle.clone());
            move |app| _ = handle(Event::Launch(app), &apps)
        };
        app::all().for_each(&launch);
        let _launch = app::observe(
            unsafe { NSWorkspaceWillLaunchApplicationNotification },
            launch,
        );
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
                match handle(input, &apps) {
                    Verdict::Pass => true,
                    Verdict::Drop => false,
                    Verdict::Reverse => !synth::reverse_scroll(event),
                }
            },
        )
        .ok_or(Error::Tap)?;
        watchdog();
        Permission::Granted.record();
        tracing::info!("running");
        CFRunLoop::run();
        Err(Error::RunLoopEnded)
    }
}

impl Context for Apps {
    fn front(&self) -> Option<AppId> {
        self.front.borrow().clone()
    }
}

/// Exits when the main thread, which decides each tapped event, stops responding: an active tap
/// holds all input while its callback runs, and the tap ends with the process.
fn watchdog() {
    const LIMIT: Duration = Duration::from_secs(2);
    let beat = Arc::new(AtomicU64::new(0));
    thread::spawn(move || {
        let start = Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;
        loop {
            let main = beat.clone();
            let now = elapsed();
            DispatchQueue::main().exec_async(move || main.store(now, Ordering::Relaxed));
            thread::sleep(LIMIT / 2);
            if elapsed() - beat.load(Ordering::Relaxed) > LIMIT.as_millis() as u64 {
                tracing::error!("the main thread stopped responding; exiting to release input");
                process::exit(1);
            }
        }
    });
}

/// Prompts once when untrusted, then waits for the grant and restarts into it: a process sees a
/// grant from its next start, so a fresh child process checks for it.
fn trust() -> Result<(), Error> {
    let prompt = CFDictionary::from_slices(
        &[unsafe { kAXTrustedCheckOptionPrompt }],
        &[CFBoolean::new(true)],
    );
    if unsafe { AXIsProcessTrustedWithOptions(Some(prompt.as_opaque())) } {
        return Ok(());
    }
    Permission::Waiting.record();
    tracing::info!("waiting for Accessibility permission");
    let exe = env::current_exe()?;
    loop {
        thread::sleep(Duration::from_secs(3));
        if process::Command::new(&exe)
            .arg("trusted")
            .status()?
            .success()
        {
            tracing::info!("permission granted; restarting");
            return Err(process::Command::new(&exe)
                .args(env::args_os().skip(1))
                .exec()
                .into());
        }
    }
}

/// The event a tapped `CGEvent` is, if it is one the configuration decides.
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
        SYSTEM_DEFINED => SystemKey::pressed(event)
            .filter(|key| key.is_brightness())
            .map(|_| Event::BrightnessKey),
        _ => None,
    }
}
