//! Connects to our Bluetooth GATT service and exercises the characteristic.

use bluer::{
    gatt::remote::Characteristic, gatt::remote::Service, gatt::CharacteristicWriter, AdapterEvent,
    Device, UuidExt,
};
use futures::{future, pin_mut, StreamExt};
use std::{
    ops::Rem,
    os::fd::FromRawFd,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{
    io::{unix::AsyncFd, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::sleep,
};
// /// Service UUID for GATT example.
// const SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xFEEDC0DE);

// /// Characteristic UUID for GATT example.
// const CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xF00DC0DE00001);

// /// Manufacturer id for LE advertisement.
// #[allow(dead_code)]
// const MANUFACTURER_ID: u16 = 0xf00d;

const LIGHT_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xFFE9);

const UPDATE_SERVICE_UUID: u16 = 0x729e;
// const UPDATE_SERVICE_UUID: bluer::Uuid = ;
const UPDATE_SERVICE_RECEIVE_DATA_UUID: u16 = 13443;

async fn find_our_characteristic(device: Device) -> Result<UpdateTarget, UpdateTargetError> {
    // let addr: bluer::Address = device.address();
    // let uuids = device.uuids().await?.unwrap_or_default();
    // println!("Discovered device {} with service UUIDs {:?}", addr, &uuids);
    // let md = device.manufacturer_data().await?;
    // println!("    Manufacturer data: {:x?}", &md);

    // if uuids.contains(&SERVICE_UUID) {
    //     println!("    Device provides our service!");

    //     sleep(Duration::from_secs(2)).await;
    //     if !device.is_connected().await? {
    //         println!("    Connecting...");
    //         let mut retries = 2;
    //         loop {
    //             match device.connect().await {
    //                 Ok(()) => break,
    //                 Err(err) if retries > 0 => {
    //                     println!("    Connect error: {}", &err);
    //                     retries -= 1;
    //                 }
    //                 Err(err) => return Err(err),
    //             }
    //         }
    //         println!("    Connected");
    //     } else {
    //         println!("    Already connected");
    //     }

    //     println!("    Enumerating services...");
    //     for service in device.services().await? {
    //         let uuid = service.uuid().await?;
    //         println!("    Service UUID: {}", &uuid);
    //         println!("    Service data: {:?}", service.all_properties().await?);
    //         if uuid == SERVICE_UUID {
    //             println!("    Found our service!");
    //             for char in service.characteristics().await? {
    //                 let uuid = char.uuid().await?;
    //                 println!("    Characteristic UUID: {}", &uuid);
    //                 println!(
    //                     "    Characteristic data: {:?}",
    //                     char.all_properties().await?
    //                 );
    //                 if uuid == CHARACTERISTIC_UUID {
    //                     println!("    Found our characteristic!");
    //                     return Ok(Some(char));
    //                 }
    //             }
    //         }
    //     }

    //     println!("    Not found!");
    // }

    // Ok(None)
    let update_target = UpdateTarget::new_from_peripheral(device).await?;
    return Ok(update_target);
}

async fn exercise_characteristic(char: &Characteristic) -> bluer::Result<()> {
    println!("    Characteristic flags: {:?}", char.flags().await?);
    sleep(Duration::from_secs(1)).await;

    if char.flags().await?.read {
        println!("    Reading characteristic value");
        let value = char.read().await?;
        println!("    Read value: {:x?}", &value);
        sleep(Duration::from_secs(1)).await;
    }

    let data = vec![0xee, 0x11, 0x11, 0x0];
    println!(
        "    Writing characteristic value {:x?} using function call",
        &data
    );
    char.write(&data).await?;
    sleep(Duration::from_secs(1)).await;

    if char.flags().await?.read {
        let value = char.read().await?;
        println!("    Read value back: {:x?}", &value);
        sleep(Duration::from_secs(1)).await;
    }

    println!("    Obtaining write IO");
    let mut write_io = char.write_io().await?;
    println!("    Write IO obtained");
    println!(
        "    Writing characteristic value {:x?} five times using IO",
        &data
    );
    for _ in 0..5u8 {
        let written = write_io.write(&data).await?;
        println!("    {written} bytes written");
    }
    println!("    Closing write IO");
    drop(write_io);
    sleep(Duration::from_secs(1)).await;

    println!("    Starting notification session");
    {
        let notify = char.notify().await?;
        pin_mut!(notify);
        for _ in 0..5u8 {
            match notify.next().await {
                Some(value) => {
                    println!("    Notification value: {:x?}", &value);
                }
                None => {
                    println!("    Notification session was terminated");
                }
            }
        }
        println!("    Stopping notification session");
    }
    sleep(Duration::from_secs(1)).await;

    println!("    Obtaining notification IO");
    let mut notify_io = char.notify_io().await?;
    println!("    Obtained notification IO with MTU={}", notify_io.mtu());
    for _ in 0..5u8 {
        let mut buf = vec![0; notify_io.mtu()];
        match notify_io.read(&mut buf).await {
            Ok(0) => {
                println!("    Notification IO end of stream");
                break;
            }
            Ok(read) => {
                println!("    Notified with {} bytes: {:x?}", read, &buf[0..read]);
            }
            Err(err) => {
                println!("    Notification IO failed: {}", &err);
                break;
            }
        }
    }
    println!("    Stopping notification IO");
    drop(notify_io);
    sleep(Duration::from_secs(1)).await;

    Ok(())
}

#[derive(Error, Debug)]
pub enum UpdateTargetError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("Not an update target")]
    NotAnUpdateTarget,
    #[error("Not an update target")]
    MacDoesNotLookLikeAnUpdateTarget,
    #[error("Failed to connect to device")]
    FailedToConnect(bluer::Error),
    // TODO: Write better message
    #[error("Something weird happened")]
    WeirdError,
    #[error(transparent)]
    DoesNotProvideUpdateService(#[from] FindUpdateServiceError),
    #[error(transparent)]
    ServiceIsMissingACharacteristic(#[from] FindCharacteristicError),
}

#[derive(Error, Debug)]
pub enum FindUpdateServiceError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("Does not contain an update service")]
    NoUpdateService,
}

pub async fn find_update_service(device: &Device) -> Result<Service, FindUpdateServiceError> {
    for service in device.services().await? {
        if service.uuid().await? == uuid::Uuid::from_u16(UPDATE_SERVICE_UUID) {
            return Ok(service);
        }
    }

    return Err(FindUpdateServiceError::NoUpdateService);
}

#[derive(Error, Debug)]
pub enum FindCharacteristicError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("Does not contain the specified characteristic")]
    NotFound,
}

pub async fn find_characteristic(
    service: &Service,
) -> Result<Characteristic, FindCharacteristicError> {
    for characteristic in service.characteristics().await? {
        if characteristic.uuid().await? == uuid::Uuid::from_u16(UPDATE_SERVICE_RECEIVE_DATA_UUID) {
            return Ok(characteristic);
        }
    }

    return Err(FindCharacteristicError::NotFound);
}

struct UpdateTarget {
    pub device: Device,
    pub update_service: Service,
}

impl UpdateTarget {
    async fn new_from_peripheral(device: Device) -> Result<UpdateTarget, UpdateTargetError> {
        let address = device.address();
        println!("Checking {}", address);
        if !(address.0.starts_with(&[0x24, 0xec, 0x4b])) {
            return Err(UpdateTargetError::MacDoesNotLookLikeAnUpdateTarget);
        }
        println!("Checked MAC for {}", address);

        if !device.is_connected().await? {
            println!("Connecting...");
            for attempt in 0..=2 {
                match device.connect().await {
                    Ok(()) => break,
                    Err(err) if attempt == 2 => {
                        if !(device.is_connected().await.unwrap_or(false)) {
                            return Err(UpdateTargetError::FailedToConnect(err));
                        }
                        break;
                    }
                    Err(err) => {
                        println!("Connect error: {}", &err);
                    }
                }
            }
        } else {
            println!("Already connected");
        }
        println!("Connected to {}", address);

        // // // Sometimes this is required to actually discover services
        let update_service = find_update_service(&device).await?;
        println!("Found service UUID for {}", address);

        return Ok(UpdateTarget {
            device,
            update_service,
        });
    }

    async fn program(&self, code: &[u8]) -> Result<(), UpdateTargetError> {
        let characteristic = find_characteristic(&self.update_service).await?;

        let mut write_io = characteristic.write_io().await?;
        let mut ten_kilo = [0u8; 1024 * 10];
        for num in 0..(1024 * 10) {
            ten_kilo[num] = num.to_le_bytes()[0];
        }

        let now = Instant::now();
        for chunk in ten_kilo.chunks(write_io.mtu()) {
            write_io.send(chunk).await.unwrap();
        }
        write_io.flush().await.unwrap();
        write_io.shutdown().await.unwrap();
        write_io.closed().await.unwrap();
        // let mut asyncfd = AsyncFd::new(write_io);
        // unsafe {
        //     let mut file = tokio::fs::File::from_raw_fd(write_io.into_raw_fd().unwrap());
        //     // file.sync_all().await.unwrap();
        //     // file.sync_data().await.unwrap();
        //     file.flush().await.unwrap();
        let duration = now.elapsed();
        println!("Sending 10k took {} millis", duration.as_millis());
        // }

        // let Some(characteristic) = self
        //     .service
        //     .characteristics
        //     .iter()
        //     .find(|c| c.uuid == UPDATE_SERVICE_RECEIVE_DATA_UUID)
        // else {
        //     return Err(UpdateTargetError::NotAnUpdateTarget);
        // };

        // // self.peripheral.

        // for i in 0..5 {
        //     // self.peripheral
        //     //     .write(characteristic, &code, WriteType::WithResponse)
        //     //     .await?;
        //     self.peripheral
        //         .write(characteristic, &code, WriteType::WithoutResponse)
        //         .await?;
        // }

        return Ok(());
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    {
        println!(
            "Discovering on Bluetooth adapter {} with address {}\n",
            adapter.name(),
            adapter.address().await?
        );
        let discover = adapter.discover_devices().await?;
        pin_mut!(discover);
        let mut done = false;
        while let Some(evt) = discover.next().await {
            match evt {
                AdapterEvent::DeviceAdded(addr) => {
                    println!("Found {}", addr);

                    let device = adapter.device(addr)?;
                    let Ok(update_target) = UpdateTarget::new_from_peripheral(device).await else {
                        continue;
                    };
                    update_target.program(&[3]).await.unwrap();

                    // match find_our_characteristic(&device).await {
                    //     Ok(Some(char)) => match exercise_characteristic(&char).await {
                    //         Ok(()) => {
                    //             println!("    Characteristic exercise completed");
                    //             done = true;
                    //         }
                    //         Err(err) => {
                    //             println!("    Characteristic exercise failed: {}", &err);
                    //         }
                    //     },
                    //     Ok(None) => (),
                    //     Err(err) => {
                    //         println!("    Device failed: {}", &err);
                    //         let _ = adapter.remove_device(device.address()).await;
                    //     }
                    // }
                    // match device.disconnect().await {
                    //     Ok(()) => println!("    Device disconnected"),
                    //     Err(err) => println!("    Device disconnection failed: {}", &err),
                    // }
                }
                AdapterEvent::DeviceRemoved(addr) => {
                    println!("Device removed {addr}");
                }
                _ => (),
            }
            if done {
                break;
            }
        }
        println!("Stopping discovery");
    }

    sleep(Duration::from_secs(1)).await;
    Ok(())
}
