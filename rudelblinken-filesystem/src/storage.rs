use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    IoError(#[from] std::io::Error),
    #[error("Address is bigger than the storage size")]
    AddressTooBig,
    #[error("Size is bigger than the storage size")]
    SizeTooBig,
    /// Only returned by write_checked
    #[error("Size is not a multiple of the page size")]
    ReadDataDoesNotMatchWrittenData,
    #[error("{0}")]
    Other(String),
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
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Size is not a multiple of the page size")]
    SizeNotAMultipleOfPageSize,
    #[error("Can only erase along block boundaries")]
    CanOnlyEraseAlongBlockBoundaries,
    #[error("The size needs to be a multiple of the block size as we can only erase whole blocks")]
    CanOnlyEraseInBlockSizedChunks,
}

/// Storage with wraparound
///
/// Implementing write_readback is optional, but can be done for better performance in some places.
pub trait Storage {
    /// Size in which blocks can be erased
    const BLOCK_SIZE: u32;
    /// Total number of blocks
    const BLOCKS: u32;

    /// Read at a specific location.
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size. If address + length go over the bounds of the storage the storage needs to wrap around there. You should use an MMU for this
    fn read(&self, address: u32, length: u32) -> Result<&'static [u8], StorageError>;
    /// Write at a specific location
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size.
    ///
    /// This operation can only set 0 bits to 1 but not back. If you want to reset bits to 0 use the erase function.
    fn write(&self, address: u32, data: &[u8]) -> Result<(), StorageError>;
    /// Reset a block of bits to 0
    ///
    /// address must be inside the storage size. length must be lower or equal to the storage size. address must be block aligned. length must be a multiple of block size
    fn erase(&self, address: u32, length: u32) -> Result<(), EraseStorageError>;

    /// Filesystem metadata is not stored in the main storage block
    ///
    /// Storage must provide these functions to store metadata.

    /// Read a metadata key from persistent storage
    fn read_metadata(&self, key: &str) -> std::io::Result<Box<[u8]>>;
    /// Write a metadata key from persistent storage
    fn write_metadata(&self, key: &str, value: &[u8]) -> std::io::Result<()>;

    /// Write metadata and return a memorymapped slice to the metadata
    fn write_readback(&self, address: u32, data: &[u8]) -> Result<&'static [u8], StorageError> {
        self.write(address, data)?;
        let data = self.read(address, data.len() as u32)?;
        return Ok(data);
    }
    /// Write metadata and verify afterwards that the read data matches the written data.
    fn write_checked(&self, address: u32, data: &[u8]) -> Result<&'static [u8], StorageError> {
        let read_data = self.write_readback(address, data)?;
        if data != read_data {
            return Err(StorageError::ReadDataDoesNotMatchWrittenData);
        }
        return Ok(read_data);
    }
}

#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

#[cfg(test)]
#[derive(Debug)]
pub struct SimulatedStorage {
    pool: Box<[u8; Self::SIZE as usize * 2]>,
    pool_ptr: *mut [u8; Self::SIZE as usize * 2],
    key_value: Arc<Mutex<HashMap<String, Box<[u8]>>>>,
}

#[cfg(test)]
unsafe impl Send for SimulatedStorage {}
#[cfg(test)]
unsafe impl Sync for SimulatedStorage {}

#[cfg(test)]
impl SimulatedStorage {
    pub const SIZE: u32 = Self::BLOCKS * Self::BLOCK_SIZE;

    pub fn new() -> Result<SimulatedStorage, CreateStorageError> {
        let mut pool = Box::new([0u8; Self::SIZE as usize * 2]);
        return Ok(SimulatedStorage {
            pool_ptr: &mut *pool,
            pool: pool,
            key_value: Default::default(),
        });
    }
}

#[cfg(test)]
impl Storage for SimulatedStorage {
    const BLOCKS: u32 = 16;
    const BLOCK_SIZE: u32 = 4096;

    fn read(&self, address: u32, length: u32) -> Result<&'static [u8], StorageError> {
        if address >= Self::SIZE {
            return Err(StorageError::AddressTooBig);
        }
        if length >= Self::SIZE {
            return Err(StorageError::SizeTooBig);
        }
        let static_slice = unsafe {
            std::mem::transmute::<&[u8], &'static [u8]>(
                &self.pool[address as usize..(address + length) as usize],
            )
        };

        return Ok(static_slice);
    }

    fn write(&self, address: u32, data: &[u8]) -> Result<(), StorageError> {
        if address >= Self::SIZE {
            return Err(StorageError::AddressTooBig);
        }
        if data.len() as u32 >= Self::SIZE {
            return Err(StorageError::SizeTooBig);
        }
        let pool = unsafe { &mut *self.pool_ptr };

        pool[address as usize..address as usize + data.len()].copy_from_slice(data);
        // The part of the data that is overlapping
        let overlapping_length = (address + data.len() as u32).saturating_sub(Self::SIZE);
        let nonoverlapping_length = data.len() as u32 - overlapping_length;

        pool[(Self::SIZE + address) as usize
            ..((Self::SIZE + address) + nonoverlapping_length) as usize]
            .copy_from_slice(&data[0..nonoverlapping_length as usize]);
        if overlapping_length > 0 {
            pool[0..overlapping_length as usize]
                .copy_from_slice(&data[data.len() - (overlapping_length as usize)..data.len()]);
        }
        Ok(())
    }

    fn erase(&self, address: u32, length: u32) -> Result<(), EraseStorageError> {
        if address % Self::BLOCK_SIZE != 0 || length % Self::BLOCK_SIZE != 0 {
            return Err(EraseStorageError::SizeNotAMultipleOfPageSize);
        }
        if (address + length) > Self::BLOCKS * Self::BLOCK_SIZE {
            return Err(EraseStorageError::SizeNotAMultipleOfPageSize);
        }
        let pool = unsafe { &mut *self.pool_ptr };

        let number_of_blocks = length.div_ceil(Self::BLOCK_SIZE);
        for block in 0..number_of_blocks {
            let base_address = address + block * Self::BLOCK_SIZE;
            pool[base_address as usize..(base_address + Self::BLOCK_SIZE) as usize]
                .copy_from_slice(&[0u8; Self::BLOCK_SIZE as usize]);
        }
        return Ok(());
    }

    fn read_metadata(&self, key: &str) -> Result<Box<[u8]>, std::io::Error> {
        return self
            .key_value
            .lock()
            .map_err(|_| std::io::Error::other("Failed to lock mutex"))?
            .get(key)
            .map(|m| m.clone())
            .ok_or(std::io::Error::other("Failed to get a key for that value"));
    }

    fn write_metadata(&self, key: &str, value: &[u8]) -> Result<(), std::io::Error> {
        self.key_value
            .lock()
            .map_err(|_| std::io::Error::other("Failed to lock mutex"))?
            .insert(key.into(), value.into());
        return Ok(());
    }
}

#[cfg(test)]
static STATIC_STORAGES: LazyLock<RwLock<Vec<Box<SimulatedStorage>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

#[cfg(test)]
pub(crate) fn get_test_storage() -> &'static SimulatedStorage {
    let mut backing_storage = Box::new(SimulatedStorage::new().unwrap());
    let backing_storage_ptr: *mut SimulatedStorage = &mut (*backing_storage);
    STATIC_STORAGES.write().unwrap().push(backing_storage);
    let backing_storage: &'static SimulatedStorage = unsafe { &*backing_storage_ptr };
    return backing_storage;
}
