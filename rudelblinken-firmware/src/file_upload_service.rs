use std::{
    io::{Seek, Write},
    sync::Arc,
};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEServer, DescriptorProperties, NimbleProperties,
};
use esp_idf_sys as _;
use rudelblinken_filesystem::{
    file_content::{FileContent, FileContentState},
    Filesystem,
};
use thiserror::Error;

use crate::storage::{get_filesystem, FlashStorage};

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
    name: String,
    pub content: FileContent<FlashStorage, { FileContentState::Weak }>,
}

#[derive(Debug)]
struct IncompleteFile {
    incomplete_file: FileContent<FlashStorage, { FileContentState::Writer }>,
    checksums: Vec<u8>,
    received_chunks: Vec<bool>,
    chunk_length: u16,
    length: u32,
    name: String,
    hash: [u8; 32],
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
    pub fn new(
        hash: [u8; 32],
        checksums: Vec<u8>,
        chunk_length: u16,
        length: u32,
        writer: FileContent<FlashStorage, { FileContentState::Writer }>,
        name: String,
    ) -> Self {
        Self {
            incomplete_file: writer,
            received_chunks: vec![false; checksums.len()],
            checksums,
            chunk_length,
            length,
            name,
            hash,
        }
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
        self.incomplete_file
            .seek(std::io::SeekFrom::Start(offset as u64))
            .unwrap();
        self.incomplete_file.write(data).unwrap();
        // self.incomplete_file.content[offset..(data.len() + offset)].copy_from_slice(data);
        self.received_chunks[index as usize] = true;

        Ok(())
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
    pub fn verify_hash(
        self,
        filesystem: &Filesystem<FlashStorage>,
    ) -> Result<FileContent<FlashStorage, { FileContentState::Weak }>, VerifyFileError> {
        if !self.is_complete() {
            return Err(VerifyFileError::NotComplete);
        }
        self.incomplete_file.commit().unwrap();
        let file = filesystem.read_file(&self.name).unwrap();
        let mut hasher = blake3::Hasher::new();
        hasher.update(file.upgrade().unwrap().as_ref());

        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_bytes());

        if hash != self.hash {
            ::log::warn!(target: "file-upload", "Hashes dont match.\nExpected: {:?}\nGot     : {:?}", self.hash, hash);
            return Err(VerifyFileError::HashMismatch);
        }
        ::log::info!(target: "file-upload", "Hashes match");

        Ok(file)
    }
    /// Get the uploaded file, if the upload is finished, otherwise this return None and you just destroyed your incomplete file for no reason
    pub fn into_file(
        self,
        filesystem: &Filesystem<FlashStorage>,
    ) -> Result<FileContent<FlashStorage, { FileContentState::Weak }>, VerifyFileError> {
        let file = self.verify_hash(filesystem)?;
        Ok(file)
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
        let mut filesystem = get_filesystem().unwrap().write().unwrap();
        let writer = filesystem.get_file_writer("toast", length, hash).unwrap();

        self.currently_receiving = Some(IncompleteFile::new(
            *hash,
            checksums.clone(),
            chunk_length,
            length,
            writer,
            "toast".into(),
        ));

        Ok(())
    }

    /// Starts an upload if there is no active upload
    ///
    /// If this returns Ok, self.currently_receiving is always set to Some
    fn ensure_upload(&mut self) -> Result<(), StartUploadError> {
        if self.currently_receiving.is_some() {
            return Ok(());
        }
        self.start_upload()?;
        Ok(())
    }

    fn log_error(&mut self, error: FileUploadError) {
        ::log::error!(target: "file-upload", "{}", error);
        self.last_error = Some(error);
    }

    /// Get the UUID of the file upload service
    pub const fn uuid() -> BleUuid {
        FILE_UPLOAD_SERVICE_UUID
    }

    /// This will be called on writes to the data characteristic
    ///
    /// We use this wrapper to make error handling easier
    fn data_write(
        &mut self,
        args: &mut esp32_nimble::OnWriteArgs<'_>,
    ) -> Result<(), FileUploadError> {
        let received_data = args.recv_data();
        ::log::info!(target: "file-upload", "chunk length {}", received_data.len());

        if received_data.len() < 3 {
            ::log::info!(target: "file-upload", "data length is too short {}", received_data.len());

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
            let incomplete_file = self
                .currently_receiving
                .take()
                .ok_or(FileUploadError::NoUploadActive)?;
            let hash = incomplete_file.hash.clone();
            let name = incomplete_file.name.clone();
            let file = incomplete_file.into_file(&get_filesystem().unwrap().read().unwrap())?;
            self.files.push(File {
                hash,
                name: name,
                content: file,
            });
        }
        Ok(())
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
            ::log::info!(target: "file-upload", "hash length is too short {}", received_data.len());

            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let new_hash: [u8; 32] = received_data.try_into().unwrap();
        ::log::info!(target: "file-upload", "Received hash {:?}", new_hash);
        if self.latest_hash.as_ref() == Some(&new_hash) {
            return Ok(());
        }
        self.latest_hash = Some(new_hash);
        self.currently_receiving = None;
        Ok(())
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
            let new_checksums: Vec<u8> = file.content.upgrade().unwrap().to_vec();
            if self.latest_checksums.as_ref() == Some(&new_checksums) {
                return Ok(());
            }
            ::log::info!(target: "file-upload", "Loaded checksums from file");
            self.latest_checksums = Some(new_checksums);
            self.currently_receiving = None;
            return Ok(());
        }

        ::log::info!(target: "file-upload", "checksums write length is too short {}", received_data.len());

        Err(FileUploadError::ReceivedChunkWayTooShort)
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
            ::log::info!(target: "file-upload", "length is too short {}", received_data.len());

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

        Ok(())
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
            ::log::info!(target: "file-upload", "chunk length is too short {}", received_data.len());

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

        Ok(())
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
        data_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::OPAQUE)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        data_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Chunk Upload".as_bytes());

        let hash_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_HASH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        hash_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::OPAQUE)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        hash_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("File Hash".as_bytes());

        let checksums_characteristic = service
            .lock()
            .create_characteristic(FILE_UPLOAD_SERVICE_CHECKSUMS_UUID, NimbleProperties::WRITE);
        checksums_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::OPAQUE)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        checksums_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Chunk Checksums".as_bytes());

        let length_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_LENGTH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        length_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT32)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        length_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("File Length".as_bytes());

        let chunk_length_characteristic = service.lock().create_characteristic(
            FILE_UPLOAD_SERVICE_CHUNK_LENGTH_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        chunk_length_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT16)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        chunk_length_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Chunk Length".as_bytes());

        let file_upload_service_clone = file_upload_service.clone();
        data_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            if let Err(e) = service.data_write(args) {
                service.log_error(e);
            }
        });

        let file_upload_service_clone = file_upload_service.clone();
        hash_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            if let Err(e) = service.hash_write(args) {
                service.log_error(e);
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        hash_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let hash = service.latest_hash.unwrap_or([0; 32]);
            value.set_value(&hash);
        });

        let file_upload_service_clone = file_upload_service.clone();
        checksums_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            if let Err(e) = service.checksums_write(args) {
                service.log_error(e);
            }
        });

        let file_upload_service_clone = file_upload_service.clone();
        length_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            if let Err(e) = service.length_write(args) {
                service.log_error(e);
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        length_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let length = service.latest_length.unwrap_or(0).to_le_bytes();
            value.set_value(&length);
        });

        let file_upload_service_clone = file_upload_service.clone();
        chunk_length_characteristic.lock().on_write(move |args| {
            let mut service = file_upload_service_clone.lock();
            if let Err(e) = service.chunk_length_write(args) {
                service.log_error(e);
            }
        });
        let file_upload_service_clone = file_upload_service.clone();
        chunk_length_characteristic.lock().on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let chunk_length = service.latest_chunk_length.unwrap_or(0).to_le_bytes();
            value.set_value(&chunk_length);
        });

        file_upload_service
    }
}
