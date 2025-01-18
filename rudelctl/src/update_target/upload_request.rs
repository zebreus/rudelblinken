use async_recursion::async_recursion;
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest, Service},
    Device, UuidExt,
};
use std::time::Duration;
use thiserror::Error;
use tokio::{io::AsyncWriteExt, time::sleep};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

use super::UpdateTargetError;

#[derive(Error, Debug)]
pub enum CreateUploadRequestError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("Not an update target")]
    MacDoesNotLookLikeAnUpdateTarget,
    #[error("Failed to connect to device")]
    FailedToConnect(bluer::Error),
    #[error("Failed to upload file. Maybe a timeout or connection loss: {0}")]
    UploadError(bluer::Error),
    #[error("The update target seemingly ignored our upload request")]
    UploadRequestIgnored,
    #[error("We lost connection to the target device and failed to reconnect")]
    ReconnectFailed,
}

// TODO: Implement better debug printing
#[derive(Debug, Clone, TryFromBytes, IntoBytes, Immutable, KnownLayout, PartialEq, PartialOrd)]
#[repr(C)]
pub struct UploadRequest {
    /// Size of the file in bytes
    pub file_size: u32,
    /// Blake3 hash of the file
    pub hash: [u8; 32],
    /// CRC checksums of the chunks.
    /// If the number of chunks is <= 32 then this is interpreted as an array of 1-byte CRC checksums
    /// If the number of chunks is > 32 then this interpreted as the hash of a previously uploaded file containing an array of 1-byte CRC checksums
    pub checksums: [u8; 32],
    /// File name
    pub file_name: [u8; 16],
    /// Size of a single chunk
    pub chunk_size: u16,
    /// Unused padding. Reserved for future use
    pub _padding: u16,
}

impl UploadRequest {
    pub fn create(
        file_size: u32,
        hash: [u8; 32],
        checksums: [u8; 32],
        file_name: [u8; 16],
        chunk_size: u16,
    ) -> Self {
        Self {
            file_size,
            hash,
            checksums,
            file_name,
            chunk_size,
            _padding: 0,
        }
    }
    // Get the total number of chunks
    pub fn chunk_count(&self) -> u32 {
        self.file_size.div_ceil(self.chunk_size as u32)
    }

    pub async fn new(
        file_name: &str,
        data: &[u8],
        chunk_size: u16,
        upload_checksums: impl async Fn(&[u8]) -> Result<[u8; 32], UpdateTargetError>,
    ) -> Result<Self, UpdateTargetError> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&data);
        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_bytes());

        // -2 for the length
        // -28 was found to be good by empirical methods

        let crc8_generator = crc::Crc::<u8>::new(&crc::CRC_8_LTE);
        let checksums: Vec<u8> = data
            .chunks(chunk_size as usize)
            .map(|chunk| crc8_generator.checksum(chunk))
            .collect();

        let checksums: [u8; 32] = if checksums.len() > 32 {
            let checksums_file_hash = upload_checksums(&checksums).await?;
            checksums_file_hash
        } else {
            let mut checksums_array = [0u8; 32];
            checksums_array[0..checksums.len()].copy_from_slice(&checksums);
            checksums_array
        };

        let mut file_name_array = [0u8; 16];
        let boundary = file_name.floor_char_boundary(16);
        let file_name = &file_name[0..boundary];
        // TODO: Fix the name story on both sides.
        // TODO: Fix boundary logic
        file_name_array[0..boundary].copy_from_slice(&file_name.as_bytes()[0..boundary]);

        Ok(UploadRequest::create(
            data.len() as u32,
            hash,
            checksums,
            file_name_array,
            chunk_size,
        ))
    }
}
