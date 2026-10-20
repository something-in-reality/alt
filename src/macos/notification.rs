//! Notifications, through the UserNotifications framework.

use std::{ptr::NonNull, sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::{rc::Retained, runtime::Bool};
use objc2_foundation::{NSBundle, NSError, NSString, NSUUID};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNUserNotificationCenter,
};

use super::Error;

/// The app's permission to post notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::Display, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Authorization {
    Authorized,
    Denied,
    #[strum(to_string = "not determined")]
    NotDetermined,
    Provisional,
    Ephemeral,
}

impl TryFrom<UNAuthorizationStatus> for Authorization {
    type Error = Error;

    fn try_from(status: UNAuthorizationStatus) -> Result<Self, Error> {
        match status {
            UNAuthorizationStatus::Authorized => Ok(Self::Authorized),
            UNAuthorizationStatus::Denied => Ok(Self::Denied),
            UNAuthorizationStatus::NotDetermined => Ok(Self::NotDetermined),
            UNAuthorizationStatus::Provisional => Ok(Self::Provisional),
            UNAuthorizationStatus::Ephemeral => Ok(Self::Ephemeral),
            _ => Err(Error::Unavailable("a known authorization status")),
        }
    }
}

/// The notification center of the app bundle that this process runs from. The framework
/// terminates a process whose main bundle has no identifier, so that is checked first.
fn center() -> Result<Retained<UNUserNotificationCenter>, Error> {
    NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|_| UNUserNotificationCenter::currentNotificationCenter())
        .ok_or(Error::NoBundle)
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

/// The permission of the app bundle that this process runs from, read from its notification
/// settings.
pub fn authorization() -> Result<Authorization, Error> {
    const TIMEOUT: Duration = Duration::from_secs(5);
    let (sender, receiver) = mpsc::sync_channel(1);
    let handler = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: a completion handler's settings are valid for the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        let _ = sender.try_send(status);
    });
    center()?.getNotificationSettingsWithCompletionHandler(&handler);
    receiver
        .recv_timeout(TIMEOUT)
        .map_err(|_| Error::Unavailable("the notification settings"))?
        .try_into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_round_trips_through_its_name() {
        for (authorization, name) in [
            (Authorization::Authorized, "authorized"),
            (Authorization::Denied, "denied"),
            (Authorization::NotDetermined, "not determined"),
            (Authorization::Provisional, "provisional"),
            (Authorization::Ephemeral, "ephemeral"),
        ] {
            assert_eq!(authorization.to_string(), name);
            assert_eq!(name.parse(), Ok(authorization));
        }
    }
}
