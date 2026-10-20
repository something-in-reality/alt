//! Windows, through the Accessibility API: the focused window, or an application's window. Tiling
//! and display moves press items of the application's Window menu.

use std::{
    ptr::{self, NonNull},
    thread,
    time::Duration,
};

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType, CGSize};

use super::{Error, app};
use crate::action::{AppId, Screen, Side, Window};

/// Polls for an application's window, which opens after its launch notification: 15 seconds.
const POLL: Duration = Duration::from_millis(250);
const ATTEMPTS: u32 = 60;

/// The Window menu's items that move the window to a display are titled `Move to <display>`.
const MOVE_TO: &str = "Move to ";

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
            let size: CGSize = get(&window, "AXSize", AXValueType::CGSize)?;
            let size = CGSize {
                width: size.width + f64::from(change.width),
                height: size.height + f64::from(change.height),
            };
            put(&window, "AXSize", AXValueType::CGSize, &size)
        }
        Window::Fullscreen => set(&window, "AXFullScreen", CFBoolean::new(true)),
        Window::FullscreenTile(side) => press_menu(
            &application,
            &["Window", "Full Screen Tile"],
            match side {
                Side::Left => "Left of Screen",
                Side::Right => "Right of Screen",
            },
        ),
        Window::Tile(tile) => press_menu(
            &application,
            &["Window", "Move & Resize"],
            &tile.to_string(),
        ),
        Window::Fill => press_menu(&application, &["Window"], "Fill"),
        Window::Center => press_menu(&application, &["Window"], "Center"),
        Window::Display(screen) => {
            let items = menu(&application, &["Window"])?;
            let targets: Vec<_> = items
                .iter()
                .filter_map(|item| {
                    let title = title(item)?;
                    Some((item, title.strip_prefix(MOVE_TO)?.to_owned()))
                })
                .collect();
            let names: Vec<_> = targets.iter().map(|(_, name)| name.as_str()).collect();
            let (item, _) = select(screen, &names)
                .map(|index| &targets[index])
                .ok_or_else(|| Error::NoDisplay(screen.clone()))?;
            press(item)
        }
    }
}

/// The index of the display `screen` names among `names`, the Window menu's displays in menu
/// order: the first for `next`, the last for `previous`.
fn select(screen: &Screen, names: &[&str]) -> Option<usize> {
    match screen {
        Screen::Next => (!names.is_empty()).then_some(0),
        Screen::Previous => names.len().checked_sub(1),
        Screen::Builtin => names.iter().position(|name| name.starts_with("Built-in")),
        Screen::Named(wanted) => names.iter().position(|name| name == wanted),
    }
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

/// The items of `item`'s menu, its only child.
fn submenu(item: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    children(item)
        .first()
        .map(|menu| children(menu))
        .unwrap_or_default()
}

/// The items of the menu at `path` in `application`'s menu bar; an empty `path` is the menu bar.
fn menu(application: &AXUIElement, path: &[&str]) -> Result<Vec<CFRetained<AXUIElement>>, Error> {
    let bar = element(attribute(application, "AXMenuBar")?)?;
    let mut items = children(&bar);
    for (depth, name) in path.iter().enumerate() {
        items = items
            .iter()
            .find(|item| title(item).as_deref() == Some(*name))
            .map(|item| submenu(item))
            .ok_or_else(|| Error::NoMenuItem(path[..=depth].join(" > ")))?;
    }
    Ok(items)
}

/// Presses the item `name` of the menu at `path` in `application`'s menu bar.
fn press_menu(application: &AXUIElement, path: &[&str], name: &str) -> Result<(), Error> {
    let items = menu(application, path)?;
    let item = items
        .iter()
        .find(|item| title(item).as_deref() == Some(name))
        .ok_or_else(|| Error::NoMenuItem(format!("{} > {name}", path.join(" > "))))?;
    press(item)
}

fn press(item: &AXUIElement) -> Result<(), Error> {
    check("AXPress", unsafe {
        item.perform_action(&CFString::from_str("AXPress"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_are_selected_in_menu_order() {
        let names = ["Built-in Retina Display", "MSI MP341CQ", "LG HDR 4K"];
        let select = |screen: &str| select(&screen.parse().unwrap(), &names);
        assert_eq!(select("next"), Some(0));
        assert_eq!(select("previous"), Some(2));
        assert_eq!(select("builtin"), Some(0));
        assert_eq!(select("MSI MP341CQ"), Some(1));
        assert_eq!(select("Unknown"), None);
        assert_eq!(super::select(&Screen::Next, &[]), None);
        assert_eq!(super::select(&Screen::Previous, &[]), None);
    }
}
