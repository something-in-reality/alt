//! Windows, through the Accessibility API: the focused window, or an application's window.

use std::{
    ptr::{self, NonNull},
    thread,
    time::Duration,
};

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize,
};

use super::{
    Error, app,
    screen::{Display, displays},
};
use crate::action::{AppId, Screen, Side, Window};

/// Polls for an application's window, which opens after its launch notification: 15 seconds.
const POLL: Duration = Duration::from_millis(250);
const ATTEMPTS: u32 = 60;

/// Performs `action` on `app`'s main window once it opens, or without `app` on the focused window.
pub fn perform(action: &Window, app: Option<&AppId>) -> Result<(), Error> {
    let (application, window) = match app {
        Some(app) => {
            let application = unsafe { AXUIElement::new_application(app::pid(app)?) };
            let window = main_window(&application)?;
            (application, window)
        }
        None => {
            let system = unsafe { AXUIElement::new_system_wide() };
            let application = element(attribute(&system, "AXFocusedApplication")?)?;
            let window = element(attribute(&application, "AXFocusedWindow")?)?;
            (application, window)
        }
    };
    match action {
        Window::Resize(change) => {
            let size = frame(&window)?.size;
            let size = CGSize {
                width: size.width + f64::from(change.width),
                height: size.height + f64::from(change.height),
            };
            put(&window, "AXSize", AXValueType::CGSize, &size)
        }
        Window::Fullscreen => set(&window, "AXFullScreen", CFBoolean::new(true)),
        Window::FullscreenTile(side) => press_menu(
            &application,
            &[
                "Window",
                "Full Screen Tile",
                match side {
                    Side::Left => "Left of Screen",
                    Side::Right => "Right of Screen",
                },
            ],
        ),
        Window::Tile(side) => {
            let displays = displays()?;
            let visible = displays[containing(&displays, frame(&window)?)].visible;
            let width = visible.size.width / 2.0;
            let x = match side {
                Side::Left => visible.origin.x,
                Side::Right => visible.origin.x + width,
            };
            set_frame(
                &window,
                CGRect::new(
                    CGPoint::new(x, visible.origin.y),
                    CGSize::new(width, visible.size.height),
                ),
            )
        }
        Window::Display(screen) => {
            let fullscreen = attribute(&window, "AXFullScreen")
                .ok()
                .and_then(|value| value.downcast::<CFBoolean>().ok())
                .is_some_and(|value| value.as_bool());
            if fullscreen {
                return Err(Error::Fullscreen);
            }
            let displays = displays()?;
            let current = frame(&window)?;
            let from = containing(&displays, current);
            let to = select(&displays, from, screen)?;
            set_frame(
                &window,
                remap(current, displays[from].visible, displays[to].visible),
            )
        }
    }
}

/// `rect` at the same place relative to `to` as it is to `from`.
fn remap(rect: CGRect, from: CGRect, to: CGRect) -> CGRect {
    let x = |value: f64| (value - from.origin.x) / from.size.width * to.size.width;
    let y = |value: f64| (value - from.origin.y) / from.size.height * to.size.height;
    CGRect::new(
        CGPoint::new(
            to.origin.x + x(rect.origin.x),
            to.origin.y + y(rect.origin.y),
        ),
        CGSize::new(
            rect.size.width / from.size.width * to.size.width,
            rect.size.height / from.size.height * to.size.height,
        ),
    )
}

fn check(name: &str, error: AXError) -> Result<(), Error> {
    match error {
        AXError::Success => Ok(()),
        error => Err(Error::Accessibility {
            name: name.to_owned(),
            code: error.0,
        }),
    }
}

fn attribute(element: &AXUIElement, name: &str) -> Result<CFRetained<CFType>, Error> {
    let mut value = ptr::null();
    let error = unsafe {
        element.copy_attribute_value(&CFString::from_str(name), NonNull::from(&mut value))
    };
    check(name, error)?;
    NonNull::new(value.cast_mut())
        .map(|value| unsafe { CFRetained::from_raw(value) })
        .ok_or(Error::Unavailable("an attribute value"))
}

fn set(element: &AXUIElement, name: &str, value: &CFType) -> Result<(), Error> {
    check(name, unsafe {
        element.set_attribute_value(&CFString::from_str(name), value)
    })
}

/// Reads the `AXValue` attribute `name` as a `T` of `kind`.
fn get<T: Default>(element: &AXUIElement, name: &str, kind: AXValueType) -> Result<T, Error> {
    let value = attribute(element, name)?
        .downcast::<AXValue>()
        .map_err(|_| Error::Unavailable("an AXValue"))?;
    let mut out = T::default();
    if unsafe { value.value(kind, NonNull::from(&mut out).cast()) } {
        Ok(out)
    } else {
        Err(Error::Unavailable("an AXValue"))
    }
}

/// Sets the `AXValue` attribute `name` to `value`, a `T` of `kind`.
fn put<T>(element: &AXUIElement, name: &str, kind: AXValueType, value: &T) -> Result<(), Error> {
    let value = unsafe { AXValue::new(kind, NonNull::from(value).cast()) }
        .ok_or(Error::Unavailable("an AXValue"))?;
    set(element, name, &value)
}

fn element(value: CFRetained<CFType>) -> Result<CFRetained<AXUIElement>, Error> {
    value
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("an accessibility element"))
}

/// The main window of `application`, waiting for it to open.
fn main_window(application: &AXUIElement) -> Result<CFRetained<AXUIElement>, Error> {
    for _ in 0..ATTEMPTS {
        if let Ok(main) = attribute(application, "AXMainWindow") {
            return element(main);
        }
        thread::sleep(POLL);
    }
    Err(Error::Unavailable("the application's window"))
}

fn frame(window: &AXUIElement) -> Result<CGRect, Error> {
    Ok(CGRect::new(
        get(window, "AXPosition", AXValueType::CGPoint)?,
        get(window, "AXSize", AXValueType::CGSize)?,
    ))
}

/// Sets the size, the position, then the size again: applications limit a size to the display
/// the window is on.
fn set_frame(window: &AXUIElement, frame: CGRect) -> Result<(), Error> {
    put(window, "AXSize", AXValueType::CGSize, &frame.size)?;
    put(window, "AXPosition", AXValueType::CGPoint, &frame.origin)?;
    put(window, "AXSize", AXValueType::CGSize, &frame.size)
}

fn children(parent: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    attribute(parent, "AXChildren")
        .ok()
        .and_then(|value| value.downcast::<CFArray>().ok())
        .map(|array| {
            // SAFETY: AXChildren holds accessibility elements, which are CF types.
            let array = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) };
            array
                .iter()
                .filter_map(|child| child.downcast::<AXUIElement>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn title(item: &AXUIElement) -> Option<String> {
    let title = attribute(item, "AXTitle")
        .ok()?
        .downcast::<CFString>()
        .ok()?;
    Some(title.to_string())
}

/// Presses the item at `path` in `application`'s menu bar: a menu bar item, then its menu's items,
/// each item holding its menu as its only child.
fn press_menu(application: &AXUIElement, path: &[&str]) -> Result<(), Error> {
    let missing = || Error::NoMenuItem(path.join(" > "));
    let bar = element(attribute(application, "AXMenuBar")?)?;
    let mut items = children(&bar);
    let mut item = None;
    for name in path {
        let found = items
            .into_iter()
            .find(|item| title(item).as_deref() == Some(*name))
            .ok_or_else(missing)?;
        items = children(&found)
            .first()
            .map(|menu| children(menu))
            .unwrap_or_default();
        item = Some(found);
    }
    let item = item.ok_or_else(missing)?;
    check("AXPress", unsafe {
        item.perform_action(&CFString::from_str("AXPress"))
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remapped_frames_keep_their_place() {
        let laptop = CGRect::new(CGPoint::new(0.0, 25.0), CGSize::new(1512.0, 950.0));
        let external = CGRect::new(CGPoint::new(1512.0, -200.0), CGSize::new(3440.0, 1415.0));
        let left_half = CGRect::new(laptop.origin, CGSize::new(756.0, 950.0));
        let moved = remap(left_half, laptop, external);
        assert_eq!(moved.origin, external.origin);
        assert_eq!(moved.size, CGSize::new(1720.0, 1415.0));
    }
}
