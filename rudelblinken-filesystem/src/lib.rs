use file_content::FileContent;
use std::collections::BTreeMap;
use std::ops::Bound::Included;
use std::sync::Arc;
use std::sync::RwLock;
use storage::EraseStorageError;
use storage::ReadStorageError;
use storage::Storage;
use storage::WriteStorageError;
use thiserror::Error;
use zerocopy::IntoBytes;
use zerocopy::{FromBytes, Immutable, KnownLayout};
pub mod file_content;
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
pub enum WriteFileError {
    #[error(transparent)]
    FindFreeSpaceError(#[from] FindFreeSpaceError),
    #[error("The filename can not be longer than 16 bytes")]
    FileNameTooLong,
    #[error(transparent)]
    CreateFileInformationError(#[from] CreateFileInformationError),
    #[error(transparent)]
    WriteStorageError(#[from] WriteStorageError),
    #[error("The read file does not match the written file")]
    ReadFileDoesNotMatch,
    #[error(transparent)]
    StorageLockError(#[from] StorageLockError),
}

#[derive(Error, Debug)]
pub enum DeleteFileError {
    #[error(transparent)]
    EraseStorageError(#[from] EraseStorageError),
    #[error("The file does not exist")]
    FileNotFound,
}

#[derive(Error, Debug)]
pub enum CreateFileInformationError {
    #[error(transparent)]
    ReadFileError(#[from] std::io::Error),
    #[error(transparent)]
    StorageLockError(#[from] StorageLockError),
    #[error(transparent)]
    ReadStorageError(#[from] ReadStorageError),
    #[error("Failed to read block metadata")]
    FailedToReadBlockMetadata(
        #[from]
        zerocopy::ConvertError<
            zerocopy::AlignmentError<&'static [u8], BlockMetadata>,
            zerocopy::SizeError<&'static [u8], BlockMetadata>,
            std::convert::Infallible,
        >,
    ),
    #[error("No metadata found because the block is empty")]
    NoMetadata,
}

#[derive(Error, Debug)]
pub enum StorageLockError {
    #[error("Failed to aquire a read lock to the underlying storage")]
    FailedToAquireReadLock,
    #[error("Failed to aquire a write lock to the underlying storage")]
    FailedToAquireWriteLock,
}

#[derive(Error, Debug)]
pub enum MetadataAccessError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    StorageLockError(#[from] StorageLockError),
}

#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq, KnownLayout)]
pub enum BlockType {
    /// If this is a wasm binary
    WASM,
    /// This file has been deleted
    DELETED,
}

#[derive(KnownLayout, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
struct BlockMetadata {
    /// Type of this block
    /// Access only via the supplied functions
    block_type: u16,
    /// Length in bits
    length: u16,
    /// SHA3-256 hash of the file
    hash: [u8; 32],
    /// Name of the file, null terminated or 16 chars
    name: [u8; 16],
    /// Reserved space to fill the metadata to 64 byte
    _reserved: [u8; 12],
}

impl BlockMetadata {
    fn is_empty(&self) -> bool {
        return self.block_type == 0;
    }
    fn length(&self) -> u16 {
        return self.length;
    }
    fn hash(&self) -> [u8; 32] {
        return self.hash;
    }
    fn name(&self) -> &str {
        let nul_range_end = self.name.iter().position(|&c| c == b'\0').unwrap_or(16);
        return std::str::from_utf8(&self.name[0..nul_range_end]).unwrap_or_default();
    }
}

pub struct File {
    // Content
    pub content: FileContent<false>,
    // Name
    pub name: String,
}

struct FileInformation {
    /// Block number
    location_in_storage: usize,
    /// Length in bytes
    length: usize,
    /// Name of the file
    name: String,
    /// Content of the file
    /// Will be None if the file has been deleted
    content: Option<FileContent<true>>,
    /// Weak pointer of
    weak_content: FileContent<false>,
    metadata: &'static BlockMetadata,
}

impl FileInformation {
    /// Return information about a new file
    ///
    /// None means that there is definitely no file starting there.
    /// If a file information is returned, there is no guarantee, that it is actually a real file.
    pub fn new<T: Storage + 'static>(
        storage: Arc<RwLock<T>>,
        location_in_storage: usize,
    ) -> Result<FileInformation, CreateFileInformationError> {
        let static_metadata_slice: &'static [u8];
        let static_content: &'static [u8];
        let metadata: &BlockMetadata;

        {
            let binding = storage
                .read()
                .map_err(|_| StorageLockError::FailedToAquireReadLock)?;
            let metadata_slice = binding.read(location_in_storage, size_of::<BlockMetadata>())?;
            // TODO: This is unsafe AF
            static_metadata_slice =
                unsafe { std::mem::transmute::<&[u8], &'static [u8]>(metadata_slice) };

            metadata = BlockMetadata::ref_from_bytes(static_metadata_slice)?;
            if metadata.is_empty() {
                return Err(CreateFileInformationError::NoMetadata);
            }

            let content = binding.read(
                location_in_storage + size_of::<BlockMetadata>(),
                metadata.length() as usize,
            )?;
            static_content = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(content) };
        }

        let full_file_length = metadata.length() as usize + size_of::<BlockMetadata>();

        let strong_content = FileContent::new(
            static_content.as_ptr(),
            static_content.len(),
            move |marked_for_deletion| {
                if marked_for_deletion {
                    let length = full_file_length.div_ceil(T::BLOCK_SIZE) * T::BLOCK_SIZE;

                    storage
                        .write()
                        .unwrap()
                        .erase(location_in_storage, length)
                        .unwrap();
                }
            },
        );

        let information = FileInformation {
            location_in_storage: location_in_storage,
            length: metadata.length() as usize,
            name: metadata.name().into(),
            metadata: metadata,
            weak_content: strong_content.downgrade(),
            content: Some(strong_content),
        };
        return Ok(information);
    }

    /// Check if there are no other references to the file left
    pub fn no_strong_references_left(&self) -> bool {
        return FileContent::strong_count(&self.weak_content) == 0;
    }

    pub fn into_file(&self) -> File {
        return File {
            content: self.weak_content.clone(),
            name: self.name.clone(),
        };
    }
}

// impl Drop for FileInformation {
//     fn drop(&mut self) {
//         println!("Dropping it");
//         if self.can_be_dropped().is_err() {
//             // TODO: Better implementation
//             panic!(
//                 "Can not drop FileInformation if there are still active references to its content"
//             );
//         }
//         println!("Panic check survived");

//         println!("Ref count before drop {}", FileContent::is_last(&content));
//         let content = self.content.clone();

//         println!("Clone survived");

//         // Get back the raw pointer and destroy the last reference in the process
//         let ptr = Rc::into_raw(old_content);
//         // let slice = ptr as &[u8]
//         println!("Dropped it");
//     }
// }

/// Filesystem implementation
pub struct Filesystem<T: Storage> {
    storage: Arc<RwLock<T>>,
    files: Vec<FileInformation>,
}

impl<T: Storage + 'static> Filesystem<T> {
    fn get_first_block(&self) -> Result<u16, MetadataAccessError> {
        let first_block_slice: [u8; 2] = self
            .storage
            .read()
            .map_err(|_| StorageLockError::FailedToAquireReadLock)?
            .read_metadata(&"first_block")?
            .try_into()
            .unwrap();
        return Ok(u16::from_le_bytes(first_block_slice));
    }
    fn set_first_block(&mut self, first_block: u16) -> Result<(), MetadataAccessError> {
        self.storage
            .write()
            .map_err(|_| StorageLockError::FailedToAquireWriteLock)?
            .write_metadata(&"first_block", &first_block.to_le_bytes())?;
        return Ok(());
    }

    pub fn new(storage: Arc<RwLock<T>>) -> Self {
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
            let current_block_number = (block_number + first_block as usize) % T::BLOCKS;
            let file_information = FileInformation::new(
                filesystem.storage.clone(),
                current_block_number * T::BLOCK_SIZE,
            );
            let Ok(file_information) = file_information else {
                block_number += 1;
                continue;
            };
            block_number += ((file_information.length + 64) / T::BLOCK_SIZE) + 1;
            filesystem.files.push(file_information);
        }

        return filesystem;
    }

    /// Get a reference to the underlying storage
    pub fn get_storage(self) -> Arc<RwLock<T>> {
        return self.storage.clone();
    }

    pub fn read_file(&self, name: &str) -> Option<File> {
        let file = self
            .files
            .iter()
            .find(|file| file.name == name && file.content.is_some())?;
        return Some(file.into_file());
    }

    /// Find a free space in storage of at least the given length.
    ///
    /// For now the space is guaranteed to start at a block boundary
    fn find_free_space(&self, length: usize) -> Result<usize, FindFreeSpaceError> {
        let mut free_ranges: BTreeMap<u16, u16> = Default::default();
        free_ranges.insert(0, T::BLOCKS as u16 * 2);
        // for file in &self.files {
        //     let in_range = free_ranges.lower_bound(bound)
        // }

        // let mut used_blocks = bitvec![u8, Msb0; 0; T::BLOCKS];
        // let mut used_blocks = vec![false; T::BLOCKS];
        for file in &self.files {
            let start_block = file.location_in_storage as u16;
            let length_in_blocks =
                (file.length + size_of::<BlockMetadata>()).div_ceil(T::BLOCK_SIZE) as u16;
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

            // for block in 0..length_in_blocks {
            //     used_blocks[(start_block + block) % T::BLOCKS] = true;
            // }
        }

        // Fix the last entry for wraparound
        let last_free_space_start = free_ranges.last_key_value().map_or(0, |(start, _)| *start);
        let wraparound_length = last_free_space_start.saturating_sub(T::BLOCKS as u16);
        if wraparound_length > 0 {
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
        else {
            return Err(FindFreeSpaceError::NoFreeSpace);
        };

        if (longest_range.1 as usize * T::BLOCK_SIZE) < length {
            return Err(FindFreeSpaceError::NotEnoughSpace);
        }

        // println!("Found the longest free range at {:?}", longest_range);

        return Ok(longest_range.0 as usize * T::BLOCK_SIZE);
    }

    pub fn write_file(
        &mut self,
        name: &str,
        content: &[u8],
        hash: &[u8; 32],
    ) -> Result<(), WriteFileError> {
        self.cleanup_files();
        let mut name_array = [0u8; 16];
        let name_bytes = name.as_bytes();
        if name_bytes.len() > 16 {
            return Err(WriteFileError::FileNameTooLong);
        }
        name_array[0..name_bytes.len()].copy_from_slice(name_bytes);
        let free_location = self.find_free_space(content.len() + size_of::<BlockMetadata>())?;

        let metadata = BlockMetadata {
            block_type: 1,
            length: content.len() as u16,
            hash: hash.clone(),
            name: name_array,
            _reserved: [0u8; 12],
        };
        {
            let mut writable_storage = self
                .storage
                .write()
                .map_err(|_| StorageLockError::FailedToAquireWriteLock)?;
            writable_storage.write(free_location, metadata.as_bytes())?;
            writable_storage.write(free_location + size_of::<BlockMetadata>(), content)?;
        }
        let file_data = FileInformation::new(self.storage.clone(), free_location)?;

        // Verify file
        if file_data.length != content.len() {
            return Err(WriteFileError::ReadFileDoesNotMatch);
        }
        if file_data.name != name {
            return Err(WriteFileError::ReadFileDoesNotMatch);
        }

        self.files.push(file_data);
        return Ok(());
    }

    /// Delete a file
    ///
    /// The file will only be deleted once there are no strong references to its content left. Strong references can be obtained by calling upgrade on the content of a file
    pub fn delete_file(&mut self, filename: &str) -> Result<(), DeleteFileError> {
        let Some((index, _)) = self
            .files
            .iter()
            .enumerate()
            .find(|(_, file)| file.name == filename)
        else {
            return Err(DeleteFileError::FileNotFound);
        };
        let file = &mut self.files[index];
        if let Some(content) = &file.content {
            FileContent::mark_for_deletion(content);
        }
        file.content = None;
        if file.no_strong_references_left() {
            self.files.swap_remove(index);
        }
        return Ok(());
    }

    /// Remove all files with no remaining strong pointers
    fn cleanup_files(&mut self) {
        let mut remove_indices: Vec<usize> = Vec::new();
        for index in 0..self.files.len() {
            if self.files[index].no_strong_references_left() {
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
    use storage::SimulatedStorage;

    use super::*;

    #[test]
    fn writing_and_reading_a_simple_file_works() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
        // print!("LELELELLELELE {}", result.length);
    }

    #[test]
    fn can_read_a_file_from_an_old_storage() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut filesystem = Filesystem::new(storage);
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let filesystem = Filesystem::new(filesystem.get_storage());
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn writing_multiple_files() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
        let result = filesystem.read_file("fancy2").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_works() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
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
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.delete_file("fancy").unwrap();
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };

        let filesystem = Filesystem::new(filesystem.get_storage());
        let None = filesystem.read_file("fancy") else {
            panic!("Should not be able to read a deleted file");
        };
    }

    #[test]
    fn file_cant_be_upgraded_if_it_has_been_deleted_and_there_are_only_weak_references() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let content = vec![0; SimulatedStorage::SIZE - size_of::<BlockMetadata>()];
        filesystem
            .write_file("fancy", &content, &[0u8; 32])
            .unwrap();
        let file = filesystem.read_file("fancy").unwrap();
        let weak_ref = file.content;
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
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let content = vec![0; SimulatedStorage::SIZE - size_of::<BlockMetadata>()];
        filesystem
            .write_file("fancy", &content, &[0u8; 32])
            .unwrap();
        let file = filesystem.read_file("fancy").unwrap();
        let strong_ref = file.content.upgrade().unwrap();
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
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE - size_of::<BlockMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_makes_space_for_a_new_file() {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE - size_of::<BlockMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        filesystem.delete_file("fancy").unwrap();
        filesystem.write_file("fancy2", &file, &[0u8; 32]).unwrap();
        let result = filesystem.read_file("fancy2").unwrap();
        assert_eq!(result.content.upgrade().unwrap().as_ref(), file);
    }

    #[test]
    fn deleting_a_file_does_not_make_space_for_a_new_file_if_there_are_still_strong_references_to_its_content(
    ) {
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE - size_of::<BlockMetadata>()];
        filesystem.write_file("fancy", &file, &[0u8; 32]).unwrap();
        let fancy_file = filesystem.read_file("fancy").unwrap();
        let strong_ref = fancy_file.content.upgrade().unwrap();
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
        let storage = Arc::new(RwLock::new(SimulatedStorage::new().unwrap()));
        let mut filesystem = Filesystem::new(storage);
        let file = [0u8; SimulatedStorage::SIZE + 1];
        let Err(_) = filesystem.write_file("fancy", &file, &[0u8; 32]) else {
            panic!("Should fail when there is not enough space");
        };
    }
}
