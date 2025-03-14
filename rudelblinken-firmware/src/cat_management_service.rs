//! The cat management service is reponsible for managing the currently running program and its environment
use crate::config::{self, get_config, set_config, LedStripColor, WasmGuestConfig};
use crate::service_helpers::DocumentableCharacteristic;
use esp32_nimble::BLEServer;
use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    NimbleProperties,
};
use esp_idf_sys::{self as _, BLE_GATT_CHR_UNIT_UNITLESS};
use main_program::WasmRunner;
use rudelblinken_runtime::host::LedColor;
use std::sync::Arc;
use tracing::error;
mod main_program;

const CAT_MANAGEMENT_SERVICE: u16 = 0x7992;
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH: u16 = 0x7893;
const CAT_MANAGEMENT_SERVICE_NAME: u16 = 0x7894;
const CAT_MANAGEMENT_SERVICE_STRIP_COLOR: u16 = 0x7895;
const CAT_MANAGEMENT_SERVICE_WASM_GUEST_CONFIG: u16 = 0x7896;

const CAT_MANAGEMENT_SERVICE_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE);
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_PROGRAM_HASH);
const CAT_MANAGEMENT_SERVICE_NAME_UUID: BleUuid = BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_NAME);
const CAT_MANAGEMENT_SERVICE_STRIP_COLOR_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_STRIP_COLOR);
const CAT_MANAGEMENT_SERVICE_WASM_GUEST_CONFIG_UUID: BleUuid =
    BleUuid::from_uuid16(CAT_MANAGEMENT_SERVICE_WASM_GUEST_CONFIG);

pub struct CatManagementService {
    pub wasm_runner: WasmRunner,
}

impl CatManagementService {
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<CatManagementService>> {
        let wasm_runner = WasmRunner::new();

        let cat_management_service = Arc::new(Mutex::new(CatManagementService {
            wasm_runner: wasm_runner,
        }));

        let service = server.create_service(CAT_MANAGEMENT_SERVICE_UUID);

        let program_hash_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_PROGRAM_HASH_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        program_hash_characteristic.document(
            "Current program hash",
            esp32_nimble::BLE2904Format::UTF8,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        let name_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_NAME_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        name_characteristic.document(
            "Name",
            esp32_nimble::BLE2904Format::UTF8,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );
        let strip_color_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_STRIP_COLOR_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        strip_color_characteristic.document(
            "LED Strip Color (three u8 values)",
            esp32_nimble::BLE2904Format::OPAQUE,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );
        let wasm_guest_config_characteristic = service.lock().create_characteristic(
            CAT_MANAGEMENT_SERVICE_WASM_GUEST_CONFIG_UUID,
            NimbleProperties::WRITE | NimbleProperties::READ,
        );
        wasm_guest_config_characteristic.document(
            "Configuration data for the wasm guest",
            esp32_nimble::BLE2904Format::OPAQUE,
            0,
            BLE_GATT_CHR_UNIT_UNITLESS,
        );

        program_hash_characteristic.lock().on_read(move |value, _| {
            let hash = config::main_program::get();
            value.set_value(&hash.unwrap_or([0u8; 32]));
        });
        let cat_management_service_clone = cat_management_service.clone();
        program_hash_characteristic.lock().on_write(move |args| {
            let mut service = cat_management_service_clone.lock();
            let Ok(hash): Result<[u8; 32], _> = args.recv_data().try_into() else {
                error!("Wrong hash length");
                return;
            };

            service.wasm_runner.set_new_file(&hash);
        });

        name_characteristic.lock().on_read(move |value, _| {
            value.set_value(config::device_name::get().unwrap_or_default().as_bytes());
        });
        name_characteristic.lock().on_write(move |args| {
            let data = args.recv_data();
            if data.len() <= 3 {
                error!("Name too short");
                return;
            }
            if data.len() > 16 {
                error!("Name too long");
                return;
            }

            let Ok(new_name) = String::from_utf8(data.into()) else {
                error!("Name not UTF 8");
                return;
            };

            config::device_name::set(&Some(new_name));
        });

        strip_color_characteristic.lock().on_read(move |value, _| {
            value.set_value(&get_config::<LedStripColor>().to_array());
        });
        strip_color_characteristic.lock().on_write(move |args| {
            let data = args.recv_data();
            if data.len() != 3 {
                error!(
                    len = data.len(),
                    "strip color write with length different from 3"
                );
                return;
            }

            set_config::<LedStripColor>(LedColor::new(data[0], data[1], data[2]));
        });

        wasm_guest_config_characteristic
            .lock()
            .on_read(move |value, _| {
                value.set_value(&get_config::<WasmGuestConfig>());
            });
        wasm_guest_config_characteristic
            .lock()
            .on_write(move |args| {
                set_config::<WasmGuestConfig>(args.recv_data().to_vec());
            });

        // TODO: Age files on file system

        cat_management_service
    }
}
