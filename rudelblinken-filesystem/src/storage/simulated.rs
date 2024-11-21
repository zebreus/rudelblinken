//! A Storage for testing purposes that is backed by a heap allocated buffer

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::{EraseStorageError, Storage, StorageError};

#[derive(Debug)]
#[repr(C, align(4096))]
struct AlignedBuffer<const SIZE: usize>([u8; SIZE]);

#[derive(Debug)]
/// A storage that is backed by a heap allocated buffer
///
/// ```
/// use rudelblinken_filesystem::storage::simulated::SimulatedStorage;
/// let storage = SimulatedStorage::new();
/// ```
pub struct SimulatedStorage {
    pool: Box<AlignedBuffer<{ Self::SIZE as usize * 2 }>>,
    pool_ptr: *mut [u8; Self::SIZE as usize * 2],
    key_value: Arc<Mutex<HashMap<String, Box<[u8]>>>>,
}

unsafe impl Send for SimulatedStorage {}
unsafe impl Sync for SimulatedStorage {}

impl SimulatedStorage {
    /// Size of the storage
    pub const SIZE: u32 = Self::BLOCKS * Self::BLOCK_SIZE;

    /// Create a new storage for testing purposes
    pub fn new() -> SimulatedStorage {
        let mut pool = Box::new(AlignedBuffer([0b11111111u8; Self::SIZE as usize * 2]));
        return SimulatedStorage {
            pool_ptr: &mut (pool.0),
            pool: pool,
            key_value: Default::default(),
        };
    }
}

/// Copies zeroes from src to dest and ignores ones in src.
fn copy_zeroes_from_slice(dest: &mut [u8], src: &[u8]) {
    let new_data: Vec<u8> = src
        .iter()
        .zip(dest.iter())
        .map(|(src, dest)| src & dest)
        .collect();
    dest.copy_from_slice(&new_data);
}

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
                &self.pool.0[address as usize..(address + length) as usize],
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

        copy_zeroes_from_slice(
            &mut pool[address as usize..address as usize + data.len()],
            data,
        );
        // The part of the data that is overlapping
        let overlapping_length = (address + data.len() as u32).saturating_sub(Self::SIZE);
        let nonoverlapping_length = data.len() as u32 - overlapping_length;

        copy_zeroes_from_slice(
            &mut pool[(Self::SIZE + address) as usize
                ..((Self::SIZE + address) + nonoverlapping_length) as usize],
            &data[0..nonoverlapping_length as usize],
        );
        if overlapping_length > 0 {
            copy_zeroes_from_slice(
                &mut pool[0..overlapping_length as usize],
                &data[data.len() - (overlapping_length as usize)..data.len()],
            );
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
                .copy_from_slice(&[0b11111111u8; Self::BLOCK_SIZE as usize]);
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
use std::sync::{LazyLock, RwLock};

#[cfg(test)]
static STATIC_STORAGES: LazyLock<RwLock<Vec<Box<SimulatedStorage>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

#[cfg(test)]
pub(crate) fn get_test_storage() -> &'static SimulatedStorage {
    let backing_storage = Box::new(SimulatedStorage::new());
    let backing_storage_ptr: *const SimulatedStorage = Box::as_ptr(&backing_storage);
    STATIC_STORAGES.write().unwrap().push(backing_storage);
    let backing_storage: &'static SimulatedStorage = unsafe { &*backing_storage_ptr };
    return backing_storage;
}
