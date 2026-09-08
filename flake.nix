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
          default = pkgs.mkShell {
            packages = with pkgs; [
              beam28Packages.elixir_1_20
            ];
          };
        }
      );

      formatter = forAllSystems (system: legacyPackages.${system}.nixfmt);
    };
}
