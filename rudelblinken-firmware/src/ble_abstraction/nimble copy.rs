use std::sync::Arc;

use bitflags::bitflags;
use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid}, AttValue, BLE2904Format, BLEConnDesc, BLEServer, DescriptorProperties, OnWriteArgs
};
use esp_idf_sys as _;

pub trait DocumentableCharacteristic {
    fn document(&self, name: &str, format: BLE2904Format, exponent: u8, unit: u32);
}
impl DocumentableCharacteristic for Arc<Mutex<esp32_nimble::BLECharacteristic>> {
    fn document(&self, name: &str, format: BLE2904Format, exponent: u8, unit: u32) {
        let mut characteristic = self.lock();
        characteristic
            .create_2904_descriptor()
            .format(format)
            .exponent(exponent)
            .unit(unit as u16)
            .namespace(0x01)
            .description(0x00);
        characteristic
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value(name.as_bytes());

        // let test = DocumentableCharacteristicBuilder {
        //     uuid: characteristic.uuid(),
        //     name,
        //     format,
        //     exponent,
        //     unit,
        //     read: None,
        //     write: None,
        // };
    }
}

// struct DocumentableCharacteristicBuilder<'a, ReadFunction = !, WriteFunction = !>
// where
//     ReadFunction: FnMut(&mut AttValue, &BLEConnDesc) + Send + Sync + 'static,
//     WriteFunction: FnMut(&mut OnWriteArgs) + Send + Sync + 'static,
// {
//     uuid: BleUuid,
//     name: &'a str,
//     format: BLE2904Format,
//     exponent: u8,
//     unit: u32,
//     read: Option<ReadFunction>,
// }


bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct CharacteristicProperties: u16 {
      /// Read Access Permitted
      const READ = esp_idf_sys::BLE_GATT_CHR_F_READ as _;
      /// Read Requires Encryption
      const READ_ENC = esp_idf_sys::BLE_GATT_CHR_F_READ_ENC as _;
      /// Read requires Authentication
      const READ_AUTHEN = esp_idf_sys::BLE_GATT_CHR_F_READ_AUTHEN as _;
      /// Read requires Authorization
      const READ_AUTHOR = esp_idf_sys::BLE_GATT_CHR_F_READ_AUTHOR as _;
      /// Write Permited
      const WRITE = esp_idf_sys::BLE_GATT_CHR_F_WRITE as _;
      /// Write with no Ack Response
      const WRITE_NO_RSP = esp_idf_sys::BLE_GATT_CHR_F_WRITE_NO_RSP as _;
      /// Write Requires Encryption
      const WRITE_ENC = esp_idf_sys::BLE_GATT_CHR_F_WRITE_ENC as _;
      /// Write requires Authentication
      const WRITE_AUTHEN = esp_idf_sys::BLE_GATT_CHR_F_WRITE_AUTHEN as _;
      /// Write requires Authorization
      const WRITE_AUTHOR = esp_idf_sys::BLE_GATT_CHR_F_WRITE_AUTHOR as _;
      /// Broadcasts are included in the advertising data
      const BROADCAST = esp_idf_sys::BLE_GATT_CHR_F_BROADCAST as _;
      /// Notifications are Sent from Server to Client with no Response
      const NOTIFY = esp_idf_sys::BLE_GATT_CHR_F_NOTIFY as _;
      /// Indications are Sent from Server to Client where Server expects a Response
      const INDICATE = esp_idf_sys::BLE_GATT_CHR_F_INDICATE as _;
    }
  }


  

#[derive(Clone)]
pub struct WrappedBleService {
    service: Arc<Mutex<esp32_nimble::BLEService>>,
}

impl WrappedBleService {
    pub fn new(server: &mut BLEServer, uuid: BleUuid) -> Self {
        let service = server.create_service(uuid);
        Self { service }
    }
    fn create_characteristic(&self, uuid: BleUuid, name: &str, format: BLE2904Format, exponent: u8, unit: u32) {
        let data_characteristic = ble_service.create_characteristic(
            uuid,
            NimbleProperties::WRITE_NO_RSP | NimbleProperties::WRITE,
        );

        characteristic
            .create_2904_descriptor()
            .format(format)
            .exponent(exponent)
            .unit(unit as u16)
            .namespace(0x01)
            .description(0x00);
        characteristic
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value(name.as_bytes());
        
    }
    fn create_characteristic_r(&self, name: &str, format: BLE2904Format, exponent: u8, unit: u32, read: impl FnMut(&mut AttValue, &BLEConnDesc) + Send + Sync + 'static) {
        let mut service = self.service.lock();
        
    }
}

fn create_characteristic(Arc<Mutex<BleService>>, name: &str, format: BLE2904Format, exponent: u8, unit: u32) {
    let mut characteristic = self.lock();
    characteristic
        .create_2904_descriptor()
        .format(format)
        .exponent(exponent)
        .unit(unit as u16)
        .namespace(0x01)
        .description(0x00);
    characteristic
        .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
        .lock()
        .set_value(name.as_bytes());

    let test = DocumentableCharacteristicBuilder {
        uuid: characteristic.uuid(),
        name,
        format,
        exponent,
        unit,
        read: None,
        write: None,
    };
}

fn create_readable_characteristic(name: &str, format: BLE2904Format, exponent: u8, unit: u32) {
    let mut characteristic = self.lock();
    characteristic
        .create_2904_descriptor()
        .format(format)
        .exponent(exponent)
        .unit(unit as u16)
        .namespace(0x01)
        .description(0x00);
    characteristic
        .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
        .lock()
        .set_value(name.as_bytes());

    let test = DocumentableCharacteristicBuilder {
        uuid: characteristic.uuid(),
        name,
        format,
        exponent,
        unit,
        read: None,
        write: None,
    };
}
