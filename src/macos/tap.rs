//! A session event tap on the current thread's run loop.

use std::{
    ffi::c_void,
    panic::{self, AssertUnwindSafe},
    ptr::{self, NonNull},
};

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};

/// What happens to an event after the handler sees it.
pub enum Verdict {
    Pass,
    Drop,
}

type Handler = dyn Fn(CGEventType, &CGEvent) -> Verdict;

struct State {
    handler: Box<Handler>,
    port: *const CFMachPort,
}

/// An installed tap; dropping it removes the tap.
pub struct Tap {
    port: CFRetained<CFMachPort>,
    _state: Box<State>,
}

impl Tap {
    /// Installs a tap for `events` on the current run loop, or `None` without permission.
    pub fn new(
        events: &[CGEventType],
        handler: impl Fn(CGEventType, &CGEvent) -> Verdict + 'static,
    ) -> Option<Self> {
        let mut state = Box::new(State {
            handler: Box::new(handler),
            port: ptr::null(),
        });
        let mask = events.iter().fold(0, |mask, event| mask | 1 << event.0);
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                ptr::from_mut(&mut *state).cast(),
            )
        }?;
        state.port = CFRetained::as_ptr(&port).as_ptr();
        let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)?;
        CFRunLoop::current()?.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        Some(Self {
            port,
            _state: state,
        })
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.port.invalidate();
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
        if let Some(port) = unsafe { state.port.as_ref() } {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let event_ref = unsafe { event.as_ref() };
    match panic::catch_unwind(AssertUnwindSafe(|| (state.handler)(kind, event_ref))) {
        Ok(Verdict::Drop) => ptr::null_mut(),
        Ok(Verdict::Pass) | Err(_) => event.as_ptr(),
    }
}
