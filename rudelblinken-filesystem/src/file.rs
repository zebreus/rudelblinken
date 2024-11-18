use std::{
    fmt::{Debug, Formatter},
    io::Write,
    sync::{Arc, RwLock},
};
use thiserror::Error;

use crate::{
    file_content::{
        self, CreateFileContentError, CreateFileContentWriterError, FileContent, FileContentState,
    },
    file_metadata::{CreateMetadataError, FileMetadata, ReadMetadataError},
    file_writer::FileWriter,
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
    CreateFileContentError(#[from] CreateFileContentError),
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

// impl FileState {
//     pub fn metadata(&self) -> Option<&'static FileMetadata> {
//         match self {
//             FileState::NotReady { metadata } => Some(metadata),
//             FileState::Ready { metadata, .. } => Some(metadata),
//             FileState::MarkedForDeletion { metadata } => Some(metadata),
//             FileState::Deleted {} => None,
//         }
//     }
// }

fn take<T, F>(mut_ref: &mut T, closure: F)
where
    F: FnOnce(T) -> T,
{
    use std::ptr;

    unsafe {
        let old_t = ptr::read(mut_ref);
        let new_t = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| closure(old_t)))
            .unwrap_or_else(|_| ::std::process::abort());
        ptr::write(mut_ref, new_t);
    }
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

    // /// Transition to ready by reading content from storage
    // fn ready(&self) -> Result<(), CreateFileInformationError> {
    //     let mut file_state = self.content.write().unwrap();
    //     let FileState::NotReady { metadata } = *file_state else {
    //         panic!("Can only transition to Ready from NotReady");
    //     };

    //     let memory_mapped_content = self
    //         .storage
    //         .read()
    //         .map_err(|_| StorageLockError::FailedToAquireReadLock)?
    //         .read(self.address + size_of::<FileMetadata>() as u32, self.length)?;

    //     unsafe {
    //         metadata.set_flag_in_storage(
    //             &mut *self
    //                 .storage
    //                 .write()
    //                 .map_err(|_| StorageLockError::FailedToAquireWriteLock)?,
    //             self.address,
    //             FileFlags::Ready,
    //         )
    //     };

    //     let cloned_file = self.clone();
    //     let content = FileContent::new(memory_mapped_content, move || {
    //         let marked_for_deletion = matches!(
    //             *cloned_file.content.read().unwrap(),
    //             FileState::MarkedForDeletion { .. }
    //         );
    //         if marked_for_deletion {
    //             cloned_file.delete().unwrap();
    //         }
    //     });
    //     *file_state = FileState::Ready {
    //         content: content,
    //         metadata: metadata,
    //     };

    //     return Ok(());
    // }

    /// Transition to ready by reading content from storage
    pub fn mark_for_deletion(&self) -> Result<(), CreateFileInformationError> {
        self.content.mark_for_deletion();
        return Ok(());
        // let mut file_state = self.content.write().unwrap();

        // if let Some(metadata) = file_state.metadata() {
        //     unsafe {
        //         metadata.set_flag_in_storage(
        //             &mut *self
        //                 .storage
        //                 .write()
        //                 .map_err(|_| StorageLockError::FailedToAquireWriteLock)?,
        //             self.address,
        //             FileFlags::MarkedForDeletion,
        //         )?;
        //     }
        // };

        // let mut defer_drop_for_content: Option<FileContent> = None;
        // take(&mut *file_state, |previous_state| match previous_state {
        //     FileState::NotReady { metadata } => FileState::MarkedForDeletion { metadata },
        //     FileState::Ready { content, metadata } => {
        //         defer_drop_for_content.replace(content);
        //         FileState::MarkedForDeletion { metadata }
        //     }
        //     FileState::MarkedForDeletion { metadata } => FileState::MarkedForDeletion { metadata },
        //     FileState::Deleted {} => FileState::Deleted {},
        // });
        // drop(file_state);
        // // Defer dropping the content untion the filestate is available again. This is neccessary because the last drop will change the file state to deleted
        // drop(defer_drop_for_content);
        // return Ok(());
    }

    // /// Actually delete the file and mark it as deleted
    // ///
    // /// Should never be called, if the file is NotReady and still has an active writer
    // fn delete(&self) -> Result<(), CreateFileInformationError> {
    //     let mut file_state = self.content.write().unwrap();

    //     if let Some(metadata) = file_state.metadata() {
    //         unsafe {
    //             metadata.set_flag_in_storage(
    //                 &mut *self
    //                     .storage
    //                     .write()
    //                     .map_err(|_| StorageLockError::FailedToAquireWriteLock)?,
    //                 self.address,
    //                 FileFlags::Deleted,
    //             );
    //         }
    //     };

    //     let (address, length) = match *file_state {
    //         FileState::Deleted {} => {
    //             return Ok(());
    //         }
    //         _ => (self.address, self.length),
    //     };

    //     let full_file_length = length + size_of::<FileMetadata>() as u32;
    //     let length = full_file_length.div_ceil(T::BLOCK_SIZE) * T::BLOCK_SIZE;

    //     self.storage.write().unwrap().erase(address, length)?;

    //     *file_state = FileState::Deleted {};
    //     return Ok(());
    // }

    /// Check if there are no other references to the file left
    // pub fn no_strong_references_left(&self) -> bool {
    //     return FileContent::strong_count(&self.weak_content) == 0;
    // }

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
