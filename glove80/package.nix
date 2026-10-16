# Glove80 firmware: MoErgo's ZMK, built for both halves from glove80.keymap and combined into
# $out/share/alt/glove80.uf2, which either half's bootloader accepts and `alt setup` finds on
# XDG_DATA_DIRS. The keymap sends standard keys; the app's bindings give them meaning.
{
  zmk,
  system,
  runCommand,
}:
let
  firmware = import zmk { pkgs = import "${zmk}/nix/pinned-nixpkgs.nix" { inherit system; }; };
  half =
    board:
    firmware.zmk.override {
      inherit board;
      keymap = ./glove80.keymap;
    };
in
runCommand "glove80-firmware" { } ''
  install -D ${firmware.combine_uf2 (half "glove80_lh") (half "glove80_rh")}/glove80.uf2 $out/share/alt/glove80.uf2
''
