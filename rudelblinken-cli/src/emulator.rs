//! Test wasm files on an emulated rudelblinken device.
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmulatorError {
    #[error("Failed to read the WASM source file")]
    FailedToReadWasmFile(#[from] std::io::Error),
    #[error(transparent)]
    RuntimeError(#[from] rudelblinken_runtime::Error),
}

pub struct Emulator {
    file: PathBuf,
}

impl Emulator {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }
    pub fn emulate(&self) -> Result<(), EmulatorError> {
        let wasm = std::fs::read(&self.file)?;
        let host = rudelblinken_runtime::emulated_host::EmulatedHost::new();
        let mut instance = rudelblinken_runtime::linker::setup(&wasm, host)?;
        instance.run()?;
        Ok(())
    }
}
