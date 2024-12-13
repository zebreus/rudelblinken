use std::{sync::Arc, u8};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    uuid128, BLEServer, DescriptorProperties, NimbleProperties,
};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

const BOOT_CONFIG_SERVICE: BleUuid = uuid128!("B007B007-0000-1000-8000-00805F9B34FB");
const SWITCH_COLORS_CHARACTERISTIC: BleUuid = uuid128!("00000001-0000-1000-8000-008025000000");
const MODE_CHARACTERISTIC: BleUuid = uuid128!("00000002-0000-1000-8000-008025000000");
const SPEED_CHARACTERISTIC: BleUuid = uuid128!("00000003-0000-1000-8000-008025000000");
const BLUE_MULTIPLIER_CHARACTERISTIC: BleUuid = uuid128!("00000004-0000-1000-8000-008025000000");
const RED_MULTIPLIER_CHARACTERISTIC: BleUuid = uuid128!("00000005-0000-1000-8000-008025000000");
const RED_LED_CHARACTERISTIC: BleUuid = uuid128!("00000006-0000-1000-8000-008025000000");
const BLUE_LED_CHARACTERISTIC: BleUuid = uuid128!("00000007-0000-1000-8000-008025000000");

#[derive(Debug, Clone)]
pub struct BootConfigService {
    pub switch_colors: bool,
    pub mode: u8,
    pub speed: u8,
    pub red_brightness: u8,
    pub blue_brightness: u8,
    pub red_pin: u8,
    pub blue_pin: u8,
}

// impl BootConfigService {
//     /// Create a new state
//     pub fn new() -> Self {
//         // grab storage partition

//         // return state
//         Self {
//             id,
//             channel,
//             intensity,
//             action,
//             nvs,
//         }
//     }
//     /// Save the state to storage
//     fn store(&self) {
//         self.nvs.set_u16("id", self.id).unwrap();
//         self.nvs.set_u8("intensity", self.intensity).unwrap();
//         self.nvs.set_u8("action", self.action as u8).unwrap();
//         self.nvs.set_u8("channel", self.channel as u8).unwrap();
//     }
// }

impl BootConfigService {
    pub fn new(server: &mut BLEServer) -> Arc<Mutex<BootConfigService>> {
        let nvs_default_partition = EspDefaultNvsPartition::take().unwrap();
        let nvs = Arc::new(Mutex::new(
            EspNvs::new(nvs_default_partition, "boot_config", true).unwrap(),
        ));

        // grab values from storage

        let switch_colors = nvs
            .lock()
            .get_u8("switch_colors")
            .unwrap_or(None)
            .unwrap_or(0)
            == 0;
        let mode = nvs.lock().get_u8("mode").unwrap_or(None).unwrap_or(0);
        let speed = nvs.lock().get_u8("speed").unwrap_or(None).unwrap_or(100);
        let red_brightness = nvs
            .lock()
            .get_u8("red_brightness")
            .unwrap_or(None)
            .unwrap_or(255);
        let blue_brightness = nvs
            .lock()
            .get_u8("blue_brightness")
            .unwrap_or(None)
            .unwrap_or(255);
        let blue_pin = nvs.lock().get_u8("blue_pin").unwrap_or(None).unwrap_or(8);
        let red_pin = nvs.lock().get_u8("red_pin").unwrap_or(None).unwrap_or(6);

        let file_upload_service = Arc::new(Mutex::new(BootConfigService {
            switch_colors,
            mode,
            speed,
            red_brightness,
            blue_brightness,
            blue_pin,
            red_pin,
        }));

        let service = server.create_service(BOOT_CONFIG_SERVICE);

        let switch_colors_characteristic = service.lock().create_characteristic(
            SWITCH_COLORS_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        switch_colors_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::BOOLEAN)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        switch_colors_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Switch colors".as_bytes());

        let mode_characteristic = service.lock().create_characteristic(
            MODE_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        mode_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        mode_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Mode ???".as_bytes());

        let speed_characteristic = service.lock().create_characteristic(
            SPEED_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        speed_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        speed_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Speed".as_bytes());

        let red_multiplier_characteristic = service.lock().create_characteristic(
            RED_MULTIPLIER_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        red_multiplier_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        red_multiplier_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Red brightness multiplier".as_bytes());

        let blue_multiplier_characteristic = service.lock().create_characteristic(
            BLUE_MULTIPLIER_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        blue_multiplier_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        blue_multiplier_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Blue brightness multiplier".as_bytes());

        let red_led_characteristic = service.lock().create_characteristic(
            RED_LED_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        red_led_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        red_led_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Red LED pin".as_bytes());

        let blue_led_characteristic = service.lock().create_characteristic(
            BLUE_LED_CHARACTERISTIC,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        blue_led_characteristic
            .lock()
            .create_2904_descriptor()
            .format(esp32_nimble::BLE2904Format::UINT8)
            .exponent(0)
            .unit(esp_idf_sys::BLE_GATT_CHR_UNIT_UNITLESS as u16)
            .namespace(0x01)
            .description(0x00);
        blue_led_characteristic
            .lock()
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value("Blue LED pin".as_bytes());

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        switch_colors_characteristic.lock().on_write(move |args| {
            cc.lock().switch_colors = args.recv_data().first().unwrap() != &0;
            nvs_clone
                .lock()
                .set_u8("switch_colors", cc.lock().switch_colors as u8)
                .unwrap();
        });
        let cc = file_upload_service.clone();
        switch_colors_characteristic.lock().on_read(move |args, _| {
            let data = if cc.lock().switch_colors { [1] } else { [0] };
            args.set_value(&data);
        });

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        mode_characteristic.lock().on_write(move |args| {
            cc.lock().mode = *args.recv_data().first().unwrap();
            nvs_clone.lock().set_u8("mode", cc.lock().mode).unwrap();
        });
        let cc = file_upload_service.clone();
        mode_characteristic.lock().on_read(move |args, _| {
            let data = [cc.lock().mode];
            args.set_value(&data);
        });

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        speed_characteristic.lock().on_write(move |args| {
            cc.lock().speed = *args.recv_data().first().unwrap();
            nvs_clone.lock().set_u8("speed", cc.lock().speed).unwrap();
        });
        let cc = file_upload_service.clone();
        speed_characteristic.lock().on_read(move |args, _| {
            let data = [cc.lock().speed];
            args.set_value(&data);
        });

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        red_multiplier_characteristic.lock().on_write(move |args| {
            cc.lock().red_brightness = *args.recv_data().first().unwrap();
            nvs_clone
                .lock()
                .set_u8("red_brightness", cc.lock().red_brightness)
                .unwrap();
        });
        let cc = file_upload_service.clone();
        red_multiplier_characteristic
            .lock()
            .on_read(move |args, _| {
                let data = [cc.lock().red_brightness];
                args.set_value(&data);
            });

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        blue_multiplier_characteristic.lock().on_write(move |args| {
            cc.lock().blue_brightness = *args.recv_data().first().unwrap();
        });
        let cc = file_upload_service.clone();
        blue_multiplier_characteristic
            .lock()
            .on_read(move |args, _| {
                let data = [cc.lock().blue_brightness];
                args.set_value(&data);
            });

        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        red_led_characteristic.lock().on_write(move |args| {
            cc.lock().red_pin = *args.recv_data().first().unwrap();
            nvs_clone
                .lock()
                .set_u8("red_pin", cc.lock().red_pin)
                .unwrap();
        });
        let cc = file_upload_service.clone();
        red_led_characteristic.lock().on_read(move |args, _| {
            let data = [cc.lock().red_pin];
            args.set_value(&data);
        });
        let cc = file_upload_service.clone();
        let nvs_clone = nvs.clone();
        blue_led_characteristic.lock().on_write(move |args| {
            cc.lock().blue_pin = *args.recv_data().first().unwrap();
            nvs_clone
                .lock()
                .set_u8("blue_pin", cc.lock().blue_pin)
                .unwrap();
        });
        let cc = file_upload_service.clone();
        blue_led_characteristic.lock().on_read(move |args, _| {
            let data = [cc.lock().blue_pin];
            args.set_value(&data);
        });

        file_upload_service
    }
}
