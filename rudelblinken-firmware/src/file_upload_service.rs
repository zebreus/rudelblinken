use std::sync::Arc;

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEServer, NimbleProperties,
};
use esp_idf_sys as _;
use sha3::{Digest, Sha3_256};
use thiserror::Error;

const FILE_UPLOAD_SERVICE: u16 = 0x7892;
const FILE_UPLOAD_SERVICE_DATA: u16 = 0x7893;
const FILE_UPLOAD_SERVICE_HASH: u16 = 0x7894;
const FILE_UPLOAD_SERVICE_CHECKSUMS: u16 = 0x7895;
const FILE_UPLOAD_SERVICE_LENGTH: u16 = 0x7896;
const FILE_UPLOAD_SERVICE_CHUNK_LENGTH: u16 = 0x7897;

const FILE_UPLOAD_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE);
const FILE_UPLOAD_SERVICE_DATA_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_DATA);
const FILE_UPLOAD_SERVICE_HASH_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_HASH);
const FILE_UPLOAD_SERVICE_CHECKSUMS_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_CHECKSUMS);
const FILE_UPLOAD_SERVICE_LENGTH_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_LENGTH);
const FILE_UPLOAD_SERVICE_CHUNK_LENGTH_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_CHUNK_LENGTH);

#[derive(Clone, Debug)]
pub struct File {
    hash: [u8; 32],
    pub content: Vec<u8>,
}

#[derive(Clone, Debug)]
struct IncompleteFile {
    incomplete_file: File,
    checksums: Vec<u8>,
    received_chunks: Vec<bool>,
    chunk_length: u16,
    length: u32,
}

#[derive(Error, Debug, Clone)]
pub enum ReceiveChunkError {
    #[error("Chunk has an invalid length")]
    InvalidLength,
    #[error("Chunk has the wrong checksum")]
    WrongChecksum,
}

#[derive(Error, Debug, Clone)]
pub enum VerifyFileError {
    #[error("File is not complete")]
    NotComplete,
    #[error("Hashes do not match")]
    HashMismatch,
}

impl IncompleteFile {
    pub fn new(hash: [u8; 32], checksums: Vec<u8>, chunk_size: u16, length: u32) -> Self {
        return IncompleteFile {
            incomplete_file: File {
                hash,
                content: vec![0; length as usize],
            },
            received_chunks: vec![false; checksums.len()],
            checksums: checksums,
            chunk_length: chunk_size,
            length,
        };
    }
    pub fn receive_chunk(&mut self, data: &[u8], index: u16) -> Result<(), ReceiveChunkError> {
        // Verify length for all but the last chunk
        if (index as usize != self.checksums.len() - 1)
            && (data.len() != self.chunk_length as usize)
        {
            return Err(ReceiveChunkError::InvalidLength);
        }
        // Verify length for the last chunk
        if (index as usize == self.checksums.len() - 1)
            && (data.len() != (self.length as usize % self.chunk_length as usize))
        {
            return Err(ReceiveChunkError::InvalidLength);
        }

        // TODO: Find out if generating a new crc8 generator costs anything
        let crc8_generator = crc::Crc::<u8>::new(&crc::CRC_8_LTE);
        let checksum = crc8_generator.checksum(data);

        if self.checksums[index as usize] != checksum {
            ::log::error!(target: "file-upload", "Received chunk with invalid checksum");
            return Err(ReceiveChunkError::WrongChecksum);
        }

        let offset = (self.chunk_length * index) as usize;
        self.incomplete_file.content[offset..(data.len() + offset)].copy_from_slice(data);
        self.received_chunks[index as usize] = true;

        return Ok(());
    }
    // /// Get the ID of the next missing chunk. Returns [None], if all chunks were already received.
    // pub fn get_next_missing_chunk(&self) -> Option<usize> {
    //     self.received_chunks
    //         .iter()
    //         .enumerate()
    //         .find(|(_, received)| received == &&false)
    //         .map(|(index, _)| index)
    // }
    /// Check if the file is complete
    pub fn is_complete(&self) -> bool {
        self.received_chunks.iter().all(|received| *received)
    }
    /// Verify that the received file is complete and has the correct hash
    pub fn verify_hash(&self) -> Result<(), VerifyFileError> {
        if !self.is_complete() {
            return Err(VerifyFileError::NotComplete);
        }
        let mut hasher = Sha3_256::new();
        hasher.update(&self.incomplete_file.content);

        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_slice());

        if hash != self.incomplete_file.hash {
            ::log::warn!(target: "file-upload", "Hashes dont match.\nExpected: {:?}\nGot     : {:?}", self.incomplete_file.hash, hash);
            return Err(VerifyFileError::HashMismatch);
        }
        ::log::info!(target: "file-upload", "Hashes match");

        return Ok(());
    }
    /// Get the uploaded file, if the upload is finished, otherwise this return None and you just destroyed your incomplete file for no reason
    pub fn into_file(self) -> Result<File, VerifyFileError> {
        self.verify_hash()?;
        return Ok(self.incomplete_file);
    }
}

pub struct FileUploadService {
    files: Vec<File>,
    currently_receiving: Option<IncompleteFile>,

    latest_hash: Option<[u8; 32]>,
    latest_checksums: Option<Vec<u8>>,
    latest_length: Option<u32>,
    latest_chunk_length: Option<u16>,

    last_error: Option<FileUploadError>,
}

#[derive(Error, Debug, Clone)]
pub enum FileUploadError {
    #[error(transparent)]
    StartUploadError(#[from] StartUploadError),
    #[error(transparent)]
    ReceiveChunkError(#[from] ReceiveChunkError),
    #[error(transparent)]
    VerifyFileError(#[from] VerifyFileError),
    #[error("Cannot receive chunk when no upload is active")]
    NoUploadActive,
    #[error("Received chunk is way too short")]
    ReceivedChunkWayTooShort,
    #[error("There is no checksum file with the supplied hash")]
    ChecksumFileDoesNotExist,
}

#[derive(Error, Debug, Clone)]
pub enum StartUploadError {
    #[error("Content length needs to be set before starting an upload")]
    LengthMissing,
    #[error("Chunk length needs to be set before starting an upload")]
    ChunkLengthMissing,
    #[error("Hash needs to be set before starting an upload")]
    HashMissing,
    #[error("Checksums need to be specified before starting an upload")]
    ChecksumsMissing,
    #[error("Content length seems incorrect, as it does not match chunk length multiplied by chunk size")]
    LengthIncorrect,
}

impl FileUploadService {
    /// Start an upload with the last received settings. Cancels a currently ongoing upload
    fn start_upload(&mut self) -> Result<(), StartUploadError> {
        let Some(length) = self.latest_length else {
            return Err(StartUploadError::LengthMissing);
        };
        let Some(chunk_length) = self.latest_chunk_length else {
            return Err(StartUploadError::ChunkLengthMissing);
        };
        let Some(hash) = &self.latest_hash else {
            return Err(StartUploadError::HashMissing);
        };
        let Some(checksums) = &self.latest_checksums else {
            return Err(StartUploadError::ChecksumsMissing);
        };
        let min_length =
            ((chunk_length as usize) * checksums.len() - (chunk_length as usize - 1)) as u32;
        let max_length = (chunk_length as usize * checksums.len()) as u32;
        if (length < min_length) || (length > max_length) {
            return Err(StartUploadError::LengthIncorrect);
        }

        self.currently_receiving = Some(IncompleteFile::new(
            hash.clone(),
            checksums.clone(),
            chunk_length,
            length,
        ));

        return Ok(());
    }

    /// Starts an upload if there is no active upload
    ///
    /// If this returns Ok, self.currently_receiving is always set to Some
    fn ensure_upload(&mut self) -> Result<(), StartUploadError> {
        if self.currently_receiving.is_some() {
            return Ok(());
        }
        self.start_upload()?;
        return Ok(());
    }

    fn log_error(&mut self, error: FileUploadError) {
        ::log::error!(target: "file-upload", "{}", error);
        self.last_error = Some(error);
    }

    /// Get the UUID of the file upload service
    pub const fn uuid() -> BleUuid {
        return FILE_UPLOAD_SERVICE_UUID;
    }

    /// This will be called on writes to the data characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn data_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        if received_data.len() < 3 {
            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let index = u16::from_le_bytes([received_data[0], received_data[1]]);
        let data = &received_data[2..];

        ::log::info!(target: "file-upload", "Received data chunk {}", index);
        self.ensure_upload()?;

        let Some(current_upload) = &mut self.currently_receiving else {
            // Should never happen, because we called ensure_upload above
            return Err(FileUploadError::NoUploadActive);
        };
        current_upload.receive_chunk(data, index)?;
        if current_upload.is_complete() {
            let file = self
                .currently_receiving
                .take()
                .ok_or(FileUploadError::NoUploadActive)?
                .into_file()?;
            self.files.push(file);
        }
        return Ok(());
    }

    /// This will be called on writes to the hash characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn hash_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        if received_data.len() != 32 {
            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let new_hash: [u8; 32] = received_data.try_into().unwrap();
        ::log::info!(target: "file-upload", "Received hash {:?}", new_hash);
        if self.latest_hash.as_ref() == Some(&new_hash) {
            return Ok(());
        }
        self.latest_hash = Some(new_hash);
        self.currently_receiving = None;
        return Ok(());
    }

    pub fn get_file(&self, hash: &[u8; 32]) -> Option<&File> {
        self.files.iter().find(|file| &file.hash == hash)
    }

    /// This will be called on writes to the checksum characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn checksums_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        ::log::info!(target: "file-upload", "Received checksums with length {}", received_data.len());

        if received_data.len() < 32 {
            let new_checksums = received_data.to_vec();
            if self.latest_checksums.as_ref() == Some(&new_checksums) {
                return Ok(());
            }
            ::log::info!(target: "file-upload", "Directly set checksums");
            self.latest_checksums = Some(new_checksums);
            self.currently_receiving = None;
            return Ok(());
        }

        if received_data.len() == 32 {
            let hash: &[u8; 32] = received_data.try_into().unwrap();
            let Some(file) = self.get_file(hash) else {
                return Err(FileUploadError::ChecksumFileDoesNotExist);
            };
            let new_checksums: Vec<u8> = file.content.iter().cloned().collect();
            if self.latest_checksums.as_ref() == Some(&new_checksums) {
                return Ok(());
            }
            ::log::info!(target: "file-upload", "Loaded checksums from file");
            self.latest_checksums = Some(new_checksums);
            self.currently_receiving = None;
            return Ok(());
        }

        return Err(FileUploadError::ReceivedChunkWayTooShort);
    }

    /// This will be called on writes to the length characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn length_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        if received_data.len() != 4 {
            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let new_length = u32::from_le_bytes([
            received_data[0],
            received_data[1],
            received_data[2],
            received_data[3],
        ]);
        ::log::info!(target: "file-upload", "Received length {}", new_length);

        if self
            .latest_length
            .map_or(false, |old_length| old_length == new_length)
        {
            // Not changed, nothing to do
            return Ok(());
        }

        self.latest_length = Some(new_length);
        self.currently_receiving = None;

        return Ok(());
    }

    /// This will be called on writes to the chunk length characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn chunk_length_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        if received_data.len() != 2 {
            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let new_chunk_length = u16::from_le_bytes([received_data[0], received_data[1]]);
        ::log::info!(target: "file-upload", "Received chunk length {}", new_chunk_length);

        if self.latest_chunk_length.map_or(false, |old_chunk_length| {
            old_chunk_length == new_chunk_length
        }) {
            // Not changed, nothing to do
            return Ok(());
        }

        self.latest_chunk_length = Some(new_chunk_length);
        self.currently_receiving = None;

        return Ok(());
    }

    pub fn new(server: &mut BLEServer) -> Arc<Mutex<FileUploadService>> {
        let file_upload_service = Arc::new(Mutex::new(FileUploadService {
            files: Vec::new(),
            currently_receiving: None,

            latest_checksums: None,
            latest_chunk_length: None,
            latest_hash: None,
            latest_length: None,

            last_error: None,
        }));

        let service = server.create_service(FILE_UPLOAD_SERVICE_UUID);

        let data_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_DATA_UUID,
            NimbleProperties::WRITE_NO_RSP,
        );
        let file_upload_service_clone = file_upload_service.clone();
        data_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            match service.data_write(args) {
                Err(e) => service.log_error(e),
                _ => (),
            }
        });

        let hash_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_HASH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        let file_upload_service_clone = file_upload_service.clone();
        hash_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            match service.hash_write(args) {
                Err(e) => service.log_error(e),
                _ => (),
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        hash_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let hash = service.latest_hash.unwrap_or([0; 32]);
            value.set_value(&hash);
        });

        let checksums_characteristic = service
            .lock()
            .create_characteristic(FILE_UPLOAD_SERVICE_CHECKSUMS_UUID, NimbleProperties::WRITE);
        let file_upload_service_clone = file_upload_service.clone();
        checksums_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            match service.checksums_write(args) {
                Err(e) => service.log_error(e),
                _ => (),
            }
        });

        let length_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_LENGTH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        let file_upload_service_clone = file_upload_service.clone();
        length_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            match service.length_write(args) {
                Err(e) => service.log_error(e),
                _ => (),
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        length_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let length = service.latest_length.unwrap_or(0).to_le_bytes();
            value.set_value(&length);
        });

        let chunk_length_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_CHUNK_LENGTH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        let file_upload_service_clone = file_upload_service.clone();
        chunk_length_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            match service.chunk_length_write(args) {
                Err(e) => service.log_error(e),
                _ => (),
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        chunk_length_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let chunk_length = service.latest_chunk_length.unwrap_or(0).to_le_bytes();
            value.set_value(&chunk_length);
        });

        return file_upload_service;
    }
}
