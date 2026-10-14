//! External displays follow the built-in display's brightness over DDC/CI.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use ddc::Ddc;
use ddc_macos::Monitor;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained, DispatchTime};
use libloading::Library;
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayIsBuiltin, CGGetActiveDisplayList};

use super::Error;

/// macOS applies a brightness key after its event; syncing waits until presses settle.
const SETTLE: Duration = Duration::from_millis(150);
const LUMINANCE: u8 = 0x10;
const DISPLAY_SERVICES: &str =
    "/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices";

type GetBrightness = unsafe extern "C" fn(CGDirectDisplayID, *mut f32) -> i32;

/// External displays, which follow the built-in display's brightness.
pub struct Displays {
    queue: DispatchRetained<DispatchQueue>,
    builtin: Arc<Builtin>,
    /// Counts sync requests; a delayed sync runs only if no later request followed it.
    requests: Arc<AtomicU64>,
}

/// The built-in display's brightness, through the private DisplayServices framework.
struct Builtin {
    get: GetBrightness,
    _library: Library,
}

impl Displays {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            queue: DispatchQueue::new("alt.display", DispatchQueueAttr::SERIAL),
            builtin: Arc::new(Builtin::load()?),
            requests: Arc::default(),
        })
    }

    /// Copies the built-in display's brightness to external displays once presses settle.
    pub fn sync(&self) {
        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        let (builtin, requests) = (self.builtin.clone(), self.requests.clone());
        let when = DispatchTime::try_from(SETTLE).unwrap_or(DispatchTime::NOW);
        let scheduled = self.queue.after(when, move || {
            if requests.load(Ordering::Relaxed) == request {
                match builtin.brightness() {
                    Ok(level) => follow(level),
                    Err(error) => tracing::warn!(%error, "reading brightness"),
                }
            }
        });
        if let Err(error) = scheduled {
            tracing::warn!(?error, "scheduling a brightness sync");
        }
    }
}

impl Builtin {
    fn load() -> Result<Self, Error> {
        let library = unsafe { Library::new(DISPLAY_SERVICES) }?;
        let get = *unsafe { library.get::<GetBrightness>(b"DisplayServicesGetBrightness") }?;
        Ok(Self {
            get,
            _library: library,
        })
    }

    /// From 0 to 1.
    fn brightness(&self) -> Result<f32, Error> {
        let mut displays = [0; 16];
        let mut count = 0;
        unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) };
        let display = displays[..count as usize]
            .iter()
            .copied()
            .find(|&display| CGDisplayIsBuiltin(display))
            .ok_or(Error::NoBuiltinDisplay)?;
        let mut level = 0.0;
        match unsafe { (self.get)(display, &mut level) } {
            0 => Ok(level),
            error => Err(Error::DisplayServices(error)),
        }
    }
}

/// Sets each external display to `level`, from 0 to 1.
fn follow(level: f32) {
    let monitors = match Monitor::enumerate() {
        Ok(monitors) => monitors,
        Err(error) => return tracing::warn!(%error, "listing displays"),
    };
    for mut monitor in monitors {
        let result = monitor.get_vcp_feature(LUMINANCE).and_then(|value| {
            let target = (level * f32::from(value.maximum())).round() as u16;
            monitor.set_vcp_feature(LUMINANCE, target)
        });
        if let Err(error) = result {
            tracing::warn!(%error, display = monitor.description(), "setting brightness");
        }
    }
}
