//! Connects to our Bluetooth GATT service and exercises the characteristic.

use async_recursion::async_recursion;
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest, Service},
    Device, UuidExt,
};
use sha3::{Digest, Sha3_256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

const FILE_UPLOAD_SERVICE: u16 = 0x7892;
const FILE_UPLOAD_SERVICE_DATA: u16 = 0x7893;
const FILE_UPLOAD_SERVICE_HASH: u16 = 0x7894;
const FILE_UPLOAD_SERVICE_CHECKSUMS: u16 = 0x7895;
const FILE_UPLOAD_SERVICE_LENGTH: u16 = 0x7896;
const FILE_UPLOAD_SERVICE_CHUNK_LENGTH: u16 = 0x7897;

#[derive(Error, Debug)]
pub enum UpdateTargetError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("Not an update target")]
    MacDoesNotLookLikeAnUpdateTarget,
    #[error("Failed to connect to device")]
    FailedToConnect(bluer::Error),
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

pub struct UpdateTarget {
    data_characteristic: Characteristic,
    hash_characteristic: Characteristic,
    checksums_characteristic: Characteristic,
    length_characteristic: Characteristic,
    chunk_length_characteristic: Characteristic,
}

impl UpdateTarget {
    pub async fn new_from_peripheral(device: Device) -> Result<UpdateTarget, UpdateTargetError> {
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

        let data_characteristic =
            find_characteristic(&update_service, FILE_UPLOAD_SERVICE_DATA).await?;
        let hash_characteristic =
            find_characteristic(&update_service, FILE_UPLOAD_SERVICE_HASH).await?;
        let checksums_characteristic =
            find_characteristic(&update_service, FILE_UPLOAD_SERVICE_CHECKSUMS).await?;
        let length_characteristic =
            find_characteristic(&update_service, FILE_UPLOAD_SERVICE_LENGTH).await?;
        let chunk_length_characteristic =
            find_characteristic(&update_service, FILE_UPLOAD_SERVICE_CHUNK_LENGTH).await?;

        return Ok(UpdateTarget {
            data_characteristic,
            hash_characteristic,
            checksums_characteristic,
            length_characteristic,
            chunk_length_characteristic,
        });
    }

    #[async_recursion]
    pub async fn write_file(&self, data: &[u8]) -> Result<[u8; 32], UpdateTargetError> {
        let mut hasher = Sha3_256::new();
        hasher.update(&data);
        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_slice());

        // -2 for the length
        // -28 was found to be good by empirical methods
        let chunk_size: u16 = (self.data_characteristic.mtu().await? as u16) - 28 - 2;
        // println!("{chunk_size}");

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
            self.checksums_characteristic.write(checksums_data).await?;
        } else {
            let checksums_file_hash = self.write_file(checksums_data).await?;
            self.checksums_characteristic
                .write(&checksums_file_hash)
                .await?;
        }

        self.length_characteristic
            .write(&(data.len() as u32).to_le_bytes())
            .await?;
        self.chunk_length_characteristic
            .write(&(chunk_size as u16).to_le_bytes())
            .await?;
        self.hash_characteristic.write(&hash).await?;

        let mut write_io = self.data_characteristic.write_io().await?;
        for chunk in chunks {
            write_io.send(chunk.as_slice()).await?;
        }
        write_io.flush().await?;
        write_io.shutdown().await?;

        // Force flushing by doing a reliable write
        self.length_characteristic
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

        return Ok(hash);
    }
}
