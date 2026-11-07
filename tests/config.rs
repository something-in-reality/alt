//! The configuration language: the full example, the errors of common mistakes, and the JSON
//! Schema's agreement with the parser.

use alt::{
    action::{Action, BundleId, Size},
    combo::Combo,
    config::{Binding, Config},
};
use serde_json::Value;

const FULL: &str = include_str!("full_example.toml");

fn app(id: &str) -> BundleId {
    id.parse().unwrap()
}

fn combo(s: &str) -> Combo {
    s.parse().unwrap()
}

fn actions(config: &Config, front: Option<&str>, shortcut: &str) -> Option<Vec<Action>> {
    let front = front.map(app);
    config
        .shortcut(front.as_ref(), &combo(shortcut))
        .map(|command| command.as_ref().to_vec())
}

fn validates(toml: &str) -> bool {
    let schema = Config::schema().to_value();
    let instance: Value = toml::from_str(toml).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .is_valid(&instance)
}

/// The message and the text it points at.
fn error(toml: &str) -> (String, &str) {
    let error = toml.parse::<Config>().unwrap_err();
    let span = error.span().expect("an error with a location");
    (error.message().to_owned(), &toml[span])
}

#[test]
fn the_full_example_parses() {
    let config: Config = FULL.parse().unwrap();
    assert_eq!(config.mouse.natural_scrolling, Some(false));
    assert!(config.displays.sync_brightness);
    assert_eq!(config.apps.len(), 3);
    assert_eq!(
        actions(&config, None, "f6"),
        Some(vec![Action::Press(combo("audio_volume_mute"))])
    );
    assert_eq!(
        actions(&config, None, "ctrl-t"),
        Some(vec![Action::Type("✓".to_owned())])
    );
    assert_eq!(
        actions(&config, None, "home"),
        Some(vec![Action::Open(app("org.mozilla.firefox"))])
    );
    let width = |width| vec![Action::Resize(Size { width, height: 0 })];
    assert_eq!(actions(&config, None, "cmd-home"), Some(width(100)));
    assert_eq!(
        actions(&config, Some("org.mozilla.firefox"), "cmd-home"),
        Some(width(-100))
    );
    assert_eq!(
        actions(&config, Some("org.mozilla.firefox"), "page_up"),
        Some(vec![Action::Press(combo("ctrl-shift-tab"))])
    );
    let music = &config.apps[&app("com.apple.Music")];
    assert_eq!(
        music.on_launch.as_ref().map(AsRef::as_ref),
        Some([Action::ForceQuit(app("com.apple.Music"))].as_slice())
    );
    assert!(music.shortcuts.get(&combo("f1")).is_none());
}

#[test]
fn the_full_example_matches_the_schema() {
    assert!(validates(FULL));
    assert!(validates(include_str!("../config.toml")));
}

#[test]
fn an_empty_command_drops_and_false_passes() {
    let config: Config = FULL.parse().unwrap();
    assert_eq!(actions(&config, None, "cmd-h"), Some(vec![]));
    assert_eq!(
        actions(&config, Some("org.mozilla.firefox"), "cmd-h"),
        Some(vec![])
    );
    assert_eq!(
        actions(&config, Some("com.mitchellh.ghostty"), "cmd-h"),
        None
    );
    let ghostty = &config.apps[&app("com.mitchellh.ghostty")];
    assert_eq!(
        ghostty.shortcuts.get(&combo("cmd-h")),
        Some(&Binding::Unbound)
    );
}

#[test]
fn common_mistakes_point_at_the_offending_text() {
    let mistakes = [
        (
            "[bindsym]\nf1 = []",
            "bindsym",
            "unknown section `bindsym`; expected shortcuts, mouse, displays, or a quoted bundle identifier such as [\"com.apple.Safari\"]",
        ),
        (
            "[org.mozilla.firefox]\non_launch = []",
            "org",
            "unknown section `org`; expected shortcuts, mouse, displays, or a quoted bundle identifier",
        ),
        (
            "[input.pointer]\nnatural_scroll = false",
            "input",
            "unknown section `input`",
        ),
        (
            "[mouse]\nnatural_scroll = false",
            "natural_scroll",
            "unknown field `natural_scroll`, expected `natural_scrolling`",
        ),
        (
            "[displays]\nsync = true",
            "sync",
            "unknown field `sync`, expected `sync_brightness`",
        ),
        (
            "[\"com.apple.Music\"]\nbindsym.f1 = []",
            "bindsym",
            "unknown field `bindsym`, expected `on_launch` or `shortcuts`",
        ),
        (
            "[shortcuts]\nshift-ctrl-tab = []",
            "shift-ctrl-tab",
            "modifiers go in the order ctrl, alt, shift, cmd: write `ctrl-shift-tab`",
        ),
        (
            "[shortcuts]\nctrl-ctrl-a = []",
            "ctrl-ctrl-a",
            "modifiers go in the order ctrl, alt, shift, cmd: write `ctrl-a`",
        ),
        (
            "[shortcuts]\nsuper-h = []",
            "super-h",
            "unknown modifier `super`; expected ctrl, alt, shift or cmd in lower case",
        ),
        (
            "[shortcuts]\n\"ctrl+h\" = []",
            "\"ctrl+h\"",
            "unknown key `ctrl+h`; expected a lower case key such as a, 1, home, page_up or backquote",
        ),
        (
            "[shortcuts]\nctrl-tee = []",
            "ctrl-tee",
            "unknown key `tee`; expected a lower case key such as a, 1, home, page_up or backquote",
        ),
        (
            "[shortcuts]\naudio_volume_up = []",
            "audio_volume_up",
            "a shortcut cannot match `audio_volume_up`",
        ),
        (
            "[shortcuts]\nctrl-shift_left = []",
            "ctrl-shift_left",
            "a shortcut cannot match `shift_left`",
        ),
        (
            "[shortcuts]\nf1 = { tlie = \"left\" }",
            "{ tlie = \"left\" }",
            "unknown variant `tlie`, expected one of `press`, `type`, `open`",
        ),
        (
            "[shortcuts]\nf1 = { tile = \"middle\" }",
            "{ tile = \"middle\" }",
            "unknown variant `middle`, expected one of `left`, `right`",
        ),
        (
            "[shortcuts]\nf1 = \"flll\"",
            "\"flll\"",
            "unknown variant `flll`, expected one of `press`",
        ),
        (
            "[shortcuts]\nf1 = true",
            "true",
            "expected an action, a list of actions, or false",
        ),
        (
            "[shortcuts]\nf1 = 1",
            "1",
            "expected an action, a list of actions, or false",
        ),
        (
            "[\"com.apple.Music\"]\non_launch = false",
            "false",
            "expected an action or a list of actions",
        ),
        (
            "[shortcuts]\nf1 = { open = \"Firefox\" }",
            "{ open = \"Firefox\" }",
            "expected a bundle identifier such as org.mozilla.firefox",
        ),
        (
            "[shortcuts]\nf1 = { press = \"ctrl--a\" }",
            "{ press = \"ctrl--a\" }",
            "unknown modifier ``",
        ),
        (
            "[shortcuts]\nf1 = { notify = \"%H:%\" }",
            "{ notify = \"%H:%\" }",
            "strftime",
        ),
        (
            "[shortcuts]\nf1 = { resize = { depth = 1 } }",
            "{ resize = { depth = 1 } }",
            "unknown field `depth`, expected `width` or `height`",
        ),
        ("[shortcuts]\nf1 = []\nf1 = \"fill\"", "f1", "duplicate key"),
        (
            "[shortcuts]\nf1 = []\n[\"com.apple.Music\"]\n[\"com.apple.Music\"]",
            "\"com.apple.Music\"",
            "duplicate key",
        ),
    ];
    for (toml, text, message) in mistakes {
        let (found, at) = error(toml);
        assert!(found.contains(message), "{toml:?}: {found}");
        assert_eq!(at.trim_end(), text, "{toml:?}: {found}");
    }
}

#[test]
fn a_root_key_after_an_application_table_is_that_applications() {
    let (message, _) = error("[\"com.apple.Music\"]\nmouse.natural_scrolling = true");
    assert!(message.contains("unknown field `mouse`"), "{message}");
}

#[test]
fn the_schema_agrees_with_the_parser() {
    let cases = [
        ("", true),
        (
            "[shortcuts]\nf1 = []\nctrl-alt-shift-cmd-a = \"fill\"",
            true,
        ),
        ("[shortcuts]\nf1 = false", true),
        ("[shortcuts]\nf1 = true", false),
        ("[shortcuts]\nf1 = [false]", false),
        ("[shortcuts]\nf1 = [\"fill\", { tile = \"left\" }]", true),
        ("[shortcuts]\nf1 = [[\"fill\"]]", false),
        ("[shortcuts]\n\"ctrl+a\" = []", false),
        ("[shortcuts]\nshift-ctrl-a = []", false),
        ("[shortcuts]\nctrl-ctrl-a = []", false),
        ("[shortcuts]\n\"-a\" = []", false),
        ("[shortcuts]\n\"ctrl--a\" = []", false),
        ("[shortcuts]\nsuper-a = []", false),
        ("[shortcuts]\nCtrl-a = []", false),
        ("[shortcuts]\ncaps_lock = []", false),
        ("[shortcuts]\nfn = []", false),
        ("[shortcuts]\ncontrol_left = []", false),
        ("[shortcuts]\nbrightness_up = []", false),
        ("[shortcuts]\nf1 = { press = \"brightness_up\" }", true),
        (
            "[shortcuts]\nf1 = { press = \"cmd-audio_volume_up\" }",
            true,
        ),
        ("[shortcuts]\nf1 = { press = \"-a\" }", false),
        ("[shortcuts]\nf1 = { press = \"ctrl++a\" }", false),
        ("[shortcuts]\nf1 = { press = \"shift-ctrl-a\" }", false),
        ("[shortcuts]\nf1 = \"fill\"", true),
        ("[shortcuts]\nf1 = \"tile\"", false),
        ("[shortcuts]\nf1 = { fill = {} }", false),
        ("[shortcuts]\nf1 = { tile = \"middle\" }", false),
        ("[shortcuts]\nf1 = { tile = \"left\", fill = 1 }", false),
        ("[shortcuts]\nf1 = { move_to = \"built_in\" }", true),
        ("[shortcuts]\nf1 = { move_to = \"Studio Display\" }", true),
        ("[shortcuts]\nf1 = { move_to = 1 }", false),
        ("[shortcuts]\nf1 = { resize = { width = -100 } }", true),
        ("[shortcuts]\nf1 = { resize = { depth = 1 } }", false),
        ("[shortcuts]\nf1 = { open = \"org.mozilla.firefox\" }", true),
        ("[shortcuts]\nf1 = { open = \"Firefox\" }", false),
        (
            "[shortcuts]\nf1 = { toggle_connection = \"f8-8f-07-40-54-34\" }",
            true,
        ),
        ("[shortcuts]\nf1 = { bluetooth = \"x\" }", false),
        ("[mouse]\nnatural_scrolling = true", true),
        ("[mouse]\nnatural_scroll = true", false),
        ("[displays]\nsync_brightness = true", true),
        ("[displays]\nsync = true", false),
        ("[bindsym]\nf1 = []", false),
        ("[input.pointer]\nnatural_scroll = true", false),
        ("[org.mozilla.firefox]\non_launch = []", false),
        (
            "[\"org.mozilla.firefox\"]\non_launch = \"full_screen\"",
            true,
        ),
        ("[\"org.mozilla.firefox\"]\non_launch = 1", false),
        ("[\"org.mozilla.firefox\"]\nshortcuts.cmd-h = false", true),
        (
            "[\"org.mozilla.firefox\"]\nshortcuts.shift-ctrl-h = false",
            false,
        ),
        ("[\"org.mozilla.firefox\"]\nbindsym.f1 = []", false),
        ("[\"Firefox\"]\non_launch = []", false),
        ("[\"org\"]\non_launch = []", false),
        ("[\"org..mozilla\"]\non_launch = []", false),
    ];
    for (toml, valid) in cases {
        assert_eq!(toml.parse::<Config>().is_ok(), valid, "parser: {toml:?}");
        assert_eq!(validates(toml), valid, "schema: {toml:?}");
    }
}

#[test]
fn key_names_are_checked_by_the_parser_alone() {
    let toml = "[shortcuts]\nctrl-tee = []";
    assert!(toml.parse::<Config>().is_err());
    assert!(validates(toml));
}
