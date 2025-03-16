//! Load the main program from the filesystem or return the default program
use crate::config::main_program;
use crate::storage::get_filesystem;
use crate::{storage::FlashStorage, wasm_service::wasm_host::WasmHost};
use rudelblinken_filesystem::file::{File, FileState};
use std::time::Duration;

/// The delay between attempts to load the main program
const LOAD_MAIN_PROGRAM_RETRY_DELAY: Duration = Duration::from_millis(200);
/// The max number of attempts to acquire the filesystem lock before returning the default program
const MAX_MAIN_PROGRAM_FS_LOCK_ATTEMPTS: usize = 50;
/// The max number of attempts to read the main program before deleting it and returning the default program
const MAX_MAIN_PROGRAM_UPGRADE_ATTEMPTS: usize = 5;

const DEFAULT_MAIN_PROGRAM: &[u8] =
    include_bytes!("../../../../wasm-binaries/binaries/board_test.wasm");

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
            WasmProgram::Default => DEFAULT_MAIN_PROGRAM,
            WasmProgram::MainProgram(file) => &file,
        }
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
