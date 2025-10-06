//! Load the main program from the filesystem or return the default program
use crate::config::main_program;
use crate::storage::{get_filesystem, CreateStorageError};
use crate::{storage::FlashStorage, wasm_service::wasm_host::WasmHost};
use esp_idf_sys::{
    esp_partition_find, esp_partition_get, esp_partition_mmap,
    esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA, esp_partition_subtype_t,
    esp_partition_type_t,
};
use rudelblinken_filesystem::file::{File, FileState};
use std::sync::LazyLock;
use std::{os::raw::c_void, slice, time::Duration};

/// The delay between attempts to load the main program
const LOAD_MAIN_PROGRAM_RETRY_DELAY: Duration = Duration::from_millis(200);
/// The max number of attempts to acquire the filesystem lock before returning the default program
const MAX_MAIN_PROGRAM_FS_LOCK_ATTEMPTS: usize = 50;
/// The max number of attempts to read the main program before deleting it and returning the default program
const MAX_MAIN_PROGRAM_UPGRADE_ATTEMPTS: usize = 5;

/// A wasm program as a byte slice
///
/// Can be either the built-in default program or a program from the filesystem
///
/// You can get the wasm bytecode as a byte slice with `as_ref`
#[derive(Debug, Clone)]
pub enum WasmProgram {
    Default,
    MainProgram(File<FlashStorage, { FileState::Reader }>),
}
impl AsRef<[u8]> for WasmProgram {
    fn as_ref(&self) -> &[u8] {
        match self {
            WasmProgram::Default => DEFAULT_PROGRAM.get_content(),
            WasmProgram::MainProgram(file) => &file,
        }
    }
}

const PROG_PART_TYPE: esp_partition_type_t = 0x41;
const PROG_PART_SUBTYPE: esp_partition_subtype_t = 0x1;

struct FlashProgram {
    partition: *const esp_idf_sys::esp_partition_t,
    storage: *const u8,
}

unsafe impl Sync for FlashProgram {}

unsafe impl Send for FlashProgram {}

impl FlashProgram {
    fn get_content(&self) -> &'static [u8] {
        let part_size = unsafe { (*self.partition).size };
        let len_ptr = unsafe { self.storage.add(part_size as usize).sub(4) } as *const u32;
        let len = unsafe { *len_ptr } as usize;
        unsafe { slice::from_raw_parts(self.storage, len) }
    }
}

static DEFAULT_PROGRAM: LazyLock<FlashProgram> =
    LazyLock::new(|| get_default_program().expect("failed to load default program"));

fn get_default_program() -> Result<FlashProgram, CreateStorageError> {
    let mut label: Vec<u8> = String::from("default_program").bytes().collect();
    label.push(0);

    // Find the partition
    let partition;
    unsafe {
        let partition_iterator =
            esp_partition_find(PROG_PART_TYPE, PROG_PART_SUBTYPE, label.as_mut_ptr());
        if partition_iterator == std::ptr::null_mut() {
            return Err(CreateStorageError::NoPartitionFound);
        }
        partition = esp_partition_get(partition_iterator);
    }

    let memory_mapped_flash: *mut u8;
    let mut storage_handle: u32 = 0;
    unsafe {
        let mut mmap_pointer: *const c_void = std::ptr::null_mut();
        let err = esp_partition_mmap(
            partition,
            0,
            (*partition).size as usize,
            esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA,
            std::ptr::addr_of_mut!(mmap_pointer),
            std::ptr::addr_of_mut!(storage_handle),
        );
        if err != 0 {
            return Err(CreateStorageError::FailedToMmapSecrets);
        }

        memory_mapped_flash = mmap_pointer as _;

        return Ok(FlashProgram {
            partition,
            storage: memory_mapped_flash,
        });
    }
}

/// Load the main program or return the default program
pub fn load_main_program(host: &mut WasmHost) -> WasmProgram {
    let mut fs_lock_attempts_left = MAX_MAIN_PROGRAM_FS_LOCK_ATTEMPTS;
    let mut upgrade_attempts_left = MAX_MAIN_PROGRAM_UPGRADE_ATTEMPTS;
    loop {
        std::thread::sleep(LOAD_MAIN_PROGRAM_RETRY_DELAY);
        // Drain the event queue
        while host.host_events.lock().try_recv().is_ok() {}

        let Some(current_main_program) = main_program::get() else {
            // No main program set
            // Return the default program
            return WasmProgram::Default;
        };

        let filesystem = get_filesystem().unwrap();
        let Ok(filesystem_reader) = filesystem.read() else {
            // This will change,
            fs_lock_attempts_left = fs_lock_attempts_left.saturating_sub(1);
            if fs_lock_attempts_left == 0 {
                // If we can't acquire the lock, we can't read the file
                // We can't continue without the main program, so we use the default program
                tracing::warn!("Failed to acquire filesystem lock");
                return WasmProgram::Default;
            }
            continue;
        };
        let Some(file) = filesystem_reader.read_file_by_hash(&current_main_program) else {
            // If the main program does not exist on the filesystem, we can remove the reference to it
            main_program::set(&None);
            return WasmProgram::Default;
        };
        let Ok(reader) = file.upgrade() else {
            // If the file is not readable, it may have been deleted or is still beeing created.
            // We wait a bit and delete it, if it does not become available
            upgrade_attempts_left = upgrade_attempts_left.saturating_sub(1);
            if upgrade_attempts_left == 0 {
                tracing::warn!("Failed to open main program; Deleting it");
                // TODO: Check that this does not allow for arbitrary file deletion
                let _ = file.delete();
                main_program::set(&None);
                return WasmProgram::Default;
            }
            continue;
        };
        return WasmProgram::MainProgram(reader);
    }
}
