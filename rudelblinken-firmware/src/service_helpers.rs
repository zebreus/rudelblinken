use std::sync::Arc;

use esp32_nimble::{
    cpfd::{ChrFormat, ChrUnit, Cpfd},
    utilities::{mutex::Mutex, BleUuid},
    DescriptorProperties,
};
use esp_idf_sys as _;

pub trait DocumentableCharacteristic {
    fn document(&self, name: &str, format: ChrFormat, exponent: i8, unit: ChrUnit);
}

impl DocumentableCharacteristic for Arc<Mutex<esp32_nimble::BLECharacteristic>> {
    fn document(&self, name: &str, format: ChrFormat, exponent: i8, unit: ChrUnit) {
        let mut characteristic = self.lock();
        characteristic.cpfd(Cpfd {
            format,
            exponent,
            unit,
            name_space: 0x01,
            description: 0x00,
        });
        characteristic
            .create_descriptor(BleUuid::Uuid16(0x2901), DescriptorProperties::READ)
            .lock()
            .set_value(name.as_bytes());
    }
}
