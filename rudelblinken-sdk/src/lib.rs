//! # Rudelblinken SDK
//!
//! This is the SDK for the Rudelblinken platform. It provides a set of functions to interact with the connected hardware.
#![feature(split_array)]

mod rudel;
pub use rudel::{
    export, exports,
    exports::rudel::base::ble_guest::{
        Advertisement, BleEvent, Guest as BleGuest, ManufacturerData, ServiceData,
    },
    exports::rudel::base::run::Guest,
    rudel::base::base::{get_base_version, log, sleep, time, yield_now, LogLevel, SemanticVersion},
    rudel::base::ble::{
        configure_advertisement, get_ble_version, set_advertisement_data, AdvertisementData,
        AdvertisementSettings,
    },
    rudel::base::hardware::{
        get_ambient_light, get_ambient_light_type, get_hardware_version, get_led_info,
        get_vibration, get_vibration_sensor_type, get_voltage, get_voltage_sensor_type, led_count,
        set_leds, set_rgb, AmbientLightType, LedColor, LedInfo, VibrationSensorType,
        VoltageSensorType,
    },
};

pub fn get_name() -> String {
    let tuple = rudel::rudel::base::base::get_name();
    let array: [u8; 16] = [
        tuple.0, tuple.1, tuple.2, tuple.3, tuple.4, tuple.5, tuple.6, tuple.7, tuple.8, tuple.9,
        tuple.10, tuple.11, tuple.12, tuple.13, tuple.14, tuple.15,
    ];
    let length = array
        .iter()
        .enumerate()
        .find(|(_, x)| **x == 0)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let array = &array[0..length];
    String::from_utf8_lossy(array).to_string()
}

pub fn get_config() -> Vec<u8> {
    rudel::rudel::base::base::get_config()
}

impl exports::rudel::base::ble_guest::Advertisement {
    /// Get the sender address
    pub fn get_address(&self) -> &[u8; 6] {
        let (start, _) =
            unsafe { std::mem::transmute::<&u64, &[u8; 8]>(&self.address) }.split_array_ref::<6>();
        return start;
    }
    /// Get the sender address
    pub fn get_address_mut(&mut self) -> &mut [u8; 6] {
        let (start, _) =
            unsafe { std::mem::transmute::<&mut u64, &mut [u8; 8]>(&mut self.address) }
                .split_array_mut::<6>();
        return start;
    }
}
