//! Notifications, through the UserNotifications framework.

use block2::RcBlock;
use objc2::{exception, rc::Retained, runtime::Bool};
use objc2_foundation::{NSError, NSString, NSUUID};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};

use super::Error;

/// The notification center of the app bundle; it raises an exception outside one.
fn center() -> Result<Retained<UNUserNotificationCenter>, Error> {
    exception::catch(UNUserNotificationCenter::currentNotificationCenter)
        .map_err(|_| Error::Unavailable("the notification center"))
}

/// Logs the error of a completion handler, which runs on a framework queue.
fn log(what: &'static str, error: *mut NSError) {
    // SAFETY: a completion handler's error is nil or a valid NSError for the call.
    if let Some(error) = unsafe { error.as_ref() } {
        tracing::warn!(%error, "{what}");
    }
}

/// Asks the user to allow alerts once; macOS remembers the answer for the app.
pub fn authorize() -> Result<(), Error> {
    let handler = RcBlock::new(|granted: Bool, error: *mut NSError| {
        log("requesting notification authorization", error);
        if !granted.as_bool() {
            tracing::warn!("notifications are not allowed; enable them in System Settings");
        }
    });
    center()?
        .requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &handler);
    Ok(())
}

/// Delivers a notification titled `text` immediately.
pub fn post(text: &str) -> Result<(), Error> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(text));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSUUID::new().UUIDString(),
        &content,
        None,
    );
    let handler = RcBlock::new(|error: *mut NSError| log("delivering a notification", error));
    center()?.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
    Ok(())
}
