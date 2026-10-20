# alt

Keyboard, pointer, window and display behaviour outside the terminal, on macOS.

A daemon taps key, scroll and system-key events and runs the actions their bindings name, from
`$XDG_CONFIG_HOME/alt/config.toml`. The configuration reloads on change.

## Commands

```text
alt          checks the configuration; reports the process, signature and permission
alt run      the daemon
alt edit     edits the configuration; writes it once valid
alt log      streams the daemon's log
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

| Namespace   | Verbs                                                                       |
| ----------- | --------------------------------------------------------------------------- |
| `app`       | `focus`, `kill`                                                             |
| `window`    | `fullscreen`, `fullscreen_tile`, `tile`, `display`, `resize`                |
| `keyboard`  | `press`, `type`                                                             |
| `media`     | `mute`, `volume_up`, `volume_down`, `play_pause`, `next`, `previous`        |
| `display`   | `brightness_up`, `brightness_down`                                          |
| `bluetooth` | `connect`, `disconnect`, `toggle`                                           |
| `exec`      | a shell command                                                             |
| `notify`    | a `strftime` template                                                       |

`config.toml` is the full example.

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
