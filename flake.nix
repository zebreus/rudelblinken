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
        fenixPkgs = fenix.packages.${system};
        toolchain = fenixPkgs.complete;
        combinedToolchain = fenixPkgs.combine [
          toolchain.toolchain
          fenixPkgs.targets.wasm32-unknown-unknown.stable.completeToolchain
          fenixPkgs.targets.wasm32-wasi.stable.completeToolchain
          fenixPkgs.targets.riscv32imc-unknown-none-elf.stable.completeToolchain
        ];

        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-HAFn+jo7K/dwbCKRHNXQU+x9b+8LJ8xlQGL/tE0rNlE=";
        };

      in
      {
        name = "rudelblinken-rs";

        devShell =
          with pkgs;
          pkgs.mkShell {
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            RUST_SRC_PATH = "${combinedToolchain}/lib/rustlib/src/rust/library";
            ESP_IDF_TOOLS_INSTALL_DIR = "fromenv";
            MCU = "esp32c3";

            buildInputs = [
              (pkgs.writeShellScriptBin "git" ''
                if test "$1" = "rev-parse" && test "$2" = "--show-toplevel" ; then
                  echo /nix/store/mqmhvcs41lkpvgfk6a4w6nbm0v784lx2-esp-idf-v5.3
                  exit 0
                fi
                if test "$1" = "rev-parse" && test "$2" = "--git-dir" ; then
                  echo /nix/store/mqmhvcs41lkpvgfk6a4w6nbm0v784lx2-esp-idf-v5.3
                  exit 0
                fi
                if test "$5" = "describe" && test "$6" = "--all" ; then
                  echo /nix/store/mqmhvcs41lkpvgfk6a4w6nbm0v784lx2-esp-idf-v5.3
                  exit 0
                fi
                if test "$1" = "rev-parse" && test "$2" = "--short" && test "$3" = "HEAD" ; then
                  echo "v5.3"
                  exit 0
                fi
                if test "$1" = "rev-parse" && test "$2" = "HEAD" ; then
                  echo "v5.3"
                  exit 0
                fi

                ${pkgs.git}/bin/git "$@"
              '')
              pkgs.esp-idf-esp32c3
              openssl
              pkg-config
              rustToolchain
              cargo-generate
              cargo-espflash
              ldproxy
              libclang.dev
              libclang.lib
              libclang
            ];
          };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
