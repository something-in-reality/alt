{
  lib,
  rustPlatform,
  installShellFiles,
}:
let
  manifest = (lib.importTOML ./Cargo.toml).package;
  identifier = "something-in-reality.alt";
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
      ./build.rs
      ./config.toml
      ./signing-identity
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  cargoBuildFlags = [ "--workspace" ];
  # build.rs writes shell completions here.
  env.COMPLETIONS = "completions";
  cargoTestFlags = [ "--workspace" ];

  nativeBuildInputs = [ installShellFiles ];

  # An app bundle gives the binary a bundle identifier for its permissions; bin/signing-identity
  # derives the identity that signs it.
  postInstall = ''
    app=$out/Applications/Alt.app/Contents
    install -D $out/bin/alt $app/MacOS/alt
    substitute Info.plist $app/Info.plist --subst-var version --subst-var-by identifier ${identifier}
    installShellCompletion completions/{alt.bash,_alt}
  '';

  # The bundle identifier, which names the app's permissions and log subsystem.
  passthru = { inherit identifier; };

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
