//! The login session's keys, scrolling and system keys, from an event tap, and application
//! launches, from NSWorkspace.

use std::{
    cell::RefCell,
    convert::Infallible,
    env,
    os::unix::process::CommandExt,
    process,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceWillLaunchApplicationNotification,
};
use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use objc2_core_foundation::{CFBoolean, CFDictionary};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventType};
use objc2_foundation::{NSString, NSUserDefaults};

use super::{
    Error, app, keymap, scroll, synth,
    system_key::{SYSTEM_DEFINED, SystemKey},
    tap::Tap,
};
use crate::{
    action::BundleId,
    combo::Combo,
    event::{Context, Event, Verdict},
};

/// The live platform state: the front application, which NSWorkspace's notifications keep current,
/// so a decision reads it without a call to another process.
pub(super) struct Front {
    front: RefCell<Option<BundleId>>,
}

/// Runs until the process ends, deciding each event with `handle`.
pub(super) fn run(
    handle: impl Fn(Event, &Front) -> Verdict + 'static,
) -> Result<Infallible, Error> {
    let mtm = MainThreadMarker::new().expect("the session runs on the main thread");
    trust()?;
    let apps = Rc::new(Front {
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
                Verdict::Reverse => {
                    scroll::reverse(event);
                    true
                }
            }
        },
    )
    .ok_or(Error::Tap)?;
    watchdog();
    tracing::info!("running");
    // AppKit's event loop, which delivers the NSWorkspace notifications and the UserNotifications
    // authorization prompt.
    NSApplication::sharedApplication(mtm).run();
    Err(Error::RunLoopEnded)
}

impl Context for Front {
    fn front(&self) -> Option<BundleId> {
        self.front.borrow().clone()
    }

    /// Reads the preference on each call, which System Settings changes while alt runs; an unset
    /// preference is natural scrolling, the macOS default.
    fn natural_scrolling(&self) -> bool {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str("com.apple.swipescrolldirection");
        defaults.objectForKey(&key).is_none() || defaults.boolForKey(&key)
    }
}

/// Exits when the main thread, which decides each tapped event, stops responding: an active tap
/// holds all input while its callback runs, and the tap ends with the process.
fn watchdog() {
    const LIMIT: Duration = Duration::from_secs(2);
    let alive = Arc::new(AtomicBool::new(true));
    thread::spawn(move || {
        loop {
            alive.store(false, Ordering::Relaxed);
            let beat = alive.clone();
            DispatchQueue::main().exec_async(move || beat.store(true, Ordering::Relaxed));
            thread::sleep(LIMIT);
            if !alive.load(Ordering::Relaxed) {
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
