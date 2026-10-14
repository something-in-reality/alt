//! System keys: the media and brightness keys that NX_SYSDEFINED events carry.

use keycode::KeyMappingCode;
use num_enum::TryFromPrimitive;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventSubtype, NSEventType};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGEvent, CGEventTapLocation, CGEventType};

use super::Error;
use crate::{
    action::{Display, Media},
    combo::Key,
};

/// NX_SYSDEFINED, the event type of system keys.
pub const SYSTEM_DEFINED: CGEventType = CGEventType(14);
/// The NSEvent subtype of system keys.
const SUBTYPE: NSEventSubtype = NSEventSubtype(8);
const DOWN: isize = 0xa;
const UP: isize = 0xb;

/// A system key, by its key type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive)]
#[repr(isize)]
pub enum SystemKey {
    VolumeUp = 0,
    VolumeDown = 1,
    BrightnessUp = 2,
    BrightnessDown = 3,
    Mute = 7,
    PlayPause = 16,
    Next = 17,
    Previous = 18,
}

impl SystemKey {
    /// The key `event` presses, when it is a system key going down.
    pub fn pressed(event: &CGEvent) -> Option<Self> {
        let event = NSEvent::eventWithCGEvent(event)?;
        let data = event.data1();
        if event.r#type() == NSEventType::SystemDefined
            && event.subtype() == SUBTYPE
            && (data >> 8) & 0xff == DOWN
        {
            Self::try_from(data >> 16).ok()
        } else {
            None
        }
    }

    pub fn is_brightness(self) -> bool {
        matches!(self, Self::BrightnessUp | Self::BrightnessDown)
    }

    /// Posts the key down, then up, unmarked: the tap decides them as physical presses, so a
    /// posted brightness key syncs displays.
    pub fn press(self) -> Result<(), Error> {
        [DOWN, UP]
            .into_iter()
            .try_for_each(|state| self.post(state))
    }

    fn post(self, state: isize) -> Result<(), Error> {
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::SystemDefined,
            CGPoint::ZERO,
            NSEventModifierFlags(state.unsigned_abs() << 8),
            0.0,
            0,
            None,
            SUBTYPE.0,
            (self as isize) << 16 | state << 8,
            -1,
        )
        .and_then(|event| event.CGEvent())
        .ok_or(Error::Unavailable("a system key event"))?;
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    }
}

impl TryFrom<Key> for SystemKey {
    type Error = Key;

    fn try_from(key: Key) -> Result<Self, Key> {
        match key {
            Key::Code(KeyMappingCode::AudioVolumeUp) => Ok(Self::VolumeUp),
            Key::Code(KeyMappingCode::AudioVolumeDown) => Ok(Self::VolumeDown),
            Key::Code(KeyMappingCode::AudioVolumeMute) => Ok(Self::Mute),
            Key::Code(KeyMappingCode::MediaPlayPause) => Ok(Self::PlayPause),
            Key::Code(KeyMappingCode::MediaTrackNext) => Ok(Self::Next),
            Key::Code(KeyMappingCode::MediaTrackPrevious) => Ok(Self::Previous),
            Key::Code(KeyMappingCode::BrightnessUp) => Ok(Self::BrightnessUp),
            Key::Code(KeyMappingCode::BrightnessDown) => Ok(Self::BrightnessDown),
            key => Err(key),
        }
    }
}

impl From<Media> for SystemKey {
    fn from(media: Media) -> Self {
        match media {
            Media::Mute => Self::Mute,
            Media::VolumeUp => Self::VolumeUp,
            Media::VolumeDown => Self::VolumeDown,
            Media::PlayPause => Self::PlayPause,
            Media::Next => Self::Next,
            Media::Previous => Self::Previous,
        }
    }
}

impl From<Display> for SystemKey {
    fn from(display: Display) -> Self {
        match display {
            Display::BrightnessUp => Self::BrightnessUp,
            Display::BrightnessDown => Self::BrightnessDown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_types_round_trip() {
        for key in [SystemKey::VolumeUp, SystemKey::Mute, SystemKey::Previous] {
            assert_eq!(SystemKey::try_from(key as isize), Ok(key));
        }
        assert!(SystemKey::try_from(4).is_err());
    }
}
