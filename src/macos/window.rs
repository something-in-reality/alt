//! Windows, through the Accessibility API: the focused window, or an application's window.

use std::{
    ptr::{self, NonNull},
    thread,
    time::Duration,
};

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFBoolean, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGDisplayIsBuiltin;
use objc2_foundation::{NSNumber, NSString};

use super::{Error, app};
use crate::action::{AppId, Screen, Side, Window};

/// How long an application's window may take to open after its launch notification.
const OPENING: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(250);

/// A display, its frames in the Accessibility API's coordinates: points from the primary display's
/// top-left corner.
struct Display {
    name: String,
    builtin: bool,
    frame: CGRect,
    visible: CGRect,
}

/// Performs `action` on `app`'s main window once it opens, or without `app` on the focused window.
pub fn perform(action: &Window, app: Option<&AppId>) -> Result<(), Error> {
    let window = match app {
        Some(app) => main_window(app)?,
        None => focused_window()?,
    };
    match action {
        Window::Resize(change) => {
            let size = frame(&window)?.size;
            set_size(
                &window,
                CGSize {
                    width: size.width + f64::from(change.width),
                    height: size.height + f64::from(change.height),
                },
            )
        }
        Window::Fullscreen => set(&window, "AXFullScreen", CFBoolean::new(true)),
        Window::Tile(side) => {
            let displays = displays();
            let visible = displays[containing(&displays, frame(&window)?)].visible;
            let width = visible.size.width / 2.0;
            let x = match side {
                Side::Left => visible.origin.x,
                Side::Right => visible.origin.x + width,
            };
            set_frame(
                &window,
                CGRect {
                    origin: CGPoint {
                        x,
                        y: visible.origin.y,
                    },
                    size: CGSize {
                        width,
                        height: visible.size.height,
                    },
                },
            )
        }
        Window::Display(screen) => {
            if attribute(&window, "AXFullScreen")
                .ok()
                .and_then(|value| value.downcast::<CFBoolean>().ok())
                .is_some_and(|value| value.as_bool())
            {
                return Err(Error::Fullscreen);
            }
            let displays = displays();
            let current = frame(&window)?;
            let from = containing(&displays, current);
            let to = select(&displays, from, screen)?;
            let (from, to) = (displays[from].visible, displays[to].visible);
            // The same place relative to each display's visible frame.
            let scale =
                |value: f64, from_origin: f64, from_length: f64, to_origin: f64, to_length: f64| {
                    to_origin + (value - from_origin) / from_length * to_length
                };
            set_frame(
                &window,
                CGRect {
                    origin: CGPoint {
                        x: scale(
                            current.origin.x,
                            from.origin.x,
                            from.size.width,
                            to.origin.x,
                            to.size.width,
                        ),
                        y: scale(
                            current.origin.y,
                            from.origin.y,
                            from.size.height,
                            to.origin.y,
                            to.size.height,
                        ),
                    },
                    size: CGSize {
                        width: current.size.width / from.size.width * to.size.width,
                        height: current.size.height / from.size.height * to.size.height,
                    },
                },
            )
        }
    }
}

fn attribute(element: &AXUIElement, name: &str) -> Result<CFRetained<CFType>, Error> {
    let mut value = ptr::null();
    let error = unsafe {
        element.copy_attribute_value(&CFString::from_str(name), NonNull::from(&mut value))
    };
    match NonNull::new(value.cast_mut()) {
        Some(value) if error == AXError::Success => Ok(unsafe { CFRetained::from_raw(value) }),
        _ => Err(Error::Accessibility(error.0)),
    }
}

fn set(element: &AXUIElement, name: &str, value: &CFType) -> Result<(), Error> {
    match unsafe { element.set_attribute_value(&CFString::from_str(name), value) } {
        AXError::Success => Ok(()),
        error => Err(Error::Accessibility(error.0)),
    }
}

fn window(element: CFRetained<CFType>) -> Result<CFRetained<AXUIElement>, Error> {
    element
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("the window"))
}

fn focused_window() -> Result<CFRetained<AXUIElement>, Error> {
    let system = unsafe { AXUIElement::new_system_wide() };
    let app = attribute(&system, "AXFocusedApplication")?
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("the focused application"))?;
    window(attribute(&app, "AXFocusedWindow")?)
}

/// `app`'s main window, waiting for it to open.
fn main_window(app: &AppId) -> Result<CFRetained<AXUIElement>, Error> {
    let element = unsafe { AXUIElement::new_application(app::pid(app)?) };
    for _ in 0..OPENING.div_duration_f64(POLL) as u32 {
        if let Ok(main) = attribute(&element, "AXMainWindow") {
            return window(main);
        }
        thread::sleep(POLL);
    }
    Err(Error::Unavailable("the application's window"))
}

fn value<T: Default>(element: &AXUIElement, name: &str, kind: AXValueType) -> Result<T, Error> {
    let value = attribute(element, name)?
        .downcast::<AXValue>()
        .map_err(|_| Error::Unavailable("a window attribute"))?;
    let mut out = T::default();
    match unsafe { value.value(kind, NonNull::from(&mut out).cast()) } {
        true => Ok(out),
        false => Err(Error::Unavailable("a window attribute")),
    }
}

fn frame(window: &AXUIElement) -> Result<CGRect, Error> {
    Ok(CGRect {
        origin: value(window, "AXPosition", AXValueType::CGPoint)?,
        size: value(window, "AXSize", AXValueType::CGSize)?,
    })
}

fn set_size(window: &AXUIElement, size: CGSize) -> Result<(), Error> {
    let value = unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(&size).cast()) }
        .ok_or(Error::Unavailable("a size value"))?;
    set(window, "AXSize", &value)
}

/// Sets the size, the position, then the size again: applications limit a size to the display
/// the window is on.
fn set_frame(window: &AXUIElement, frame: CGRect) -> Result<(), Error> {
    let position =
        unsafe { AXValue::new(AXValueType::CGPoint, NonNull::from(&frame.origin).cast()) }
            .ok_or(Error::Unavailable("a position value"))?;
    set_size(window, frame.size)?;
    set(window, "AXPosition", &position)?;
    set_size(window, frame.size)
}

/// The displays from left to right, read on the main thread, which owns AppKit's screens.
fn displays() -> Vec<Display> {
    let mut displays = Vec::new();
    DispatchQueue::main().exec_sync(|| {
        let mtm = MainThreadMarker::new().expect("the main queue runs on the main thread");
        let screens = NSScreen::screens(mtm);
        let height = screens
            .firstObject()
            .map_or(0.0, |primary| primary.frame().size.height);
        let flip = |rect: CGRect| CGRect {
            origin: CGPoint {
                x: rect.origin.x,
                y: height - rect.origin.y - rect.size.height,
            },
            size: rect.size,
        };
        displays = screens
            .iter()
            .map(|screen| Display {
                name: screen.localizedName().to_string(),
                builtin: screen
                    .deviceDescription()
                    .objectForKey(&NSString::from_str("NSScreenNumber"))
                    .and_then(|number| number.downcast::<NSNumber>().ok())
                    .is_some_and(|number| CGDisplayIsBuiltin(number.unsignedIntValue())),
                frame: flip(screen.frame()),
                visible: flip(screen.visibleFrame()),
            })
            .collect();
    });
    displays.sort_by(|a, b| a.frame.origin.x.total_cmp(&b.frame.origin.x));
    displays
}

/// The display holding the centre of `frame`, or the leftmost.
fn containing(displays: &[Display], frame: CGRect) -> usize {
    let (x, y) = (
        frame.origin.x + frame.size.width / 2.0,
        frame.origin.y + frame.size.height / 2.0,
    );
    displays
        .iter()
        .position(|display| {
            let d = display.frame;
            (d.origin.x..d.origin.x + d.size.width).contains(&x)
                && (d.origin.y..d.origin.y + d.size.height).contains(&y)
        })
        .unwrap_or(0)
}

fn select(displays: &[Display], from: usize, screen: &Screen) -> Result<usize, Error> {
    let count = displays.len();
    match screen {
        Screen::Next => Some((from + 1) % count),
        Screen::Previous => Some((from + count - 1) % count),
        Screen::Builtin => displays.iter().position(|display| display.builtin),
        Screen::Named(name) => displays.iter().position(|display| &display.name == name),
    }
    .ok_or_else(|| Error::NoDisplay(screen.clone()))
}
