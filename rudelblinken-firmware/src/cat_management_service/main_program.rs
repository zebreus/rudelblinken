//! The cat management service is reponsible for managing the currently running program and its environment
//!
//! ## Logic for managing the program
//! ```
//! // TODO: Actually implement this
//! ```
//!
//! Main program: The program that was last set via the program hash characteristic
//!
//! Default program: A wasm binary which provides default blinking behaviour and thats included in the firmware
//!
//! Program failure counter: Counts the number of failures of the main program since the last success
//!
//! Failure flag: A flag that gets set when a program is launched and reset if the program didn't crash in [PROGRAM_SUCCESS_DURATION]
//!
//! ### MVP
//!
//! - If the program ran for for [PROGRAM_SUCCESS_DURATION] -> Unset the failure flag
//! - If a program is already running -> Stop here
//! - If the failure flag is set -> Increment the program failure counter. Unset the failure flag.
//! - If the program failure counter exceeds [MAX_CONSECUTIVE_FAILURES] -> Unset the main program file
//! - If there is no main program file set -> Run the default program
//! - If there is an error finding the main program file -> Unset the main program file
//! - If the program file was found and opened -> Set the failure flag
//! - If there is an error starting the main program file -> Nothing
//! - If the program crashed or exited -> Nothing
//! - If a new main program is received -> Stop the current program, reset the failure counter, reset failure flag
//!
//! ### Future
//!
//! - Temporary programs
//! - A secure way to update the default program
//!
use crate::config::{failure_counter, failure_flag, main_program};
use crate::wasm_service::wasm_host::HostEvent;
use crate::wasm_service::wasm_host::WasmHost;
use crate::{wasm_service, BLE_DEVICE};
use esp32_nimble::BLEScan;
use esp_idf_hal::task;
use load_main_program::load_main_program;
use rudelblinken_runtime::host::Advertisement;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::{sync::mpsc, time::Duration};
use tracing::{error, info, warn};

mod load_main_program;

/// The delay after booting, until the main program is launched
const MAIN_PROGRAM_DELAY: Duration = Duration::from_secs(3);

/// The duration a program needs to run for to be marked as not crashed
const PROGRAM_SUCCESS_DURATION: Duration = Duration::from_secs(30);
/// The max number of consecutive crashed a program is allowed to have before its deleted
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

fn log_heap_stats() {
    info!(
        free_heap = unsafe { esp_idf_sys::esp_get_free_heap_size() },
        largest_block = unsafe {
            esp_idf_sys::heap_caps_get_largest_free_block(
                esp_idf_sys::MALLOC_CAP_DMA
                    | esp_idf_sys::MALLOC_CAP_32BIT
                    | esp_idf_sys::MALLOC_CAP_DEFAULT,
            )
        },
        "heap stats",
    )
}

/// The wasmrunner represents a background task that manages the currently running wasm program
pub struct WasmRunner {
    sender: mpsc::Sender<HostEvent>,
}

impl WasmRunner {
    pub fn new() -> Self {
        let (sender, _receiver, host) = wasm_service::wasm_host::WasmHost::new();

        let _runner_thread = std::thread::Builder::new()
            .name("wasm_runner".to_owned())
            .stack_size(0x2000)
            .spawn(|| {
                Self::runner_thread(host);
            });

        let sender_clone = sender.clone();
        let _ble_thread = std::thread::Builder::new()
            .name("ble_scanning".to_owned())
            .stack_size(0x8000)
            .spawn(|| {
                Self::ble_thread(sender_clone);
            });

        return WasmRunner { sender };
    }

    pub fn set_new_file(&mut self, hash: &[u8; 32]) {
        main_program::set(&Some(*hash));
        failure_counter::set(&0);
        failure_flag::set(&false);
        self.sender.send(HostEvent::ProgramChanged()).unwrap();
    }

    /// The main loop of the wasm runner. Won't return
    fn runner_thread(mut host: WasmHost) -> ! {
        std::thread::sleep(MAIN_PROGRAM_DELAY);

        loop {
            if failure_flag::get() {
                let last_failure_counter = failure_counter::get();
                let next_failure_counter = last_failure_counter + 1;
                warn!(
                    "Failure flag was set. Incrementing failure counter to {}",
                    next_failure_counter
                );
                failure_counter::set(&next_failure_counter);
                failure_flag::set(&false);
            }
            if failure_counter::get() > MAX_CONSECUTIVE_FAILURES {
                warn!("Too many consecutive failures. Deleting main program");
                main_program::set(&None);
                failure_counter::set(&0);
            }

            let program = load_main_program(&mut host);
            failure_flag::set(&true);

            info!("before creating and linking instance");
            log_heap_stats();

            let mut instance =
                match rudelblinken_runtime::linker::setup(program.as_ref(), host.clone()) {
                    Ok(instance) => instance,
                    Err(error) => {
                        error!("Linker Error:\n {}", error);
                        continue;
                    }
                };

            info!("after creating and linking inhstance");
            log_heap_stats();

            let process_exited_by_now = Arc::new(AtomicBool::new(false));
            let process_exited_by_now_clone = process_exited_by_now.clone();
            std::thread::spawn(move || {
                std::thread::sleep(PROGRAM_SUCCESS_DURATION);
                if process_exited_by_now_clone.load(Ordering::Relaxed) {
                    // Dont reset the failure flag if the process exited by now
                    return;
                }
                failure_flag::set(&false);
            });
            let result = instance.run();
            process_exited_by_now.store(true, Ordering::Relaxed);

            match result {
                Ok(_) => info!("Wasm module finished execution"),
                Err(err) => {
                    error!("Wasm module failed to execute: {}", err);
                }
            }
        }
        // panic!("The runner thread should never return");
    }

    fn ble_thread(sender: Sender<HostEvent>) {
        task::block_on(async {
            let mut ble_scan = BLEScan::new();
            ble_scan.active_scan(false).interval(100).window(99);
            // We can only start scanning after we started the ble server/ advertising.
            // TODO: Figure out how to properly wait until the server started
            std::thread::sleep(MAIN_PROGRAM_DELAY);
            loop {
                // tracing::info!("Scanning for BLE devices");
                ble_scan
                    .start(&BLE_DEVICE, 1000, |dev, data| {
                        if let Some(md) = data.manufacture_data() {
                            let now = unsafe { esp_idf_sys::esp_timer_get_time() as u64 };

                            let mut padded_mac = [0u8; 8];
                            padded_mac[0..6].copy_from_slice(&dev.addr().as_le_bytes());
                            let mut data = [0u8; 32];
                            let data_length = std::cmp::min(md.payload.len(), 32);
                            data[..data_length].copy_from_slice(&md.payload[..data_length]);
                            sender
                                .send(HostEvent::AdvertisementReceived(Advertisement {
                                    company: md.company_identifier,
                                    address: padded_mac,
                                    data,
                                    data_length: data_length as u8,
                                    received_at: now,
                                }))
                                .unwrap();
                        }
                        None::<()>
                    })
                    .await
                    .expect("scan failed");
            }
        });
    }
}
