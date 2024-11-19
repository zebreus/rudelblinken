use crate::{
    file_content::{
        DeleteFileContentError, FileContent, FileContentState, ReadFileFromStorageError,
        WriteFileToStorageError,
    },
    storage::Storage,
};
use std::fmt::Formatter;

/// Internal proxy for a file that tracks some metadata in memory
pub(crate) struct File<T: Storage + 'static + Send + Sync> {
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
    ) -> Result<File<T>, ReadFileFromStorageError> {
        let file_content =
            FileContent::<T, { FileContentState::Reader }>::from_storage(storage, address)?;

        let information = File {
            address,
            length: file_content.len() as u32,
            name: file_content.name_str().into(),
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
    ) -> Result<(Self, FileContent<T, { FileContentState::Writer }>), WriteFileToStorageError> {
        let file_content = FileContent::<T, { FileContentState::Writer }>::to_storage(
            storage, address, length, name,
        )?;

        let information = File {
            address: address,
            length: length,
            name: name.into(),
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
