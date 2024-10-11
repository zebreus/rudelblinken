{
  description = "Rudelblinken in Rust";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs";
    nixpkgs-esp-dev = {
      url = "github:mirrexagon/nixpkgs-esp-dev";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      nixpkgs-esp-dev,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: { esp-idf-esp32c3 = nixpkgs-esp-dev.packages.${system}.esp-idf-esp32c3; })
          ];
        };
      in
      {
        name = "rudelblinken-rs";

        devShell = pkgs.mkShellNoCC {
          buildInputs = [
            pkgs.esp-idf-esp32c3
            (pkgs.clang-tools.override { enableLibcxx = false; })
            pkgs.glibc_multi.dev
          ];
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
