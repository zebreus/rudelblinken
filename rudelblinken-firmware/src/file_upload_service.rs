use crate::storage::{get_filesystem, CreateStorageError, FlashStorage};
use incomplete_file::{IncompleteFile, ReceiveChunkError, VerifyFileError};
use rudelblinken_filesystem::file::{FileState, UpgradeFileError};
use thiserror::Error;
use upload_request::UploadRequest;
mod incomplete_file;
mod low_level;
mod upload_request;

#[derive(Debug)]
pub struct FileUploadService {
    currently_receiving: Option<IncompleteFile>,
    last_error: Option<FileUploadError>,
}

#[derive(Error, Debug, Clone)]
#[repr(u8)]
pub enum FileUploadError {
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
    #[error("Failed to decode upload request {0}")]
    MalformedUploadRequest(String),
    #[error("There was an error reading the checksums file {0}")]
    FailedToReadChecksums(UpgradeFileError),
    #[error("The checksums file does not have the expected size (Expected {expected}; Got {got}")]
    WrongNumberOfChecksums { expected: u32, got: u32 },
    #[error(transparent)]
    SetupFilesystemError(#[from] CreateStorageError),
    #[error("Failed to lock filesystem")]
    LockFilesystemError,
    #[error("Failed to create file: FilesystemWriteError: {0}")]
    FailedToCreateFile(String),
}

impl FileUploadService {
    /// Start an upload with the last received settings. Cancels a currently ongoing upload
    fn start_upload(&mut self, upload_request: &UploadRequest) -> Result<(), FileUploadError> {
        ::tracing::info!(target: "file-upload", "Received request {:?}", upload_request);
        ::tracing::info!(target: "file-upload", "Received hash {:?}", upload_request.hash);

        let checksums =
            self.load_checksums(&upload_request.checksums, &upload_request.chunk_count())?;

        let mut bytes = [0u8; 4];
        unsafe { esp_idf_sys::esp_fill_random(bytes.as_mut_ptr() as *mut core::ffi::c_void, 4) };
        let random_name = format!("fw-{}", u32::from_le_bytes(bytes));
        let writer = {
            let mut filesystem_writer = get_filesystem()?
                .write()
                .map_err(|_| FileUploadError::LockFilesystemError)?;
            filesystem_writer
                .get_file_writer(&random_name, upload_request.file_size, &upload_request.hash)
                .map_err(|error| FileUploadError::FailedToCreateFile(format!("{}", error)))?
        };

        let file = IncompleteFile::new(
            upload_request.hash,
            checksums.clone(),
            upload_request.chunk_size,
            upload_request.file_size,
            writer,
            random_name,
        );
        self.currently_receiving = Some(file);
        Ok(())
    }

    /// Called when an error occurs
    fn log_error(&mut self, error: FileUploadError) {
        ::tracing::error!(target: "file-upload", "{}", error);
        self.last_error = Some(error);
    }

    /// Called when a new chunk is received
    fn data_write(&mut self, chunk: &[u8]) -> Result<(), FileUploadError> {
        let maybe_current_upload = &mut self.currently_receiving;

        if chunk.len() < 3 {
            ::tracing::warn!(target: "file-upload", "data length is too short {}", chunk.len());

            return Err(FileUploadError::ReceivedChunkWayTooShort);
        }

        let index = u16::from_le_bytes([chunk[0], chunk[1]]);
        let data = &chunk[2..];

        ::tracing::info!(target: "file-upload", "Received chunk #{}", index);

        let Some(current_upload) = maybe_current_upload.as_mut() else {
            // Should never happen, because we called ensure_upload above
            return Err(FileUploadError::NoUploadActive);
        };
        current_upload.receive_chunk(data, index)?;
        if current_upload.is_complete() {
            let incomplete_file = maybe_current_upload
                .take()
                .ok_or(FileUploadError::NoUploadActive)?;
            incomplete_file.into_file(&get_filesystem().unwrap().read().unwrap())?;
        }
        Ok(())
    }

    /// Read a file from the filesystem
    pub fn get_file(
        &self,
        hash: &[u8; 32],
    ) -> Option<rudelblinken_filesystem::file::File<FlashStorage, { FileState::Weak }>> {
        let filesystem = get_filesystem().unwrap();
        let filesystem_reader: std::sync::RwLockReadGuard<
            '_,
            rudelblinken_filesystem::Filesystem<FlashStorage>,
        > = filesystem.read().unwrap();
        filesystem_reader.read_file_by_hash(hash)
    }

    /// Load
    fn load_checksums(
        &self,
        checksums: &[u8; 32],
        chunk_count: &u32,
    ) -> Result<Vec<u8>, FileUploadError> {
        if chunk_count <= &32 {
            ::tracing::info!(target: "file-upload", "Successfully loaded {} checksums from request", chunk_count);

            return Ok(checksums[0..(*chunk_count as usize)].to_vec());
        }

        let hash: &[u8; 32] = checksums.into();
        let Some(file) = self.get_file(hash) else {
            return Err(FileUploadError::ChecksumFileDoesNotExist);
        };
        let new_checksums: Vec<u8> = file
            .upgrade()
            .map_err(|error| FileUploadError::FailedToReadChecksums(error))?
            .to_vec();
        if (new_checksums.len() as u32) != *chunk_count {
            return Err(FileUploadError::WrongNumberOfChecksums {
                expected: *chunk_count,
                got: new_checksums.len() as u32,
            });
        }

        ::tracing::info!(target: "file-upload", "Successfully loaded {} checksums from file", new_checksums.len());

        return Ok(new_checksums);
    }

    /// Get the hash of the currently uploaded file.
    fn current_hash(&self) -> Option<&[u8; 32]> {
        self.currently_receiving
            .as_ref()
            .map(|incomplete_file| incomplete_file.get_hash())
    }

    /// Get the status of the currently uploaded file.
    fn get_status(&self) -> Option<(u16, Vec<u16>)> {
        self.currently_receiving
            .as_ref()
            .map(|incomplete_file| incomplete_file.get_status())
    }
}
