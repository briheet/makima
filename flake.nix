{
  description = "makima: multi agent spawning and stuff";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    systems = {
      url = "github:nix-systems/default";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
    }:
    let
      inherit (nixpkgs) lib legacyPackages;

      forAllSystems = lib.genAttrs (import systems);
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = legacyPackages.${system};
        in
        {
          # No C compiler wrapper: Swift comes from the system toolchain
          # (Xcode or Command Line Tools), and nix's cc/SDK env breaks it.
          default = pkgs.mkShellNoCC {
            packages = with pkgs; [
              just
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
            ];
          };
        }
      );

      formatter = forAllSystems (system: legacyPackages.${system}.nixfmt);
    };
}
