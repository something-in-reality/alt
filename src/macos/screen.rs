//! AppKit's screens as displays, read on the main thread.

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayIsBuiltin};
use objc2_foundation::{NSNumber, NSString};

use super::Error;

/// A display, its frames in the Accessibility API's coordinates: points from the primary display's
/// top-left corner.
pub struct Display {
    pub id: CGDirectDisplayID,
    pub name: String,
    pub builtin: bool,
    pub frame: CGRect,
    pub visible: CGRect,
}

/// The displays from left to right, read on the main thread, which owns AppKit's screens; callers
/// run on other threads.
pub fn displays() -> Result<Vec<Display>, Error> {
    let mut displays = Vec::new();
    DispatchQueue::main().exec_sync(|| {
        let mtm = MainThreadMarker::new().expect("the main queue runs on the main thread");
        let screens = NSScreen::screens(mtm);
        let height = screens
            .firstObject()
            .map_or(0.0, |primary| primary.frame().size.height);
        let flip = |rect: CGRect| {
            CGRect::new(
                CGPoint::new(rect.origin.x, height - rect.origin.y - rect.size.height),
                rect.size,
            )
        };
        displays = screens
            .iter()
            .filter_map(|screen| {
                let id = screen
                    .deviceDescription()
                    .objectForKey(&NSString::from_str("NSScreenNumber"))?
                    .downcast::<NSNumber>()
                    .ok()?
                    .unsignedIntValue();
                Some(Display {
                    id,
                    name: screen.localizedName().to_string(),
                    builtin: CGDisplayIsBuiltin(id),
                    frame: flip(screen.frame()),
                    visible: flip(screen.visibleFrame()),
                })
            })
            .collect();
    });
    if displays.is_empty() {
        return Err(Error::Unavailable("a display"));
    }
    displays.sort_by(|a, b| a.frame.origin.x.total_cmp(&b.frame.origin.x));
    Ok(displays)
}
