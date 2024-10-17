#![feature(async_closure)]
//! Connects to our Bluetooth GATT service and exercises the characteristic.

mod update_target;

use bluer::{AdapterEvent, Device};
use futures::{
    pin_mut,
    stream::{AbortHandle, Abortable},
    StreamExt as STTT,
};
use futures_time::stream::StreamExt;
use futures_time::time::Duration;
use std::{future::Future, time::Instant};
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
        println!(
            "Discovering on Bluetooth adapter {} with address {}\n",
            adapter.name(),
            adapter.address().await?
        );
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
                        println!("is ok");
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
        println!("Stopping discovery");
    }

    return Ok(());
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();
    scan_for(
        Duration::from_millis(2000000),
        1,
        &async |device| -> Result<(), UpdateTargetError> {
            let update_target = UpdateTarget::new_from_peripheral(device).await?;

            const FILESIZE: usize = 1024 * 50;
            let mut ten_kilo = [0u8; FILESIZE];
            for num in 0..(FILESIZE) {
                ten_kilo[num] = num.to_le_bytes()[0];
            }

            let now = Instant::now();
            update_target.write_file(&ten_kilo).await?;
            let duration = now.elapsed();
            println!(
                "Sending {}k took {} millis",
                FILESIZE / 1024,
                duration.as_millis()
            );
            println!(
                "Thats {}kb/s",
                (FILESIZE as f64 / duration.as_millis() as f64)
            );
            return Ok(());
            // update_target.device.disconnect().await.unwrap();
        },
    )
    .await
    .unwrap();

    // sleep(Duration::from_secs(1)).await;
    Ok(())
}
