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
    app=$out/Applications/Accessibility.app/Contents
    install -D $out/bin/accessibility $app/MacOS/accessibility
    substitute Info.plist $app/Info.plist --subst-var version
  '';

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
