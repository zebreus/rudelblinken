#![feature(async_closure)]
//! Connects to our Bluetooth GATT service and exercises the characteristic.

mod update_target;

use bluer::{AdapterEvent, Device};
use clap::{Parser, Subcommand};
use futures::{
    pin_mut,
    stream::{AbortHandle, Abortable},
    StreamExt as STTT,
};
use futures_time::stream::StreamExt;
use futures_time::time::Duration;
use std::{future::Future, path::PathBuf, time::Instant};
use update_target::{UpdateTarget, UpdateTargetError};

async fn scan_for<Fut, Err>(
    duration: Duration,
    max_devices: u32,
    f: &dyn Fn(Device) -> Fut,
) -> bluer::Result<()>
where
    Fut: Future<Output = Result<(), Err>>,
{
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    {
        // eprintln!(
        //     "Discovering on Bluetooth adapter {} with address {}\n",
        //     adapter.name(),
        //     adapter.address().await?
        // );
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let discover = adapter.discover_devices().await?;
        pin_mut!(discover);
        let stream = Abortable::new(discover, abort_registration);
        let mut stream = stream.timeout(duration);
        let mut programmed_devices = 0;
        while let Some(evt) = stream.next().await {
            let Ok(evt) = evt else {
                break;
            };
            match evt {
                AdapterEvent::DeviceAdded(addr) => {
                    let device = adapter.device(addr)?;
                    let result = f(device).await;
                    if result.is_ok() {
                        programmed_devices += 1;
                    }
                    if programmed_devices >= max_devices {
                        abort_handle.abort();
                    }
                }
                // AdapterEvent::DeviceRemoved(addr) => {
                //     // println!("Device removed {addr}");
                // }
                _ => (),
            }
        }
    }

    return Ok(());
}

/// Tool to control rudelblinken devices
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
        #[arg(short, long, default_value = "5")]
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
        #[arg(short, long, default_value = "5")]
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
        #[arg(short, long, default_value = "5")]
        timeout: f32,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();
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
                &async |device| -> Result<(), UpdateTargetError> {
                    let update_target = UpdateTarget::new_from_peripheral(&device).await?;

                    let data = &file_content;

                    let now = Instant::now();
                    update_target.upload_file(&data).await?;
                    let duration = now.elapsed();
                    println!(
                        "Sending {}k took {} millis",
                        data.len() / 1024,
                        duration.as_millis()
                    );
                    println!(
                        "Thats {}kb/s",
                        (data.len() as f64 / duration.as_millis() as f64)
                    );
                    return Ok(());
                    // update_target.device.disconnect().await.unwrap();
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
                &async |device| -> Result<(), UpdateTargetError> {
                    let update_target = UpdateTarget::new_from_peripheral(&device).await?;

                    let data = &file_content;

                    let now = Instant::now();
                    update_target.run_program(&data).await?;
                    return Ok(());
                    // update_target.device.disconnect().await.unwrap();
                },
            )
            .await
            .unwrap();
        }
        Commands::Scan { timeout } => {
            eprintln!("name, mac, rssi");
            scan_for(
                Duration::from_millis((timeout * 1000.0) as u64),
                999,
                &async |device| -> Result<(), UpdateTargetError> {
                    let address = device.address();
                    let update_target = UpdateTarget::new_from_peripheral(&device).await?;
                    let rssi = device.rssi().await?;

                    let name = update_target.get_name().await?;
                    println!("{}, {}, {}", name, address, rssi.unwrap_or(-200));
                    return Ok(());
                    // update_target.device.disconnect().await.unwrap();
                },
            )
            .await
            .unwrap();
        }
    };

    // sleep(Duration::from_secs(1)).await;
    Ok(())
}
