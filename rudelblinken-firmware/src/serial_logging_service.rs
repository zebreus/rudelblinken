use std::{
    ffi::CStr,
    io::{BufRead, Read},
    sync::Arc,
    u8,
};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    uuid128, BLEServer, DescriptorProperties, NimbleProperties,
};
use esp_idf_sys::esp_log_timestamp;
use log::{Level, LevelFilter};

pub struct BleLogger;
static LOGGER: BleLogger = BleLogger;
impl BleLogger {
    pub fn initialize_default() {
        ::log::set_logger(&LOGGER)
            .map(|()| LOGGER.initialize())
            .unwrap();
    }

    pub fn initialize(&self) {
        ::log::set_max_level(LevelFilter::max());
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
}

impl ::log::Log for BleLogger {
    fn enabled(&self, metadata: &::log::Metadata) -> bool {
        metadata.level() <= ::log::Level::Error
    }

    fn log(&self, record: &::log::Record) {
        let metadata = record.metadata();
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

    fn flush(&self) {}
}

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

// https://play.google.com/store/apps/details?id=com.telit.tiosample
// https://web.archive.org/web/20190121050719/https://www.telit.com/wp-content/uploads/2017/09/TIO_Implementation_Guide_r6.pdf
const SERIAL_LOGGING_TIO_SERVICE: BleUuid = uuid128!("0000FEFB-0000-1000-8000-00805F9B34FB");
const SERIAL_LOGGING_TIO_CHAR_RX: BleUuid = uuid128!("00000001-0000-1000-8000-008025000000"); // Notify
const SERIAL_LOGGING_TIO_CHAR_TX: BleUuid = uuid128!("00000002-0000-1000-8000-008025000000"); // Write no response
const SERIAL_LOGGING_TIO_CHAR_RX_CREDITS: BleUuid =
    uuid128!("00000003-0000-1000-8000-008025000000"); // Indicate
const SERIAL_LOGGING_TIO_CHAR_TX_CREDITS: BleUuid =
    uuid128!("00000004-0000-1000-8000-008025000000"); // Write

const BUFFER_SIZE: usize = 512;

static mut RX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut TX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut RX_CREDITS_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut TX_CREDITS_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static RX_CREDITS: std::sync::RwLock<u8> = std::sync::RwLock::new(0);

pub struct SerialLoggingService {
    connection: SerialConnection,
}

#[derive(Debug, Clone)]
struct SerialConnection {
    /// The buffer for the serial connection
    buffer: [u8; 512],
    /// The number of bytes in the buffer
    buffer_length: usize,
    /// How many credits the remote device has
    remote_credits: u8,
}

impl SerialConnection {
    fn new() -> SerialConnection {
        SerialConnection {
            buffer: [0u8; BUFFER_SIZE],
            buffer_length: 0,
            remote_credits: 0,
        }
    }

    /// Get the amount of credits that we can give
    fn credits(&self) -> u8 {
        let remaining_bytes = BUFFER_SIZE - self.buffer_length;
        let remaining_credits = remaining_bytes / 20;
        let remaining_credits: u8 = remaining_credits.try_into().unwrap_or(u8::MAX);
        return remaining_credits;
    }

    /// Update the credits on all remote devices, if necessary
    fn update_credits(&mut self) {
        let remote_credits = self.remote_credits;
        let local_credits = self.credits();
        let credits_diff = local_credits.abs_diff(remote_credits);
        if credits_diff == 0 || (credits_diff < 10 && remote_credits > 2) {
            // log::debug!("Credits diff is less than 10; not updating credits");
            return;
        }
        self.notify_credits();
    }

    /// Update all connected devices with the current amount of credits
    fn notify_credits(&mut self) {
        let local_credits = self.credits();
        let Some(tx_credits_characteristic) = (unsafe { TX_CREDITS_CHARACTERISTIC.as_ref() })
        else {
            return;
        };
        let mut tx_credits_characteristic = tx_credits_characteristic.lock();
        tx_credits_characteristic.set_value(&[local_credits]);
        tx_credits_characteristic.notify();
        self.remote_credits = local_credits;
    }

    /// Adds a line of data to the buffer
    ///
    /// This function gets called when the BLE device sends us a line of data
    fn ble_receive_line(&mut self, data: &[u8]) {
        let read_length = std::cmp::min(data.len(), self.buffer.len() - self.buffer_length);
        if read_length != data.len() {
            log::error!("Received more data than we can store in the buffer; truncating");
            log::error!("Maybe your client doesn't respect the credits?");
        }
        self.buffer[self.buffer_length..self.buffer_length + read_length]
            .copy_from_slice(&data[0..read_length]);
        self.buffer_length += read_length;
        self.update_credits();
    }

    fn reset(&mut self) {
        self.buffer_length = 0;
        self.update_credits();
    }
}

impl Read for SerialConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read_length = std::cmp::min(buf.len(), self.buffer_length);
        if read_length == 0 {
            return Ok(0);
        }

        buf[0..read_length].copy_from_slice(&self.buffer[0..read_length]);
        self.consume(read_length);
        return Ok(read_length);
    }
}
impl BufRead for SerialConnection {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        return Ok(&self.buffer[0..self.buffer_length]);
    }

    fn consume(&mut self, amt: usize) {
        let read_length = std::cmp::min(amt, self.buffer_length);
        self.buffer.copy_within(read_length.., 0);
        self.buffer_length -= read_length;
        self.update_credits();
    }
}

impl SerialLoggingService {
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<SerialLoggingService>> {
        let file_upload_service = Arc::new(Mutex::new(SerialLoggingService {
            connection: SerialConnection::new(),
        }));

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

        let cc = file_upload_service.clone();
        rx_characteristic.lock().on_write(move |args| {
            cc.lock().connection.ble_receive_line(args.recv_data());
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

            ::log::debug!("Received {} credits", new_credits);
        });
        let cc = file_upload_service.clone();
        tx_credits_characteristic
            .lock()
            .on_subscribe(move |this, _, _| {
                if this.subscribed_count() == 0 {
                    cc.lock().connection.reset();
                    return;
                }
                cc.lock().connection.notify_credits();
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

        file_upload_service
    }
}
