# alt

Keyboard, pointer, window and display behaviour outside the terminal, on macOS.

A daemon taps key, scroll and system-key events and runs the actions their shortcuts name, from
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
[mouse]
natural_scrolling = false

[displays]
sync_brightness = true                 # external displays follow the built-in, over DDC/CI

[shortcuts]
f6 = { press = "audio_volume_mute" }
home = { open = "org.mozilla.firefox" }
ctrl-alt-arrow_left = { tile = "left" }
cmd-h = []                             # drops the key

["com.apple.Music"]
on_launch = { force_quit = "com.apple.Music" }

["com.mitchellh.ghostty"]
on_launch = "full_screen"
shortcuts.cmd-h = false                # the key reaches Ghostty
```

`config.toml` is the full example.

### Tables

| Table                       | Keys                                                                   |
| --------------------------- | ---------------------------------------------------------------------- |
| `[shortcuts]`               | shortcut = binding, in every application                              |
| `[mouse]`                   | `natural_scrolling`: bool for mouse wheels; unset follows System Settings |
| `[displays]`                | `sync_brightness`: bool, default `false`                               |
| `["<bundle identifier>"]`   | `on_launch`: command; `shortcuts`: shortcut = binding while the application is in front |

An application's table is named by its quoted bundle identifier, which always has a dot:
`["org.mozilla.firefox"]`. Its shortcuts override `[shortcuts]`. Write them as `shortcuts.cmd-h = ...`
under one header per application. `on_launch` also runs for applications already running when alt
starts, and window verbs wait up to 15 seconds for the main window.

### Shortcuts

A shortcut is modifiers and a key joined by `-`: `ctrl-alt-shift-cmd-key`. Modifiers appear at most
once, in that order, in lower case; each shortcut has one spelling, so a repeated shortcut is
TOML's duplicate key error. Keys are W3C `KeyboardEvent.code` names in snake case, letters and
digits by character, and `dictation`: `a`, `1`, `f3`, `page_up`, `arrow_left`, `backquote`.
Modifier keys, Caps Lock, Fn and the media and brightness keys never arrive as key presses, so a
shortcut cannot be one of them; `press` can send the media and brightness keys.

A binding is a command or `false`:

| Binding      | Meaning                                                              |
| ------------ | -------------------------------------------------------------------- |
| command      | runs the command and drops the key; `[]` runs nothing                |
| `false`      | not a shortcut here: the key reaches the application                 |

The front application's binding wins over the global one, and `false` in an application ends the
lookup. At the root, `false` is the same as no entry.

### Actions

A command is an action, or a list of actions that run in order. An action is a verb, `"fill"`, or
`{ verb = argument }`.

| Verb                | Argument                                                                  | Effect                                                    |
| ------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------- |
| `press`             | shortcut                                                                  | presses the combination as a keyboard does                |
| `type`              | text                                                                      | types the text                                            |
| `open`              | bundle identifier                                                         | launches the application or brings it to the front        |
| `force_quit`        | bundle identifier                                                         | force-quits the application                               |
| `full_screen`       |                                                                           | Window > Enter Full Screen                                |
| `full_screen_tile`  | `left`, `right`                                                           | Window > Full Screen Tile                                 |
| `tile`              | `left`, `right`, `top`, `bottom`, `top_left`, `top_right`, `bottom_left`, `bottom_right` | Window > Move & Resize              |
| `fill`              |                                                                           | Window > Fill                                             |
| `center`            |                                                                           | Window > Center                                           |
| `move_to`           | `built_in`, `next`, `previous`, or a display's name                       | Window > Move to                                          |
| `resize`            | `{ width = int, height = int }`, each optional                            | changes the window's size by points; moves a split divider |
| `connect`           | Bluetooth device name or address                                          | connects the paired device                                |
| `disconnect`        | Bluetooth device name or address                                          | disconnects it                                            |
| `toggle_connection` | Bluetooth device name or address                                          | connects it when disconnected, otherwise disconnects it   |
| `exec`              | shell command                                                             | runs it with `sh -c`                                      |
| `notify`            | `strftime` template                                                       | shows a notification of the template filled with the time |

Window verbs act on the focused window in a shortcut, and on the application's main window in
`on_launch`. Those that name a Window menu item press it, found by its English title; `resize` and
`full_screen` set the window's Accessibility attributes. `move_to` lists the other displays, in menu
order. A held key repeats a command when every action in it is `press`, `type` or `resize`.

### Errors

An invalid file reports the line and the offending key or value. `alt` exits non-zero for an
invalid file, `alt edit` offers to edit it again, and the daemon keeps the last valid configuration
when a changed file is invalid. A daemon that starts with a missing or invalid file runs the default
configuration until the file is valid.

The first line, `#:schema <path or url>`, makes taplo, and so helix, validate and complete the file
against the schema; the package installs it at `share/alt/config.schema.json`, which the Nix module
links to `/run/current-system/sw/share/alt/config.schema.json`.

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
