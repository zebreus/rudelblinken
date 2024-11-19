use crate::{
    file_metadata::FileMetadata,
    storage::{EraseStorageError, Storage, StorageError},
};
use std::{
    fmt::{self, Debug},
    io::{SeekFrom, Write},
    ops::Deref,
    ptr::NonNull,
    sync::RwLock,
};
use std::{io::Seek, marker::ConstParamTy};
use thiserror::Error;
use zerocopy::IntoBytes;

pub enum FileContentTransition {
    /// Writer gets committed
    Commit,
    /// Writer gets dropped without commit
    Abort,
    /// Last reader gets dropped
    DropLastReader,
}

struct FileContentInfo<T: Storage + 'static> {
    /// Number of weak references
    weak_count: usize,
    /// Number of strong references
    reader_count: usize,
    /// Number of writer references
    writer_count: usize,
    // Reference to the storage
    storage: &'static T,
    // Reference to the address in storage
    storage_address: u32,
    // Offset from the base address; only used for writer
    current_offset: u32,
    /// Destructor that will be called when the last strong reference is dropped
    transition: Box<dyn FnOnce(FileContentTransition) -> () + 'static + Send + Sync>,
    // We need to track this in memory because the flags in memory-mapped flash will be reset when a new file is created in the same place
    /// Set if the file has been deleted.
    has_been_deleted: bool,
}

impl<T: Storage + 'static> fmt::Debug for FileContentInfo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileContentInfo")
            .field("weak_count", &self.weak_count)
            .field("reader_count", &self.reader_count)
            .field("writer_count", &self.writer_count)
            .finish()
    }
}

#[derive(ConstParamTy, PartialEq, Eq, Clone, Debug)]
pub enum FileContentState {
    /// Obtain a writer by creating a new file
    Writer,
    /// Can be obtained by upgrading a weak reference
    Reader,
    /// Can always be obtained by downgrading
    Weak,
}

pub struct FileContent<
    T: Storage + 'static,
    const STATE: FileContentState = { FileContentState::Reader },
> {
    // content: VolatileRef<'static, [u8], ReadOnly>,
    // metadata: VolatileRef<'static, FileMetadata, ReadOnly>,
    content: &'static [u8],
    metadata: &'static FileMetadata,
    // TODO: Change this to an arcmutex
    info: NonNull<RwLock<FileContentInfo<T>>>,
}

#[derive(Error, Debug)]
pub enum CreateFileContentReaderError {
    #[error("No metadata found the metadata does not have the correct marker.")]
    InvalidMetadataMarker,
    #[error("File has already been deleted")]
    FileWasDeleted,
    #[error("File is not marked as ready. Maybe you lost power during writing?")]
    FileNotReady,
}

#[derive(Error, Debug)]
pub enum CreateFileContentWriterError {
    #[error("No metadata found the metadata does not have the correct marker.")]
    InvalidMetadataMarker,
    #[error("File has already been deleted")]
    FileWasDeleted,
    #[error("File is already marked as ready. You should not write to this anymore.")]
    FileIsAlreadyReady,
    #[error("The backing storage for a new file needs to be empty")]
    NotZeroed,
}

#[derive(Error, Debug)]
pub enum DeleteFileContentError {
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
}

impl<T: Storage + 'static> FileContent<T, { FileContentState::Reader }> {
    /// Create a new file content with the given memory area
    ///
    /// It is only safe to call this function if there are no other instances pointing to the file
    // TODO: Make this function unsafe or the argument a &'static [u8]
    pub fn new(
        // data: VolatileRef<'static, [u8], ReadOnly>,
        // metadata: VolatileRef<'static, FileMetadata, ReadOnly>,
        data: &'static [u8],
        metadata: &'static FileMetadata,
        storage: &'static T,
        storage_address: u32,
        transition: impl FnOnce(FileContentTransition) -> () + 'static + Send + Sync,
    ) -> Result<Self, CreateFileContentReaderError> {
        if !metadata.valid_marker() {
            return Err(CreateFileContentReaderError::InvalidMetadataMarker);
        }

        if !metadata.ready() {
            return Err(CreateFileContentReaderError::FileNotReady);
        }

        let file = Self {
            content: data,
            metadata: metadata,
            info: Box::into_non_null(Box::new(RwLock::new(FileContentInfo {
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
            return Err(CreateFileContentReaderError::FileWasDeleted);
        }

        if metadata.deleted() {
            // This should only happen if a deletion was interrupted by a crash or something.
            unsafe {
                let _ = file.internal_delete();
            };
            return Err(CreateFileContentReaderError::FileWasDeleted);
        };

        return Ok(file);
    }
}

impl<T: Storage + 'static> FileContent<T, { FileContentState::Writer }> {
    /// Create a new file content with the given memory area
    pub fn new_writer(
        data: &'static [u8],
        metadata: &'static FileMetadata,
        storage: &'static T,
        storage_address: u32,
        transition: impl FnOnce(FileContentTransition) -> () + 'static + Send + Sync,
    ) -> Result<Self, CreateFileContentWriterError> {
        if !metadata.valid_marker() {
            return Err(CreateFileContentWriterError::InvalidMetadataMarker);
        }

        if metadata.deleted() {
            return Err(CreateFileContentWriterError::FileWasDeleted);
        }

        if metadata.ready() {
            return Err(CreateFileContentWriterError::FileIsAlreadyReady);
        }

        if metadata.marked_for_deletion() {
            // For now I allow this
        }

        if !data.iter().all(|byte| *byte == 0xff) {
            return Err(CreateFileContentWriterError::NotZeroed);
        }

        return Ok(Self {
            content: data,
            metadata: metadata,
            info: Box::into_non_null(Box::new(RwLock::new(FileContentInfo {
                reader_count: 0,
                weak_count: 0,
                writer_count: 1,
                storage,
                storage_address,
                current_offset: 0,
                transition: Box::new(transition),
                has_been_deleted: false,
            }))),
        });
    }

    pub fn commit(self) -> Result<FileContent<T, { FileContentState::Reader }>, StorageError> {
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
        return unsafe {
            Ok(std::mem::transmute::<
                _,
                FileContent<T, { FileContentState::Reader }>,
            >(self))
        };
    }
}

impl<T: Storage + 'static, const STATE: FileContentState> FileContent<T, STATE> {
    /// Creates a new weak pointer to this data
    pub fn downgrade(&self) -> FileContent<T, { FileContentState::Weak }> {
        unsafe {
            self.info.as_ref().write().unwrap().weak_count += 1;
        }
        return FileContent::<T, { FileContentState::Weak }> {
            content: self.content,
            metadata: self.metadata,
            info: self.info.clone(),
        };
    }

    /// Creates a new strong pointer to this data
    ///
    /// The file will not be deleted, while you hold any strong reference to it. For this reason it is best to only store the strong reference if you really need the file.
    ///
    /// Upgrading will always fail if the data has been marked for deletion.
    ///
    /// Upgrading weak references will fail if there are no strong references left.
    ///
    /// Upgrading a writer will alwayse fail. Use commit instead.
    ///
    /// Upgrading will always fail while there is a writer alive
    pub fn upgrade(&self) -> Option<FileContent<T, { FileContentState::Reader }>> {
        if STATE == FileContentState::Writer {
            return None;
        }
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        if info.has_been_deleted {
            return None;
        }
        if !self.metadata.ready() {
            return None;
        }
        if self.metadata.marked_for_deletion() {
            return None;
        }

        info.reader_count += 1;
        return Some(FileContent::<T, { FileContentState::Reader }> {
            content: self.content,
            metadata: self.metadata,
            info: self.info.clone(),
        });
    }

    /// Check if the data will be dropped if this reference is dropped.
    pub fn is_last(&self) -> bool {
        if STATE == FileContentState::Reader {
            return unsafe { self.info.as_ref().read().unwrap().reader_count == 1 };
        }
        if STATE == FileContentState::Writer {
            return unsafe { self.info.as_ref().read().unwrap().writer_count == 1 };
        }

        return false;
    }

    pub fn reader_count(&self) -> usize {
        return unsafe { self.info.as_ref().read().unwrap().reader_count };
    }

    pub fn writer_count(&self) -> usize {
        return unsafe { self.info.as_ref().read().unwrap().writer_count };
    }

    pub fn marked_for_deletion(&self) -> bool {
        return self.metadata.marked_for_deletion();
    }
    pub fn deleted(&self) -> bool {
        let info = unsafe { self.info.as_ref().read().unwrap() };
        if info.has_been_deleted {
            return true;
        }
        return self.metadata.deleted();
    }
    pub fn ready(&self) -> bool {
        return self.metadata.ready();
    }

    /// Mark this file for deletion
    ///
    /// No new strong references can be created to a file thats marked for deletion, except with clone on a strong reference.
    ///
    /// If there are no strong references left, the file will be deleted right away
    pub fn mark_for_deletion(&self) -> Result<(), DeleteFileContentError> {
        let info = unsafe { self.info.as_ref().read().unwrap() };

        unsafe {
            self.metadata
                .set_marked_for_deletion(info.storage, info.storage_address)
                .unwrap();
        };
        if info.has_been_deleted == false && info.writer_count == 0 && info.reader_count == 0 {
            drop(info);
            unsafe { self.internal_delete()? };
        }
        Ok(())
    }

    /// Internal delete function that does not consume the file.
    ///
    /// Any access to this file afterwards is not safe.
    unsafe fn internal_delete(&self) -> Result<(), DeleteFileContentError> {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };

        let previous_transition: &mut Box<
            dyn FnOnce(FileContentTransition) -> () + 'static + Send + Sync,
        > = &mut info.transition;
        let empty_transition: Box<dyn FnOnce(FileContentTransition) -> () + 'static + Send + Sync> =
            Box::new(|_| ());
        let transition = std::mem::replace(previous_transition, empty_transition);
        (transition)(FileContentTransition::DropLastReader);

        self.metadata
            .set_deleted(info.storage, info.storage_address)
            .map_err(|e| EraseStorageError::from(e))?;
        info.has_been_deleted = true;

        let full_file_length = self.metadata.length + size_of::<FileMetadata>() as u32;
        let length = full_file_length.div_ceil(T::BLOCK_SIZE) * T::BLOCK_SIZE;

        // TODO: Make sure the block with the metadata gets erased last
        info.storage.erase(info.storage_address, length)?;
        return Ok(());
    }

    /// Zero out the backing storage of this file and mark it as deleted.
    ///
    /// Only safe if no further reads or writes will be performed to the file.
    pub fn delete(self) -> Result<(), DeleteFileContentError> {
        unsafe { self.internal_delete() }
    }

    /// Check if the backing storage for the file is completely zeroed out
    ///
    /// A valid file should never be zeroed, so I marked this as unsafe
    pub unsafe fn erased(&self) -> bool {
        let metadata_slice = FileMetadata::as_bytes(self.metadata);
        if metadata_slice.iter().any(|i| *i != 0xff) {
            return false;
        }
        if self.content.iter().any(|i| *i != 0xff) {
            return false;
        }
        return true;
    }
}

impl<T: Storage + 'static, const STATE: FileContentState> Debug for FileContent<T, STATE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContent")
            // .field("content", &self.content)
            .field("metadata", &self.metadata)
            .field("info", &self.info)
            .finish()
    }
}

impl<T: Storage + 'static> Deref for FileContent<T, { FileContentState::Reader }> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        return self.content;
    }
}

impl<T: Storage + 'static> PartialEq<Self> for FileContent<T, { FileContentState::Reader }> {
    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }

    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
}

impl<T: Storage + 'static> Clone for FileContent<T, { FileContentState::Reader }> {
    fn clone(&self) -> Self {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        info.reader_count += 1;
        Self {
            content: self.content,
            metadata: self.metadata,
            info: self.info.clone(),
        }
    }
}

impl<T: Storage + 'static> Clone for FileContent<T, { FileContentState::Weak }> {
    fn clone(&self) -> Self {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };
        info.weak_count += 1;
        Self {
            content: self.content,
            metadata: self.metadata,
            info: self.info.clone(),
        }
    }
}

impl<T: Storage + 'static, const STATE: FileContentState> Drop for FileContent<T, STATE> {
    fn drop(&mut self) {
        let mut info = unsafe { self.info.as_ref().write().unwrap() };

        if STATE == { FileContentState::Weak } {
            info.weak_count -= 1;
            return;
        }
        if STATE == { FileContentState::Writer } {
            info.writer_count = info.writer_count.saturating_sub(1);
        }
        if STATE == { FileContentState::Reader } {
            info.reader_count = info.reader_count.saturating_sub(1);
        }

        if info.reader_count != 0 || info.writer_count != 0 {
            return;
        }

        let weak_count = info.weak_count;
        let has_been_deleted = info.has_been_deleted;
        drop(info);
        if has_been_deleted == false && self.metadata.marked_for_deletion() {
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

impl<T: Storage + 'static + Send + Sync> Seek for FileContent<T, { FileContentState::Writer }> {
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
            SeekFrom::Start(offset) => offset.try_into().unwrap_or(std::u32::MAX).clamp(0, length),
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
        return Ok(*current_offset as u64);
    }
}

impl<T: Storage + 'static + Send + Sync> Write for FileContent<T, { FileContentState::Writer }> {
    /// The same as [std::io::Write::write] but you can only flip bits from 0 to 1
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
            .map_err(|e| std::io::Error::other(e))?;
        info.current_offset += write_length;
        return Ok(write_length as usize);
    }

    fn flush(&mut self) -> std::io::Result<()> {
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::{get_test_storage, SimulatedStorage};

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
            100
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

    fn call_new() -> FileContent<SimulatedStorage, { FileContentState::Reader }> {
        let (storage, content, metadata) = get_backing();
        let content = FileContent::<_, { FileContentState::Reader }>::new(
            content,
            metadata,
            storage,
            0,
            |_| (),
        );
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
        let content1 = FileContent::<_, { FileContentState::Reader }>::new(
            content1,
            metadata1,
            storage1,
            0,
            |_| (),
        )
        .unwrap();
        let (storage2, content2, metadata2) = get_backing();
        let content2 = FileContent::<_, { FileContentState::Reader }>::new(
            content2,
            metadata2,
            storage2,
            0,
            |_| (),
        )
        .unwrap();
        let (storage3, content3, metadata3) = get_backing();
        content3[1] = 17;
        let content3 = FileContent::<_, { FileContentState::Reader }>::new(
            content3,
            metadata3,
            storage3,
            0,
            |_| (),
        )
        .unwrap();
        assert_eq!(content1, content2);
        assert_ne!(content2, content3);
    }

    #[test]
    fn cloning_works() {
        let (storage, content, metadata) = get_backing();
        content[3] = 17;
        let content = FileContent::<_, { FileContentState::Reader }>::new(
            content,
            metadata,
            storage,
            0,
            |_| (),
        )
        .unwrap();
        let cloned_content = content.clone();
        assert_eq!(content, cloned_content);
    }

    #[test]
    fn is_last_works() {
        let content = call_new();

        assert!(FileContent::is_last(&content));
        let other_content = content.clone();
        assert!(!FileContent::is_last(&content));
        assert!(!FileContent::is_last(&other_content));
        drop(content);
        assert!(FileContent::is_last(&other_content));
    }

    #[test]
    fn downgrading_works() {
        let content = call_new();
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        drop(weak_content);
        assert!(FileContent::is_last(&content));
    }

    #[test]
    fn upgrading_works() {
        let content = call_new();
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        let upgraded_content = weak_content.upgrade().unwrap();
        assert!(!FileContent::is_last(&content));
        assert!(!FileContent::is_last(&upgraded_content));
        drop(content);
        assert!(FileContent::is_last(&upgraded_content));
    }

    #[test]
    fn upgrading_works_even_if_there_are_no_readers_left() {
        let content = call_new();
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        drop(content);
        weak_content.upgrade().unwrap();
    }

    #[test]
    fn upgrading_does_not_work_when_reader_was_marked_for_deletion() {
        let content = call_new();
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        content.mark_for_deletion().unwrap();
        drop(content);
        let None = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }

    #[test]
    fn upgrading_does_not_work_when_weak_was_marked_for_deletion() {
        let content = call_new();
        assert!(FileContent::is_last(&content));
        let weak_content = content.downgrade();
        assert!(FileContent::is_last(&content));
        drop(content);
        weak_content.mark_for_deletion().unwrap();
        let None = weak_content.upgrade() else {
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
        FileContent::mark_for_deletion(&content).unwrap();
        let None = content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
        let None = weak_content.upgrade() else {
            panic!("Should not be able to upgrade when there are no strong references left");
        };
    }
}
