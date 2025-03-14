use crate::{
    file_upload_service::{upload_request::UploadRequest, FileUploadError},
    service_helpers::DocumentableCharacteristic,
};
use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLE2904Format, BLEServer, BLEService, NimbleProperties,
};
use esp_idf_sys::{ble_svc_gatt_changed, BLE_GATT_CHR_UNIT_UNITLESS};
use std::sync::Arc;
use zerocopy::TryFromBytes;

use super::FileUploadService;

const FILE_UPLOAD_SERVICE: u16 = 0x9160;
// Write data chunks here
const FILE_UPLOAD_SERVICE_DATA: u16 = 0x9161;
// Write metadata here to initiate an upload.
const FILE_UPLOAD_SERVICE_START_UPLOAD: u16 = 0x9162;
// Read this to get the number of uploaded chunks and the IDs of some missing chunks. Returns a list of u16
const FILE_UPLOAD_SERVICE_UPLOAD_PROGRESS: u16 = 0x9163;
// Read here to get the last error as a string
const FILE_UPLOAD_SERVICE_LAST_ERROR: u16 = 0x9164;
// Read to get the hash of the current upload.
const FILE_UPLOAD_SERVICE_CURRENT_HASH: u16 = 0x9166;

const FILE_UPLOAD_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE);
const FILE_UPLOAD_SERVICE_DATA_UUID: BleUuid = BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_DATA);
const FILE_UPLOAD_SERVICE_START_UPLOAD_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_START_UPLOAD);
const FILE_UPLOAD_SERVICE_MISSING_CHUNKS_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_UPLOAD_PROGRESS);
const FILE_UPLOAD_SERVICE_LAST_ERROR_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_LAST_ERROR);
const FILE_UPLOAD_SERVICE_CURRENT_HASH_UUID: BleUuid =
    BleUuid::from_uuid16(FILE_UPLOAD_SERVICE_CURRENT_HASH);

fn setup_service(server: &mut BLEServer) -> Arc<Mutex<BLEService>> {
    server.create_service(FILE_UPLOAD_SERVICE_UUID)
}

fn setup_data_characteristic(
    service: &Arc<Mutex<BLEService>>,
    file_upload_service: &Arc<Mutex<FileUploadService>>,
) {
    let data_characteristic = service.lock().create_characteristic(
        FILE_UPLOAD_SERVICE_DATA_UUID,
        NimbleProperties::WRITE_NO_RSP | NimbleProperties::WRITE,
    );
    data_characteristic.document(
        "Chunk Upload",
        BLE2904Format::OPAQUE,
        0,
        BLE_GATT_CHR_UNIT_UNITLESS,
    );

    let file_upload_service_clone = file_upload_service.clone();
    data_characteristic.lock().on_write(move |args| {
        let mut service = file_upload_service_clone.lock();
        let chunk = args.recv_data();
        if let Err(e) = service.data_write(chunk) {
            service.log_error(e);
        }
    });
}

fn setup_upload_request_characteristic(
    service: &Arc<Mutex<BLEService>>,
    file_upload_service: &Arc<Mutex<FileUploadService>>,
) {
    // Write a upload request to start a new upload.
    // Read to get the hash of the current upload.
    let upload_request_characteristic = service.lock().create_characteristic(
        FILE_UPLOAD_SERVICE_START_UPLOAD_UUID,
        NimbleProperties::READ | NimbleProperties::WRITE,
    );
    upload_request_characteristic.document(
        "File Upload Request",
        BLE2904Format::OPAQUE,
        0,
        BLE_GATT_CHR_UNIT_UNITLESS,
    );

    let file_upload_service_clone = file_upload_service.clone();
    upload_request_characteristic.lock().on_write(move |args| {
        println!("Writing upload request");
        let mut service = file_upload_service_clone.lock();
        let received_data = args.recv_data();
        let upload_request = match UploadRequest::try_ref_from_bytes(received_data) {
            Ok(upload_request) => upload_request,
            Err(e) => {
                service.log_error(FileUploadError::MalformedUploadRequest(e.to_string()));
                return;
            }
        };

        if let Err(e) = service.start_upload(upload_request) {
            service.log_error(e);
        }
        unsafe {
            ble_svc_gatt_changed(FILE_UPLOAD_SERVICE_DATA, FILE_UPLOAD_SERVICE_DATA);
        };
    });
}

fn setup_current_hash_characteristic(
    service: &Arc<Mutex<BLEService>>,
    file_upload_service: &Arc<Mutex<FileUploadService>>,
) {
    let current_hash_characteristic = service.lock().create_characteristic(
        FILE_UPLOAD_SERVICE_CURRENT_HASH_UUID,
        NimbleProperties::READ,
    );
    current_hash_characteristic.document(
        "Hash of the current upload",
        BLE2904Format::OPAQUE,
        0,
        BLE_GATT_CHR_UNIT_UNITLESS,
    );

    let file_upload_service_clone = file_upload_service.clone();
    current_hash_characteristic.lock().on_read(move |value, _| {
        println!("Read current hash");
        let service = file_upload_service_clone.lock();
        let current_hash = match service.current_hash() {
            Some(current_hash) => current_hash,
            None => &[0u8; 32],
        };
        value.set_value(current_hash);
    });
}

fn setup_upload_status_characteristic(
    service: &Arc<Mutex<BLEService>>,
    file_upload_service: &Arc<Mutex<FileUploadService>>,
) {
    let upload_status_characteristic = service.lock().create_characteristic(
        FILE_UPLOAD_SERVICE_MISSING_CHUNKS_UUID,
        NimbleProperties::READ,
    );
    upload_status_characteristic.document(
        "Number of received chunks + Missing Chunks",
        BLE2904Format::OPAQUE,
        0,
        BLE_GATT_CHR_UNIT_UNITLESS,
    );

    let file_upload_service_clone = file_upload_service.clone();
    upload_status_characteristic
        .lock()
        .on_read(move |value, _| {
            let service = file_upload_service_clone.lock();
            let (progress, missing_chunks) = service.get_status().unwrap_or((0, Vec::new()));

            let mut upload_status: Vec<u8> = Vec::new();
            upload_status.extend_from_slice(&progress.to_le_bytes());
            upload_status.extend(
                missing_chunks
                    .into_iter()
                    .take(100)
                    .flat_map(u16::to_le_bytes),
            );

            value.set_value(&upload_status);
        });
}

// TODO: Refactor and actually use last error
fn setup_last_error_characteristic(
    service: &Arc<Mutex<BLEService>>,
    file_upload_service: &Arc<Mutex<FileUploadService>>,
) {
    let last_error_characteristic = service
        .lock()
        .create_characteristic(FILE_UPLOAD_SERVICE_LAST_ERROR_UUID, NimbleProperties::READ);
    last_error_characteristic.document(
        "Last error code",
        BLE2904Format::UINT16,
        0,
        BLE_GATT_CHR_UNIT_UNITLESS,
    );

    let file_upload_service_clone = file_upload_service.clone();
    last_error_characteristic.lock().on_read(move |value, _| {
        let service = file_upload_service_clone.lock();
        let Some(last_error) = &service.last_error else {
            value.set_value(&[]);
            return;
        };

        value.set_value(&(unsafe { *<*const _>::from(last_error).cast::<u8>() }).to_le_bytes());
    });
}

impl FileUploadService {
    // TODO: We should only allow one active upload service at a time.
    /// Create a new FileUploadService and set up the necessary characteristics.
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<Self>> {
        let file_upload_service = Arc::new(Mutex::new(FileUploadService {
            currently_receiving: None,
            last_error: None,
        }));

        let service = setup_service(server);
        setup_data_characteristic(&service, &file_upload_service);
        setup_upload_request_characteristic(&service, &file_upload_service);
        setup_current_hash_characteristic(&service, &file_upload_service);
        setup_upload_status_characteristic(&service, &file_upload_service);
        setup_last_error_characteristic(&service, &file_upload_service);

        file_upload_service
    }
}
