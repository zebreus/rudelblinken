//! Test wasm files on an emulated rudelblinken device.
use rudelblinken_runtime::host::{Advertisement, Event};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
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
        let (sender, host) = rudelblinken_runtime::emulated_host::EmulatedHost::new();
        let mut instance = rudelblinken_runtime::linker::setup(&wasm, host)?;
        let start_time = Instant::now();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(1000));

            let data = (0..32u8).collect::<Vec<_>>();
            let data: [u8; 32] = data.as_slice().try_into().unwrap();
            let advertisement = Advertisement {
                address: [00, 11, 23, 44, 55, 66, 00, 00],
                data: data,
                data_length: 3,
                received_at: start_time.elapsed().as_micros() as u64,
            };
            sender
                .send(Event::AdvertisementReceived(advertisement))
                .unwrap();
        });
        instance.run()?;
        Ok(())
    }
}
