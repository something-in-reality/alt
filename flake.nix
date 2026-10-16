{
  description = "Keyboard, pointer, window and display behaviour outside the terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    zmk = {
      url = "github:moergo-sc/zmk/v25.05";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      zmk,
    }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "aarch64-darwin" ];
    in
    {
      packages = forAllSystems (system: {
        default = nixpkgs.legacyPackages.${system}.callPackage ./package.nix { };
        glove80 = nixpkgs.legacyPackages.${system}.callPackage ./glove80/package.nix {
          inherit zmk system;
        };
      });

      checks = self.packages;

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      darwinModules.default = import ./module.nix self;

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
