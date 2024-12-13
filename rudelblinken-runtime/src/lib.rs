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
    use super::emulated_host::EmulatedHost;
    use super::linker::setup;

    #[test]
    fn can_execute_helloworld() {
        let module_bytes = std::fs::read("../wasm-binaries/binaries/hello_world.wasm").unwrap();

        let host = EmulatedHost::new();
        let mut instance = setup(&module_bytes, host).unwrap();
        instance.run().unwrap();
    }

    #[test]
    fn logging_works() {
        let module_bytes = std::fs::read("../wasm-binaries/binaries/test_logging.wasm").unwrap();

        let host = EmulatedHost::new();
        let mut instance = setup(&module_bytes, host).unwrap();
        instance.run().unwrap();
    }

    #[test]
    fn infinite_loop_gets_killed() {
        let module_bytes = std::fs::read("../wasm-binaries/binaries/infinite_loop.wasm").unwrap();

        let host = EmulatedHost::new();
        let mut instance = setup(&module_bytes, host).unwrap();
        let error: wasmi::Error = instance.run().unwrap_err();
        assert_eq!(
            error.as_trap_code().unwrap(),
            wasmi::core::TrapCode::OutOfFuel
        );
    }
    // // How would I even test this?
    // #[test]
    // fn infinite_loop_does_not_get_killed_if_it_yields() {
    //     let module_bytes = std::fs::read("../wasm-binaries/binaries/infinite_loop.wasm").unwrap();

    //     let host = EmulatedHost::new();
    //     let mut instance = setup(&module_bytes, host).unwrap();
    //     instance.run().unwrap();
    // }
}
