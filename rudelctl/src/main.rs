//! # rudelctl
//!
//! `rudelctl` is the cli utility for the `rudelblinken` project. It is used to program and run WASM binaries on the `rudelblinken` devices. It can also run WASM binaries in a simulated environment.
//!
//! ## Usage
//!
//! Until I have time to write proper documentation, here is the output of `rudelctl --help`:
//!
//! ```
//! Usage: rudelctl <COMMAND>
//!
//! Commands:
//! upload   Upload a file
//! run      Run a WASM binary
//! scan     Scan for cats
//! emulate  Emulate a rudelblinken device
//! help     Print this message or the help of the given subcommand(s)
//!
//! Options:
//! -h, --help     Print help
//! ```
#![feature(async_closure)]
#![feature(array_chunks)]
#![feature(int_roundings)]
#![feature(round_char_boundary)]

mod bluetooth;
mod emulator;
mod update_target;
use bluer::Device;
use bluetooth::{scan_for, Outcome};
use clap::{Parser, Subcommand};
use emulator::{EmulateCommand, Emulator};
use futures_time::time::Duration;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use std::{path::PathBuf, sync::LazyLock, time::Instant, u32};
use update_target::{UpdateTarget, UpdateTargetError};

/// Rudelblinken cli utility
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Upload a file
    Upload {
        /// Stop scanning after this many seconds
        #[arg(short, long, default_value = "2")]
        timeout: f32,

        /// Maximum number of devices to program
        #[arg(short, long, default_value = "1")]
        devices: u32,

        /// WASM file that will get flashed to the devices
        file: PathBuf,
    },
    /// Run a WASM binary
    Run {
        /// Stop scanning after this many seconds
        #[arg(short, long, default_value = "2")]
        timeout: f32,

        /// Maximum number of devices to program
        #[arg(short, long, default_value = "1")]
        devices: u32,

        /// WASM file that will get flashed to the devices
        file: PathBuf,
    },
    /// Scan for cats
    Scan {
        /// Stop scanning after this many seconds
        #[arg(short, long, default_value = "2")]
        timeout: f32,
    },
    /// Emulate a rudelblinken device
    Emulate(EmulateCommand),
}

pub static GLOBAL_LOGGER: LazyLock<MultiProgress> = LazyLock::new(|| {
    let logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .build();
    let level = logger.filter();
    let multi = MultiProgress::new();
    LogWrapper::new(multi.clone(), logger).try_init().unwrap();
    log::set_max_level(level);
    multi
});

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    LazyLock::force(&GLOBAL_LOGGER);
    let cli = Cli::parse();

    match cli.command {
        Commands::Upload {
            timeout,
            devices,
            file,
        } => {
            let file_content = tokio::fs::read(file)
                .await
                .expect("Failed to read the WASM file");

            scan_for(
                Duration::from_millis((timeout * 1000.0) as u64),
                devices,
                &async |device: Device, abort| -> Result<Outcome, UpdateTargetError> {
                    let Ok(update_target) = UpdateTarget::new_from_peripheral(&device).await else {
                        return Ok(Outcome::Ignored);
                    };
                    if devices == 1 {
                        abort.abort();
                    }
                    let target_name = device
                        .name()
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or(device.address().to_string());
                    log::info!("Connected to {}", target_name);

                    let data = &file_content;

                    let now = Instant::now();
                    log::info!(
                        "Sending {:.2}kB to {}",
                        data.len() as f32 / 1024.0,
                        device
                            .name()
                            .await
                            .ok()
                            .flatten()
                            .unwrap_or(device.address().to_string())
                    );
                    update_target.upload_file(&data, "test.txt".into()).await?;
                    let duration = now.elapsed();
                    log::info!(
                        "Sending {:.2}kB took {} millis ({:.3}kB/s)",
                        data.len() as f32 / 1024.0,
                        duration.as_millis(),
                        (data.len() as f64 / duration.as_millis() as f64)
                    );
                    return Ok(Outcome::Processed);
                },
            )
            .await
            .unwrap();
        }
        Commands::Run {
            timeout,
            devices,
            file,
        } => {
            let file_content = tokio::fs::read(file)
                .await
                .expect("Failed to read the WASM file");

            scan_for(
                Duration::from_millis((timeout * 1000.0) as u64),
                devices,
                &async |device: Device, _| -> Result<Outcome, UpdateTargetError> {
                    let Ok(update_target) = UpdateTarget::new_from_peripheral(&device).await else {
                        return Ok(Outcome::Ignored);
                    };

                    let data = &file_content;

                    update_target.run_program(&data).await?;
                    return Ok(Outcome::Processed);
                },
            )
            .await
            .unwrap();
        }
        Commands::Scan { timeout } => {
            println!("name, mac, rssi");
            scan_for(
                Duration::from_millis((timeout * 1000.0) as u64),
                u32::MAX,
                &async |device: Device, _| -> Result<Outcome, UpdateTargetError> {
                    let address = device.address();
                    let update_target = UpdateTarget::new_from_peripheral(&device).await?;
                    let rssi = device.rssi().await.ok().flatten();

                    let name = update_target.get_name().await.unwrap();
                    println!("{}, {}, {}", name, address, rssi.unwrap_or(-200));
                    device.disconnect().await.unwrap();
                    return Ok(Outcome::Processed);
                },
            )
            .await
            .unwrap();
        }
        Commands::Emulate(emulate_command) => {
            let emulator = Emulator::new(emulate_command).await.unwrap();
            emulator.emulate().await.unwrap();
        }
    };

    // sleep(Duration::from_secs(1)).await;
    Ok(())
}
