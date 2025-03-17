//! Contains the structs that are used to interact with the host system.

use crate::linker::linker::WrappedCaller;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}
impl LogLevel {
    pub fn lift(val: i32) -> LogLevel {
        match val {
            1 => LogLevel::Warn,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            4 => LogLevel::Trace,
            _ => LogLevel::Error,
        }
    }
    pub fn lower(&self) -> i32 {
        unsafe { ::core::mem::transmute(*self) }
    }
}
impl core::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
        }
    }
}

/// The semantic version of a module
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl SemanticVersion {
    pub fn new(major: u8, minor: u8, patch: u8) -> Self {
        SemanticVersion {
            major,
            minor,
            patch,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LedColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}
impl LedColor {
    pub fn new(red: u8, green: u8, blue: u8) -> LedColor {
        LedColor { red, green, blue }
    }

    pub fn to_array(&self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LedInfo {
    pub color: LedColor,
    pub max_lux: u16,
}

/// Information about the ambient light sensor.
///
/// This could be extended in the future to indicate more types of sensors in future hardware revisions.
#[repr(i32)]
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Debug)]
pub enum AmbientLightType {
    None,
    Basic,
}
impl AmbientLightType {
    pub fn lift(val: i32) -> AmbientLightType {
        match val {
            0 => AmbientLightType::None,
            _ => AmbientLightType::Basic,
        }
    }
    pub fn lower(&self) -> i32 {
        unsafe { ::core::mem::transmute(*self) }
    }
}

/// Information about the vibration sensor.
///
/// This could be extended in the future to indicate more types of sensors in future hardware revisions.
#[repr(i32)]
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Debug)]
pub enum VibrationSensorType {
    None,
    Ball,
}
impl VibrationSensorType {
    pub fn lift(val: i32) -> VibrationSensorType {
        match val {
            0 => VibrationSensorType::None,
            _ => VibrationSensorType::Ball,
        }
    }
    pub fn lower(&self) -> i32 {
        unsafe { ::core::mem::transmute(*self) }
    }
}

/// Information about the supply voltage sensor.
///
/// This could be extended in the future to indicate more types of sensors in future hardware revisions.
#[repr(i32)]
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Debug)]
pub enum VoltageSensorType {
    None,
    Basic,
}
impl VoltageSensorType {
    pub fn lift(val: i32) -> VoltageSensorType {
        match val {
            0 => VoltageSensorType::None,
            _ => VoltageSensorType::Basic,
        }
    }
    pub fn lower(&self) -> i32 {
        unsafe { ::core::mem::transmute(*self) }
    }
}

/// Configure the BLE advertisements
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdvertisementSettings {
    pub min_interval: u16,
    pub max_interval: u16,
}
impl ::core::fmt::Debug for AdvertisementSettings {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("AdvertisementSettings")
            .field("min-interval", &self.min_interval)
            .field("max-interval", &self.max_interval)
            .finish()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct ServiceData {
    pub uuid: u16,
    pub data: Vec<u8>,
}
impl ::core::fmt::Debug for ServiceData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("ServiceData")
            .field("uuid", &self.uuid)
            .field("data", &self.data)
            .finish()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct ManufacturerData {
    pub manufacturer_id: u16,
    pub data: Vec<u8>,
}
impl ::core::fmt::Debug for ManufacturerData {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("ManufacturerData")
            .field("manufacturer-id", &self.manufacturer_id)
            .field("data", &self.data)
            .finish()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct Advertisement {
    /// The address of the sender 48bit integer
    pub address: u64,
    /// When the advertisement was received
    /// There may be some delay between when the advertisement was received and when the WASM guest is notified
    pub received_at: u64,
    /// Company identifier
    pub manufacturer_data: Option<ManufacturerData>,
    /// Service data
    pub service_data: Vec<ServiceData>,
}

impl ::core::fmt::Debug for Advertisement {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_struct("Advertisement")
            .field("address", &self.address)
            .field("received-at", &self.received_at)
            .field("manufacturer-data", &self.manufacturer_data)
            .field("service-data", &self.service_data)
            .finish()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub enum BleEvent {
    Advertisement(Advertisement),
}
impl ::core::fmt::Debug for BleEvent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            BleEvent::Advertisement(e) => {
                f.debug_tuple("BleEvent::Advertisement").field(e).finish()
            }
        }
    }
}

pub trait Host
where
    Self: Sized,
{
    #[doc = "You need to yield periodically, as the watchdog will kill you if you dont"]
    fn yield_now(context: &mut WrappedCaller<'_, Self>, micros: u64) -> Result<u32, wasmi::Error>;
    #[doc = " Sleep for a given amount of time."]
    fn sleep(context: &mut WrappedCaller<'_, Self>, micros: u64) -> Result<(), wasmi::Error>;

    #[doc = " Returns the number of microseconds that have passed since boot"]
    fn time(context: &mut WrappedCaller<'_, Self>) -> Result<u64, wasmi::Error>;

    #[doc = " Log a message"]
    fn log(
        context: &mut WrappedCaller<'_, Self>,
        level: LogLevel,
        message: &str,
    ) -> Result<(), wasmi::Error>;

    /// The name for this host. You can assume that this is unique
    ///
    /// Gets truncated to the first 16 bytes
    fn get_name(context: &mut WrappedCaller<'_, Self>) -> Result<String, wasmi::Error>;

    /// The configuration set on the host via BLE; to be treaded as an opaque byte slice
    fn get_config(context: &mut WrappedCaller<'_, Self>) -> Result<Vec<u8>, wasmi::Error>;

    fn set_leds(
        context: &mut WrappedCaller<'_, Self>,
        first_id: u16,
        lux: &[u16],
    ) -> Result<u32, wasmi::Error>;
    fn set_rgb(
        context: &mut WrappedCaller<'_, Self>,
        color: &LedColor,
        lux: u32,
    ) -> Result<u32, wasmi::Error>;
    fn led_count(context: &mut WrappedCaller<'_, Self>) -> Result<u16, wasmi::Error>;
    fn get_led_info(
        context: &mut WrappedCaller<'_, Self>,
        id: u16,
    ) -> Result<LedInfo, wasmi::Error>;

    /// Check if this board has an ambient light sensor
    fn get_ambient_light_type(
        context: &mut WrappedCaller<'_, Self>,
    ) -> Result<AmbientLightType, wasmi::Error>;
    /// Get the ambient light in lux
    fn get_ambient_light(context: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error>;

    fn get_vibration_sensor_type(
        context: &mut WrappedCaller<'_, Self>,
    ) -> Result<VibrationSensorType, wasmi::Error>;
    fn get_vibration(context: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error>;

    fn get_voltage_sensor_type(
        context: &mut WrappedCaller<'_, Self>,
    ) -> Result<VoltageSensorType, wasmi::Error>;
    fn get_voltage(context: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error>;

    fn configure_advertisement(
        context: &mut WrappedCaller<'_, Self>,
        settings: AdvertisementSettings,
    ) -> Result<u32, wasmi::Error>;
    fn set_advertisement_data(
        context: &mut WrappedCaller<'_, Self>,
        data: &[u8],
    ) -> Result<u32, wasmi::Error>;
}

pub fn to_error_code<T, E>(result: Result<T, E>, code: u32) -> Result<u32, wasmi::Error> {
    match result {
        Ok(_) => Ok(0),
        Err(_) => Ok(code),
    }
}

pub fn map_to_error_code<T, E, F>(result: Result<T, E>, f: F) -> Result<u32, wasmi::Error>
where
    F: FnOnce(E) -> u32,
{
    match result {
        Ok(_) => Ok(0),
        Err(err) => Ok(f(err)),
    }
}
