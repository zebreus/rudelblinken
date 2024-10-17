//! Connects to our Bluetooth GATT service and exercises the characteristic.

use async_recursion::async_recursion;
use bluer::{
    gatt::{
        remote::{Characteristic, CharacteristicWriteRequest, Service},
        CharacteristicWriter,
    },
    AdapterEvent, Device, UuidExt,
};
use core::hash;
use futures::{future, pin_mut, StreamExt};
use sha3::{Digest, Sha3_256};
use std::{
    mem,
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

const FILE_UPLOAD_SERVICE: u16 = 0x7892;
const FILE_UPLOAD_SERVICE_DATA: u16 = 0x7893;
const FILE_UPLOAD_SERVICE_HASH: u16 = 0x7894;
const FILE_UPLOAD_SERVICE_CHECKSUMS: u16 = 0x7895;
const FILE_UPLOAD_SERVICE_LENGTH: u16 = 0x7896;
const FILE_UPLOAD_SERVICE_CHUNK_LENGTH: u16 = 0x7897;

// const UPDATE_SERVICE_UUID: u16 = 0x729e;
// // const UPDATE_SERVICE_UUID: bluer::Uuid = ;
// const UPDATE_SERVICE_RECEIVE_DATA_UUID: u16 = 13443;

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
        if service.uuid().await? == uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE) {
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
    uuid: u16,
) -> Result<Characteristic, FindCharacteristicError> {
    for characteristic in service.characteristics().await? {
        if characteristic.uuid().await? == uuid::Uuid::from_u16(uuid) {
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
        // println!("Checking {}", address);
        if !(address.0.starts_with(&[0x24, 0xec, 0x4b])) {
            return Err(UpdateTargetError::MacDoesNotLookLikeAnUpdateTarget);
        }
        println!("Found MAC {}", address);

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

    #[async_recursion]
    async fn write_file(&self, data: &[u8]) -> Result<[u8; 32], UpdateTargetError> {
        // const FILE_UPLOAD_SERVICE_DATA: u16 = 0x7893;
        // const FILE_UPLOAD_SERVICE_HASH: u16 = 0x7894;
        // const FILE_UPLOAD_SERVICE_CHECKSUMS: u16 = 0x7895;
        // const FILE_UPLOAD_SERVICE_LENGTH: u16 = 0x7896;
        // const FILE_UPLOAD_SERVICE_CHUNK_LENGTH: u16 = 0x7897;

        let data_characteristic =
            find_characteristic(&self.update_service, FILE_UPLOAD_SERVICE_DATA).await?;
        let hash_characteristic =
            find_characteristic(&self.update_service, FILE_UPLOAD_SERVICE_HASH).await?;
        let checksums_characteristic =
            find_characteristic(&self.update_service, FILE_UPLOAD_SERVICE_CHECKSUMS).await?;
        let length_characteristic =
            find_characteristic(&self.update_service, FILE_UPLOAD_SERVICE_LENGTH).await?;
        let chunk_length_characteristic =
            find_characteristic(&self.update_service, FILE_UPLOAD_SERVICE_CHUNK_LENGTH).await?;

        let mut hasher = Sha3_256::new();
        hasher.update(&data);
        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_slice());

        // -2 for the length
        // -28 was found to be good by empirical methods
        let chunk_size: u16 = (data_characteristic.mtu().await? as u16) - 28 - 2;
        // println!("{chunk_size}");
        // chunk_size = 493;

        let crc8_generator = crc::Crc::<u8>::new(&crc::CRC_8_LTE);
        let checksums: Vec<u8> = data
            .chunks(chunk_size as usize)
            .map(|chunk| crc8_generator.checksum(chunk))
            .collect();

        let chunks: Vec<Vec<u8>> = data
            .chunks(chunk_size as usize)
            .enumerate()
            .map(|(index, data)| {
                let mut new_chunk = vec![0; data.len() + 2];
                new_chunk[0..2].copy_from_slice(&(index as u16).to_le_bytes());
                new_chunk[2..(2 + data.len())].copy_from_slice(data);
                return new_chunk;
            })
            .collect();

        let checksums_data = checksums.as_slice();
        if checksums_data.len() < 32 {
            checksums_characteristic.write(checksums_data).await?;
        } else {
            let checksums_file_hash = self.write_file(checksums_data).await?;
            checksums_characteristic.write(&checksums_file_hash).await?;
        }

        length_characteristic
            .write(&(data.len() as u32).to_le_bytes())
            .await?;
        chunk_length_characteristic
            .write(&(chunk_size as u16).to_le_bytes())
            .await?;
        hash_characteristic.write(&hash).await?;

        let mut write_io = data_characteristic.write_io().await?;

        for chunk in chunks {
            write_io.send(chunk.as_slice()).await.unwrap();
            // data_characteristic.write(chunk.as_slice()).await?;
            // data_characteristic
            //     .write_ext(
            //         chunk.as_slice(),
            //         &CharacteristicWriteRequest {
            //             offset: 0,
            //             op_type: bluer::gatt::WriteOp::Reliable,
            //             prepare_authorize: false,
            //             _non_exhaustive: (),
            //         },
            //     )
            //     .await?;
        }
        write_io.flush().await.unwrap();
        write_io.shutdown().await.unwrap();
        // write_io.closed().await.unwrap();
        length_characteristic
            .write_ext(
                &[0],
                &CharacteristicWriteRequest {
                    offset: 0,
                    op_type: bluer::gatt::WriteOp::Reliable,
                    prepare_authorize: false,
                    _non_exhaustive: (),
                },
            )
            .await?;
        // let mut asyncfd = AsyncFd::new(write_io);
        // unsafe {
        //     let mut file = tokio::fs::File::from_raw_fd(write_io.into_raw_fd().unwrap());
        //     // file.sync_all().await.unwrap();
        //     // file.sync_data().await.unwrap();
        //     file.flush().await.unwrap();

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

        return Ok(hash);
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
                    // println!("Found {}", addr);

                    let device = adapter.device(addr)?;
                    let Ok(update_target) = UpdateTarget::new_from_peripheral(device).await else {
                        continue;
                    };

                    const filesize: usize = 1024 * 50;
                    let mut ten_kilo = [0u8; filesize];
                    for num in 0..(filesize) {
                        ten_kilo[num] = num.to_le_bytes()[0];
                    }

                    let now = Instant::now();
                    update_target.write_file(&ten_kilo).await.unwrap();
                    let duration = now.elapsed();
                    println!(
                        "Sending {}k took {} millis",
                        filesize / 1024,
                        duration.as_millis()
                    );
                    println!(
                        "Thats {}kb/s",
                        (filesize as f64 / duration.as_millis() as f64)
                    );
                    update_target.device.disconnect().await.unwrap();

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
                    // println!("Device removed {addr}");
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
