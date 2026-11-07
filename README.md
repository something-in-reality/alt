# alt

Keyboard, pointer, window and display behaviour outside the terminal, on macOS.

A daemon taps key, scroll and system-key events and runs the actions their bindings name, from
`$XDG_CONFIG_HOME/alt/config.toml`. The configuration reloads on change.

## Commands

```text
alt          checks the configuration, exiting non-zero when invalid; reports the app
alt run      the daemon
alt edit     edits the configuration; writes it once valid
alt log      streams the daemon's log
alt schema   prints the configuration's JSON Schema
alt setup    grants Accessibility; flashes the Glove80
```

## Configuration

```toml
[input.pointer]
natural_scroll = false

[display]
sync_brightness = true                 # external displays follow the built-in, over DDC/CI

[bindsym]
f6 = "media.mute"
home = { app.focus = "org.nixos.firefox" }
"ctrl+alt+arrow_left" = { window.tile = "left" }

[app_id."com.apple.Music"]
on_launch = { app.kill = "com.apple.Music" }
```

Keys are W3C `KeyboardEvent.code` names in snake case; modifiers are `ctrl`, `alt`, `shift`,
`super`. An action is `"namespace.verb"` or `{ namespace.verb = argument }`:

| Namespace   | Verbs                                                                          |
| ----------- | ------------------------------------------------------------------------------ |
| `app`       | `focus`, `kill`                                                                |
| `window`    | `fullscreen`, `fullscreen_tile`, `tile`, `fill`, `center`, `display`, `resize` |
| `keyboard`  | `press`, `type`                                                                |
| `media`     | `mute`, `volume_up`, `volume_down`, `play_pause`, `next`, `previous`           |
| `display`   | `brightness_up`, `brightness_down`                                             |
| `bluetooth` | `connect`, `disconnect`, `toggle`                                              |
| `exec`      | a shell command                                                                |
| `notify`    | a `strftime` template                                                          |

Window verbs press items of the application's Window menu. `window.tile` takes `left`, `right`,
`top`, `bottom`, `top_left`, `top_right`, `bottom_left` or `bottom_right` (Move & Resize);
`window.fill` and `window.center` take nothing; `window.fullscreen_tile` takes `left` or `right`
(Full Screen Tile); `window.display` takes `builtin`, `next`, `previous` or a display's name, for
the Window menu's "Move to" items, which list the other displays. `window.resize` and
`window.fullscreen` set the window's Accessibility attributes. Menu items are found by their
English titles.

`config.toml` is the full example. Its first line, `#:schema <path or url>`, makes taplo, and so
helix, validate and complete the file against the schema; the package installs it at
`share/alt/config.schema.json`, which the Nix module links to
`/run/current-system/sw/share/alt/config.schema.json`.

## Nix

`darwinModules.default` installs the app at `/Applications/Alt.app`, signed under an identity
derived from `seed`, so its Accessibility permission survives rebuilds, and runs it at login.

```nix
services.alt = {
  enable = true;
  seed = "/path/to/seed";  # 64 hex characters
  glove80.enable = true;   # firmware on XDG_DATA_DIRS, for `alt setup`
};
```
