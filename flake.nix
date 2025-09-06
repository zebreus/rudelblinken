{
  description = "Rudelblinken in Rust";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs";
    nixpkgs-wit-bindgen.url = "github:nixos/nixpkgs/563c21191ff0600457bd85dc531462c073a1574b";
    nixpkgs-esp-dev = {
      url = "github:mirrexagon/nixpkgs-esp-dev/c25c658e2648bf71316c0389752ae9fc155e8b83";
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
      nixpkgs-wit-bindgen,
      fenix,
      flake-utils,
      nixpkgs-esp-dev,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs-wit-bindgen = import nixpkgs-wit-bindgen {
          inherit system;
        };
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: {
              esp-idf-esp32c3 = nixpkgs-esp-dev.packages.${system}.esp-idf-esp32c3;
              wit-bindgen = pkgs-wit-bindgen.wit-bindgen;
            })
            fenix.overlays.default
          ];
        };

        esp-idf-esp32c3 = nixpkgs-esp-dev.packages.${system}.esp-idf-esp32c3;
        fenixPkgs = fenix.packages.${system};
        rustToolchain = fenixPkgs.combine [
          fenixPkgs.complete.toolchain
          fenixPkgs.targets.riscv32imc-unknown-none-elf.stable.completeToolchain
          fenixPkgs.targets.wasm32-unknown-unknown.stable.completeToolchain

          (fenixPkgs.complete.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
          ])
        ];

        # rustToolchain = fenix.packages.${system}.fromToolchainFile {
        #   file = ./rust-toolchain.toml;
        #   sha256 = "sha256-HAFn+jo7K/dwbCKRHNXQU+x9b+8LJ8xlQGL/tE0rNlE=";
        # };

        fakeGit = pkgs.writeShellScriptBin "git" ''

          if test "$(pwd)" = "$IDF_PATH" ; then
            if test "$1" = "rev-parse" && test "$2" = "--show-toplevel" ; then
              pwd
              exit 0
            fi
            if test "$1" = "rev-parse" && test "$2" = "--git-dir" ; then
              pwd
              exit 0
            fi
            if test "$1" = "rev-parse" && test "$2" = "--short" && test "$3" = "HEAD" ; then
              echo "v5.2.2"
              exit 0
            fi
            if test "$1" = "rev-parse" && test "$2" = "HEAD" ; then
              echo "v5.2.2"
              exit 0
            fi
          fi
          if test "$5" = "describe" && test "$6" = "--all" && test "$7" = "--exact-match" ; then
            echo "v5.2.2"
            exit 0
          fi

          ${pkgs.git}/bin/git "$@"
        '';

      in
      rec {
        name = "rudelblinken-rs";

        devShell = pkgs.mkShell {
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          ESP_IDF_TOOLS_INSTALL_DIR = "fromenv";
          ESP_IDF_VERSION = "v5.2.2";
          MCU = "esp32c3";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.dbus.lib
            # Provides libudev for rudelctl espflash
            pkgs.systemd
          ];

          buildInputs = [
            fakeGit
            esp-idf-esp32c3
            rustToolchain
            pkgs.rust-analyzer-nightly
            pkgs.cargo-generate
            pkgs.cargo-espflash
            pkgs.ldproxy
            pkgs.probe-rs-tools
            pkgs.wasm-tools
            pkgs.wit-bindgen
            pkgs.cargo-rdme
            pkgs.jq
            # For the cli:
            pkgs.dbus
            pkgs.pkg-config
            # Provides libudev
            pkgs.systemd
          ];
        };

        packages.rudelctl =
          (pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          }).buildRustPackage
            {
              pname = "rudelctl";
              version = "0.1.0";

              src = ./rudelctl;

              cargoLock = {
                lockFile = ./rudelctl/Cargo.lock;
              };

              postPatch = ''
                sed -i 's|path = "../rudelblinken-runtime", ||' Cargo.toml || true
              '';

              nativeBuildInputs = [
                pkgs.pkg-config
              ];
              buildInputs = [
                pkgs.dbus
              ];

              meta = {
                description = "Rudelblinken cli utility";
                homepage = "https://github.com/zebreus/rudelblinken";
                license = pkgs.lib.licenses.agpl3Plus;
              };
            };
        packages.default = packages.rudelctl;

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
