//! Displays rendered at twice their resolution: each mirrors a virtual display of twice its
//! pixels, which the system scales down to the panel.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    sync::{Arc, Mutex, PoisonError},
};

use arc_swap::ArcSwap;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::autoreleasepool;
use objc2_core_graphics::{
    CGBeginDisplayConfiguration, CGCancelDisplayConfiguration, CGCompleteDisplayConfiguration,
    CGConfigureDisplayMirrorOfDisplay, CGConfigureDisplayWithDisplayMode, CGConfigureOption,
    CGDirectDisplayID, CGDisplayChangeSummaryFlags, CGDisplayCopyDisplayMode,
    CGDisplayMirrorsDisplay, CGDisplayMode, CGDisplayModelNumber,
    CGDisplayRegisterReconfigurationCallback, CGDisplayScreenSize, CGDisplaySerialNumber, CGError,
    CGGetOnlineDisplayList,
};

use super::{
    Error,
    screen::displays,
    virtual_display::{Descriptor, VirtualDisplay},
};
use crate::config::Config;

/// The vendor of virtual displays, so macOS keeps their settings apart from the panels'.
const VENDOR: u32 = 0x616c74;
/// Used when a mode reports no refresh rate.
const REFRESH_RATE: f64 = 60.0;

/// Keeps a virtual display mirrored for each display the configuration lists in `display.hidpi`.
#[derive(Clone)]
pub struct HiDpi(Arc<State>);

struct State {
    config: Arc<ArcSwap<Config>>,
    /// Runs every reconciliation in order, and receives the virtual displays' callbacks.
    queue: DispatchRetained<DispatchQueue>,
    mirrors: Mutex<BTreeMap<String, Mirror>>,
}

/// The attempt to mirror a display. A refused attempt holds no display, and is not repeated until
/// the display disconnects or leaves the configuration.
struct Mirror {
    physical: CGDirectDisplayID,
    display: Option<VirtualDisplay>,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Drop(String),
    Create {
        name: String,
        physical: CGDirectDisplayID,
    },
    Mirror(String),
}

impl HiDpi {
    /// Registers for display changes, which last for the process, and reconciles now. The main
    /// thread's run loop delivers the changes.
    pub fn new(config: Arc<ArcSwap<Config>>) -> Result<Self, Error> {
        let state = Arc::new(State {
            config,
            queue: DispatchQueue::new("alt.hidpi", DispatchQueueAttr::SERIAL),
            mirrors: Mutex::default(),
        });
        let info = Arc::into_raw(state.clone());
        let registered = check("registering for display changes", unsafe {
            CGDisplayRegisterReconfigurationCallback(Some(reconfigured), info.cast_mut().cast())
        });
        if let Err(error) = registered {
            // SAFETY: `info` came from `Arc::into_raw` above, and no callback holds it.
            drop(unsafe { Arc::from_raw(info) });
            return Err(error);
        }
        let hidpi = Self(state);
        hidpi.reconcile();
        Ok(hidpi)
    }

    /// Matches the virtual displays to the configuration and the connected displays, in order on
    /// the queue.
    pub fn reconcile(&self) {
        let state = self.0.clone();
        self.0
            .queue
            .exec_async(move || autoreleasepool(|_| state.reconcile()));
    }
}

/// Reconciles after each completed display configuration, including those of this module's own:
/// a new virtual display has its modes once its addition is delivered.
unsafe extern "C-unwind" fn reconfigured(
    _: CGDirectDisplayID,
    flags: CGDisplayChangeSummaryFlags,
    info: *mut c_void,
) {
    if flags.contains(CGDisplayChangeSummaryFlags::BeginConfigurationFlag) {
        return;
    }
    // SAFETY: `info` is the `State` that `HiDpi::new` leaked; the callback borrows its count.
    let state = ManuallyDrop::new(unsafe { Arc::from_raw(info.cast_const().cast::<State>()) });
    HiDpi(Arc::clone(&state)).reconcile();
}

impl State {
    fn reconcile(&self) {
        let desired = self.config.load().display.hidpi.clone();
        let mut mirrors = self.mirrors.lock().unwrap_or_else(PoisonError::into_inner);
        if desired.is_empty() && mirrors.is_empty() {
            return;
        }
        let present = match present(&mirrors) {
            Ok(present) => present,
            Err(error) => return tracing::warn!(%error, "listing displays"),
        };
        let current = mirrors
            .iter()
            .map(|(name, mirror)| (name.clone(), mirror.physical))
            .collect();
        let unmirrored = mirrors
            .iter()
            .filter(|(_, mirror)| {
                mirror
                    .display
                    .as_ref()
                    .is_some_and(|display| CGDisplayMirrorsDisplay(mirror.physical) != display.id())
            })
            .map(|(name, _)| name.clone())
            .collect();
        for step in plan(&desired, &present, &current, &unmirrored) {
            match step {
                Step::Drop(name) => {
                    mirrors.remove(&name);
                    tracing::info!(%name, "dropped the HiDPI display");
                }
                Step::Create { name, physical } => {
                    let display = virtual_display(&name, physical, &self.queue)
                        .inspect(|_| tracing::info!(%name, "created the HiDPI display"))
                        .inspect_err(|error| tracing::warn!(%error, %name, "creating"))
                        .ok();
                    mirrors.insert(name, Mirror { physical, display });
                }
                Step::Mirror(name) => {
                    let Some(Mirror {
                        physical,
                        display: Some(display),
                    }) = mirrors.get(&name)
                    else {
                        continue;
                    };
                    match mirror(*physical, display) {
                        Ok(()) => tracing::info!(%name, "rendering at twice the resolution"),
                        Err(Error::Unavailable(_)) => {
                            tracing::debug!(%name, "waiting for the HiDPI display's modes");
                        }
                        Err(error) => tracing::warn!(%error, %name, "mirroring"),
                    }
                }
            }
        }
    }
}

/// The steps from `current` mirrors to those `desired` of the `present` displays, which map names
/// to IDs: a mirror ends when its display is unlisted, gone, or has another ID; a listed display
/// without a mirror gets a virtual display; a kept virtual display that its display does not
/// mirror, one of `unmirrored`, is mirrored.
fn plan(
    desired: &BTreeSet<String>,
    present: &BTreeMap<String, CGDirectDisplayID>,
    current: &BTreeMap<String, CGDirectDisplayID>,
    unmirrored: &BTreeSet<String>,
) -> Vec<Step> {
    let wanted = |name: &String| desired.contains(name).then(|| present.get(name)).flatten();
    let drops = current
        .iter()
        .filter(|(name, id)| wanted(name) != Some(id))
        .map(|(name, _)| Step::Drop(name.clone()));
    let creates = desired.iter().filter_map(|name| {
        let id = present.get(name)?;
        (current.get(name) != Some(id)).then(|| Step::Create {
            name: name.clone(),
            physical: *id,
        })
    });
    let mirrors = unmirrored
        .iter()
        .filter(|name| wanted(name).is_some_and(|id| current.get(*name) == Some(id)))
        .map(|name| Step::Mirror(name.clone()));
    drops.chain(creates).chain(mirrors).collect()
}

/// The connected displays by name, other than the virtual displays. AppKit lists a mirrored
/// display as its virtual display, which takes its name, so a mirrored display keeps the name it
/// was given.
fn present(
    mirrors: &BTreeMap<String, Mirror>,
) -> Result<BTreeMap<String, CGDirectDisplayID>, Error> {
    let online = online()?;
    let virtuals: BTreeSet<_> = mirrors
        .values()
        .filter_map(|mirror| mirror.display.as_ref().map(VirtualDisplay::id))
        .collect();
    let mut present: BTreeMap<_, _> = displays()?
        .into_iter()
        .filter(|display| !virtuals.contains(&display.id))
        .map(|display| (display.name, display.id))
        .collect();
    present.extend(
        mirrors
            .iter()
            .filter(|(_, mirror)| online.contains(&mirror.physical))
            .map(|(name, mirror)| (name.clone(), mirror.physical)),
    );
    Ok(present)
}

/// Displays that are connected, mirrored ones included.
fn online() -> Result<Vec<CGDirectDisplayID>, Error> {
    let mut displays = [0; 32];
    let mut count = 0;
    check("listing displays", unsafe {
        CGGetOnlineDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count)
    })?;
    Ok(displays[..count as usize].to_vec())
}

/// A virtual display of twice the pixels of `physical`, named after it.
fn virtual_display(
    name: &str,
    physical: CGDirectDisplayID,
    queue: &DispatchQueue,
) -> Result<VirtualDisplay, Error> {
    let mode = CGDisplayCopyDisplayMode(physical).ok_or(Error::Unavailable("a display mode"))?;
    let refresh_rate = CGDisplayMode::refresh_rate(Some(&mode));
    let pixels = |points: usize| {
        u32::try_from(points).map_err(|_| Error::Unavailable("a display mode of this size"))
    };
    VirtualDisplay::new(
        &Descriptor {
            name: name.to_owned(),
            vendor: VENDOR,
            product: CGDisplayModelNumber(physical),
            serial: CGDisplaySerialNumber(physical),
            millimeters: CGDisplayScreenSize(physical),
            width: pixels(CGDisplayMode::pixel_width(Some(&mode)))?,
            height: pixels(CGDisplayMode::pixel_height(Some(&mode)))?,
            refresh_rate: if refresh_rate > 0.0 {
                refresh_rate
            } else {
                REFRESH_RATE
            },
        },
        queue,
    )
}

/// Sets the HiDPI mode of `master` and mirrors `display` onto it while this process runs.
fn mirror(display: CGDirectDisplayID, master: &VirtualDisplay) -> Result<(), Error> {
    let mode = master.mode()?;
    let mut config = ptr::null_mut();
    check("beginning a display configuration", unsafe {
        CGBeginDisplayConfiguration(&mut config)
    })?;
    let configured = check("setting the HiDPI mode", unsafe {
        CGConfigureDisplayWithDisplayMode(config, master.id(), Some(&mode), None)
    })
    .and_then(|()| {
        check("mirroring a display", unsafe {
            CGConfigureDisplayMirrorOfDisplay(config, display, master.id())
        })
    });
    if let Err(error) = configured {
        unsafe { CGCancelDisplayConfiguration(config) };
        return Err(error);
    }
    check("completing a display configuration", unsafe {
        CGCompleteDisplayConfiguration(config, CGConfigureOption::ForAppOnly)
    })
}

fn check(what: &'static str, error: CGError) -> Result<(), Error> {
    match error {
        CGError::Success => Ok(()),
        error => Err(Error::CoreGraphics {
            what,
            code: error.0,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<const N: usize>(names: [&str; N]) -> BTreeSet<String> {
        names.map(str::to_owned).into()
    }

    fn ids<const N: usize>(
        ids: [(&str, CGDirectDisplayID); N],
    ) -> BTreeMap<String, CGDirectDisplayID> {
        ids.map(|(name, id)| (name.to_owned(), id)).into()
    }

    fn create(name: &str, physical: CGDirectDisplayID) -> Step {
        Step::Create {
            name: name.to_owned(),
            physical,
        }
    }

    #[test]
    fn a_listed_display_gets_a_mirror() {
        let present = ids([("Dell", 2), ("LG", 3)]);
        assert_eq!(
            plan(&names(["Dell"]), &present, &ids([]), &names([])),
            [create("Dell", 2)]
        );
    }

    #[test]
    fn a_mirrored_display_is_left_alone() {
        let present = ids([("Dell", 2)]);
        assert_eq!(plan(&names(["Dell"]), &present, &present, &names([])), []);
    }

    #[test]
    fn a_listed_display_that_is_absent_waits() {
        assert_eq!(
            plan(&names(["Dell"]), &ids([("LG", 3)]), &ids([]), &names([])),
            []
        );
    }

    #[test]
    fn an_unlisted_mirror_ends() {
        let present = ids([("Dell", 2)]);
        assert_eq!(
            plan(&names([]), &present, &present, &names(["Dell"])),
            [Step::Drop("Dell".to_owned())]
        );
    }

    #[test]
    fn a_disconnected_display_ends_its_mirror() {
        assert_eq!(
            plan(&names(["Dell"]), &ids([]), &ids([("Dell", 2)]), &names([])),
            [Step::Drop("Dell".to_owned())]
        );
    }

    #[test]
    fn a_reconnected_display_with_a_new_id_is_mirrored_again() {
        assert_eq!(
            plan(
                &names(["Dell"]),
                &ids([("Dell", 5)]),
                &ids([("Dell", 2)]),
                &names(["Dell"])
            ),
            [Step::Drop("Dell".to_owned()), create("Dell", 5)]
        );
    }

    #[test]
    fn a_virtual_display_its_display_does_not_mirror_is_mirrored() {
        let present = ids([("Dell", 2)]);
        assert_eq!(
            plan(&names(["Dell"]), &present, &present, &names(["Dell"])),
            [Step::Mirror("Dell".to_owned())]
        );
    }
}
