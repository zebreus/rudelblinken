use crate::service_helpers::DocumentableCharacteristic;
use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    uuid128, BLE2904Format, BLEServer, NimbleProperties,
};
use esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS;
use std::{
    ffi::CStr,
    io::{self, BufRead, Read},
    sync::Arc,
    u8,
};

// https://docs.nordicsemi.com/bundle/ncs-latest/page/nrf/libraries/bluetooth/services/nus.html#nus-service-readme
const SERIAL_LOGGING_TIO_SERVICE: BleUuid = uuid128!("6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
const SERIAL_LOGGING_TIO_CHAR_RX: BleUuid = uuid128!("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"); // Write no response
const SERIAL_LOGGING_TIO_CHAR_TX: BleUuid = uuid128!("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"); // Notify

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

    return formatted_string.to_string_lossy().len() as i32;
}

fn write_ble(content: &[u8]) -> usize {
    // SAFETY: The logger functionality is only used after TX_CHARACTERISTIC has been initialized
    #[allow(static_mut_refs)]
    let Some(tx_characteristic) = (unsafe { TX_CHARACTERISTIC.as_ref() }) else {
        return 0;
    };

    let mut tx_characteristic = tx_characteristic.lock();
    let mut sent_bytes = 0;

    for chunk in content.chunks(200) {
        tx_characteristic.set_value(chunk);
        tx_characteristic.notify();

        sent_bytes += chunk.len();
    }

    return sent_bytes;
}

static mut RX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;
static mut TX_CHARACTERISTIC: Option<Arc<Mutex<esp32_nimble::BLECharacteristic>>> = None;

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
        tx_characteristic.document(
            "UART TX",
            BLE2904Format::OPAQUE,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        let rx_characteristic = service
            .lock()
            .create_characteristic(SERIAL_LOGGING_TIO_CHAR_RX, NimbleProperties::WRITE_NO_RSP);
        rx_characteristic.document(
            "UART RX",
            BLE2904Format::OPAQUE,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        let cc = serial_logging_service.clone();
        rx_characteristic.lock().on_write(move |args| {
            cc.lock().connection.ble_receive_line(args.recv_data());
        });

        unsafe {
            RX_CHARACTERISTIC = Some(rx_characteristic);
            TX_CHARACTERISTIC = Some(tx_characteristic);
            esp_idf_sys::esp_log_set_vprintf(Some(logger));
        }
        std::panic::set_hook(Box::new(|args| {
            ::tracing::error!(target: "panic", "{}", args);
        }));

        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(|| SerialWriter {})
            .init();

        serial_logging_service
    }
}
