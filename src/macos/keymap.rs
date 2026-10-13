//! Mac virtual key codes and event flags, to and from `Combo`.

use keycode::{KeyMap, KeyMapping, KeyMappingCode};
use objc2_core_graphics::CGEventFlags;

use crate::combo::{Key, Modifiers};

const DICTATION: u16 = 176;

/// Each modifier's flag, and the key code of its left key.
const MODIFIERS: [(Modifiers, CGEventFlags, u16); 4] = [
    (Modifiers::CTRL, CGEventFlags::MaskControl, 59),
    (Modifiers::ALT, CGEventFlags::MaskAlternate, 58),
    (Modifiers::SHIFT, CGEventFlags::MaskShift, 56),
    (Modifiers::SUPER, CGEventFlags::MaskCommand, 55),
];

pub fn key(code: u16) -> Option<Key> {
    match code {
        DICTATION => Some(Key::Dictation),
        _ => KeyMap::from_key_mapping(KeyMapping::Mac(code))
            .ok()?
            .code
            .map(Key::Code),
    }
}

pub fn code(key: Key) -> Option<u16> {
    match key {
        Key::Dictation => Some(DICTATION),
        Key::Code(code) => KeyMap::from_key_mapping(KeyMapping::Code(Some(code)))
            .ok()
            .map(|map| map.mac)
            .filter(|&mac| mac != 0xffff),
    }
}

pub fn modifiers(flags: CGEventFlags) -> Modifiers {
    MODIFIERS
        .into_iter()
        .filter(|&(_, flag, _)| flags.contains(flag))
        .map(|(modifier, _, _)| modifier)
        .collect()
}

/// The flag and key code of each modifier in `modifiers`.
pub fn modifier_keys(modifiers: Modifiers) -> impl DoubleEndedIterator<Item = (CGEventFlags, u16)> {
    MODIFIERS
        .into_iter()
        .filter(move |&(modifier, _, _)| modifiers.contains(modifier))
        .map(|(_, flag, code)| (flag, code))
}

/// The flags a keyboard sets with `key` itself: arrow and navigation keys carry fn.
pub fn implicit_flags(key: Key) -> CGEventFlags {
    match key {
        Key::Code(
            KeyMappingCode::ArrowLeft
            | KeyMappingCode::ArrowRight
            | KeyMappingCode::ArrowUp
            | KeyMappingCode::ArrowDown,
        ) => CGEventFlags::MaskSecondaryFn | CGEventFlags::MaskNumericPad,
        Key::Code(
            KeyMappingCode::Home
            | KeyMappingCode::End
            | KeyMappingCode::PageUp
            | KeyMappingCode::PageDown
            | KeyMappingCode::Delete,
        ) => CGEventFlags::MaskSecondaryFn,
        _ => CGEventFlags::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for name in [
            "t",
            "home",
            "page_up",
            "backquote",
            "f3",
            "enter",
            "arrow_left",
            "dictation",
        ] {
            let key: Key = name.parse().unwrap();
            assert_eq!(code(key).and_then(super::key), Some(key), "{name}");
        }
    }

    #[test]
    fn modifiers_round_trip() {
        let flags = modifier_keys(Modifiers::all())
            .fold(CGEventFlags::MaskSecondaryFn, |flags, (flag, _)| {
                flags | flag
            });
        assert_eq!(modifiers(flags), Modifiers::all());
    }
}
