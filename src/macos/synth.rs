//! Synthetic input, marked so the tap passes it through.

use keycode::KeyMappingCode;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGScrollEventUnit,
};

use super::{Error, keymap};
use crate::combo::{Combo, Key};

const MARKER: i64 = 0x6163_6365_7373;

/// Whether this process posted `event`.
pub fn is_synthetic(event: &CGEvent) -> bool {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == MARKER
}

/// A key that NX_SYSDEFINED events carry, by its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Presses the key: down, then up.
    pub fn press(self) -> Result<(), Error> {
        [0xa, 0xb]
            .into_iter()
            .try_for_each(|state| post_system_key(self as isize, state))
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

/// A private event source, which posts keys and text.
pub struct Source {
    source: CFRetained<CGEventSource>,
}

impl Source {
    pub fn new() -> Result<Self, Error> {
        CGEventSource::new(CGEventSourceStateID::Private)
            .map(|source| Self { source })
            .ok_or(Error::Unavailable("an event source"))
    }

    /// Presses `combo` as a keyboard does: modifiers down, the key down and up, modifiers up.
    pub fn press(&self, combo: Combo) -> Result<(), Error> {
        if let Ok(key) = SystemKey::try_from(combo.key) {
            return key.press();
        }
        let code = keymap::code(combo.key).ok_or(Error::Unmapped(combo.key))?;
        let modifiers: Vec<_> = keymap::modifier_keys(combo.modifiers).collect();
        let mut held = CGEventFlags::empty();
        for &(flag, modifier) in &modifiers {
            held |= flag;
            self.key(modifier, true, held, CGEventType::FlagsChanged)?;
        }
        let flags = held | keymap::implicit_flags(combo.key);
        self.key(code, true, flags, CGEventType::KeyDown)?;
        self.key(code, false, flags, CGEventType::KeyUp)?;
        for &(flag, modifier) in modifiers.iter().rev() {
            held &= !flag;
            self.key(modifier, false, held, CGEventType::FlagsChanged)?;
        }
        Ok(())
    }

    /// Posts one key event of `kind` with `flags`.
    fn key(
        &self,
        code: u16,
        down: bool,
        flags: CGEventFlags,
        kind: CGEventType,
    ) -> Result<(), Error> {
        let event = CGEvent::new_keyboard_event(Some(&self.source), code, down)
            .ok_or(Error::Unavailable("a keyboard event"))?;
        CGEvent::set_type(Some(&event), kind);
        CGEvent::set_flags(Some(&event), flags);
        post(&event);
        Ok(())
    }

    pub fn type_text(&self, text: &str) -> Result<(), Error> {
        utf16_chunks(text).iter().try_for_each(|chunk| {
            [true, false].into_iter().try_for_each(|down| {
                let event = CGEvent::new_keyboard_event(Some(&self.source), 0, down)
                    .ok_or(Error::Unavailable("a keyboard event"))?;
                unsafe {
                    CGEvent::keyboard_set_unicode_string(
                        Some(&event),
                        chunk.len() as _,
                        chunk.as_ptr(),
                    );
                }
                post(&event);
                Ok(())
            })
        })
    }
}

/// A keyboard event carries at most 20 UTF-16 units; chunks never split a character.
fn utf16_chunks(text: &str) -> Vec<Vec<u16>> {
    const MAX: usize = 20;
    text.chars()
        .fold(Vec::new(), |mut chunks: Vec<Vec<u16>>, c| {
            let mut buffer = [0; 2];
            let units = c.encode_utf16(&mut buffer);
            match chunks.last_mut() {
                Some(chunk) if chunk.len() + units.len() <= MAX => chunk.extend_from_slice(units),
                _ => chunks.push(units.to_vec()),
            }
            chunks
        })
}

/// Posts `event`'s discrete scroll reversed on both axes, in lines; apps read the scroll from
/// the posted event, which an edited event does not change. Returns whether it posted.
pub fn reverse_scroll(event: &CGEvent) -> bool {
    let lines = |field| -CGEvent::integer_value_field(Some(event), field) as i32;
    let vertical = lines(CGEventField::ScrollWheelEventDeltaAxis1);
    let horizontal = lines(CGEventField::ScrollWheelEventDeltaAxis2);
    CGEvent::new_scroll_wheel_event2(None, CGScrollEventUnit::Line, 2, vertical, horizontal, 0)
        .map(|reversed| post(&reversed))
        .is_some()
}

fn post(event: &CGEvent) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, MARKER);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

/// Posts a system key of type `kind` going down (`0xa`) or up (`0xb`), unmarked: the tap decides
/// system keys by their type alone, so a posted brightness key syncs displays as a pressed one does.
fn post_system_key(kind: isize, state: isize) -> Result<(), Error> {
    let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
        NSEventType::SystemDefined,
        CGPoint::ZERO,
        NSEventModifierFlags(state.unsigned_abs() << 8),
        0.0,
        0,
        None,
        8,
        kind << 16 | state << 8,
        -1,
    )
    .and_then(|event| event.CGEvent())
    .ok_or(Error::Unavailable("a system key event"))?;
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_keep_surrogate_pairs_whole() {
        let text = format!("{}😀", "a".repeat(19));
        let chunks = utf16_chunks(&text);
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [19, 2]);
        assert_eq!(String::from_utf16(&chunks.concat()).unwrap(), text);
    }
}
