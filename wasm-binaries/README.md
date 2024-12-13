# Prebuilt WASM binaries for testing

This directory contains prebuilt WASM binaries for testing. The binaries are stored in the 'binaries' directory and build using the ['build.sh'](./build.sh) script.

## Add a new binary

1. Copy one of the existing crates.
2. Adjust the crate name in the name in the new crate's `Cargo.toml` file.
3. Add the new crate as a member in the Cargo.toml file in this directory.
4. Run `./build.sh` to build the new binary.

You can use `./create.sh <NAME>` to perform steps 1-3 in one go.
