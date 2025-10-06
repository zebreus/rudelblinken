{
  description = "Rudelblinken in Rust";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
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
            (
              _: super:
              let
                pkgs = fenix.inputs.nixpkgs.legacyPackages.${super.system};
              in
              fenix.overlays.default pkgs pkgs
            )
          ];
        };

        esp-idf-riscv = nixpkgs-esp-dev.packages.${system}.esp-idf-riscv.override {
          rev = "v5.4.1";
          sha256 = "sha256-5hwoy4QJFZdLApybV0LCxFD2VzM3Y6V7Qv5D3QjI16I=";
        };

        esp_idf_version = esp-idf-riscv.version;

        fenixPkgs = fenix.packages.${system};
        rustToolchain = fenixPkgs.combine [
          fenixPkgs.complete.toolchain

          fenixPkgs.targets.riscv32imc-unknown-none-elf.latest.toolchain
          fenixPkgs.targets.wasm32-unknown-unknown.latest.toolchain

          (fenixPkgs.latest.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
          ])
        ];

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
              echo "${esp_idf_version}"
              exit 0
            fi
            if test "$1" = "rev-parse" && test "$2" = "HEAD" ; then
              echo "${esp_idf_version}"
              exit 0
            fi
          fi
          if test "$5" = "describe" && test "$6" = "--all" && test "$7" = "--exact-match" ; then
            echo "${esp_idf_version}"
            exit 0
          fi

          ${pkgs.git}/bin/git "$@"
        '';

      in
      rec {
        name = "rudelblinken-rs";

        devShell = pkgs.mkShell {
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          ESP_IDF_TOOLS_INSTALL_DIR = "fromenv";
          ESP_IDF_VERSION = esp_idf_version;
          MCU = "esp32c3";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.dbus.lib
            # Provides libudev for rudelctl espflash
            pkgs.systemd
          ];

          buildInputs = [
            fakeGit
            rustToolchain
            esp-idf-riscv
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
