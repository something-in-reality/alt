//! Mac virtual key codes and event flags, to and from `Combo`.

use keycode::{KeyMap, KeyMapping, KeyMappingCode};
use objc2_core_graphics::CGEventFlags;

use crate::combo::{Combo, Key, Modifiers};

const DICTATION: u16 = 176;

const MODIFIERS: [(Modifiers, CGEventFlags); 4] = [
    (Modifiers::CTRL, CGEventFlags::MaskControl),
    (Modifiers::ALT, CGEventFlags::MaskAlternate),
    (Modifiers::SHIFT, CGEventFlags::MaskShift),
    (Modifiers::SUPER, CGEventFlags::MaskCommand),
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
        .filter(|&(_, flag)| flags.contains(flag))
        .map(|(modifier, _)| modifier)
        .collect()
}

/// The flags a keyboard sets for `combo`, including those arrow and navigation keys carry.
pub fn flags(combo: Combo) -> CGEventFlags {
    let implicit = match combo.key {
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
    };
    MODIFIERS
        .into_iter()
        .filter(|&(modifier, _)| combo.modifiers.contains(modifier))
        .fold(implicit, |flags, (_, flag)| flags | flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for name in [
            "KeyT",
            "Home",
            "PageUp",
            "Backquote",
            "F3",
            "Enter",
            "ArrowLeft",
            "Dictation",
        ] {
            let key: Key = name.parse().unwrap();
            assert_eq!(code(key).and_then(super::key), Some(key), "{name}");
        }
    }

    #[test]
    fn modifiers_round_trip() {
        let combo = Combo {
            modifiers: Modifiers::all(),
            key: "ArrowLeft".parse().unwrap(),
        };
        assert_eq!(modifiers(flags(combo)), combo.modifiers);
    }
}
