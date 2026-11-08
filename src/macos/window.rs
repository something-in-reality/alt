//! Windows, through the Accessibility API: the focused window, or an application's window. Tiling
//! and display moves press items of the application's Window menu.

use std::{
    cell::Cell,
    ffi::c_void,
    ptr::{self, NonNull},
    rc::Rc,
    time::Duration,
};

use block2::RcBlock;
use objc2_application_services::{AXError, AXObserver, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFAbsoluteTimeGetCurrent, CFArray, CFBoolean, CFRetained, CFRunLoop, CFRunLoopSource,
    CFRunLoopTimer, CFString, CFType, CGSize, kCFRunLoopCommonModes,
};

use super::Error;
use crate::action::{Screen, Side, Size, Tile};

/// How long a launched application has to open its main window.
const OPEN: Duration = Duration::from_secs(15);

/// The Accessibility notifications that follow a window opening.
const OPENED: [&str; 2] = ["AXWindowCreated", "AXMainWindowChanged"];

/// How long one Accessibility call to an application waits for a reply, in seconds.
const TIMEOUT: f32 = 0.5;

/// The Window menu's items that move the window to a display are titled `Move to <display>`.
const MOVE_TO: &str = "Move to ";

/// A window and its application, whose menu bar tiling and display moves press.
pub struct Window {
    application: CFRetained<AXUIElement>,
    window: CFRetained<AXUIElement>,
}

impl Window {
    /// The main window of process `pid`, or without `pid` the focused window.
    pub fn new(pid: Option<i32>) -> Result<Self, Error> {
        let (application, window) = match pid {
            Some(pid) => {
                let application = unsafe { AXUIElement::new_application(pid) };
                bound(&application)?;
                let window = main_window(&application)?;
                (application, window)
            }
            None => {
                let system = unsafe { AXUIElement::new_system_wide() };
                bound(&system)?;
                let application = element(attribute(&system, "AXFocusedApplication")?)?;
                bound(&application)?;
                let window = element(attribute(&application, "AXFocusedWindow")?)?;
                (application, window)
            }
        };
        Ok(Self {
            application,
            window,
        })
    }

    pub fn resize(&self, change: Size) -> Result<(), Error> {
        let current = size(&self.window)?;
        set_size(
            &self.window,
            &CGSize {
                width: current.width + f64::from(change.width),
                height: current.height + f64::from(change.height),
            },
        )
    }

    pub fn full_screen(&self) -> Result<(), Error> {
        set(&self.window, "AXFullScreen", CFBoolean::new(true))
    }

    pub fn full_screen_tile(&self, side: Side) -> Result<(), Error> {
        press_menu(
            &self.application,
            &["Window", "Full Screen Tile"],
            match side {
                Side::Left => "Left of Screen",
                Side::Right => "Right of Screen",
            },
        )
    }

    pub fn tile(&self, tile: Tile) -> Result<(), Error> {
        press_menu(
            &self.application,
            &["Window", "Move & Resize"],
            &tile.to_string(),
        )
    }

    pub fn fill(&self) -> Result<(), Error> {
        press_menu(&self.application, &["Window"], "Fill")
    }

    pub fn center(&self) -> Result<(), Error> {
        press_menu(&self.application, &["Window"], "Center")
    }

    pub fn move_to(&self, screen: &Screen) -> Result<(), Error> {
        let items = menu(&self.application, &["Window"])?;
        let targets: Vec<_> = items
            .iter()
            .filter_map(|item| {
                let title = title(item)?;
                Some((item, title.strip_prefix(MOVE_TO)?.to_owned()))
            })
            .collect();
        let names: Vec<_> = targets.iter().map(|(_, name)| name.as_str()).collect();
        let (item, name) = select(screen, &names)
            .map(|index| &targets[index])
            .ok_or_else(|| Error::NoDisplay(screen.clone()))?;
        press(item, &format!("Window > {MOVE_TO}{name}"))
    }
}

/// The index of the display `screen` names among `names`, the Window menu's displays in menu
/// order: the first for `next`, the last for `previous`.
fn select(screen: &Screen, names: &[&str]) -> Option<usize> {
    match screen {
        Screen::Next => (!names.is_empty()).then_some(0),
        Screen::Previous => names.len().checked_sub(1),
        Screen::BuiltIn => names.iter().position(|name| name.starts_with("Built-in")),
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

/// Limits the calls to `element`'s application to `TIMEOUT`.
fn bound(element: &AXUIElement) -> Result<(), Error> {
    check("AXUIElementSetMessagingTimeout", unsafe {
        element.set_messaging_timeout(TIMEOUT)
    })
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

/// Reads the `AXSize` attribute.
fn size(element: &AXUIElement) -> Result<CGSize, Error> {
    let value = attribute(element, "AXSize")?
        .downcast::<AXValue>()
        .map_err(|_| Error::Unavailable("an AXValue"))?;
    let mut size = CGSize::default();
    // SAFETY: the pointer is to a `CGSize`, the type of `AXValueType::CGSize`.
    if unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) } {
        Ok(size)
    } else {
        Err(Error::Unavailable("an AXValue"))
    }
}

/// Sets the `AXSize` attribute.
fn set_size(element: &AXUIElement, size: &CGSize) -> Result<(), Error> {
    // SAFETY: the pointer is to a `CGSize`, the type of `AXValueType::CGSize`.
    let value = unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(size).cast()) }
        .ok_or(Error::Unavailable("an AXValue"))?;
    set(element, "AXSize", &value)
}

fn element(value: CFRetained<CFType>) -> Result<CFRetained<AXUIElement>, Error> {
    value
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("an accessibility element"))
}

/// The main window of `application`.
fn main_window(application: &AXUIElement) -> Result<CFRetained<AXUIElement>, Error> {
    element(attribute(application, "AXMainWindow")?)
}

/// A wait for the main window of one process, on the main run loop.
struct Wait {
    application: CFRetained<AXUIElement>,
    observer: CFRetained<AXObserver>,
    then: Cell<Option<Box<dyn FnOnce()>>>,
}

impl Wait {
    fn source(&self) -> CFRetained<CFRunLoopSource> {
        unsafe { self.observer.run_loop_source() }
    }

    /// Ends the wait, returning the continuation unless the wait already ended.
    fn end(&self) -> Option<Box<dyn FnOnce()>> {
        self.source().invalidate();
        self.then.take()
    }

    /// Continues when the application has a main window.
    fn check(&self) {
        if main_window(&self.application).is_ok()
            && let Some(then) = self.end()
        {
            then();
        }
    }
}

/// Runs `then` on the main run loop once process `pid` has a main window: at once when it has one,
/// otherwise when the application reports a window; after `OPEN` without one, it is dropped.
pub fn when_open(pid: i32, then: impl FnOnce() + 'static) -> Result<(), Error> {
    let application = unsafe { AXUIElement::new_application(pid) };
    bound(&application)?;
    let mut observer = ptr::null_mut();
    check("AXObserverCreate", unsafe {
        AXObserver::create(pid, Some(opened), NonNull::from(&mut observer))
    })?;
    let observer = NonNull::new(observer)
        .map(|observer| unsafe { CFRetained::from_raw(observer) })
        .ok_or(Error::Unavailable("an Accessibility observer"))?;
    let wait = Rc::new(Wait {
        application,
        observer,
        then: Cell::new(Some(Box::new(then))),
    });
    let main = CFRunLoop::main().ok_or(Error::Unavailable("the main run loop"))?;
    for name in OPENED {
        check(name, unsafe {
            wait.observer.add_notification(
                &wait.application,
                &CFString::from_str(name),
                Rc::as_ptr(&wait).cast_mut().cast(),
            )
        })?;
    }
    let timeout = RcBlock::new({
        let wait = wait.clone();
        move |_: *mut CFRunLoopTimer| {
            if wait.end().is_some() {
                tracing::warn!(pid, "the application opened no window");
            }
        }
    });
    let timer = unsafe {
        CFRunLoopTimer::with_handler(
            None,
            CFAbsoluteTimeGetCurrent() + OPEN.as_secs_f64(),
            0.0,
            0,
            0,
            Some(&timeout),
        )
    }
    .ok_or(Error::Unavailable("a timer"))?;
    main.add_timer(Some(&timer), unsafe { kCFRunLoopCommonModes });
    main.add_source(Some(&wait.source()), unsafe { kCFRunLoopCommonModes });
    wait.check();
    Ok(())
}

/// Accessibility callback: the application reported a window; `wait` outlives its source, as the
/// timer holds the `Wait` until `OPEN` passes.
unsafe extern "C-unwind" fn opened(
    _: NonNull<AXObserver>,
    _: NonNull<AXUIElement>,
    _: NonNull<CFString>,
    wait: *mut c_void,
) {
    unsafe { &*wait.cast::<Wait>() }.check();
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
    let item_path = format!("{} > {name}", path.join(" > "));
    let item = items
        .iter()
        .find(|item| title(item).as_deref() == Some(name))
        .ok_or_else(|| Error::NoMenuItem(item_path.clone()))?;
    press(item, &item_path)
}

/// Presses the menu item at `path`, which must be enabled.
fn press(item: &AXUIElement, path: &str) -> Result<(), Error> {
    let enabled = attribute(item, "AXEnabled")?
        .downcast::<CFBoolean>()
        .map_err(|_| Error::Unavailable("a boolean"))?;
    if !enabled.as_bool() {
        return Err(Error::DisabledMenuItem(path.to_owned()));
    }
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
        assert_eq!(select("built_in"), Some(0));
        assert_eq!(select("MSI MP341CQ"), Some(1));
        assert_eq!(select("Unknown"), None);
        assert_eq!(super::select(&Screen::Next, &[]), None);
        assert_eq!(super::select(&Screen::Previous, &[]), None);
    }
}
