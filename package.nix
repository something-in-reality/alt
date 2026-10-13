{
  lib,
  rustPlatform,
  installShellFiles,
}:
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
      ./signing-identity
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  cargoBuildFlags = [ "--workspace" ];
  cargoTestFlags = [ "--workspace" ];

  nativeBuildInputs = [ installShellFiles ];

  # An app bundle gives the binary a bundle identifier for its permissions; bin/signing-identity
  # derives the identity that signs it.
  postInstall = ''
    app=$out/Applications/Accessibility.app/Contents
    install -D $out/bin/accessibility $app/MacOS/accessibility
    substitute Info.plist $app/Info.plist --subst-var version
    installShellCompletion --cmd accessibility \
      --bash <($out/bin/accessibility completions bash) \
      --fish <($out/bin/accessibility completions fish) \
      --zsh <($out/bin/accessibility completions zsh)
  '';

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
