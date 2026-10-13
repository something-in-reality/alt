{ lib, rustPlatform }:
let
  manifest = (lib.importTOML ./Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./Info.plist
      ./config.toml
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  # An app bundle gives the binary a bundle identifier for its permissions.
  postInstall = ''
    install -D -m 0644 Info.plist $out/Applications/Accessibility.app/Contents/Info.plist
    install -D $out/bin/accessibility $out/Applications/Accessibility.app/Contents/MacOS/accessibility
  '';

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
