use std::fmt::{Debug, Formatter};
use thiserror::Error;

use crate::{
    file_content::{
        CreateFileContentReaderError, CreateFileContentWriterError, DeleteFileContentError,
        FileContent, FileContentState,
    },
    file_metadata::{CreateMetadataError, FileMetadata, ReadMetadataError},
    storage::{EraseStorageError, Storage, StorageError},
    StorageLockError,
};

#[derive(Error, Debug)]
pub enum CreateFileInformationError {
    #[error(transparent)]
    ReadFileError(#[from] std::io::Error),
    #[error(transparent)]
    StorageLockError(#[from] StorageLockError),
    #[error(transparent)]
    ReadStorageError(#[from] StorageError),
    #[error(transparent)]
    FailedToReadBlockMetadata(#[from] ReadMetadataError),
    #[error("No metadata found the metadata does not have the correct marker.")]
    InvalidMetadataMarker,
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
    #[error(transparent)]
    CreateFileContentError(#[from] CreateFileContentReaderError),
}

#[derive(Error, Debug)]
pub enum WriteFileError {
    #[error("The filename can not be longer than 16 bytes")]
    FileNameTooLong,
    #[error(transparent)]
    CreateFileInformationError(#[from] CreateFileInformationError),
    #[error(transparent)]
    WriteStorageError(#[from] StorageError),
    #[error("The read file does not match the written file")]
    ReadFileDoesNotMatch,
    #[error(transparent)]
    StorageLockError(#[from] StorageLockError),
    #[error(transparent)]
    CreateMetadataError(#[from] CreateMetadataError),
    #[error(transparent)]
    CreateFileContentWriterError(#[from] CreateFileContentWriterError),
}

/// Internal proxy for a file that tracks some metadata in memory
pub struct File<T: Storage + 'static + Send + Sync> {
    /// Starting address of the file (in flash)
    pub address: u32,
    /// Length of the files content in bytes
    pub length: u32,
    /// Name of the file
    pub name: String,
    /// Content of the file
    /// Will be None if the file has been deleted
    content: FileContent<T, { FileContentState::Weak }>,
}

impl<T: Storage + 'static + Send + Sync> Clone for File<T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
            length: self.length.clone(),
            name: self.name.clone(),
            content: self.content.clone(),
        }
    }
}

impl<T: Storage + 'static + Send + Sync> std::fmt::Debug for File<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("address", &self.address)
            .field("length", &self.length)
            .field("name", &self.name)
            .field("content", &self.content)
            .finish()
    }
}

impl<T: Storage + 'static + Send + Sync> File<T> {
    /// Read a file from storage.
    ///
    /// address is an address that can be used with storage
    pub fn from_storage(
        storage: &'static T,
        address: u32,
    ) -> Result<File<T>, CreateFileInformationError> {
        let metadata = FileMetadata::from_storage(storage, address)?;
        let content = storage.read(address + size_of::<FileMetadata>() as u32, metadata.length)?;
        let file_content = FileContent::<T, { FileContentState::Reader }>::new(
            content,
            metadata,
            storage,
            address,
            |_| (),
        )?;

        let information = File {
            address,
            length: metadata.length,
            name: metadata.name_str().into(),
            content: file_content.downgrade(),
        };

        return Ok(information);
    }

    /// Create a new file and return a writer
    pub fn to_storage(
        storage: &'static T,
        address: u32,
        length: u32,
        name: &str,
    ) -> Result<(Self, FileContent<T, { FileContentState::Writer }>), WriteFileError> {
        let metadata = FileMetadata::new_to_storage(storage, address, name, length)?;
        let content = storage.read(address + size_of::<FileMetadata>() as u32, metadata.length)?;
        let file_content = FileContent::<T, { FileContentState::Writer }>::new_writer(
            content,
            metadata,
            storage,
            address,
            |_| (),
        )?;

        let information = File {
            address: address,
            length: metadata.length,
            name: metadata.name_str().into(),
            content: file_content.downgrade(),
        };
        return Ok((information, file_content));
    }

    /// Transition to ready by reading content from storage
    pub fn mark_for_deletion(&self) -> Result<(), DeleteFileContentError> {
        self.content.mark_for_deletion()
    }

    pub fn marked_for_deletion(&self) -> bool {
        self.content.marked_for_deletion()
    }

    pub fn deleted(&self) -> bool {
        self.content.deleted()
    }

    pub fn valid(&self) -> bool {
        self.content.ready()
    }

    pub fn read(&self) -> FileContent<T, { FileContentState::Weak }> {
        return self.content.clone();
    }
}
