use std::{
    io::{Seek, Write},
    sync::Arc,
};

use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLE2904Format, BLEServer, DescriptorProperties, NimbleProperties,
};
use esp_idf_sys as _;
use rudelblinken_filesystem::{
    file::{File as FileContent, FileState},
    Filesystem,
};
use thiserror::Error;

use crate::storage::{get_filesystem, FlashStorage};

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
    }
}
