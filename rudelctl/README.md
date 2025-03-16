<!-- cargo-rdme start -->

# rudelctl

`rudelctl` is the cli utility for the `rudelblinken` project. It is used to program and run WASM binaries on the `rudelblinken` devices. It can also run WASM binaries in a simulated environment.

## Usage

Until I have time to write proper documentation, here is the output of `rudelctl --help`:

```rust
Usage: rudelctl <COMMAND>

Commands:
upload   Upload a file
run      Run a WASM binary
scan     Scan for cats
emulate  Emulate a rudelblinken device
flash    Flash a built-in copy of the rudelblinken firmware via USB
help     Print this message or the help of the given subcommand(s)

Options:
-h, --help     Print help
```

## Updating the integrated rudelblinken firmware binary

`rudelctl` contains a built-in rudelblinken firmware binary. To update the binary, run the `update-firmware.sh`` script in the root of this crate. This will build the firmware and copy the binary to the `firmware` directory. You need to have the entire repository checked out to run the script, because it will look for firmware sources in an adjacent directory.

<!-- cargo-rdme end -->
