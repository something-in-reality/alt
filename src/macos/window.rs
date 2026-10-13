//! The focused window, through the Accessibility API.

use std::ptr::{self, NonNull};

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFRetained, CFString, CFType, CGSize};

use super::Error;
use crate::action::{Dimension, Px, Resize};

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

fn focused_window() -> Result<CFRetained<AXUIElement>, Error> {
    let system = unsafe { AXUIElement::new_system_wide() };
    let app = attribute(&system, "AXFocusedApplication")?;
    let app = app
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("the focused application"))?;
    attribute(&app, "AXFocusedWindow")?
        .downcast::<AXUIElement>()
        .map_err(|_| Error::Unavailable("the focused window"))
}

/// Grows or shrinks the focused window; in split view this moves the divider.
pub fn resize(resize: Resize) -> Result<(), Error> {
    let window = focused_window()?;
    let size = attribute(&window, "AXSize")?
        .downcast::<AXValue>()
        .map_err(|_| Error::Unavailable("the window size"))?;
    let mut current = CGSize::ZERO;
    if !unsafe { size.value(AXValueType::CGSize, NonNull::from(&mut current).cast()) } {
        return Err(Error::Unavailable("the window size"));
    }
    let (dimension, sign) = match resize {
        Resize::Grow(dimension) => (dimension, 1.0),
        Resize::Shrink(dimension) => (dimension, -1.0),
    };
    let target = match dimension {
        Dimension::Width(Px(px)) => CGSize {
            width: current.width + sign * f64::from(px),
            ..current
        },
        Dimension::Height(Px(px)) => CGSize {
            height: current.height + sign * f64::from(px),
            ..current
        },
    };
    let value = unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(&target).cast()) }
        .ok_or(Error::Unavailable("a size value"))?;
    match unsafe { window.set_attribute_value(&CFString::from_str("AXSize"), &value) } {
        AXError::Success => Ok(()),
        error => Err(Error::Accessibility(error.0)),
    }
}
