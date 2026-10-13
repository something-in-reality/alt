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
            while rx.recv().is_ok() {
                while rx.recv_timeout(SETTLE).is_ok() {}
                if let Err(error) = sync() {
                    tracing::warn!(%error, "syncing brightness");
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

fn sync() -> anyhow::Result<()> {
    let level = builtin()?;
    for mut monitor in Monitor::enumerate()? {
        let maximum = monitor.get_vcp_feature(LUMINANCE)?.maximum();
        monitor.set_vcp_feature(LUMINANCE, (level * f32::from(maximum)).round() as u16)?;
    }
    Ok(())
}

/// The built-in display's brightness, from 0 to 1.
fn builtin() -> anyhow::Result<f32> {
    let mut displays = [0; 16];
    let mut count = 0;
    unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) };
    let display = displays[..count as usize]
        .iter()
        .copied()
        .find(|&display| CGDisplayIsBuiltin(display))
        .ok_or_else(|| anyhow::anyhow!("no built-in display"))?;
    let library = unsafe { Library::new(DISPLAY_SERVICES) }?;
    let get = unsafe {
        library.get::<unsafe extern "C" fn(CGDirectDisplayID, *mut f32) -> i32>(
            b"DisplayServicesGetBrightness",
        )
    }?;
    let mut level = 0.0;
    match unsafe { get(display, &mut level) } {
        0 => Ok(level),
        error => anyhow::bail!("DisplayServicesGetBrightness returned {error}"),
    }
}
