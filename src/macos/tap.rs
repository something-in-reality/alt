//! A session event tap on the current thread's run loop.

use std::{
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

/// Owned through a raw pointer, which the callback also reads.
struct State {
    handler: Box<Handler>,
    port: *const CFMachPort,
}

/// An installed tap; dropping it removes the tap.
pub struct Tap {
    port: CFRetained<CFMachPort>,
    state: *mut State,
}

impl Tap {
    /// Installs a tap for `events` on the current run loop, or `None` without permission.
    pub fn new(
        events: &[CGEventType],
        handler: impl Fn(CGEventType, &CGEvent) -> bool + 'static,
    ) -> Option<Self> {
        let state = Box::into_raw(Box::new(State {
            handler: Box::new(handler),
            port: ptr::null(),
        }));
        let mask = events.iter().fold(0, |mask, event| mask | 1 << event.0);
        let Some(port) = (unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                state.cast(),
            )
        }) else {
            drop(unsafe { Box::from_raw(state) });
            return None;
        };
        unsafe { (*state).port = CFRetained::as_ptr(&port).as_ptr() };
        let tap = Self { port, state };
        let source = CFMachPort::new_run_loop_source(None, Some(&tap.port), 0)?;
        CFRunLoop::current()?.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        Some(tap)
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.port.invalidate();
        drop(unsafe { Box::from_raw(self.state) });
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
        if let Some(port) = unsafe { state.port.as_ref() } {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let event_ref = unsafe { event.as_ref() };
    let pass = autoreleasepool(|_| {
        panic::catch_unwind(AssertUnwindSafe(|| (state.handler)(kind, event_ref)))
    });
    if pass.unwrap_or(true) {
        event.as_ptr()
    } else {
        ptr::null_mut()
    }
}
