//! External displays follow the built-in display's brightness over DDC/CI.

use std::{
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

use ddc::Ddc;
use ddc_macos::Monitor;
use libloading::Library;
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayIsBuiltin, CGGetActiveDisplayList};

/// macOS applies a brightness key after the event; reading waits until presses settle.
const SETTLE: Duration = Duration::from_millis(150);
const LUMINANCE: u8 = 0x10;
const DISPLAY_SERVICES: &str =
    "/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices";

/// A handle to the thread that copies brightness to external displays.
#[derive(Clone)]
pub struct Brightness(Sender<()>);

impl Brightness {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let builtin = match Builtin::load() {
                Ok(builtin) => builtin,
                Err(error) => return tracing::error!("reading brightness: {error:#}"),
            };
            while rx.recv().is_ok() {
                while rx.recv_timeout(SETTLE).is_ok() {}
                match builtin.brightness() {
                    Ok(level) => follow(level),
                    Err(error) => tracing::warn!("reading brightness: {error:#}"),
                }
            }
        });
        Self(tx)
    }

    /// Copies the built-in display's brightness to external displays once presses settle.
    pub fn sync(&self) {
        let _ = self.0.send(());
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
            monitor.set_vcp_feature(
                LUMINANCE,
                (level * f32::from(value.maximum())).round() as u16,
            )
        });
        if let Err(error) = result {
            tracing::warn!(%error, display = monitor.description(), "setting brightness");
        }
    }
}

type GetBrightness = unsafe extern "C" fn(CGDirectDisplayID, *mut f32) -> i32;

/// The built-in display, read through the private DisplayServices framework.
struct Builtin {
    display: CGDirectDisplayID,
    get: GetBrightness,
    _library: Library,
}

impl Builtin {
    fn load() -> anyhow::Result<Self> {
        let mut displays = [0; 16];
        let mut count = 0;
        unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) };
        let display = displays[..count as usize]
            .iter()
            .copied()
            .find(|&display| CGDisplayIsBuiltin(display))
            .ok_or_else(|| anyhow::anyhow!("no built-in display"))?;
        let library = unsafe { Library::new(DISPLAY_SERVICES) }?;
        let get = *unsafe { library.get::<GetBrightness>(b"DisplayServicesGetBrightness") }?;
        Ok(Self {
            display,
            get,
            _library: library,
        })
    }

    /// From 0 to 1.
    fn brightness(&self) -> anyhow::Result<f32> {
        let mut level = 0.0;
        match unsafe { (self.get)(self.display, &mut level) } {
            0 => Ok(level),
            error => anyhow::bail!("DisplayServicesGetBrightness returned {error}"),
        }
    }
}
