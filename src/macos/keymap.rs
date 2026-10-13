//! Mac virtual key codes and event flags, to and from `Combo`.

use keycode::{KeyMap, KeyMapping};
use objc2_core_graphics::CGEventFlags;

use crate::combo::{Key, Modifiers};

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

pub fn flags(modifiers: Modifiers) -> CGEventFlags {
    MODIFIERS
        .into_iter()
        .filter(|&(modifier, _)| modifiers.contains(modifier))
        .fold(CGEventFlags::empty(), |flags, (_, flag)| flags | flag)
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
        let all = Modifiers::all();
        assert_eq!(modifiers(flags(all) | CGEventFlags::MaskSecondaryFn), all);
    }
}
