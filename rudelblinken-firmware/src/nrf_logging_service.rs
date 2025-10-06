use crate::service_helpers::DocumentableCharacteristic;
use esp32_nimble::{
    cpfd::{ChrFormat, ChrUnit},
    utilities::{mutex::Mutex, BleUuid},
    uuid128, BLEServer, NimbleProperties,
};
use std::{
    ffi::CStr,
    io::{self, BufRead, Read},
    sync::{Arc, OnceLock},
    u8,
};
use tracing_subscriber::fmt::format::FmtSpan;

// https://docs.nordicsemi.com/bundle/ncs-latest/page/nrf/libraries/bluetooth/services/nus.html#nus-service-readme
const SERIAL_LOGGING_TIO_SERVICE: BleUuid = uuid128!("6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
const SERIAL_LOGGING_TIO_CHAR_RX: BleUuid = uuid128!("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"); // Write no response
const SERIAL_LOGGING_TIO_CHAR_TX: BleUuid = uuid128!("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"); // Notify

extern "C" fn logger(format_string_pointer: *const u8, va_args: *mut core::ffi::c_void) -> i32 {
    let format_string = unsafe { CStr::from_ptr(format_string_pointer) };
    let format_string = format_string.to_bytes();

    let mut format_buffer = [0u8; 1024];
    unsafe {
        esp_idf_sys::snprintf(
            format_buffer.as_mut_ptr(),
            1024,
            format_string.as_ptr(),
            va_args,
        );
    }
    let Ok(formatted_string) = CStr::from_bytes_until_nul(&format_buffer) else {
        return 0;
    };

    write_ble(formatted_string.to_bytes());
    print!("{}", formatted_string.to_string_lossy());

    return formatted_string.to_string_lossy().len() as i32;
}

fn write_ble(content: &[u8]) -> usize {
    // SAFETY: The logger functionality is only used after TX_CHARACTERISTIC has been initialized
    #[allow(static_mut_refs)]
    let Some(ble_logging) = BLE_LOGGING_GLOBALS.get() else {
        return 0;
    };
    if ble_logging.subscribers.lock().len() == 0 {
        // No subscribers, don't send anything
        return content.len();
    }

    let mut tx_characteristic = ble_logging.tx_characteristic.lock();
    let mut sent_bytes = 0;

    for chunk in content.chunks(200) {
        tx_characteristic.set_value(chunk);
        tx_characteristic.notify();

        sent_bytes += chunk.len();
    }

    return sent_bytes;
}

// We need these in the write_ble function, which needs to be used from a C function.
struct BleLoggingGlobals {
    // TODO: Figure out if we really need this
    #[allow(dead_code)]
    rx_characteristic: Arc<Mutex<esp32_nimble::BLECharacteristic>>,
    tx_characteristic: Arc<Mutex<esp32_nimble::BLECharacteristic>>,
    subscribers: Mutex<Vec<[u8; 6]>>,
}
static BLE_LOGGING_GLOBALS: OnceLock<BleLoggingGlobals> = OnceLock::new();

pub struct SerialLoggingService {
    connection: SerialConnection,
}

const BUFFER_SIZE: usize = 512;

#[derive(Debug, Clone)]
struct SerialConnection {
    /// The buffer for the serial connection
    buffer: [u8; BUFFER_SIZE],
    /// The number of bytes in the buffer
    buffer_length: usize,
}

impl SerialConnection {
    fn new() -> SerialConnection {
        SerialConnection {
            buffer: [0u8; BUFFER_SIZE],
            buffer_length: 0,
        }
    }

    /// Adds a line of data to the buffer
    ///
    /// This function gets called when the BLE device sends us a line of data
    fn ble_receive_line(&mut self, data: &[u8]) {
        let read_length = std::cmp::min(data.len(), self.buffer.len() - self.buffer_length);
        if read_length != data.len() {
            tracing::error!("Received more data than we can store in the buffer; truncating");
        }
        self.buffer[self.buffer_length..self.buffer_length + read_length]
            .copy_from_slice(&data[0..read_length]);
        self.buffer_length += read_length;
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
    }
}
struct SerialWriter {}
impl std::io::Write for SerialWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_ble(buf);
        print!("{}", String::from_utf8_lossy(buf));
        return Ok(buf.len());
    }

    fn flush(&mut self) -> io::Result<()> {
        return Ok(());
    }
}

impl SerialLoggingService {
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<SerialLoggingService>> {
        let serial_logging_service = Arc::new(Mutex::new(SerialLoggingService {
            connection: SerialConnection::new(),
        }));

        let service = server.create_service(SERIAL_LOGGING_TIO_SERVICE);

        let tx_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_TX, NimbleProperties::NOTIFY);
        tx_characteristic.document("UART TX", ChrFormat::Struct, 0, ChrUnit::Unitless);

        let rx_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_RX, NimbleProperties::WRITE_NO_RSP);
        rx_characteristic.document("UART RX", ChrFormat::Struct, 0, ChrUnit::Unitless);

        let ble_logging = BleLoggingGlobals {
            rx_characteristic: rx_characteristic.clone(),
            tx_characteristic: tx_characteristic.clone(),
            subscribers: Mutex::new(Vec::new()),
        };
        BLE_LOGGING_GLOBALS.get_or_init(move || ble_logging);

        let cc = serial_logging_service.clone();
        rx_characteristic.lock().on_write(move |args| {
            cc.lock().connection.ble_receive_line(args.recv_data());
        });

        // Track active subscribers
        tx_characteristic.lock().on_subscribe(|_char, desc, sub| {
            let ble_logging = BLE_LOGGING_GLOBALS.get().unwrap();

            tracing::info!("Subscribed: {} {:?}", desc.address(), sub);

            let address = desc.address().as_le_bytes();
            match sub.is_empty() {
                true => {
                    // Unsubscribed
                    ble_logging
                        .subscribers
                        .lock()
                        .retain(|subscriber| *subscriber != address);
                }
                false => {
                    // Subscribed
                    ble_logging.subscribers.lock().push(address);
                }
            }
        });

        // SAFETY: I dont see a reason why this would be unsafe.
        unsafe {
            esp_idf_sys::esp_log_set_vprintf(Some(logger));
        }

        std::panic::set_hook(Box::new(|args| {
            ::tracing::error!(target: "panic", "{}", args);
        }));

        tracing_subscriber::fmt()
            .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
            .with_max_level(tracing::Level::INFO)
            .with_writer(|| SerialWriter {})
            .init();

        serial_logging_service
    }
}
