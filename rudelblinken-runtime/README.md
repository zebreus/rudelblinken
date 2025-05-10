<!-- cargo-rdme start -->

# Rudelblinken Runtime

Host runtime for rudelblinken wasm modules. This crate provides functionality to link your host implementation with a rudelblinken wasm module.

For testing this provides a simulated host implementation in [rudelblinken_runtime::emulated_host::EmulatedHost]

You can use it like this:

```rust
use rudelblinken_runtime::emulated_host::EmulatedHost;
use rudelblinken_runtime::linker::setup;

const WASM_MOD: &[u8] = include_bytes!(
    "../../wasm-binaries/binaries/infinite_loop_yielding.wasm"
);

let host = EmulatedHost::new();
let mut instance = setup(WASM_MOD, host).unwrap();
instance.run().unwrap();
```

<!-- cargo-rdme end -->
