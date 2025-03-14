//! # Rudelblinken SDK
//!
//! This is the SDK for the Rudelblinken platform. It provides a set of functions to interact with the connected hardware.
#![feature(split_array)]

mod rudel;
pub use rudel::{
    export, exports,
    exports::rudel::base::ble_guest::{Advertisement, Guest as BleGuest},
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
    /// Get the manufacturer data as a byte array.
    ///
    /// Only the first self.data_length bytes are valid. You should probably use `get_data` instead.
    pub unsafe fn get_data_array(&self) -> &[u8; 32] {
        // SAFETY: Does the same as the safe function below, but without copying
        return unsafe { std::mem::transmute::<_, &[u8; 32]>(&self.data) };
    }
    /// Get the manufacturer data as a byte array.
    ///
    /// Only the first self.data_length bytes are valid. You should probably use `get_data_mut` instead.
    pub unsafe fn get_data_array_mut(&mut self) -> &mut [u8; 32] {
        // SAFETY: Does the same as the safe function below, but without copying
        return unsafe { std::mem::transmute::<_, &mut [u8; 32]>(&mut self.data) };
    }
    // // The same as the above
    // pub fn get_data_array_safe(&self) -> [u8; 32] {
    //     let mut array = [0u8; 32];
    //     array[0..4].copy_from_slice(&self.data.0.to_le_bytes());
    //     array[4..8].copy_from_slice(&self.data.1.to_le_bytes());
    //     array[8..12].copy_from_slice(&self.data.2.to_le_bytes());
    //     array[12..16].copy_from_slice(&self.data.3.to_le_bytes());
    //     array[16..20].copy_from_slice(&self.data.4.to_le_bytes());
    //     array[20..24].copy_from_slice(&self.data.5.to_le_bytes());
    //     array[24..28].copy_from_slice(&self.data.6.to_le_bytes());
    //     array[28..32].copy_from_slice(&self.data.7.to_le_bytes());
    //     return array;
    // }
    /// Get the manufacturer data as a slice
    pub fn get_data(&self) -> &[u8] {
        let length = std::cmp::min(self.data_length as usize, 32);
        let array = unsafe { self.get_data_array() };
        return &array[..length];
    }
    /// Get the manufacturer data as a slice
    pub fn get_data_mut(&mut self) -> &mut [u8] {
        let length = std::cmp::min(self.data_length as usize, 32);
        let array = unsafe { self.get_data_array_mut() };
        return &mut array[..length];
    }
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
