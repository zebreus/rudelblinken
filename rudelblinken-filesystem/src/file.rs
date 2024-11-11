use std::{
    io::Write,
    sync::{Arc, RwLock},
};
use thiserror::Error;

use crate::{
    file_content::FileContent,
    file_metadata::{CreateMetadataError, FileMetadata, ReadMetadataError},
    file_writer::FileWriter,
    storage::{Storage, StorageError},
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
}

pub struct FileHandle {
    // Content
    pub content: FileContent<false>,
    // Name
    pub name: String,
}

pub enum FileState<T: Storage + 'static> {
    NotReady(FileWriter<T>),
    Ready(FileContent<true>),
    Deleted(),
}

pub struct File<T: Storage + 'static> {
    /// Starting address of the file (in flash)
    pub address: usize,
    /// Length of the files content in bytes
    pub length: usize,
    /// Name of the file
    pub name: String,
    /// Content of the file
    /// Will be None if the file has been deleted
    content: FileState<T>,
    /// Weak pointer to the content of a file
    weak_content: FileContent<false>,
    /// Reference to the memory-mapped block metadata. May be invalid, if the block got deleted
    ///
    /// I think accessing the  is probably slow, because it will be rarely cached if we have a lot of small files in different blocks.
    /// For this reason we copy the name and the size of the file into ram
    metadata: &'static FileMetadata,
}

impl<T: Storage + 'static> File<T> {
    /// Read a file from storage.
    ///
    /// address is an address that can be used with storage
    pub fn from_storage(
        storage: Arc<RwLock<T>>,
        address: usize,
    ) -> Result<File, CreateFileInformationError> {
        let static_content: &'static [u8];
        let static_metadata: &'static FileMetadata;
        {
            let storage = storage
                .read()
                .map_err(|_| StorageLockError::FailedToAquireReadLock)?;
            let metadata = FileMetadata::from_storage(&*storage, address)?;
            if metadata.valid_marker() {
                return Err(CreateFileInformationError::InvalidMetadataMarker);
            }
            static_metadata =
                unsafe { std::mem::transmute::<&FileMetadata, &'static FileMetadata>(metadata) };

            let content = storage.read(
                address + size_of::<FileMetadata>(),
                metadata.length as usize,
            )?;
            static_content = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(content) };
        }

        let full_file_length = static_metadata.length as usize + size_of::<FileMetadata>();

        let strong_content = FileContent::new(static_content, move |marked_for_deletion| {
            if marked_for_deletion {
                let length = full_file_length.div_ceil(T::BLOCK_SIZE) * T::BLOCK_SIZE;

                storage.write().unwrap().erase(address, length).unwrap();
            }
        });

        let information = File {
            address: address,
            length: static_metadata.length as usize,
            name: static_metadata.name_str().into(),
            metadata: static_metadata,
            weak_content: strong_content.downgrade(),
            content: FileState::Ready(strong_content),
        };
        return Ok(information);
    }

    /// Check if there are no other references to the file left
    pub fn no_strong_references_left(&self) -> bool {
        return FileContent::strong_count(&self.weak_content) == 0;
    }

    pub fn valid(&self) -> bool {
        self.content.is_some()
    }

    pub fn into_file(&self) -> FileHandle {
        return FileHandle {
            content: self.weak_content.clone(),
            name: self.metadata.name_str().into(),
        };
    }

    pub fn new_to_storage(
        &mut self,
        storage: Arc<RwLock<T>>,
        address: usize,
        name: &str,
        content: &[u8],
    ) -> Result<Self, WriteFileError> {
        let length = content.len();
        let storage = &mut storage.write().unwrap().into();
        let metadata = FileMetadata::new_to_storage(storage, address, name, content.len() as u32)?;
        let writer = FileWriter::new(storage, address, content.len());
        writer.write_all(content).unwrap();
        writer.commit();
        return Ok(File {
            address,
            length,
            name: name.into(),
            content: FileState::Ready(()),
            weak_content: todo!(),
            metadata,
        });
    }
}
