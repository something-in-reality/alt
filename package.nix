{
  lib,
  rustPlatform,
  installShellFiles,
}:
let
  manifest = (lib.importTOML ./Cargo.toml).package;
  inherit (manifest.metadata.bundle) identifier path;
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
  outputs = [
    "out"
    "app"
  ];
  cargoBuildFlags = [ "--workspace" ];
  # build.rs writes shell completions here.
  env.COMPLETIONS = "completions";
  cargoTestFlags = [ "--workspace" ];

  nativeBuildInputs = [ installShellFiles ];

  # The app bundle, in its own output, gives the binary a bundle identifier for its permissions.
  postInstall = ''
    app=$app/Applications/${baseNameOf path}/Contents
    install -D $out/bin/alt $app/MacOS/alt
    substitute Info.plist $app/Info.plist --subst-var version --subst-var-by identifier ${identifier}
    installShellCompletion completions/{alt.bash,_alt}
    install -d $out/share/alt
    $out/bin/alt schema > $out/share/alt/config.schema.json
  '';

  # The bundle identifier, which names the app's permissions and log subsystem, and the path the
  # nix-darwin module installs the app at, `bundle`.
  passthru = {
    inherit identifier;
    bundle = path;
  };

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
