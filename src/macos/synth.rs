//! Synthetic input, marked so the tap passes it through.

use keycode::KeyMappingCode;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};

use super::{Error, keymap};
use crate::combo::{Combo, Key};

const MARKER: i64 = 0x6163_6365_7373;

/// Whether this process posted `event`.
pub fn is_synthetic(event: &CGEvent) -> bool {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == MARKER
}

/// The media key types of NX_SYSDEFINED events.
fn media(key: Key) -> Option<isize> {
    match key {
        Key::Code(KeyMappingCode::AudioVolumeUp) => Some(0),
        Key::Code(KeyMappingCode::AudioVolumeDown) => Some(1),
        Key::Code(KeyMappingCode::AudioVolumeMute) => Some(7),
        Key::Code(KeyMappingCode::MediaPlayPause) => Some(16),
        Key::Code(KeyMappingCode::MediaTrackNext) => Some(17),
        Key::Code(KeyMappingCode::MediaTrackPrevious) => Some(18),
        _ => None,
    }
}

pub struct Keyboard {
    source: CFRetained<CGEventSource>,
}

impl Keyboard {
    pub fn new() -> Result<Self, Error> {
        CGEventSource::new(CGEventSourceStateID::Private)
            .map(|source| Self { source })
            .ok_or(Error::Unavailable("an event source"))
    }

    pub fn press(&self, combo: Combo) -> Result<(), Error> {
        if let Some(kind) = media(combo.key) {
            return [0xa, 0xb]
                .into_iter()
                .try_for_each(|state| post_media(kind, state));
        }
        let code = keymap::code(combo.key).ok_or(Error::Unmapped(combo.key))?;
        [true, false].into_iter().try_for_each(|down| {
            let event = CGEvent::new_keyboard_event(Some(&self.source), code, down)
                .ok_or(Error::Unavailable("a keyboard event"))?;
            CGEvent::set_flags(Some(&event), keymap::flags(combo));
            post(&event);
            Ok(())
        })
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

fn post(event: &CGEvent) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, MARKER);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

/// Posts a media key `kind` going down (`0xa`) or up (`0xb`).
fn post_media(kind: isize, state: isize) -> Result<(), Error> {
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
    .ok_or(Error::Unavailable("a media key event"))?;
    post(&event);
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
