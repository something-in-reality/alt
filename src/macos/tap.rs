//! A session event tap on the current thread's run loop.

use std::{
    cell::OnceCell,
    ffi::c_void,
    panic::{self, AssertUnwindSafe},
    ptr::{self, NonNull},
};

use objc2::rc::autoreleasepool;
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};

/// Returns whether the event continues to its destination.
type Handler = dyn Fn(CGEventType, &CGEvent) -> bool;

/// The callback's state: the handler, and the tap's port to re-enable it.
struct State {
    handler: Box<Handler>,
    port: OnceCell<CFRetained<CFMachPort>>,
}

/// An installed tap; dropping it removes the tap.
pub struct Tap {
    state: Box<State>,
}

impl Tap {
    /// Installs a tap for `events` on the current run loop; `None` when the system refuses the
    /// tap or its run loop source.
    pub fn new(
        events: &[CGEventType],
        handler: impl Fn(CGEventType, &CGEvent) -> bool + 'static,
    ) -> Option<Self> {
        let tap = Self {
            state: Box::new(State {
                handler: Box::new(handler),
                port: OnceCell::new(),
            }),
        };
        let mask = events.iter().fold(0, |mask, event| mask | 1 << event.0);
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                ptr::from_ref::<State>(&tap.state).cast_mut().cast(),
            )
        }?;
        let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)?;
        CFRunLoop::current()?.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        _ = tap.state.port.set(port);
        Some(tap)
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        if let Some(port) = self.state.port.get() {
            port.invalidate();
        }
    }
}

unsafe extern "C-unwind" fn callback(
    _: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    state: *mut c_void,
) -> *mut CGEvent {
    let state = unsafe { &*state.cast::<State>() };
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        tracing::warn!(?kind, "re-enabling the event tap");
        if let Some(port) = state.port.get() {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let event_ref = unsafe { event.as_ref() };
    let pass = autoreleasepool(|_| {
        panic::catch_unwind(AssertUnwindSafe(|| (state.handler)(kind, event_ref)))
    })
    .unwrap_or_else(|_| {
        tracing::error!(?kind, "the event handler panicked; passing the event");
        true
    });
    if pass {
        event.as_ptr()
    } else {
        ptr::null_mut()
    }
}
