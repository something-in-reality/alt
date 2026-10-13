{
  description = "Keyboard, pointer, window and display behaviour outside the terminal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "aarch64-darwin" ];
    in
    {
      packages = forAllSystems (system: {
        default = nixpkgs.legacyPackages.${system}.callPackage ./package.nix { };
      });

      checks = self.packages;

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      homeModules.default = ./module.nix;

      devShells = forAllSystems (system: {
        default =
          with nixpkgs.legacyPackages.${system};
          mkShell {
            packages = [
              cargo
              clippy
              rust-analyzer
              rustc
              rustfmt
            ];
          };
      });
    };
}
