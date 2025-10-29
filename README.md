# rudelblinken-rs

Synced blinking catears in Rust. On the technical side `rudelblinken-rs` is a project focused on experimenting with embedded applications using WebAssembly (Wasm).

## Notes

Less of a Readme, more a unstructured collection of notes.

## Repository Overview

The repository is structured into a set of crates, each with a defined role:

- [**`rudelblinken-runtime`**](./rudelblinken-runtime): This crate provides the host runtime environment needed to execute Wasm modules using the wasmi runtime.
- [**`rudelblinken-filesystem`**](rudelblinken-filesystem): This crate implements a zero-copy, flash-optimized filesystem for embedded systems. It is used to persist data and program files on the device.
- [**`rudelblinken-sdk`**](rudelblinken-sdk): The Software Development Kit (SDK) for developing Wasm modules, it provides Rust APIs that allow the Wasm guest to interact with the host and peripherals through a set of common traits. You would link against this crate, when developing Wasm modules for rudelblinken.
- [**`rudelblinken-firmware`**](rudelblinken-firmware): This crate implements the bare-metal firmware running on the ESP32-C3. It uses the runtime to run WASM binaries and provides facilities for installing, and debugging WASM modules via Bluetooth Low Energy.
- [**`rudelctl`**](rudelctl): The CLI that allows interaction with the rudelblinken devices for tasks like uploading and running Wasm modules. It also has emulation capabilities for local testing.
- [**`wasm-binaries`**](wasm-binaries): A collection of example Wasm binaries used for testing and demonstration purposes.

## Motivation for Using WebAssembly

We chose WebAssembly as a foundation because:

- **Safety**: Wasm's sandboxed environment ensures a measure of safety, allowing for experimentation and deployment of new code without the fear of bricking devices.
- **Language Flexibility**: Wasm enables developers to write code in their preferred programming language, as long as it compiles to WebAssembly.

## Getting Started

1.  **Clone the repository:**

    ```bash
    git clone https://github.com/zebreus/rudelblinken-rs
    cd rudelblinken-rs
    ```

2.  **Enter an environment with the required dependencies.**
    To build rudelblinken you need to have Rust targets for RISC-V, x86, and WASM installed.

    We defined all dependencies using the nix package manager. For now this is the only supported way to build the project. After installing nix, you can use the following command to enter a shell with all dependencies available:

    ```bash
    nix develop .
    ```

3.  **Build and flash the firmware:**
    You can build and flash the firmware to the ESP32-C3 board using `cargo run`

    ```bash
    cd rudelblinken-firmware
    # Build the firmware
    cargo build
    # Build and flash the firmware
    cargo run
    ```

    Then you can flash the default program to execute using parttool.py and a bit of bash:
    ```bash
    f=../wasm-binaries/binaries/reference_sync_v1.wasm; tf=$(mktemp); \
    ( \
        l="$(wc -c "$f" | cut -d ' ' -f1)"; \
        cat $f; head -c"$((32*1024-4-$l))" /dev/zero; \
        printf "%08x" "$l" | fold -w2 | tac | xxd -p -r \
    ) > "$tf"; \
    /nix/store/k1dhx68bzlcv47wmlj0mhygl5x992sis-esp-idf-v5.2.2/components/partition_table/parttool.py \
        write_partition --partition-name default_program --input "$tf"; rm "$tf"
    ```

4.  **Build Wasm examples:**

    The WASM examples are currently broken.

    ```bash
    cd wasm-binaries
    ./build.sh
    ```

5.  **Use rudelctl to interact with your board**

Upload your WASM binary to the board:

```shell
cd rudelctl
cargo run -- upload ../wasm-binaries/binaries/test_logging.wasm
```

or emulate it locally:

```shell
cd rudelctl
cargo run -- emulate ../wasm-binaries/binaries/test_logging.wasm
```

## Hardware

Rudelblinken can be run on any ESP32-C3 board.

### Custom rudelblinken boards

The preferred way to use rudelblinken is to use the custom PCBs we designed. They are designed to fit perfectly on 3D printed cat-ears. You can join the order of the next batch of boards at https://rudelb.link. Instructions on how to attach LED strips to the board can be found at https://md.darmstadt.ccc.de/s/rudelblinken-38c3

The schematic and board files can be found at https://oshwlab.com/zebreus/rudelblinken

### ESP32-C3 supermini boards

The first prototypes were based on ESP32-C3 supermini boards. To use one, you only need an ESP board, an MOSFET, an LED strip and some wires. We provide simple build instructions at https://md.darmstadt.ccc.de/rudelblinken-mrmcd

## Developing for the rudelblinken platform

Not recommended yet, as the project is still changing all the time. If you still want to try have a look at the **`reference_sync_v1`** crate (in the `wasm-binaries` directory) and do the same thing it does.

You should also be able to use other languages to target the rudelblinken platform, as long as they can compile to WebAssembly, however we have only tested Rust so far. You can use `wit-bindgen` to generate the bindings for your preferred language.

## Core Concepts & Terms

### WebAssembly (Wasm)

WebAssembly is a binary instruction format for a stack-based virtual machine. It is designed to be a portable compilation target for languages like C, C++, and Rust, making it ideal for running code in diverse environments like web browsers and embedded devices.

### Host

The host provides functionality for executing WASM modules in a `rudel-host` world.

### Firmware

The firmware is the program that runs on the microcontroller. It provides an implementation of the `rudel-host` world and ways to manage the WASM modules.

### Guest

Guest modules are Wasm binaries that contain the application logic. They can use the functions provided by the `rudel` world to interact with the hardware. Keep in mind that guests can only use one 64K page of memory and will be killed if they attempt to get more.

## Contributing

Contributions are welcome! Please feel free to submit pull requests, report issues, and suggest enhancements.
