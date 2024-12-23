//! This module provides the `Storage` trait which defines the interface for
//! storage backends used in the application. Implementations of this trait
//! are responsible for handling theuse crate::storage::Storage;

use thiserror::Error;

#[cfg(any(test, feature = "simulated"))]
#[cfg_attr(docsrs, doc(cfg(feature = "simulated")))]
pub mod simulated;

#[cfg(feature = "esp")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp")))]
pub mod esp;

/// Some kind of error that can occur during a storage operation
#[derive(Error, Debug)]
pub enum StorageError {
    /// Failed to write to flash. Maybe the pages are not erased.
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    IoError(#[from] std::io::Error),
    /// Address is bigger than the storage size
    #[error("Address is bigger than the storage size")]
    AddressTooBig,
    /// Size is bigger than the storage size
    #[error("Size is bigger than the storage size")]
    SizeTooBig,
    /// Only returned by write_checked
    #[error("Read data does not match written data")]
    ReadDataDoesNotMatchWrittenData,
    /// Other error occurred during a storage operation
    #[error("{0}")]
    Other(String),
}

#[derive(Error, Debug)]
/// Errors that can occur during the erase operation of the storage.
pub enum EraseStorageError {
    /// Failed during storage operation
    #[error(transparent)]
    StorageError(#[from] StorageError),
    /// Size is not a multiple of the page size
    #[error("Size is not a multiple of the page size")]
    SizeNotAMultipleOfPageSize,
    /// Can only erase along block boundaries
    #[error("Can only erase along block boundaries")]
    CanOnlyEraseAlongBlockBoundaries,
    /// The size needs to be a multiple of the block size as we can only erase whole blocks
    #[error("The size needs to be a multiple of the block size as we can only erase whole blocks")]
    CanOnlyEraseInBlockSizedChunks,
}

/// Storage with wraparound
///
/// Implementing write_readback is optional, but can be done for better performance in some places.
///
/// The starting address of each block returned by read needs to be aligned to 64 bytes
///
/// Filesystem metadata is not stored in the main storage block
///
/// Storage must provide these functions to store metadata.
pub trait Storage {
    /// Size in which blocks can be erased
    const BLOCK_SIZE: u32;
    /// Total number of blocks
    const BLOCKS: u32;

    /// Read at a specific location.
    ///
    /// Address must be inside the storage size. length must be lower or equal to the storage size. If address + length go over the bounds of the storage the storage needs to wrap around there. You should use an MMU for this
    ///
    /// This function is expected to return a slice that points into memory mapped storage. This means that the data is not copied and the data is directly read from the storage. This way no copy operations are needed to read data from the storage.
    fn read(&self, address: u32, length: u32) -> Result<&'static [u8], StorageError>;
    /// Write at a specific location
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size.
    ///
    /// This operation can only set 1 bits to 0 but not back. If you want to reset bits to 1 use the erase function.
    fn write(&self, address: u32, data: &[u8]) -> Result<(), StorageError>;
    /// Reset a block of bits to 1
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size. address must be block aligned. length must be a multiple of block size
    fn erase(&self, address: u32, length: u32) -> Result<(), EraseStorageError>;

    /// Read a metadata key from persistent storage
    fn read_metadata(&self, key: &str) -> std::io::Result<Box<[u8]>>;
    /// Write a metadata key from persistent storage
    fn write_metadata(&self, key: &str, value: &[u8]) -> std::io::Result<()>;

    /// Write metadata and return a memorymapped slice to the metadata
    fn write_readback(&self, address: u32, data: &[u8]) -> Result<&'static [u8], StorageError> {
        self.write(address, data)?;
        let data = self.read(address, data.len() as u32)?;
        Ok(data)
    }
    /// Write metadata and verify afterwards that the read data matches the written data.
    fn write_checked(&self, address: u32, data: &[u8]) -> Result<&'static [u8], StorageError> {
        let read_data = self.write_readback(address, data)?;
        if data != read_data {
            return Err(StorageError::ReadDataDoesNotMatchWrittenData);
        }
        Ok(read_data)
    }
}
