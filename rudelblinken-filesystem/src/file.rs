/// [File] provides a safe interface to read and write files.
use crate::{
    file_metadata::{FileMetadata, ReadMetadataError, WriteMetadataError},
    storage::{EraseStorageError, Storage, StorageError},
};
use std::{
    fmt::Debug,
    io::{SeekFrom, Write},
    ops::Deref,
    ptr::NonNull,
    sync::RwLock,
};
use std::{io::Seek, marker::ConstParamTy};
use thiserror::Error;
use zerocopy::IntoBytes;

/// Represents an error that can occur while reading a file.
#[derive(Error, Debug)]
pub enum ReadFileError {
    /// Error occurred in the storage layer.
    #[error(transparent)]
    StorageError(#[from] StorageError),
    /// No metadata found or the metadata does not have the correct marker.
    #[error("No metadata found the metadata does not have the correct marker.")]
    InvalidMetadataMarker,
    /// The file has already been deleted.
    #[error("File has already been deleted")]
    FileWasDeleted,
    /// The file is not marked as ready, possibly due to a power loss during writing.
    #[error("File is not marked as ready. Maybe you lost power during writing?")]
    FileNotReady,
}

/// Represents an error that can occur while writing a file.
#[derive(Error, Debug)]
pub enum WriteFileError {
    /// Error occurred in the storage layer.
    #[error(transparent)]
    StorageError(#[from] StorageError),
    /// No metadata found or the metadata does not have the correct marker.
    #[error("No metadata found the metadata does not have the correct marker.")]
    InvalidMetadataMarker,
    /// The file has already been deleted.
    #[error("File has already been deleted")]
    FileWasDeleted,
    /// The file is already marked as ready and should not be written to anymore.
    #[error("File is already marked as ready. You should not write to this anymore.")]
    FileIsAlreadyReady,
    /// The backing storage for a new file needs to be empty.
    #[error("The backing storage for a new file needs to be empty")]
    NotZeroed,
}

/// Represents an error that can occur while reading a file from storage.
#[derive(Error, Debug)]
pub enum ReadFileFromStorageError {
    /// Error occurred while reading metadata.
    #[error(transparent)]
    ReadMetadataError(#[from] ReadMetadataError),
    /// Error occurred while reading file content.
    #[error(transparent)]
    ReadFileContentError(#[from] ReadFileError),
}

/// Represents an error that can occur while writing a file to storage.
#[derive(Error, Debug)]
pub enum WriteFileToStorageError {
    /// Error occurred while writing metadata.
    #[error(transparent)]
    WriteMetadataError(#[from] WriteMetadataError),
    /// Error occurred while writing file content.
    #[error(transparent)]
    WriteFileContentError(#[from] WriteFileError),
}

/// Represents an error that can occur while upgrading a file
#[derive(Error, Debug, Clone)]
pub enum UpgradeFileError {
    /// Only weak references and readers can be upgraded.
    #[error("Only weak references and readers can be upgraded.")]
    CannotUpgradeWriter,
    /// Cannot read a file that has been deleted.
    #[error("Cannot read a file that has been deleted.")]
    FileHasBeenDeleted,
    /// Cannot read a file that is not ready yet.
    #[error("Cannot read a file that is not ready yet.")]
    NotReady,
    /// Cannot read a file that is marked for deletion.
    #[error("Cannot read a file that is marked for deletion.")]
    MarkedForDeletion,
}

/// Represents an error that can occur while deleting file content.
#[derive(Error, Debug)]
pub enum DeleteFileContentError {
    /// Error occurred while erasing storage.
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
}

/// Represents an error that can occur while committing file content.
#[derive(Error, Debug)]
pub enum CommitFileContentError {
    /// Error occurred in the storage layer.
    #[error(transparent)]
    StorageError(#[from] StorageError),
}

/// Represents the transition state of file content.
pub enum FileContentTransition {
    // /// Writer gets committed
    // Commit,
    // /// Writer gets dropped without commit
    // Abort,
    /// Last reader gets dropped
    DropLastReader,
}

/// Shared data about the current state of a file.
struct InnerFile<T: Storage + 'static + Send + Sync> {
    /// Number of weak references.
    weak_count: usize,
    /// Number of strong references.
    reader_count: usize,
    /// Number of writer references.
    writer_count: usize,
    /// Reference to the storage.
    storage: &'static T,
    /// Reference to the address in storage.
    storage_address: u32,
    /// Offset from the base address; only used for writer.
    current_offset: u32,
    /// Destructor that will be called when the last strong reference is dropped.
    transition: Box<dyn FnOnce(FileContentTransition) + 'static + Send + Sync>,
    // We need to track this in memory because the flags in memory-mapped flash will be reset when a new file is created in the same place
    /// Set if the file has been deleted.
    has_been_deleted: bool,
}

impl<T: Storage + 'static + Send + Sync> std::fmt::Debug for InnerFile<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContentInfo")
            .field("weak_count", &self.weak_count)
            .field("reader_count", &self.reader_count)
            .field("writer_count", &self.writer_count)
            .finish()
    }
}

/// Represents the state of a file.
///
/// See [File] for more information.
#[derive(ConstParamTy, PartialEq, Eq, Clone, Debug)]
pub enum FileState {
    /// Obtain a writer by creating a new file.
    Writer,
    /// Can be obtained by upgrading a weak reference.
    Reader,
    /// Can always be obtained by downgrading.
    Weak,
}

/// Provides a safe interface to read and write files.
///
/// The interface resembles a reference counted smart pointer, but for files.
///
/// Depending on the STATE parameter, the file can be read from or written to.
///
/// A file has possible states:
/// - Reader: Can be read from, but not written to.
/// - Writer: Can be written to, but not read from.
/// - Weak: Can be upgraded to a reader.
///
/// It resembles a reference-counted smart pointer with an mutex inside. A writer can
/// only be obtained by creating a new file. By calling [commit] on a writer, you
/// finalize the file. From this point onwards you can only read from the file.
/// If you want to edit the file, you need to erase it and create a new file.
///
/// # Safety
///
/// Files are backed by memory-mapped storage and readers provide direct access to that
/// storage. This creates a potential problem when the file is deleted and readers suddenly
/// point to storage that does not contain valid data anymore. To prevent this from happening,
/// File resembles a reference-counted smart pointer. Only when the last reader is dropped,
/// the file is deleted. After a file has been deleted it is no longer possible to upgrade a
/// weak reference to a reader.
pub struct File<T: Storage + 'static + Send + Sync, const STATE: FileState = { FileState::Reader }>
{
    content: &'static [u8],
    metadata: &'static FileMetadata,
    info: NonNull<RwLock<InnerFile<T>>>,
}

unsafe impl<T: Storage + 'static + Send + Sync, const STATE: FileState> Send for File<T, STATE> {}
unsafe impl<T: Storage + 'static + Send + Sync, const STATE: FileState> Sync for File<T, STATE> {}

impl<T: Storage + 'static + Send + Sync> File<T, { FileState::Reader }> {
    /// Create a new file content with the given memory area.
    ///
    /// It is only safe to call this function if there are no other instances pointing to the file.
    // TODO: Make this function unsafe or the argument a &'static [u8]
    fn new(
        data: &'static [u8],
        metadata: &'static FileMetadata,
        storage: &'static T,
        storage_address: u32,
        transition: impl FnOnce(FileContentTransition) + 'static + Send + Sync,
    ) -> Result<Self, ReadFileError> {
        if !metadata.valid_marker() {
            return Err(ReadFileError::InvalidMetadataMarker);
        }

        if !metadata.ready() {
            return Err(ReadFileError::FileNotReady);
        }

        let file = Self {
            content: data,
            metadata,
            info: Box::into_non_null(Box::new(RwLock::new(InnerFile {
                reader_count: 1,
                weak_count: 0,
                writer_count: 0,
                storage,
                storage_address,
                current_offset: 0,
                transition: Box::new(transition),
                has_been_deleted: false,
            }))),
        };

        if metadata.marked_for_deletion() {
            // Delete the file if it was marked for deletion
            unsafe {
                let _ = file.internal_delete();
            };
            return Err(ReadFileError::FileWasDeleted);
        }

        if metadata.deleted() {
            // This should only happen if a deletion was interrupted by a crash or something.
            unsafe {
                let _ = file.internal_delete();
            };
            return Err(ReadFileError::FileWasDeleted);
        };

        Ok(file)
    }

    /// Read a file from storage.
    ///
    /// `address` is an address that can be used with storage.
    pub fn from_storage(
        storage: &'static T,
        address: u32,
    ) -> Result<Self, ReadFileFromStorageError> {
        let metadata = FileMetadata::from_storage(storage, address)?;
        let content = storage
            .read(address + size_of::<FileMetadata>() as u32, metadata.length)
            .map_err(ReadFileError::from)?;
        let file_content =
            File::<T, { FileState::Reader }>::new(content, metadata, storage, address, |_| ())?;

        Ok(file_content)
    }

    /// Get the name of the file as a string slice.
    pub fn name_str(&self) -> &str {
        self.metadata.name_str()
    }

    /// Get the hash of the file
    pub fn hash(&self) -> &[u8; 32] {
        &self.metadata.hash
    }
}

impl<T: Storage + 'static + Send + Sync> File<T, { FileState::Writer }> {
    /// Create a new file writer with the given memory area.
    fn new_writer(
        data: &'static [u8],
        metadata: &'static FileMetadata,
        storage: &'static T,
        storage_address: u32,
        transition: impl FnOnce(FileContentTransition) + 'static + Send + Sync,
    ) -> Result<Self, WriteFileError> {
        if !metadata.valid_marker() {
            return Err(WriteFileError::InvalidMetadataMarker);
        }

        if metadata.deleted() {
            return Err(WriteFileError::FileWasDeleted);
        }

        if metadata.ready() {
            return Err(WriteFileError::FileIsAlreadyReady);
        }

        if metadata.marked_for_deletion() {
            // For now I allow this as this should never happen
        }

        if !data.iter().all(|byte| *byte == 0xff) {
            return Err(WriteFileError::NotZeroed);
        }

        Ok(Self {
            content: data,
            metadata,
            info: Box::into_non_null(Box::new(RwLock::new(InnerFile {
                reader_count: 0,
                weak_count: 0,
                writer_count: 1,
                storage,
                storage_address,
                current_offset: 0,
                transition: Box::new(transition),
                has_been_deleted: false,
            }))),
        })
    }

    /// Create a new file and return a writer.
    pub fn to_storage(
        storage: &'static T,
        address: u32,
        length: u32,
        name: &str,
        hash: &[u8; 32],
    ) -> Result<Self, WriteFileToStorageError> {
        let metadata = FileMetadata::new_to_storage(storage, address, name, length, &hash)?;
        let content = storage
            .read(address + size_of::<FileMetadata>() as u32, metadata.length)
            .map_err(WriteFileError::from)?;
        let file_content = File::<T, { FileState::Writer }>::new_writer(
            content,
            metadata,
            storage,
            address,
            |_| (),
        )?;

        Ok(file_content)
    }

    /// Commit the file content and convert it to a reader.
    ///
    /// This will finalize the file and make it read-only.
    pub fn commit(self) -> Result<File<T, { FileState::Reader }>, CommitFileContentError> {
        {
            let mut info = unsafe { (self.info.as_ref()).write().unwrap() };
            assert!(info.writer_count == 1);
            assert!(info.reader_count == 0);
            info.writer_count = 0;
            info.reader_count = 1;
            unsafe {
                self.metadata
                    .set_ready(info.storage, info.storage_address)?;
            }
        }
        unsafe {
            Ok(std::mem::transmute::<
                File<T, { FileState::Writer }>,
                File<T, { FileState::Reader }>,
            >(self))
        }
    }
}

impl<T: Storage + 'static + Send + Sync, const STATE: FileState> File<T, STATE> {
    /// Creates a new weak pointer to this data.
    pub fn downgrade(&self) -> File<T, { FileState::Weak }> {
        unsafe {
            self.info.as_ref().write().unwrap().weak_count += 1;
        }
        File::<T, { FileState::Weak }> {
            content: self.content,
            metadata: self.metadata,
            info: self.info,
        }
    }

    /// Creates a new strong pointer to this data.
    ///
    /// The file will not be deleted while you hold any strong reference to it. For this reason, it is best to only store the strong reference if you really need the file.
    ///
    /// Upgrading will always fail if the data has been marked for deletion.
    ///
    /// Upgrading weak references will fail if there are no strong references left.
    ///
    /// Upgrading a writer will always fail. Use commit instead.
    ///
    /// Upgrading will always fail while there is a writer alive.
    pub fn upgrade(&self) -> Result<File<T, { FileState::Reader }>, UpgradeFileError> {
        if STATE == FileState::Writer {
            return Err(UpgradeFileError::CannotUpgradeWriter);
        }
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        if info.has_been_deleted {
            return Err(UpgradeFileError::FileHasBeenDeleted);
        }
        if !self.metadata.ready() {
            return Err(UpgradeFileError::NotReady);
        }
        if self.metadata.marked_for_deletion() {
            return Err(UpgradeFileError::MarkedForDeletion);
        }

        info.reader_count += 1;
        Ok(File::<T, { FileState::Reader }> {
            content: self.content,
            metadata: self.metadata,
            info: self.info,
        })
    }

    /// Check if the data will be dropped if this reference is dropped.
    pub fn is_last(&self) -> bool {
        if STATE == FileState::Reader {
            return unsafe { self.info.as_ref().read().unwrap().reader_count == 1 };
        }
        if STATE == FileState::Writer {
            return unsafe { self.info.as_ref().read().unwrap().writer_count == 1 };
        }

        false
    }

    /// Get the number of readers.
    pub fn reader_count(&self) -> usize {
        return unsafe { self.info.as_ref().read().unwrap().reader_count };
    }

    /// Get the number of writers.
    pub fn writer_count(&self) -> usize {
        return unsafe { self.info.as_ref().read().unwrap().writer_count };
    }

    /// Check if the file is marked for deletion.
    pub fn marked_for_deletion(&self) -> bool {
        self.metadata.marked_for_deletion()
    }

    /// Check if the file is deleted.
    pub fn deleted(&self) -> bool {
        let info = unsafe { self.info.as_ref().read().unwrap() };
        if info.has_been_deleted {
            return true;
        }
        self.metadata.deleted()
    }

    /// Check if the file is ready.
    pub fn ready(&self) -> bool {
        self.metadata.ready()
    }

    /// Check if the file is important.
    pub fn important(&self) -> bool {
        self.metadata.important()
    }

    /// Check the age of the file.
    pub fn age(&self) -> u8 {
        self.metadata.age()
    }

    /// Mark the file as important.
    pub fn set_important(&self) -> Result<(), WriteMetadataError> {
        let info = unsafe { self.info.as_ref().read().unwrap() };

        unsafe {
            self.metadata
                .set_important(info.storage, info.storage_address)
                .unwrap();
        }

        return Ok(());
    }

    /// Increase the age of the file.
    pub fn increase_age(&self) -> Result<(), WriteMetadataError> {
        let info = unsafe { self.info.as_ref().read().unwrap() };

        unsafe {
            self.metadata
                .increase_age(info.storage, info.storage_address)
                .unwrap();
        }

        return Ok(());
    }

    /// Mark this file for deletion.
    ///
    /// No new strong references can be created to a file that's marked for deletion, except with clone on a strong reference.
    ///
    /// If there are no strong references left, the file will be deleted right away.
    pub(crate) fn mark_for_deletion(&self) -> Result<(), DeleteFileContentError> {
        let info = unsafe { self.info.as_ref().read().unwrap() };

        // TODO: Move this block in the !info.has_been_deleted guard
        unsafe {
            self.metadata
                .set_marked_for_deletion(info.storage, info.storage_address)
                .unwrap();
        };
        if !info.has_been_deleted && info.writer_count == 0 && info.reader_count == 0 {
            drop(info);
            unsafe { self.internal_delete()? };
        }
        Ok(())
    }

    /// Check if this file can be deleted right now.
    // TODO: Provide a way to prevent creating strong references to files for a short time
    pub(crate) fn can_be_deleted(&self) -> bool {
        let info = unsafe { self.info.as_ref().read().unwrap() };

        if info.writer_count == 0 && info.reader_count == 0 {
            return true;
        }
        return false;
    }

    /// Internal delete function that does not consume the file.
    ///
    /// Any access to this file afterwards is not safe.
    unsafe fn internal_delete(&self) -> Result<(), DeleteFileContentError> {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };

        let previous_transition: &mut Box<
            dyn FnOnce(FileContentTransition) + 'static + Send + Sync,
        > = &mut info.transition;
        let empty_transition: Box<dyn FnOnce(FileContentTransition) + 'static + Send + Sync> =
            Box::new(|_| ());
        let transition = std::mem::replace(previous_transition, empty_transition);
        (transition)(FileContentTransition::DropLastReader);

        self.metadata
            .set_deleted(info.storage, info.storage_address)
            .map_err(EraseStorageError::from)?;
        info.has_been_deleted = true;

        let full_file_length = self.metadata.length + size_of::<FileMetadata>() as u32;
        let length = full_file_length.div_ceil(T::BLOCK_SIZE) * T::BLOCK_SIZE;

        // TODO: Make sure the block with the metadata gets erased last
        info.storage.erase(info.storage_address, length)?;
        Ok(())
    }

    /// Zero out the backing storage of this file and mark it as deleted.
    ///
    /// Only safe if no further reads or writes will be performed to the file.
    pub fn delete(self) -> Result<(), DeleteFileContentError> {
        unsafe { self.internal_delete() }
    }

    /// Check if the backing storage for the file is completely zeroed out.
    ///
    /// A valid file should never be zeroed, so this is marked as unsafe.
    pub unsafe fn erased(&self) -> bool {
        let metadata_slice = FileMetadata::as_bytes(self.metadata);
        if metadata_slice.iter().any(|i| *i != 0xff) {
            return false;
        }
        if self.content.iter().any(|i| *i != 0xff) {
            return false;
        }
        true
    }

    /// Check if the file has this hash
    ///
    /// Returns false if the file is not ready
    pub fn compare_hash(&self, hash: &[u8; 32]) -> bool {
        if STATE == FileState::Writer {
            return false;
        }
        let info = unsafe { self.info.as_ref().read().unwrap() };
        if info.has_been_deleted {
            return false;
        }
        if !self.metadata.ready() {
            return false;
        }
        if self.metadata.marked_for_deletion() {
            return false;
        }

        &self.metadata.hash == hash
    }
}

impl<T: Storage + 'static + Send + Sync, const STATE: FileState> Debug for File<T, STATE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContent")
            // .field("content", &self.content)
            .field("metadata", &self.metadata)
            .field("info", unsafe { self.info.as_ref() })
            .finish()
    }
}

impl<T: Storage + 'static + Send + Sync> Deref for File<T, { FileState::Reader }> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.content
    }
}

impl<T: Storage + 'static + Send + Sync> PartialEq<Self> for File<T, { FileState::Reader }> {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
}

impl<T: Storage + 'static + Send + Sync> Clone for File<T, { FileState::Reader }> {
    fn clone(&self) -> Self {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        info.reader_count += 1;
        Self {
            content: self.content,
            metadata: self.metadata,
            info: self.info,
        }
    }
}

impl<T: Storage + 'static + Send + Sync> Clone for File<T, { FileState::Weak }> {
    fn clone(&self) -> Self {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        info.weak_count += 1;
        Self {
            content: self.content,
            metadata: self.metadata,
            info: self.info,
        }
    }
}

impl<T: Storage + 'static + Send + Sync, const STATE: FileState> Drop for File<T, STATE> {
    fn drop(&mut self) {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };

        if STATE == { FileState::Weak } {
            info.weak_count = info.weak_count.saturating_sub(1);
        }
        if STATE == { FileState::Writer } {
            info.writer_count = info.writer_count.saturating_sub(1);
        }
        if STATE == { FileState::Reader } {
            info.reader_count = info.reader_count.saturating_sub(1);
        }

        if info.reader_count != 0 || info.writer_count != 0 {
            return;
        }

        let weak_count = info.weak_count;
        let has_been_deleted = info.has_been_deleted;
        drop(info);
        if !has_been_deleted && self.metadata.marked_for_deletion() {
            unsafe {
                // We cant really handle a failed deletion here
                // TODO: maybe log it
                let _ = self.internal_delete();
            };
        }

        if weak_count == 0 {
            unsafe {
                drop(Box::from_non_null(self.info));
            };
        }
    }
}

impl<T: Storage + 'static + Send + Sync> Seek for File<T, { FileState::Writer }> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let length = self.content.len() as u32;
        let current_offset = unsafe {
            &mut self
                .info
                .as_ref()
                .write()
                .map_err(|e| std::io::Error::other(e.to_string()))?
                .current_offset
        };
        let new_offset = match pos {
            SeekFrom::Start(offset) => offset.try_into().unwrap_or(u32::MAX).clamp(0, length),
            SeekFrom::End(offset) => length
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0, length),
            SeekFrom::Current(offset) => current_offset
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0, length),
        };

        *current_offset = new_offset;
        Ok(*current_offset as u64)
    }
}

impl<T: Storage + 'static + Send + Sync> Write for File<T, { FileState::Writer }> {
    /// The same as [std::io::Write::write] but you can only flip bits from 1 to 0.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let length = self.content.len() as u32;
        let info = unsafe {
            &mut self
                .info
                .as_ref()
                .write()
                .map_err(|_| std::io::ErrorKind::ResourceBusy)?
        };
        let current_offset = info.current_offset;

        let remaining_length = length.saturating_sub(current_offset);
        let write_length = std::cmp::min(remaining_length, buf.len() as u32);

        let writable_storage = info.storage;
        writable_storage
            .write(
                info.storage_address + size_of::<FileMetadata>() as u32 + current_offset,
                &buf[0..write_length as usize],
            )
            .map_err(std::io::Error::other)?;
        info.current_offset += write_length;
        Ok(write_length as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::simulated::{get_test_storage, SimulatedStorage};

    use super::*;

    fn get_backing() -> (
        &'static SimulatedStorage,
        &'static mut [u8],
        &'static FileMetadata,
    ) {
        let backing_storage = get_test_storage();
        let metadata: &'static FileMetadata = dbg!(FileMetadata::new_to_storage(
            backing_storage,
            0,
            "toast",
            100,
            &[0; 32]
        ))
        .unwrap();
        unsafe { metadata.set_ready(backing_storage, 0) }.unwrap();
        let content: &'static [u8] = &backing_storage
            .read(size_of::<FileMetadata>() as u32, 100)
            .unwrap();
        let content_ptr = content.as_ptr() as *mut u8;
        let mut_content: &'static mut [u8] =
            unsafe { std::slice::from_raw_parts_mut(content_ptr, 100) };

        return (backing_storage, mut_content, metadata);
    }

    fn call_new() -> File<SimulatedStorage, { FileState::Reader }> {
        let (storage, content, metadata) = get_backing();
        let content = File::<_, { FileState::Reader }>::new(content, metadata, storage, 0, |_| ());
        return content.unwrap();
    }

    #[test]
    fn creating_and_dropping_a_file_does_not_panic() {
        let content = call_new();
        drop(content);
    }

    #[test]
    fn equality_works() {
        let (storage1, content1, metadata1) = get_backing();
        let content1 =
            File::<_, { FileState::Reader }>::new(content1, metadata1, storage1, 0, |_| ())
                .unwrap();
        let (storage2, content2, metadata2) = get_backing();
        let content2 =
            File::<_, { FileState::Reader }>::new(content2, metadata2, storage2, 0, |_| ())
                .unwrap();
        let (storage3, content3, metadata3) = get_backing();
        content3[1] = 17;
        let content3 =
            File::<_, { FileState::Reader }>::new(content3, metadata3, storage3, 0, |_| ())
                .unwrap();
        assert_eq!(content1, content2);
        assert_ne!(content2, content3);
    }

    #[test]
    fn cloning_works() {
        let (storage, content, metadata) = get_backing();
        content[3] = 17;
        let content =
            File::<_, { FileState::Reader }>::new(content, metadata, storage, 0, |_| ()).unwrap();
        let cloned_content = content.clone();
        assert_eq!(content, cloned_content);
    }

    #[test]
    fn is_last_works() {
        let content = call_new();

        assert!(File::is_last(&content));
        let other_content = content.clone();
        assert!(!File::is_last(&content));
        assert!(!File::is_last(&other_content));
        drop(content);
        assert!(File::is_last(&other_content));
    }

    #[test]
    fn downgrading_works() {
        let content = call_new();
        assert!(File::is_last(&content));
        let weak_content = content.downgrade();
        assert!(File::is_last(&content));
        drop(weak_content);
        assert!(File::is_last(&content));
    }

    #[test]
    fn upgrading_works() {
        let content = call_new();
        assert!(File::is_last(&content));
        let weak_content = content.downgrade();
        assert!(File::is_last(&content));
        let upgraded_content = weak_content.upgrade().unwrap();
        assert!(!File::is_last(&content));
        assert!(!File::is_last(&upgraded_content));
        drop(content);
        assert!(File::is_last(&upgraded_content));
    }

    #[test]
    fn upgrading_works_even_if_there_are_no_readers_left() {
        let content = call_new();
        assert!(File::is_last(&content));
        let weak_content = content.downgrade();
        assert!(File::is_last(&content));
        drop(content);
        weak_content.upgrade().unwrap();
    }

    #[test]
    fn upgrading_does_not_work_when_reader_was_marked_for_deletion() {
        let content = call_new();
        assert!(File::is_last(&content));
        let weak_content = content.downgrade();
        assert!(File::is_last(&content));
        content.mark_for_deletion().unwrap();
        drop(content);
        let Err(_) = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }

    #[test]
    fn upgrading_does_not_work_when_weak_was_marked_for_deletion() {
        let content = call_new();
        assert!(File::is_last(&content));
        let weak_content = content.downgrade();
        assert!(File::is_last(&content));
        drop(content);
        weak_content.mark_for_deletion().unwrap();
        let Err(_) = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }

    #[test]
    fn deleting_when_there_is_no_reader_works() {
        let content = call_new();
        let weak_content = content.downgrade();
        drop(content);
        weak_content.mark_for_deletion().unwrap();
        assert!(weak_content.deleted() == true);
    }

    #[test]
    fn deleting_is_deferred_until_the_last_reader_is_dropped() {
        let content = call_new();
        let weak_content = content.downgrade();
        weak_content.mark_for_deletion().unwrap();
        assert!(weak_content.deleted() == false);
        assert!(content.deleted() == false);

        // Dropping the weak does nothing
        drop(weak_content);
        assert!(content.deleted() == false);

        let weak_content2 = content.downgrade();
        assert!(weak_content2.deleted() == false);
        drop(content);
        assert!(weak_content2.deleted() == true);
    }

    #[test]
    fn upgrading_fails_when_marked_for_deletion() {
        let content = call_new();
        let weak_content = content.downgrade();
        File::mark_for_deletion(&content).unwrap();
        let Err(_) = content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
        let Err(_) = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }
}
