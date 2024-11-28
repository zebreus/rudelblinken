use std::{
    backtrace::Backtrace,
    borrow::BorrowMut,
    ffi::CStr,
    io::{Read, Seek, Stdout, Write},
    os::fd::AsRawFd,
    sync::Arc,
};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    uuid128, BLEServer, DescriptorProperties, NimbleProperties,
};
use esp_idf_sys::{
    esp_log_timestamp, esp_log_write, fclose, funopen, fwrite, snprintf, sprintf,
    CONFIG_LOG_MAXIMUM_LEVEL,
};
use log::{Level, LevelFilter, Record};
use rudelblinken_filesystem::{
    file::{File as FileContent, FileState},
    Filesystem,
};
use thiserror::Error;

use crate::storage::{get_filesystem, FlashStorage};

// https://play.google.com/store/apps/details?id=com.telit.tiosample
// https://web.archive.org/web/20190121050719/https://www.telit.com/wp-content/uploads/2017/09/TIO_Implementation_Guide_r6.pdf
const SERIAL_LOGGING_TIO_SERVICE: BleUuid = uuid128!("0000FEFB-0000-1000-8000-00805F9B34FB");
const SERIAL_LOGGING_TIO_CHAR_RX: BleUuid = uuid128!("00000001-0000-1000-8000-008025000000"); // N
const SERIAL_LOGGING_TIO_CHAR_TX: BleUuid = uuid128!("00000002-0000-1000-8000-008025000000"); // WNR
const SERIAL_LOGGING_TIO_CHAR_RX_CREDITS: BleUuid =
    uuid128!("00000003-0000-1000-8000-008025000000"); // I
const SERIAL_LOGGING_TIO_CHAR_TX_CREDITS: BleUuid =
    uuid128!("00000004-0000-1000-8000-008025000000"); // W

pub struct SerialLoggingService {
    last_error: Option<SerialLoggingError>,
}

#[derive(Error, Debug, Clone)]
pub enum SerialLoggingError {
    #[error("There is no checksum file with the supplied hash")]
    ChecksumFileDoesNotExist,
}

pub struct BleLogger;

static LOGGER: BleLogger = BleLogger;

impl BleLogger {
    pub fn initialize_default() {
        ::log::set_logger(&LOGGER)
            .map(|()| LOGGER.initialize())
            .unwrap();
    }

    pub fn initialize(&self) {
        ::log::set_max_level(self.get_max_level());
    }

    pub fn get_max_level(&self) -> LevelFilter {
        LevelFilter::max()
    }

    fn get_marker(level: Level) -> &'static str {
        match level {
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug => "D",
            Level::Trace => "V",
        }
    }

    fn get_color(level: Level) -> Option<u8> {
        {
            match level {
                Level::Error => Some(31), // LOG_COLOR_RED
                Level::Warn => Some(33),  // LOG_COLOR_BROWN
                Level::Info => Some(32),  // LOG_COLOR_GREEN,
                _ => None,
            }
        }
    }

    // fn should_log(record: &Record) -> bool {

    //     // esp-idf function `esp_log_level_get` builds a cache using the address
    //     // of the target and not doing a string compare.  This means we need to
    //     // build a cache of our own mapping the str value to a consistant
    //     // Cstr value.
    //     static TARGET_CACHE: Mutex<BTreeMap<alloc::string::String, CString>> =
    //         Mutex::new(BTreeMap::new());
    //     let level = Newtype::<esp_log_level_t>::from(record.level()).0;

    //     let mut cache = TARGET_CACHE.lock();

    //     let ctarget = loop {
    //         if let Some(ctarget) = cache.get(record.target()) {
    //             break ctarget;
    //         }

    //         if let Ok(ctarget) = to_cstring_arg(record.target()) {
    //             cache.insert(record.target().into(), ctarget);
    //         } else {
    //             return true;
    //         }
    //     };

    //     let max_level = unsafe { esp_log_level_get(ctarget.as_c_str().as_ptr()) };
    //     level <= max_level
    // }
}

impl ::log::Log for BleLogger {
    fn enabled(&self, metadata: &::log::Metadata) -> bool {
        metadata.level() <= ::log::Level::Error
    }

    fn log(&self, record: &::log::Record) {
        let metadata = record.metadata();
        if true {
            let marker = Self::get_marker(metadata.level());
            let timestamp = unsafe { esp_log_timestamp() };
            let target = record.metadata().target();
            let args = record.args();
            let color = Self::get_color(record.level());

            // let mut stdout = EspStdout::new();
            let text: String;
            if let Some(color) = color {
                text = format!(
                    "\x1b[0;{}m{} ({}) {}: {}\x1b[0m\n",
                    color, marker, timestamp, target, args
                );
            } else {
                text = format!("{} ({}) {}: {}\n", marker, timestamp, target, args);
            }

            write_ble(text.as_bytes());
            print!("{}", text);
        }
    }

    fn flush(&self) {}
}

static mut RX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut TX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut RX_CREDITS_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut TX_CREDITS_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static TX_CREDITS: std::sync::RwLock<u8> = std::sync::RwLock::new(0);
static RX_CREDITS: std::sync::RwLock<u8> = std::sync::RwLock::new(0);

// unsafe extern "C" fn(_: *const i8, _: *mut c_void) -> i32
extern "C" fn logger(format_string_pointer: *const i8, va_args: *mut core::ffi::c_void) -> i32 {
    let format_string = unsafe { CStr::from_ptr(format_string_pointer) };
    let format_string = format_string.to_bytes();

    let mut format_buffer = [0u8; 1024];
    unsafe {
        esp_idf_sys::snprintf(
            format_buffer.as_mut_ptr() as *mut i8,
            1024,
            format_string.as_ptr() as *const i8,
            va_args,
        );
    }
    let Ok(formatted_string) = CStr::from_bytes_until_nul(&format_buffer) else {
        return 0;
    };

    write_ble(formatted_string.to_bytes());
    print!("{}", formatted_string.to_string_lossy());

    return format_string.len() as i32;
}

fn write_ble(content: &[u8]) -> usize {
    let Some(tx_characteristic) = (unsafe { TX_CHARACTERISTIC.as_ref() }) else {
        return 0;
    };

    let mut tx_characteristic = tx_characteristic.lock();

    let mut sent_bytes = 0;

    for chunk in content.chunks(20) {
        {
            let Ok(mut rx_credits) = RX_CREDITS.write() else {
                // Failed to send bytes; no credits available
                return sent_bytes;
            };
            if *rx_credits == 0 {
                // No credits available
                return sent_bytes;
            }

            *rx_credits -= 1;
            // println!("Remaining credits: {}", *rx_credits);
        }
        tx_characteristic.set_value(chunk);
        tx_characteristic.notify();
        sent_bytes += chunk.len();
    }

    return sent_bytes;
}

// struct MyCookie {
//     old_stdout: *mut esp_idf_sys::__sFILE,
// }

impl SerialLoggingService {
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<SerialLoggingService>> {
        let file_upload_service = Arc::new(Mutex::new(SerialLoggingService { last_error: None }));

        let service = server.create_service(SERIAL_LOGGING_TIO_SERVICE);

        let tx_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_TX, NimbleProperties::NOTIFY);
        tx_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::OPAQUE)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        tx_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("UART TX".as_bytes());

        let tx_credits_characteristic = service.lock().create_characteristic(
            SERIAL_LOGGING_TIO_CHAR_TX_CREDITS,
            NimbleProperties::INDICATE,
        );
        tx_credits_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        tx_credits_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("UART credits TX".as_bytes());

        let rx_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_RX, NimbleProperties::WRITE_NO_RSP);
        rx_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::OPAQUE)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        rx_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("UART RX".as_bytes());

        let rx_credits_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_RX_CREDITS, NimbleProperties::WRITE);
        rx_credits_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        rx_credits_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("UART credits RX".as_bytes());

        rx_characteristic.lock().on_write(move |args| {
            ::log::error!("RX write {:?}", args.recv_data());
        });
        tx_characteristic.lock().on_write(move |args| {
            ::log::error!("TX write {:?}", args.recv_data());
        });
        rx_credits_characteristic.lock().on_write(move |args| {
            ::log::error!("RX credits write {:?}", args.recv_data());
        });
        rx_credits_characteristic.lock().on_write(move |args| {
            let received_data = args.recv_data();
            if received_data.len() != 1 {
                ::log::error!(
                    "Received invalid data for RX credits: {} bytes, expected 1",
                    received_data.len()
                );
                return;
            }
            let new_credits = received_data.first().unwrap();

            {
                let Ok(mut rx_credits) = RX_CREDITS.write() else {
                    ::log::error!("Failed to acquire lock on the write credits store.");
                    return;
                };
                *rx_credits = *new_credits;
            }

            ::log::info!("Received {} credits", new_credits);
        });

        tx_credits_characteristic.lock().on_read(|_, _| {
            ::log::error!("TX credits read");
        });
        rx_credits_characteristic.lock().on_read(|_, _| {
            ::log::error!("RX credits read");
        });
        tx_characteristic.lock().on_read(|_, _| {
            ::log::error!("TX read");
        });
        rx_characteristic.lock().on_read(|_, _| {
            ::log::error!("RX read");
        });

        unsafe {
            RX_CHARACTERISTIC = Some(rx_characteristic);
            TX_CHARACTERISTIC = Some(tx_characteristic);
            RX_CREDITS_CHARACTERISTIC = Some(rx_credits_characteristic);
            TX_CREDITS_CHARACTERISTIC = Some(tx_credits_characteristic);
            esp_idf_sys::esp_log_set_vprintf(Some(logger));
        }
        std::panic::set_hook(Box::new(|args| {
            ::log::error!(target: "panic", "{}", args);
        }));
        BleLogger::initialize_default();

        // // Replace stdout and stderr with a function
        // unsafe {
        //     let reent = esp_idf_sys::__getreent();
        //     let old_stdout = (*reent)._stdout;
        //     let old_stderr = (*reent)._stderr;
        //     OLD_STDOUT = Some(old_stdout);
        //     OLD_STDERR = Some(old_stderr);
        //     let new_stdout = funopen(
        //         "a".as_ptr() as *mut core::ffi::c_void,
        //         None,
        //         Some(write_to_stdout_and_ble),
        //         None,
        //         None,
        //     );
        //     let new_stderr = funopen(
        //         "a".as_ptr() as *mut core::ffi::c_void,
        //         None,
        //         Some(write_to_stdout_and_ble),
        //         None,
        //         None,
        //     );
        //     (*reent)._stdout = new_stdout;
        //     (*reent)._stderr = new_stderr;
        // }

        file_upload_service
    }
}

// static mut OLD_STDOUT: Option<*mut esp_idf_sys::__sFILE> = None;
// static mut OLD_STDERR: Option<*mut esp_idf_sys::__sFILE> = None;

// extern "C" fn write_to_stdout_and_ble(
//     _cookie: *mut core::ffi::c_void,
//     buffer: *const core::ffi::c_char,
//     length: core::ffi::c_int,
// ) -> ::core::ffi::c_int {
//     let bytes = unsafe { std::slice::from_raw_parts(buffer as *const u8, length as usize) };

//     for chunk in bytes.chunks(20) {
//         if let Some(rx_characteristic) = unsafe { RX_CHARACTERISTIC.as_ref() } {
//             let mut rx_characteristic = rx_characteristic.lock();
//             rx_characteristic.set_value(chunk);
//             rx_characteristic.notify();
//         }
//         if let Some(old_stdout) = unsafe { OLD_STDOUT.as_ref() } {
//             unsafe {
//                 esp_idf_sys::fwrite(
//                     chunk.as_ptr() as *const core::ffi::c_void,
//                     chunk.len() as u32,
//                     1,
//                     *old_stdout,
//                 );
//             }
//         }
//     }

//     return bytes.len() as i32;
// }

// extern "C" fn write_to_stderr_and_ble(
//     _cookie: *mut core::ffi::c_void,
//     buffer: *const core::ffi::c_char,
//     length: core::ffi::c_int,
// ) -> ::core::ffi::c_int {
//     let bytes = unsafe { std::slice::from_raw_parts(buffer as *const u8, length as usize) };

//     for chunk in bytes.chunks(20) {
//         if let Some(rx_characteristic) = unsafe { RX_CHARACTERISTIC.as_ref() } {
//             let mut rx_characteristic = rx_characteristic.lock();
//             rx_characteristic.set_value(chunk);
//             rx_characteristic.notify();
//         }
//         if let Some(old_stderr) = unsafe { OLD_STDERR.as_ref() } {
//             unsafe {
//                 esp_idf_sys::fwrite(
//                     chunk.as_ptr() as *const core::ffi::c_void,
//                     chunk.len() as u32,
//                     1,
//                     *old_stderr,
//                 );
//             }
//         }
//     }

//     return bytes.len() as i32;
// }
