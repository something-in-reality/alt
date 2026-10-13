//! Applications, through NSWorkspace.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSString};

use super::Error;
use crate::action::AppId;

fn id(app: &NSRunningApplication) -> Option<AppId> {
    app.bundleIdentifier().map(|id| AppId::from(id.to_string()))
}

fn running(app: &AppId) -> Result<Retained<NSRunningApplication>, Error> {
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(app.as_ref()))
        .firstObject()
        .ok_or_else(|| Error::NotRunning(app.clone()))
}

/// The application in front.
pub fn front() -> Option<AppId> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .as_deref()
        .and_then(id)
}

/// Brings a running application to the front.
pub fn focus(app: &AppId) -> Result<(), Error> {
    let url = running(app)?
        .bundleURL()
        .ok_or_else(|| Error::NotRunning(app.clone()))?;
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &url,
        &NSWorkspaceOpenConfiguration::new(),
        None,
    );
    Ok(())
}

/// Calls `f` with each application that comes to the front, while the observer lives.
pub fn on_activate(f: impl Fn(AppId) + 'static) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        let activated = unsafe { notification.as_ref() }
            .userInfo()
            .and_then(|info| info.objectForKey(unsafe { NSWorkspaceApplicationKey }))
            .and_then(|app| app.downcast::<NSRunningApplication>().ok());
        if let Some(app) = activated.as_deref().and_then(id) {
            f(app);
        }
    });
    unsafe {
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceDidActivateApplicationNotification),
                None,
                None,
                &block,
            )
    }
}
