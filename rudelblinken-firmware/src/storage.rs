use std::{
    io::{Error, ErrorKind},
    os::raw::c_void,
};

use enumflags2::BitFlags;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_sys::{
    esp_err_to_name, esp_partition_erase_range, esp_partition_find, esp_partition_get,
    esp_partition_mmap, esp_partition_mmap_memory_t,
    esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA, esp_partition_next,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_UNDEFINED,
    esp_partition_type_t_ESP_PARTITION_TYPE_ANY, esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
    esp_partition_write_raw, ESP_OK,
};
use thiserror::Error;
use zerocopy::{FromBytes, FromZeros, Immutable, KnownLayout, TryFromBytes};

/// Storage with wraparound
pub trait Storage {
    /// Size of the smallest erasable block
    const BLOCK_SIZE: usize;
    /// Total number of blocks
    const BLOCKS: usize;

    /// Read with wraparound
    fn read<'a>(&'a self, address: usize, length: usize) -> std::io::Result<&'a [u8]>;
    fn write(&mut self, address: usize, data: &[u8]) -> std::io::Result<()>;
    fn erase(&mut self, address: usize, length: usize) -> std::io::Result<()>;
}

pub struct FlashStorage {
    size: usize,

    partition: *const esp_idf_sys::esp_partition_t,

    storage_arena: *mut u8,
    storage_handle_a: u32,
    storage_handle_b: u32,
    storage_handle_c: u32,
}

/// Log information about the available partitions
pub fn print_partitions() {
    unsafe {
        let mut partition_iterator = esp_partition_find(
            esp_partition_type_t_ESP_PARTITION_TYPE_ANY,
            esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
            std::ptr::null_mut(),
        );
        if partition_iterator == std::ptr::null_mut() {
            panic!("No partitions found!");
        }
        ::log::info!(target: "partition-info", "type, subtype, label, address, name");

        while partition_iterator != std::ptr::null_mut() {
            let partition = *esp_partition_get(partition_iterator);
            let label = String::from_utf8(std::mem::transmute(partition.label.to_vec()));
            // label.copy_from_slice(&partition.label.);
            ::log::info!(target: "partition-info", "{}, {}, {:?}, {:0x}, {}", partition.type_, partition.subtype,  label, partition.address, partition.size);
            partition_iterator = esp_partition_next(partition_iterator);
        }
    }
}

#[derive(Error, Debug, Clone)]
pub enum CreateStorageError {
    #[error("Failed to find a storage partition. (type: data, subtype: undefined, name: storage)")]
    NoPartitionFound,
    #[error("Failed to memorymap the secrets")]
    FailedToMmapSecrets,
}

#[derive(Error, Debug, Clone)]
pub enum WriteStorageError {
    #[error("Failed to write to flash. Maybe the pages are not erased.")]
    FailedToWriteToFlash,
}

// bitflags! {

//     pub struct BlockType: u16 {
//         const WASM = 0b00000001;
//         const B = 0b00000010;
//         const C = 0b00000100;
//     }
// }

// #[derive(KnownLayout, TryFromBytes)]
#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq, KnownLayout)]
pub enum BlockType {
    /// If this is a wasm binary
    WASM,
    /// This file has been deleted
    DELETED,
}

#[derive(KnownLayout, FromBytes, Immutable)]
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

struct FileInformation {
    /// Block number
    location_in_storage: usize,
    /// Length in bytes
    length: usize,
    /// Name of the file, null terminated or 16 chars
    name: String,
    /// Block number
    // TODO: The lifetime of this is definitely not static
    content: &'static [u8],
    /// metadata
    metadata: &'static BlockMetadata,
}

#[derive(Error, Debug)]
pub enum CreateFileInformationError {
    #[error(transparent)]
    ReadFileError(#[from] std::io::Error),
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

impl FileInformation {
    /// Return information about a new file
    ///
    /// None means that there is definitely no file starting there.
    /// If a file information is returned, there is no guarantee, that it is actually a real file.
    pub fn new<T: Storage>(
        storage: &T,
        location_in_storage: usize,
    ) -> Result<FileInformation, CreateFileInformationError> {
        // TODO: This is unsafe AF
        let maybe_metadata_slice = storage.read(location_in_storage, size_of::<BlockMetadata>())?;
        let maybe_metadata_slice =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(maybe_metadata_slice) };
        let metadata: &BlockMetadata = BlockMetadata::ref_from_bytes(maybe_metadata_slice)?;
        if metadata.is_empty() {
            return Err(CreateFileInformationError::NoMetadata);
        }

        let content = storage.read(
            location_in_storage + size_of::<BlockMetadata>(),
            metadata.length() as usize,
        )?;
        let content = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(content) };

        let information = FileInformation {
            location_in_storage: location_in_storage,
            length: metadata.length() as usize,
            name: metadata.name().into(),
            metadata: metadata,
            content: content,
        };
        return Ok(information);
    }
}

impl FlashStorage {
    pub fn new() -> Result<FlashStorage, CreateStorageError> {
        // TODO: Make sure that there is only one flash storage instance.
        let mut label: Vec<i8> = String::from("storage")
            .bytes()
            .into_iter()
            .map(|c| c as i8)
            .collect();
        label.push(0);

        // Find the partition
        let partition;
        unsafe {
            let partition_iterator = esp_partition_find(
                esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
                esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_UNDEFINED,
                label.as_mut_ptr(),
            );
            if partition_iterator == std::ptr::null_mut() {
                return Err(CreateStorageError::NoPartitionFound);
            }
            partition = esp_partition_get(partition_iterator);
        }

        // Memorymap the partition
        let memory_mapped_flash: *mut u8;
        let mut storage_handle_a: u32 = 0;
        let mut storage_handle_b: u32 = 0;
        let mut storage_handle_c: u32 = 0;
        unsafe {
            let mut first_pointer: *const c_void = std::ptr::null_mut();
            // Mount first mmu page
            let err = esp_partition_mmap(
                partition,
                0,
                esp_idf_sys::CONFIG_MMU_PAGE_SIZE as usize,
                esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA,
                std::ptr::addr_of_mut!(first_pointer),
                std::ptr::addr_of_mut!(storage_handle_a),
            );
            if err != 0 {
                return Err(CreateStorageError::FailedToMmapSecrets);
            }
            let mut idk_pointer: *const c_void = std::ptr::null_mut();
            // Mount the remaining pages
            let err = esp_partition_mmap(
                partition,
                esp_idf_sys::CONFIG_MMU_PAGE_SIZE as usize,
                (*partition).size as usize - esp_idf_sys::CONFIG_MMU_PAGE_SIZE as usize,
                esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA,
                std::ptr::addr_of_mut!(idk_pointer),
                std::ptr::addr_of_mut!(storage_handle_b),
            );
            if err != 0 {
                return Err(CreateStorageError::FailedToMmapSecrets);
            }
            // If we now mmap the whole partition, will get a pointer to the memory mapped partition directly after the first a partition.
            // If we would have mounted the whole partition in one step previously, we would have got the same pointer again
            let err = esp_partition_mmap(
                partition,
                0,
                (*partition).size as usize,
                esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA,
                std::ptr::addr_of_mut!(idk_pointer),
                std::ptr::addr_of_mut!(storage_handle_c),
            );
            if err != 0 {
                ::log::error!("Errorcode: {}", err);
                let error: &std::ffi::CStr = std::ffi::CStr::from_ptr(esp_err_to_name(err));
                ::log::error!("Error description: {}", error.to_string_lossy());
                return Err(CreateStorageError::FailedToMmapSecrets);
            }

            ::log::info!("Got out_ptr: {:0x?}", first_pointer);
            memory_mapped_flash = first_pointer as _;

            return Ok(FlashStorage {
                partition: partition,

                size: (*partition).size as usize,
                storage_arena: memory_mapped_flash,
                storage_handle_a,
                storage_handle_b,
                storage_handle_c,
            });
        }
    }

    // pub fn write(&mut self, value: &Vec<u8>) -> Result<(), WriteStorageError> {
    //     let data = value.as_slice();
    //     let data_ptr = data.as_ptr() as *const c_void;
    //     ::log::info!(
    //         "STORAGE: {:0x?}, INPUT: {:0x?}",
    //         self.storage_arena,
    //         data_ptr
    //     );
    //     unsafe {
    //         // Works with erase
    //         // esp_partition_erase_range(self.partition, 0, (*self.partition).erase_size as usize);

    //         let error_code = esp_partition_write_raw(self.partition, 0, data_ptr, value.len());
    //         if error_code != ESP_OK {
    //             ::log::error!("Failed to write to flash with code {}", error_code);
    //             let error: &std::ffi::CStr = std::ffi::CStr::from_ptr(esp_err_to_name(error_code));
    //             ::log::error!("Description: {}", error.to_string_lossy());
    //             return Err(WriteStorageError::FailedToWriteToFlash);
    //         }
    //     };
    //     // unsafe {
    //     //     std::ptr::copy_nonoverlapping(data_ptr, self.storage_arena, data.len());
    //     // }
    //     ::log::info!("Copied data");
    //     return Ok(());
    // }
    // pub fn read(&mut self, length: usize) -> Vec<u8> {}
}

impl Storage for FlashStorage {
    const BLOCKS: usize = 256;
    const BLOCK_SIZE: usize = 4096;

    fn read(&self, address: usize, length: usize) -> std::io::Result<&[u8]> {
        // TODO: Make this actually safe
        let thing: &[u8];
        unsafe {
            ::log::info!("Reading data");
            thing = std::slice::from_raw_parts(self.storage_arena.offset(address as isize), length);
            ::log::info!("Read data");
        }
        return Ok(thing);
    }

    fn write(&mut self, address: usize, data: &[u8]) -> std::io::Result<()> {
        // TODO: Make this actually safe
        let data_ptr = data.as_ptr() as *const c_void;
        ::log::info!(
            "STORAGE: {:0x?}, INPUT: {:0x?}",
            self.storage_arena,
            data_ptr
        );
        unsafe {
            // Works with erase
            // esp_partition_erase_range(self.partition, 0, (*self.partition).erase_size as usize);

            let error_code = esp_partition_write_raw(self.partition, address, data_ptr, data.len());
            if error_code != ESP_OK {
                ::log::error!("Failed to write to flash with code {}", error_code);
                let error: &std::ffi::CStr = std::ffi::CStr::from_ptr(esp_err_to_name(error_code));
                ::log::error!("Description: {}", error.to_string_lossy());
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    error.to_string_lossy(),
                ));
            }
        };
        // unsafe {
        //     std::ptr::copy_nonoverlapping(data_ptr, self.storage_arena, data.len());
        // }
        ::log::info!("Copied data");
        return Ok(());
    }

    fn erase(&mut self, address: usize, length: usize) -> std::io::Result<()> {
        if length == 0 {
            return Ok(());
        }
        if address % Self::BLOCK_SIZE != 0 || length % Self::BLOCK_SIZE != 0 {
            return Err(Error::other("Can only erase along block boundaries"));
        }
        if (address + length) > Self::BLOCKS * Self::BLOCK_SIZE {
            return Err(Error::other("Can not erase outside the storage boundaries"));
        }

        unsafe {
            if (*self.partition).erase_size as usize != Self::BLOCK_SIZE {
                return Err(Error::other("Erase size does not match block size"));
            }
            ::log::info!(
                "Erasing {} blocks starting from {}",
                length % Self::BLOCK_SIZE,
                address % Self::BLOCK_SIZE
            );
            esp_partition_erase_range(self.partition, address, length);
        }
        return Ok(());
    }
}

fn get_first_block() -> u16 {
    let nvs_default_partition = EspDefaultNvsPartition::take().unwrap();
    let Ok(nvs) = EspNvs::new(nvs_default_partition, "filesystem_ns", false) else {
        panic!("Something went wrong");
    };
    nvs.get_u16("first_block").unwrap_or(Some(0)).unwrap_or(0)
}

fn set_first_block(first_block: u16) {
    let nvs_default_partition = EspDefaultNvsPartition::take().unwrap();
    let Ok(nvs) = EspNvs::new(nvs_default_partition, "filesystem_ns", true) else {
        panic!("Something went wrong");
    };
    nvs.set_u16("first_block", first_block).unwrap();
}

struct Filesystem<T: Storage> {
    storage: T,
    files: Vec<FileInformation>,
}

impl<T: Storage> Filesystem<T> {
    pub fn new(storage: T) -> Self {
        let first_block = get_first_block() as usize;

        let mut files = Vec::new();
        let mut block_number = 0;
        while block_number < T::BLOCKS {
            let current_block_number = (block_number + first_block as usize) % T::BLOCKS;
            let file_information =
                FileInformation::new(&storage, current_block_number * T::BLOCK_SIZE);
            let Ok(file_information) = file_information else {
                block_number += 1;
                continue;
            };
            block_number += ((file_information.length + 64) / T::BLOCK_SIZE) + 1;
            files.push(file_information);
        }

        let filesystem = Self {
            storage,
            files: Vec::new(),
        };
        return filesystem;
    }
}
// fn init() {
//     // example storage backend
//     ram_storage!(tiny);
//     let mut ram = Ram::default();
//     let mut storage = RamStorage::new(&mut ram);

//     // must format before first mount
//     Filesystem::format(&mut storage).unwrap();
//     // must allocate state statically before use
//     let mut alloc = Filesystem::allocate();
//     let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

//     // may use common `OpenOptions`
//     let mut buf = [0u8; 11];
//     fs.open_file_with_options_and_then(
//         |options| options.read(true).write(true).create(true),
//         &PathBuf::from(b"example.txt"),
//         |file| {
//             file.write(b"Why is black smoke coming out?!")?;
//             file.seek(SeekFrom::End(-24)).unwrap();
//             assert_eq!(file.read(&mut buf)?, 11);
//             Ok(())
//         },
//     )
//     .unwrap();
//     assert_eq!(&buf, b"black smoke");
// }
