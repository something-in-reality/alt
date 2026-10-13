//! Applications, through NSWorkspace.

use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::NSString;

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
