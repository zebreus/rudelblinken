use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadStorageError {
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    IoError(#[from] std::io::Error),
    #[error("Address is bigger than the storage size")]
    AddressTooBig,
    #[error("Size is bigger than the storage size")]
    SizeTooBig,
}

#[derive(Error, Debug)]
pub enum WriteStorageError {
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    IoError(#[from] std::io::Error),
    #[error("Address is bigger than the storage size")]
    AddressTooBig,
    #[error("Size is bigger than the storage size")]
    SizeTooBig,
}

#[derive(Error, Debug, Clone)]
pub enum CreateStorageError {
    #[error("Failed to find a storage partition. (type: data, subtype: undefined, name: storage)")]
    NoPartitionFound,
    #[error("Failed to memorymap the secrets")]
    FailedToMmapSecrets,
}

#[derive(Error, Debug)]
pub enum EraseStorageError {
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    IoError(#[from] std::io::Error),
    #[error("Address is bigger than the storage size")]
    AddressTooBig,
    #[error("Size is bigger than the storage size")]
    SizeTooBig,
    #[error("Size is not a multiple of the page size")]
    SizeNotAMultipleOfPageSize,
}

/// Storage with wraparound
pub trait Storage {
    /// Size of the smallest erasable block
    const BLOCK_SIZE: usize;
    /// Total number of blocks
    const BLOCKS: usize;

    /// Read at a specific location.
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size. If address + length go over the bounds of the storage the storage needs to wrap around there. You should use an MMU for this
    fn read(&self, address: usize, length: usize) -> Result<&[u8], ReadStorageError>;
    /// Write at a specific location
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size.
    ///
    /// This operation can only set 0 bits to 1 but not back. If you want to reset bits to 0 use the erase function.
    fn write(&mut self, address: usize, data: &[u8]) -> Result<(), WriteStorageError>;
    /// Reset a block of bits to 0
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size. address must be block aligned. length must be a multiple of block size
    fn erase(&mut self, address: usize, length: usize) -> Result<(), EraseStorageError>;

    /// Filesystem metadata is not stored in the main storage block
    ///
    /// Storage must provide these functions to store metadata.

    /// Read a metadata key from persistent storage
    fn read_metadata(&self, key: &str) -> std::io::Result<&[u8]>;
    /// Write a metadata key from persistent storage
    fn write_metadata(&mut self, key: &str, value: &[u8]) -> std::io::Result<()>;
}

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
pub struct SimulatedStorage {
    pool: Box<[u8; Self::SIZE * 2]>,
    key_value: HashMap<String, Vec<u8>>,
}

#[cfg(test)]
impl SimulatedStorage {
    pub const SIZE: usize = Self::BLOCKS * Self::BLOCK_SIZE;

    pub fn new() -> Result<SimulatedStorage, CreateStorageError> {
        return Ok(SimulatedStorage {
            pool: Box::new([0u8; Self::SIZE * 2]),
            key_value: Default::default(),
        });
    }
}

#[cfg(test)]
impl Storage for SimulatedStorage {
    const BLOCKS: usize = 16;
    const BLOCK_SIZE: usize = 4096;

    fn read(&self, address: usize, length: usize) -> Result<&[u8], ReadStorageError> {
        if address >= Self::SIZE {
            return Err(ReadStorageError::AddressTooBig);
        }
        if length >= Self::SIZE {
            return Err(ReadStorageError::SizeTooBig);
        }

        return Ok(&self.pool[address..address + length]);
    }

    fn write(&mut self, address: usize, data: &[u8]) -> Result<(), WriteStorageError> {
        if address >= Self::SIZE {
            return Err(WriteStorageError::AddressTooBig);
        }
        if data.len() >= Self::SIZE {
            return Err(WriteStorageError::SizeTooBig);
        }

        self.pool[address..address + data.len()].copy_from_slice(data);
        // The part of the data that is overlapping
        let overlapping_length = (address + data.len()).saturating_sub(Self::SIZE);
        let nonoverlapping_length = data.len() - overlapping_length;

        self.pool[Self::SIZE + address..(Self::SIZE + address) + nonoverlapping_length]
            .copy_from_slice(&data[0..nonoverlapping_length]);
        if overlapping_length > 0 {
            self.pool[0..overlapping_length]
                .copy_from_slice(&data[data.len() - (overlapping_length)..data.len()]);
        }
        Ok(())
    }

    fn erase(&mut self, address: usize, length: usize) -> Result<(), EraseStorageError> {
        if address % Self::BLOCK_SIZE != 0 || length % Self::BLOCK_SIZE != 0 {
            return Err(EraseStorageError::SizeNotAMultipleOfPageSize);
        }
        if (address + length) > Self::BLOCKS * Self::BLOCK_SIZE {
            return Err(EraseStorageError::SizeNotAMultipleOfPageSize);
        }

        let number_of_blocks = length.div_ceil(Self::BLOCK_SIZE);
        for block in 0..number_of_blocks {
            let base_address = address + block * Self::BLOCK_SIZE;
            self.pool[base_address..(base_address + Self::BLOCK_SIZE)]
                .copy_from_slice(&[0u8; Self::BLOCK_SIZE]);
        }
        return Ok(());
    }

    fn read_metadata(&self, key: &str) -> Result<&[u8], std::io::Error> {
        return self
            .key_value
            .get(key)
            .map(|m| m.as_ref())
            .ok_or(std::io::Error::other("Failed to get a key for that value"));
    }

    fn write_metadata(&mut self, key: &str, value: &[u8]) -> Result<(), std::io::Error> {
        self.key_value.insert(key.into(), value.into());
        return Ok(());
    }
}
