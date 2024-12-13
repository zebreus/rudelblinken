//! # Rudelblinken Runtime
//!
//! Host runtime for rudelblinken wasm modules. This crate provides functionality to link your host implementation with a rudelblinken wasm module.
//!
//! For testing this provides a simulated host implementation in [rudelblinken_runtime::emulated_host::EmulatedHost]
//!
//! You can use it like this:
//!  
//! ```rust
//! use rudelblinken_runtime::emulated_host::EmulatedHost;
//! use rudelblinken_runtime::linker::setup;
//!
//! const WASM_MOD: &[u8] = include_bytes!(
//!     "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
//! );
//!
//! let host = EmulatedHost::new();
//! let mut instance = setup(WASM_MOD, host).unwrap();
//! instance.run().unwrap();
//! ```

pub mod emulated_host;
pub mod host;
pub mod linker;

/// This crate uses wasmi::Error as its main error type.
pub use wasmi::Error;

#[cfg(test)]
mod tests {

    #[test]
    fn test() {
        use super::emulated_host::EmulatedHost;
        use super::linker::setup;

        const WASM_MOD: &[u8] = include_bytes!(
            "../../rudelblinken-wasm/target/wasm32-unknown-unknown/release/rudelblinken_wasm.wasm"
        );

        let host = EmulatedHost::new();
        let mut instance = setup(WASM_MOD, host).unwrap();
        instance.run().unwrap();
    }
}
