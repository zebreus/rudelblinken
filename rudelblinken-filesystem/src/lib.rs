//! A zero-copy flash filesystem optimized for embedded systems
//!
//! `rudelblinken-filesystem` implements a flash-friendly filesystem designed for resource-constrained
//! embedded devices. Key features include:
//!
//! - **Zero-copy access**: Files are memory-mapped for direct, efficient access
//! - **Flash-optimized**: Implements wear leveling and flash-aware write patterns  
//! - **Safe concurrency**: Reference counting enables safe concurrent access with reader/writer separation
//! - **Resource efficient**: Minimal RAM overhead and no dynamic allocation during normal operation
//! - **Reliable**: Two-phase commits and deferred deletion ensure data integrity
//!
//! The filesystem provides direct memory-mapped access to file contents while maintaining safety
//! through a custom reference counting system. Multiple readers can access files concurrently
//! while writers get exclusive access. Files are only deleted once all references are dropped.
//!
//! Designed specifically for flash storage, the implementation uses block-aligned operations,
//! respects write limitations, and implements basic wear leveling.
//!
#![feature(adt_const_params)]
#![feature(box_as_ptr)]
#![feature(box_vec_non_null)]
#![feature(allocator_api)]

use file::CommitFileContentError;
use file::File;
use file::FileState;
use file::WriteFileToStorageError;
use file_information::FileInformation;
use file_metadata::FileMetadata;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Bound::Included;
use storage::EraseStorageError;
use storage::Storage;
use thiserror::Error;
mod file;
mod file_information;
mod file_metadata;
pub mod storage;

#[derive(Error, Debug, Clone)]
pub enum FindFreeSpaceError {
    #[error("Error in filesystem structure")]
    FilesystemError,
    #[error("No free space")]
    NoFreeSpace,
    #[error("Not enough space")]
    NotEnoughSpace,
}

#[derive(Error, Debug)]
pub enum FilesystemWriteError {
    #[error(transparent)]
    FindFreeSpaceError(#[from] FindFreeSpaceError),
    #[error(transparent)]
    WriteFileToStorageError(#[from] WriteFileToStorageError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    CommitFileContentError(#[from] CommitFileContentError),
}

#[derive(Error, Debug)]
pub enum FilesystemDeleteError {
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
    #[error("The file does not exist")]
    FileNotFound,
}

///  A struct representing the filesystem backed by a generic storage type `T`.
///
/// # Type Parameters
///
/// * `T` - A type that implements the `Storage` trait and is `'static`, `Send`, and `Sync`.
pub struct Filesystem<T: Storage + 'static + Send + Sync> {
    storage: &'static T,
    files: Vec<FileInformation<T>>,
}

///
/// # Methods
///
/// * `get_first_block` - Retrieves the first block number from the storage metadata.
/// * `set_first_block` - Sets the first block number in the storage metadata.
/// * `new` - Initializes a new `Filesystem` instance, reading existing files from storage.
/// * `get_storage` - Returns a reference to the underlying storage.
/// * `read_file` - Reads a file by name and returns an optional `File` object.
/// * `find_free_space` - Finds a free space in storage of at least the given length.
/// * `write_file` - Writes a file with the given name, content, and hash.
/// * `get_file_writer` - Returns a writer for writing a file over time.
/// * `delete_file` - Deletes a file by name, marking it for deletion if necessary.
/// * `cleanup_files` - Removes all files with no remaining strong pointers.
///
/// # File System Layout
/// The filesystem maintains:
/// - A "first_block" metadata entry for circular buffer management
/// - Files stored sequentially in blocks, each with metadata header
impl<T: Storage + 'static + Send + Sync> Filesystem<T> {
    /// Retrieves the first block number from the storage metadata.
    fn get_first_block(&self) -> Result<u16, std::io::Error> {
        let first_block_slice: Box<[u8; 2]> = self
            .storage
            .read_metadata(&"first_block")?
            .try_into()
            .unwrap();
        return Ok(u16::from_le_bytes(*first_block_slice));
    }
    /// Sets the first block number in the storage metadata.
    fn set_first_block(&mut self, first_block: u16) -> Result<(), std::io::Error> {
        self.storage
            .write_metadata(&"first_block", &first_block.to_le_bytes())?;
        return Ok(());
    }

    /// Creates a new filesystem instance on top of the provided storage.
    ///
    /// # Initialization Process
    /// 1. Reads or initializes the first block pointer from metadata
    /// 2. Scans through blocks starting at first_block
    /// 3. Reconstructs file list from valid file headers
    /// 4. Erases corrupted blocks (non-0xFF when invalid)
    ///
    /// # Arguments
    /// * `storage` - Static reference to storage implementing the Storage trait
    ///
    /// # Returns
    /// A new `Filesystem` instance with the reconstructed file list
    pub fn new(storage: &'static T) -> Self {
        let mut filesystem = Self {
            storage,
            files: Vec::new(),
        };
        let first_block = filesystem.get_first_block();
        let first_block = first_block.unwrap_or_else(|_| {
            filesystem.set_first_block(0).unwrap();
            0
        });

        let mut block_number = 0;

        while block_number < T::BLOCKS {
            let current_block_number = (block_number + first_block as u32) % T::BLOCKS;
            let file_information = FileInformation::from_storage(
                filesystem.storage,
                current_block_number * T::BLOCK_SIZE,
            );
            let file_information = match file_information {
                Ok(file_information) => file_information,
                Err(_) => {
                    block_number += 1;
                    let Ok(current_block) = filesystem
                        .storage
                        .read(current_block_number * T::BLOCK_SIZE, T::BLOCK_SIZE)
                    else {
                        continue;
                    };
                    if current_block.iter().any(|b| *b != 0xff) {
                        println!(
                            "Erasing block {} because it is not zeroed",
                            current_block_number
                        );
                        filesystem
                            .storage
                            .erase(current_block_number * T::BLOCK_SIZE, T::BLOCK_SIZE)
                            .unwrap();
                    };
                    continue;
                }
            };
            if file_information.deleted() {}
            block_number += ((file_information.length + 64) / T::BLOCK_SIZE) + 1;
            filesystem.files.push(file_information);
        }

        return filesystem;
    }

    /// Finds a file by name and returns a reference to it.
    pub fn read_file(&self, name: &str) -> Option<File<T, { FileState::Weak }>> {
        let file = self
            .files
            .iter()
            .find(|file| file.name == name && file.valid())?;
        return Some(file.read());
    }

    /// Find a free space in storage of at least the given length.
    ///
    /// For now the space is guaranteed to start at a block boundary
    fn find_free_space(&self, length: u32) -> Result<u32, FindFreeSpaceError> {
        let mut free_ranges: BTreeMap<u16, u16> = Default::default();
        free_ranges.insert(0, T::BLOCKS as u16 * 2);

        for file in &self.files {
            let start_block = (file.address / T::BLOCK_SIZE) as u16;
            let length_in_blocks =
                (file.length + size_of::<FileMetadata>() as u32).div_ceil(T::BLOCK_SIZE) as u16;
            let end_block = start_block + length_in_blocks;

            let Some((&surrounding_start, &surrounding_length)) = free_ranges
                .range((Included(0), Included(start_block)))
                .last()
            else {
                // There should always be a surrounding free range
                return Err(FindFreeSpaceError::FilesystemError);
            };

            let space_before = start_block - surrounding_start;
            let space_after = (surrounding_start + surrounding_length) - (end_block);

            match (space_before, space_after) {
                (0, 0) => {
                    free_ranges.remove(&surrounding_start);
                }
                (0, space_after) => {
                    free_ranges.remove(&surrounding_start);
                    free_ranges.insert(end_block, space_after);
                }
                (space_before, 0) => {
                    free_ranges.insert(start_block, space_before);
                }
                (space_before, space_after) => {
                    free_ranges.insert(start_block, space_before);
                    free_ranges.insert(end_block, space_after);
                }
            }
        }

        // Fix the last entry for wraparound
        let last_free_space_start = free_ranges.last_key_value().map_or(0, |(start, _)| *start);
        let wraparound_length: i64 = last_free_space_start as i64 - T::BLOCKS as i64;
        if wraparound_length >= 0 {
            let wraparound_length = wraparound_length as u16;
            let Some((0, &first_range_length)) = free_ranges.first_key_value() else {
                // If there is wraparound on the last file, there needs to be enough space at the start of the storage to accomodate that overlap
                return Err(FindFreeSpaceError::FilesystemError);
            };
            free_ranges.remove(&0);
            let new_first_range_length = first_range_length - wraparound_length;
            if new_first_range_length > 0 {
                free_ranges.insert(wraparound_length, new_first_range_length);
            }
            free_ranges.insert(
                last_free_space_start,
                T::BLOCKS as u16 - last_free_space_start,
            );
        }

        let Some(longest_range) = free_ranges
            .into_iter()
            .max_by(|(_, length_a), (_, length_b)| length_a.cmp(length_b))
            .map(|(a, b)| (a as u32, b as u32))
        else {
            return Err(FindFreeSpaceError::NoFreeSpace);
        };

        if (longest_range.1 * T::BLOCK_SIZE) < length {
            return Err(FindFreeSpaceError::NotEnoughSpace);
        }

        let longest_range_start = longest_range.0 % (T::BLOCKS);

        return Ok(longest_range_start * T::BLOCK_SIZE);
    }

    pub fn write_file(
        &mut self,
        name: &str,
        content: &[u8],
        _hash: &[u8; 32],
    ) -> Result<(), FilesystemWriteError> {
        let mut writer = self.get_file_writer(name, content.len() as u32, _hash)?;

        writer.write_all(content)?;
        writer.commit()?;
        return Ok(());
    }

    /// Get a writer that allows writing a file over time.
    ///
    /// The file can only be read after the content was finished
    pub fn get_file_writer(
        &mut self,
        name: &str,
        length: u32,
        _hash: &[u8; 32],
    ) -> Result<File<T, { FileState::Writer }>, FilesystemWriteError> {
        self.cleanup_files();
        let free_location = self.find_free_space(length + size_of::<FileMetadata>() as u32)?;

        let (file, writer) =
            FileInformation::to_storage(self.storage, free_location, length, name)?;
        self.files.push(file);
        Ok(writer)
    }
    /// Delete a file
    ///
    /// The file will only be deleted once there are no strong references to its content left. Strong references can be obtained by calling upgrade on the content of a file
    pub fn delete_file(&mut self, filename: &str) -> Result<(), FilesystemDeleteError> {
        let Some((index, _)) = self
            .files
            .iter()
            .enumerate()
            .find(|(_, file)| file.name == filename)
        else {
            return Err(FilesystemDeleteError::FileNotFound);
        };
        let file = &mut self.files[index];
        if !file.marked_for_deletion() {
            file.mark_for_deletion().unwrap();
        }
        if file.deleted() {
            self.files.swap_remove(index);
        }
        return Ok(());
    }

    /// Remove all files with no remaining strong pointers
    fn cleanup_files(&mut self) {
        let mut remove_indices: Vec<usize> = Vec::new();
        for index in 0..self.files.len() {
            if self.files[index].deleted() {
                remove_indices.push(index);
            }
        }
        for index in remove_indices.into_iter().rev() {
            self.files.swap_remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::SimulatedStorage;

    use super::*;

    #[test]
    fn writing_and_reading_a_simple_file_works() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn can_read_a_file_from_an_old_storage() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut filesystem = Filesystem::new(storage);
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let filesystem = Filesystem::new(storage);
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn writing_multiple_files() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
        let result = filesystem.read_file("fancy2").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_works() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.delete_file("fancy").unwrap();
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };
    }

    #[test]
    fn deleting_a_file_actually_works() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.delete_file("fancy").unwrap();
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };

        let filesystem = Filesystem::new(storage);
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };
    }

    #[test]
    fn file_cant_be_upgraded_if_it_has_been_deleted_and_there_are_only_weak_references() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let content = vec![0; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem
            .write_file("fancy", &content, &[0u8; 32])
            .unwrap();
        let file = filesystem.read_file("fancy").unwrap();
        let weak_ref = file;
        filesystem.delete_file("fancy").unwrap();
        let None = weak_ref.upgrade() else {
            panic!("Should not be able to upgrade deleted file");
        };
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };
    }

    #[test]
    fn no_new_references_can_be_created_to_a_file_marked_for_deletion() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let content = vec![0; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem
            .write_file("fancy", &content, &[0u8; 32])
            .unwrap();
        let file = filesystem.read_file("fancy").unwrap();
        let strong_ref = file.upgrade().unwrap();
        filesystem.delete_file("fancy").unwrap();
        let None = filesystem.read_file("fancy").unwrap().upgrade() else {
            panic!(
                "Should not be able to create a new reference to a file marked for deletion file"
            );
        };
        // Strong ref still has the same correct content
        assert_eq!(strong_ref.as_ref(), content);
    }

    #[test]
    fn writing_a_maximum_size_file_works() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_makes_space_for_a_new_file() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.delete_file("fancy").unwrap();
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy2").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_does_not_make_space_for_a_new_file_if_there_are_still_strong_references_to_its_content(
    ) {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let fancy_file = filesystem.read_file("fancy").unwrap();
        let strong_ref = fancy_file.upgrade().unwrap();
        filesystem.delete_file("fancy").unwrap();
        let Err(_) = filesystem.write_file("fancy2", &file, &[0u8; 32]) else {
            panic!("Should fail because the file was not yet deleted");
        };
        assert_eq!(strong_ref.as_ref(), file);
        drop(strong_ref);
        // Should work now, because we dropped the last strong reference
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
    }

    #[test]
    fn writing_a_file_thats_too_big_fails() {
        let owned_storage = SimulatedStorage::new().unwrap();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize + 1];
        let Err(_) = filesystem.write_file("fancy", &file, &[0u8; 32]) else {
            panic!("Should fail when there is not enough space");
        };
    }
}
