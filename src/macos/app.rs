//! Applications, through NSWorkspace.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{NSNotification, NSNotificationName, NSObjectProtocol, NSString};

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

/// A running application's process identifier.
pub fn pid(app: &AppId) -> Result<i32, Error> {
    Ok(running(app)?.processIdentifier())
}

/// The application in front.
pub fn front() -> Option<AppId> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .as_deref()
        .and_then(id)
}

/// Brings an application to the front, launching it when it is not running.
pub fn focus(app: &AppId) -> Result<(), Error> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(app.as_ref()))
        .ok_or_else(|| Error::NotInstalled(app.clone()))?;
    workspace.openApplicationAtURL_configuration_completionHandler(
        &url,
        &NSWorkspaceOpenConfiguration::new(),
        None,
    );
    Ok(())
}

/// Force-quits a running application.
pub fn kill(app: &AppId) -> Result<(), Error> {
    running(app)?.forceTerminate();
    Ok(())
}

/// The running applications.
pub fn all() -> impl Iterator<Item = AppId> {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .into_iter()
        .filter_map(|app| id(&app))
}

/// Calls `f` with the application of each NSWorkspace `notification`, while the observer lives.
pub fn observe(
    notification: &NSNotificationName,
    f: impl Fn(AppId) + 'static,
) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        let app = unsafe { notification.as_ref() }
            .userInfo()
            .and_then(|info| info.objectForKey(unsafe { NSWorkspaceApplicationKey }))
            .and_then(|app| app.downcast::<NSRunningApplication>().ok());
        if let Some(app) = app.as_deref().and_then(id) {
            f(app);
        }
    });
    unsafe {
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(Some(notification), None, None, &block)
    }
}
