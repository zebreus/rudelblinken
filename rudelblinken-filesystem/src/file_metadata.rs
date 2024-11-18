use crate::storage::Storage;
use crate::storage::StorageError;
use thiserror::Error;
use zerocopy::IntoBytes;
use zerocopy::TryFromBytes;
use zerocopy::{Immutable, KnownLayout};

#[derive(Error, Debug)]
pub enum ReadMetadataError {
    #[error("Failed to read metadata from flash")]
    ReadStorageError,
    #[error("The read metadata does not have valid marker flags")]
    InvalidMarkers,
}

#[derive(Error, Debug)]
pub enum CreateMetadataError {
    #[error("Metadata seems invalid. This should never happen")]
    CreateMetadataError,
    #[error(transparent)]
    StorageError(#[from] StorageError),
}

// #[enumflags2::bitflags]
// #[repr(u16)]
// #[derive(Copy, Clone, Debug, PartialEq)]
// pub enum FileFlags {
//     /// All markers need to be set correctly for the memory to be a valid file
//     MarkerHighA = 0b0010000000100001,
//     MarkerHighB = 0b0000000000100000,
//     MarkerHighC = 0b0100000000000000,
//     MarkerLowA = 0b0000001000010100,
//     MarkerLowB = 0b0000001000000000,
//     MarkerLowC = 0b0000000000010000,
//     /// If this file has been written completly
//     Ready,
//     /// This file is marked for deletion
//     MarkedForDeletion,
//     /// This file has been deleted. It contains invalid content, but its metablock may still be valid
//     Deleted,
// }

struct FileFlags2 {}
#[rustfmt::skip]
impl FileFlags2 {
    const HIGH_MARKERS: u16 =        0b0010000000100001;
    const LOW_MARKERS: u16 =         0b0000001000010100;
    const READY: u16 =               0b0000000000000010;
    const MARKED_FOR_DELETION: u16 = 0b0000000000001000;
    const DELETED: u16 =             0b0000000001000000;
}

/// Represents a the metadata segment of a file that is memory-mapped into storage.
///
/// Read an existing metadata segment at an address with [from_storage] or place a new one with [new_from_storage]
#[derive(Debug, PartialEq, Eq, Clone, KnownLayout, IntoBytes, Immutable, TryFromBytes)]
#[repr(C)]
pub struct FileMetadata {
    /// Type of this block
    /// Access only via the supplied functions
    flags: u16,
    /// Reserved space for alignment reasons
    _reserved: [u8; 2],
    /// Length in bytes
    pub length: u32,
    /// SHA3-256 hash of the file
    pub hash: [u8; 32],
    /// Name of the file, null terminated or 16 chars
    pub name: [u8; 16],
    /// Reserved space to fill the metadata to 64 byte
    _padding: [u8; 8],
}

impl FileMetadata {
    /// Create a new file metadata object in ram
    fn new(name: &str, length: u32) -> Self {
        let mut metadata = FileMetadata {
            flags: u16::MAX ^ FileFlags2::LOW_MARKERS,
            _reserved: [0; 2],
            length: length,
            hash: [0; 32],
            name: [0; 16],
            _padding: [0; 8],
        };
        metadata.set_name(name);
        return metadata;
    }
    /// Assert that the marker flags have been set correctly for this file
    pub fn valid_marker(&self) -> bool {
        if self.flags & FileFlags2::HIGH_MARKERS != FileFlags2::HIGH_MARKERS {
            return false;
        }
        if self.flags & FileFlags2::LOW_MARKERS != 0 {
            return false;
        }
        return true;
    }
    /// Convenience function to get the name as a string slice
    pub fn name_str(&self) -> &str {
        let nul_range_end = self.name.iter().position(|&c| c == b'\0').unwrap_or(16);
        return std::str::from_utf8(&self.name[0..nul_range_end]).unwrap_or_default();
    }
    /// Internal function to set the name from a string slice
    fn set_name(&mut self, name: &str) -> () {
        let name_bytes = name.as_bytes();
        let name_length = name.len().clamp(0, 16);
        self.name[0..name_length].copy_from_slice(&name_bytes[0..name_length]);
    }
    /// Set flags of the metadata in storage
    ///
    /// Assumes that this metadata is located at `address`. Undefined behaviour if it is not or has since been deleted
    unsafe fn set_flags<T: Storage>(
        &self,
        storage: &T,
        address: u32,
        flags: u16,
    ) -> Result<(), StorageError> {
        let flags: u16 = self.flags & !flags;
        storage.write(address, flags.as_bytes())
    }
    /// Set the ready flag of the metadata in storage
    ///
    /// Assumes that this metadata is located at `address`. Undefined behaviour if it is not or has since been deleted
    pub unsafe fn set_ready<T: Storage>(
        &self,
        storage: &T,
        address: u32,
    ) -> Result<(), StorageError> {
        self.set_flags(storage, address, FileFlags2::READY)
    }
    /// Set the marked for deletion flag of the metadata in storage
    ///
    /// Assumes that this metadata is located at `address`. Undefined behaviour if it is not or has since been deleted
    pub unsafe fn set_marked_for_deletion<T: Storage>(
        &self,
        storage: &T,
        address: u32,
    ) -> Result<(), StorageError> {
        self.set_flags(storage, address, FileFlags2::MARKED_FOR_DELETION)
    }
    /// Set the deleted flag of the metadata in storage
    ///
    /// Assumes that this metadata is located at `address`. Undefined behaviour if it is not or has since been deleted
    pub unsafe fn set_deleted<T: Storage>(
        &self,
        storage: &T,
        address: u32,
    ) -> Result<(), StorageError> {
        self.set_flags(storage, address, FileFlags2::DELETED)
    }
    pub fn ready(&self) -> bool {
        self.flags & FileFlags2::READY == 0
    }
    pub fn marked_for_deletion(&self) -> bool {
        self.flags & FileFlags2::MARKED_FOR_DELETION == 0
    }
    pub fn deleted(&self) -> bool {
        self.flags & FileFlags2::DELETED == 0
    }

    /// Store the metadata to the specified storage address
    pub fn new_to_storage<T: Storage>(
        storage: &T,
        address: u32,
        name: &str,
        length: u32,
    ) -> Result<&'static Self, CreateMetadataError> {
        let new_metadata = Self::new(name, length);
        let as_bytes = new_metadata.as_bytes();
        let memory_mapped_metadata = storage.write_checked(address, as_bytes)?;
        Ok(FileMetadata::try_ref_from_bytes(memory_mapped_metadata)
            .map_err(|e| CreateMetadataError::CreateMetadataError)?)
    }
    /// Read a file metadata object from storage
    ///
    /// Returns a reference to memorymapped flash storage
    pub fn from_storage<T: Storage>(
        storage: &T,
        address: u32,
    ) -> Result<&'static Self, ReadMetadataError> {
        let data = storage
            .read(address, size_of::<FileMetadata>() as u32)
            .map_err(|_| ReadMetadataError::ReadStorageError)?;

        let metadata = FileMetadata::try_ref_from_bytes(data)
            .map_err(|_| ReadMetadataError::ReadStorageError)?;
        if !metadata.valid_marker() {
            return Err(ReadMetadataError::InvalidMarkers);
        }
        Ok(&metadata)
    }
}

#[cfg(test)]
mod tests {

    use crate::storage::SimulatedStorage;

    use super::*;

    #[test]
    fn storing_metadata_works() {
        let mut storage = SimulatedStorage::new().unwrap();
        let metadata = FileMetadata::new_to_storage(&mut storage, 0, "toast", 300).unwrap();
        assert_eq!(metadata.length, 300);
        assert_eq!(metadata.name_str(), "toast");
    }

    #[test]
    fn marker_gets_set_for_new_metadata() {
        let mut storage = SimulatedStorage::new().unwrap();
        let metadata = FileMetadata::new_to_storage(&mut storage, 0, "toast", 300).unwrap();
        assert!(metadata.valid_marker());
    }

    #[test]
    fn reading_metadata_works() {
        let mut storage = SimulatedStorage::new().unwrap();
        let metadata = FileMetadata::new_to_storage(&mut storage, 0, "toast", 300).unwrap();
        let read_metadata = FileMetadata::from_storage(&storage, 0).unwrap();
        assert_eq!(read_metadata.length, 300);
        assert_eq!(read_metadata.name_str(), "toast");
        assert!(read_metadata.valid_marker());
    }
}
