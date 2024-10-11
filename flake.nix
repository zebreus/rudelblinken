{
  description = "Rudelblinken in Rust";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs";
    nixpkgs-esp-dev = {
      url = "github:mirrexagon/nixpkgs-esp-dev";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
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
        lib = nixpkgs.lib;
        fenixPkgs = fenix.packages.${system};
        toolchain = fenixPkgs.complete;
        combinedToolchain = fenixPkgs.combine [
          toolchain.toolchain
          fenixPkgs.targets.wasm32-unknown-unknown.stable.completeToolchain
          fenixPkgs.targets.wasm32-wasi.stable.completeToolchain
          fenixPkgs.targets.riscv32imc-unknown-none-elf.stable.completeToolchain
        ];

      in
      {
        name = "rudelblinken-rs";

        devShell = pkgs.mkShell {
          RUST_SRC_PATH = "${combinedToolchain}/lib/rustlib/src/rust/library";

          buildInputs = [
            pkgs.esp-idf-esp32c3
            (pkgs.clang-tools.override { enableLibcxx = false; })
            pkgs.glibc_multi.dev
            combinedToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            pkgs.cargo-outdated
            pkgs.ldproxy
          ];
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
