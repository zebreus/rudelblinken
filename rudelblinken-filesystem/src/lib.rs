//! A minimalistic zero-copy flash filesystem optimized for the ESP32-C3
//!
//! `rudelblinken-filesystem` implements a flash-friendly filesystem designed for resource-constrained
//! embedded devices. Key features include:
//!
//! - **Zero-copy access**: Files are memory-mapped for direct, efficient access
//! - **Flash-optimized**: Implements wear leveling and flash-aware write patterns
//! - **Safe concurrency**: Reference counting enables safe concurrent access with reader/writer separation. Deferred deletion ensure data integrity
//! - **Resource efficient**: Minimal RAM overhead during normal operation
//!
//! The filesystem provides direct memory-mapped access to file contents while maintaining safety
//! through a custom reference counting system. Multiple readers can access files concurrently
//! while writers get exclusive access. Files are only deleted once all references are dropped.
//!
//! Designed specifically for flash storage, the implementation uses block-aligned operations,
//! respects write limitations, and implements basic wear leveling.
//!
//! # Additional features
//!
//! ## Autodeleting unimportant files
// TODO: Improve documentation and move it somewhere else
//!
//! When you want to create a new file, but there is not enough space, the filesystem will automatically delete the oldest unimportant files to make space.
//! Unimportant files are files that have not explicitly been marked as important.
//!
//! The age of a file is determined by the number of ticks and reboots since it was created. It can be a number between 0 and 15. A file with age 16 has just been created, while a file with age 1 is the oldest file. Every reboot increases the age of all files by 1. You can manually call the tick method to age all files.
//! Files with age 16 require 1 tick to go to 15. Files with age 15 require 2 ticks to go to 14. Files with age 14 require 3 ticks. The recommended tick rate is once per minute.
#![warn(missing_docs)]
#![allow(static_mut_refs)]
#![feature(adt_const_params)]
#![feature(box_as_ptr)]
#![feature(box_vec_non_null)]
#![feature(allocator_api)]
#![feature(doc_cfg)]
#[cfg_attr(
    feature = "simulated",
    doc = r##"

# Examples

Create an in-memory filesystem for testing:

```
use rudelblinken_filesystem::storage::simulated::SimulatedStorage;
use rudelblinken_filesystem::Filesystem;

let storage = SimulatedStorage::new();
// TODO: Improve the interface to allow storages with lifetimes
let static_storage_ref = unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&storage) };
let mut filesystem = Filesystem::new(static_storage_ref);
```
"##
)]
use file::{CommitFileContentError, File, FileState, WriteFileToStorageError};
use file_information::FileInformation;
use file_metadata::FileMetadata;
use std::{
    collections::{BTreeMap, VecDeque},
    io::Write,
    ops::Bound::Included,
    u16,
};
use storage::{EraseStorageError, Storage};
use thiserror::Error;

/// [file::File] provides a safe interface to read and write files.
pub mod file;
mod file_information;
mod file_metadata;
/// Storage traits and implementations
pub mod storage;

/// Errors that can occur when finding free space
#[derive(Error, Debug, Clone)]
pub enum FindFreeSpaceError {
    /// Error in filesystem structure
    #[error("Error in filesystem structure")]
    FilesystemError,
    /// No free space
    #[error("No free space")]
    NoFreeSpace,
    /// Not enough space
    #[error("Not enough space")]
    NotEnoughSpace,
}

/// Errors that can occur when writing a file
#[derive(Error, Debug)]
pub enum FilesystemWriteError {
    /// Error while finding free space
    #[error(transparent)]
    FindFreeSpaceError(#[from] FindFreeSpaceError),
    /// Error while writing file to storage
    #[error(transparent)]
    WriteFileToStorageError(#[from] WriteFileToStorageError),
    /// Some kind of io error
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    /// Error while committing file content
    #[error(transparent)]
    CommitFileContentError(#[from] CommitFileContentError),
    /// There already exists a file with that name. Delete it first
    #[error("There already exists a file with that name. Delete it first")]
    NameAlreadyTaken,
}

/// Errors that can occur when deleting a file
#[derive(Error, Debug)]
pub enum FilesystemDeleteError {
    /// Error while erasing storage
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
    /// Some kind of io error
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    /// The file does not exist
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Importance {
    Free,
    Unimportant { age: u8 },
    Important,
}

impl Importance {
    fn get_cost(&self) -> Option<u8> {
        return match self {
            Importance::Free => Some(0),
            Importance::Unimportant { age } => Some(16 - age),
            Importance::Important => None,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
    importance: Importance,
    length: u16,
}

impl<T: Storage + 'static + Send + Sync> Filesystem<T> {
    /// Retrieves the first block number from the storage metadata.
    fn get_first_block(&self) -> Result<u16, std::io::Error> {
        let first_block_slice: Box<[u8; 2]> = self
            .storage
            .read_metadata("first_block")?
            .try_into()
            .unwrap();
        Ok(u16::from_le_bytes(*first_block_slice))
    }
    /// Sets the first block number in the storage metadata.
    fn set_first_block(&self, first_block: u16) -> Result<(), std::io::Error> {
        self.storage
            .write_metadata("first_block", &first_block.to_le_bytes())?;
        Ok(())
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
        // Create a fs with an empty files table
        let mut filesystem = Self {
            storage,
            files: Vec::new(),
        };

        // Find all files
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
            block_number += ((file_information.length + 64) / T::BLOCK_SIZE) + 1;
            filesystem.files.push(file_information);
        }

        unsafe { filesystem.selfcheck() };

        filesystem
    }

    /// Check the filesystem for errors and try to fix them
    ///
    /// Only safe, if none of the files have been read yet. This should only be called in new.
    unsafe fn selfcheck(&mut self) {
        // Fix the first block number, if the first file is marked for deletion or deleted
        if let Some(first_file) = self.files.first() {
            if first_file.marked_for_deletion() || first_file.deleted() {
                let new_first_block = self.find_new_first_block();
                self.set_first_block(new_first_block).unwrap();
            }
        }

        // TODO: Find and remove files with a duplicate name

        // TODO: Cleanup
    }

    /// Finds a file by name and returns a reference to it.
    pub fn read_file(&self, name: &str) -> Option<File<T, { FileState::Weak }>> {
        let file = self.files.iter().find(|file| {
            file.name == name && !file.marked_for_deletion() && !file.deleted() && file.valid()
        })?;
        Some(file.read())
    }

    /// Finds a file by name and returns a reference to it.
    pub fn read_file_by_hash(&self, hash: &[u8; 32]) -> Option<File<T, { FileState::Weak }>> {
        let file = self.files.iter().find(|file| {
            file.compare_hash(hash)
                && !file.marked_for_deletion()
                && !file.deleted()
                && file.valid()
        })?;
        Some(file.read())
    }

    /// Get information about the free space in the storage
    fn analyze_free_space(&self) -> Result<BTreeMap<u16, Range>, FindFreeSpaceError> {
        let mut free_ranges: BTreeMap<u16, Range> = Default::default();
        free_ranges.insert(
            0,
            Range {
                importance: Importance::Free,
                length: T::BLOCKS as u16 * 2,
            },
        );

        for file in &self.files {
            let file_importance = if file.important() || !file.can_be_deleted() {
                Importance::Important
            } else {
                Importance::Unimportant { age: file.age() }
            };

            let start_block = (file.address / T::BLOCK_SIZE) as u16;
            let length_in_blocks =
                (file.length + size_of::<FileMetadata>() as u32).div_ceil(T::BLOCK_SIZE) as u16;
            let end_block = start_block + length_in_blocks;

            let Some((
                &surrounding_start,
                &Range {
                    length: surrounding_length,
                    importance: surrounding_importance,
                },
            )) = free_ranges
                .range((Included(0), Included(start_block)))
                .last()
            else {
                // There should always be a surrounding free range
                return Err(FindFreeSpaceError::FilesystemError);
            };

            let space_before = start_block - surrounding_start;
            let space_after = (surrounding_start + surrounding_length) - (end_block);

            if space_before != 0 {
                free_ranges.insert(
                    surrounding_start,
                    Range {
                        importance: surrounding_importance,
                        length: space_before,
                    },
                );
            }
            free_ranges.insert(
                surrounding_start + space_before,
                Range {
                    importance: file_importance,
                    length: length_in_blocks,
                },
            );
            if space_after != 0 {
                free_ranges.insert(
                    surrounding_start + space_before + length_in_blocks,
                    Range {
                        importance: surrounding_importance,
                        length: space_after,
                    },
                );
            }
        }

        // Remove all trailing free space
        let last_free_space_start = free_ranges.last_key_value().map_or(0, |(start, _)| *start);
        let wraparound_length: i64 = last_free_space_start as i64 - T::BLOCKS as i64;

        // Remove the free space that is occupied by the wraparound from the first block
        if wraparound_length > 0 {
            let first_entry = free_ranges.first_key_value().unwrap();
            if Importance::Free == first_entry.1.importance {
                panic!("In case of wraparound, the first entry should always be free");
            }
            if first_entry.1.length < wraparound_length as u16 {
                panic!("In case of wraparound, the first entry should always be large enough to accommodate the wraparound");
            }
            free_ranges.insert(wraparound_length as u16, first_entry.1.clone());
            free_ranges.remove(&0);
        }

        // Remove the free space in the end
        if wraparound_length >= 0 {
            free_ranges.remove(&last_free_space_start);
        } else {
            let end_space = free_ranges.last_key_value().unwrap();
            free_ranges.insert(
                *end_space.0,
                Range {
                    importance: end_space.1.importance,
                    length: end_space.1.length - T::BLOCKS as u16,
                },
            );
        }

        // Now the only overlap is the wrapping section

        // Duplicate all ranges to the back
        for range in free_ranges.clone().into_iter() {
            free_ranges.insert(range.0 + T::BLOCKS as u16, range.1);
        }

        return Ok(free_ranges);
    }

    /// Find a free space in storage of at least the given length.
    ///
    /// For now the space is guaranteed to start at a block boundary
    fn find_free_space(&self, length: u32) -> Result<u32, FindFreeSpaceError> {
        let free_ranges = self.analyze_free_space()?;

        for range in free_ranges.iter() {
            println!("Free range: {:?}", range);
        }

        let length_in_blocks = length.div_ceil(T::BLOCK_SIZE) as u16;

        if let Some((free_range_start, free_range_length)) = free_ranges
            .iter()
            .filter(|(&start, _)| start < T::BLOCKS as u16)
            .filter(|(_, range)| range.importance == Importance::Free)
            .filter(|(_, range)| range.length >= (length_in_blocks))
            .min_by(|(_, range_a), (_, range_b)| range_a.length.cmp(&range_b.length))
            .map(|(a, b)| (*a as u32, b.length as u32))
        {
            // let longest_range_start = longest_range.0 % (T::BLOCKS);
            println!(
                "Found free space at {} with length {}",
                free_range_start, free_range_length
            );
            return Ok(free_range_start * T::BLOCK_SIZE);
        }
        // println!("No unused free space found");

        let mut cheapest_range: VecDeque<(u16, Range)> = VecDeque::new();
        let mut cheapest_range_cost: u16 = u16::MAX;
        let mut current_range: VecDeque<(u16, Range)> = VecDeque::new();
        let mut current_range_cost: u16 = 0;
        let mut current_range_length: u16 = 0;
        for (check_start, check_range) in free_ranges.iter() {
            let Some(cost) = check_range.importance.get_cost() else {
                // println!("Skipping important range {:?}", check_range);
                continue;
            };
            let current_start = current_range.front().map_or(0, |front| front.0);
            if current_start + current_range_length != *check_start {
                current_range.clear();
                current_range_cost = 0;
                current_range_length = 0;
            }
            current_range.push_back((*check_start, *check_range));
            current_range_cost += cost as u16;
            current_range_length += check_range.length;
            // println!(
            //     "Current range: {:?}, cost: {}, length: {}",
            //     current_range, current_range_cost, current_range_length
            // );

            if current_range_length >= length_in_blocks {
                // Try to remove from start of the current range, until it is shortest
                loop {
                    let first_range_length = current_range.front().unwrap().1.length;
                    if current_range_length - first_range_length < length_in_blocks {
                        break;
                    }
                    let removed = current_range.pop_front().unwrap();
                    let removed_cost = removed.1.importance.get_cost().unwrap();
                    current_range_cost -= removed_cost as u16;
                    current_range_length -= removed.1.length;
                }
            }
            if let Some(front) = current_range.front() {
                if front.0 >= T::BLOCKS as u16 {
                    break;
                }
            }

            if current_range_length >= length_in_blocks && current_range_cost < cheapest_range_cost
            {
                cheapest_range = current_range.clone();
                cheapest_range_cost = current_range_cost;
            }
        }

        if cheapest_range_cost == u16::MAX {
            return Err(FindFreeSpaceError::NotEnoughSpace);
        }

        for range in cheapest_range.iter() {
            println!("Cheapest range: {:?}", range);
            let matched_file = self
                .files
                .iter()
                .find(|f| f.address == range.0 as u32 * T::BLOCK_SIZE);

            if let Some(file) = matched_file {
                file.mark_for_deletion().unwrap();
                if !file.deleted() {
                    eprintln!("File should have been deleted");
                    panic!("File should have been deleted");
                }
            }
        }

        let first = cheapest_range.front().unwrap();
        let start = first.0 as u32 * T::BLOCK_SIZE;
        println!("Found unimportant space at {}", start);
        return Ok(start);

        // todo!("Clear cheapest range and return it");
        // if let Some((free_range_start, free_range_length)) = free_ranges
        //     .iter()
        //     .filter(|(_, range)| range.importance != Importance::Important)
        //     .filter(|(_, range)| range.length >= (length.div_ceil(T::BLOCK_SIZE) as u16))
        //     .min_by(|(_, range_a), (_, range_b)| range_a.length.cmp(&range_b.length))
        //     .map(|(a, b)| (*a as u32, b.length as u32))
        // {
        //     // let longest_range_start = longest_range.0 % (T::BLOCKS);
        //     println!(
        //         "Found free space at {} with length {}",
        //         free_range_start, free_range_length
        //     );
        //     return Ok(free_range_start * T::BLOCK_SIZE);
        // }
        // return Err(FindFreeSpaceError::NotEnoughSpace);
    }

    /// Write a file to storage.
    pub fn write_file(
        &mut self,
        name: &str,
        content: &[u8],
        _hash: &[u8; 32],
    ) -> Result<(), FilesystemWriteError> {
        let mut writer = self.get_file_writer(name, content.len() as u32, _hash)?;

        writer.write_all(content)?;
        writer.commit()?;
        Ok(())
    }

    /// Get a writer that allows writing a file over time.
    ///
    /// The file can only be read after the content was finished
    pub fn get_file_writer(
        &mut self,
        name: &str,
        length: u32,
        hash: &[u8; 32],
    ) -> Result<File<T, { FileState::Writer }>, FilesystemWriteError> {
        self.cleanup_files();
        if self
            .files
            .iter()
            .any(|file| !file.deleted() && !file.marked_for_deletion() && file.name == name)
        {
            return Err(FilesystemWriteError::NameAlreadyTaken);
        }
        let free_location = self.find_free_space(length + size_of::<FileMetadata>() as u32)?;

        let (file, writer) =
            FileInformation::to_storage(self.storage, free_location, length, name, hash)?;
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

        let file = &self.files[index];
        let file_block = (file.address / T::BLOCK_SIZE) as u16;
        let first_block = self.get_first_block().unwrap_or(0);
        if file.deleted() {
            self.files.swap_remove(index);
        }

        if file_block == first_block {
            let new_first_block = self.find_new_first_block() as u16;
            if new_first_block != first_block {
                self.set_first_block(new_first_block)?;
            }
        }

        Ok(())
    }

    fn find_new_first_block(&self) -> u16 {
        let good_file = self
            .files
            .iter()
            .find(|file| file.valid() && !file.deleted() && !file.marked_for_deletion());

        if let Some(file) = good_file {
            return (file.address / T::BLOCK_SIZE) as u16;
        }

        let acceptable_file = self
            .files
            .iter()
            .find(|file| file.valid() && !file.deleted());

        if let Some(file) = acceptable_file {
            return (file.address / T::BLOCK_SIZE) as u16;
        }

        let any_file = self.files.iter().find(|file| file.valid());

        if let Some(file) = any_file {
            return (file.address / T::BLOCK_SIZE) as u16;
        }

        return 0;
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
    use crate::storage::simulated::SimulatedStorage;

    use super::*;

    #[test]
    fn writing_and_reading_a_simple_file_works() {
        let owned_storage = SimulatedStorage::new();
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
        let owned_storage = SimulatedStorage::new();
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
    fn can_read_a_file_by_hash() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut filesystem = Filesystem::new(storage);
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.write_file("fancy2", &file, &[5u8; 32]).unwrap();
        filesystem.read_file_by_hash(&[0u8; 32]).unwrap();
        assert!(filesystem.read_file_by_hash(&[3u8; 32]).is_none());
        filesystem.read_file_by_hash(&[5u8; 32]).unwrap();
    }

    #[test]
    fn writing_multiple_files() {
        let owned_storage = SimulatedStorage::new();
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
    fn unimportant_files_get_deleted() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        // A bit bigger than half the storage size
        let file = vec![0u8; SimulatedStorage::SIZE as usize / 2 + 1 - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
        assert!(filesystem.read_file("fancy").is_none());
        let result = filesystem.read_file("fancy2").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn important_files_dont_get_deleted() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        // A bit bigger than half the storage size
        let file = vec![0u8; SimulatedStorage::SIZE as usize / 2 + 1 - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        result.set_important().unwrap();

        filesystem
            .write_file("fancy2", &file, &[0u8; 32])
            .unwrap_err();
    }

    #[test]
    fn open_reader_protects_files_from_being_deleted() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        // A bit bigger than half the storage size
        let file = vec![0u8; SimulatedStorage::SIZE as usize / 2 + 1 - size_of::<FileMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        let _strong_ref = result.upgrade().unwrap();

        filesystem
            .write_file("fancy2", &file, &[0u8; 32])
            .unwrap_err();
    }

    #[test]
    fn deleting_a_file_works() {
        let owned_storage = SimulatedStorage::new();
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
        let owned_storage = SimulatedStorage::new();
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
        let owned_storage = SimulatedStorage::new();
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
        let Err(_) = weak_ref.upgrade() else {
            panic!("Should not be able to upgrade deleted file");
        };
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };
    }

    #[test]
    fn no_new_references_can_be_created_to_a_file_marked_for_deletion() {
        let owned_storage = SimulatedStorage::new();
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
        let None = filesystem.read_file("fancy") else {
            panic!(
                "Should not be able to create a new reference to a file marked for deletion file"
            );
        };
        // Strong ref still has the same correct content
        assert_eq!(strong_ref.as_ref(), content);
    }

    #[test]
    fn writing_a_maximum_size_file_works() {
        let owned_storage = SimulatedStorage::new();
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
        let owned_storage = SimulatedStorage::new();
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
    fn can_write_a_big_file() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);

        let file = [42u8; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem
            .write_file("big_file", &file, &[0u8; 32])
            .unwrap();
        let result = filesystem.read_file("big_file").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn multiple_unimportant_files_can_get_overwritten() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);

        for i in 0..SimulatedStorage::BLOCKS {
            filesystem
                .write_file(&format!("small_file_{}", i), &[0; 32], &[0u8; 32])
                .unwrap();
        }

        let file = [42u8; SimulatedStorage::SIZE as usize - size_of::<FileMetadata>()];
        filesystem
            .write_file("big_file", &file, &[0u8; 32])
            .unwrap();
        let result = filesystem.read_file("big_file").unwrap();
        assert_eq!(result.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_does_not_make_space_for_a_new_file_if_there_are_still_strong_references_to_its_content(
    ) {
        let owned_storage = SimulatedStorage::new();
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
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize + 1];
        let Err(_) = filesystem.write_file("fancy", &file, &[0u8; 32]) else {
            panic!("Should fail when there is not enough space");
        };
    }

    #[test]
    fn deleting_the_first_file_moves_the_marker() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE as usize
            - size_of::<FileMetadata>()
            - SimulatedStorage::BLOCK_SIZE as usize * 2];
        filesystem.write_file("first", &file, &[0u8; 32]).unwrap();
        assert_eq!(filesystem.get_first_block().unwrap(), 0);
        let file2 = [0u8; SimulatedStorage::BLOCK_SIZE as usize - size_of::<FileMetadata>()];
        filesystem.write_file("second", &file2, &[0u8; 32]).unwrap();
        assert_eq!(filesystem.get_first_block().unwrap(), 0);
        filesystem.delete_file("first").unwrap();
        assert_ne!(filesystem.get_first_block().unwrap(), 0);
        // drop(filesystem);
        // let mut filesystem = Filesystem::new(storage);
    }

    #[test]
    fn can_not_create_two_files_with_the_same_name() {
        let owned_storage = SimulatedStorage::new();
        let storage =
            unsafe { std::mem::transmute::<_, &'static SimulatedStorage>(&owned_storage) };
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::BLOCK_SIZE as usize - size_of::<FileMetadata>()];
        filesystem.write_file("cool", &file, &[0u8; 32]).unwrap();
        filesystem
            .write_file("cool", &file, &[0u8; 32])
            .unwrap_err();
    }
}
