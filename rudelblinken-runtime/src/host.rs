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
    Basic,
}
impl VibrationSensorType {
    pub fn lift(val: i32) -> VibrationSensorType {
        match val {
            0 => VibrationSensorType::None,
            _ => VibrationSensorType::Basic,
        }
    }
    pub fn lower(&self) -> i32 {
        unsafe { ::core::mem::transmute(*self) }
    }
}

pub trait Host {
    #[doc = "You need to yield periodically, as the watchdog will kill you if you dont"]
    fn yield_now(&mut self) -> Result<(), wasmi::Error>;
    #[doc = " Sleep for a given amount of time."]
    fn sleep(&mut self, micros: u64) -> Result<(), wasmi::Error>;

    #[doc = " Returns the number of microseconds that have passed since boot"]
    fn time(&mut self) -> Result<u64, wasmi::Error>;

    #[doc = " Log a message"]
    fn log(&mut self, level: LogLevel, message: &str) -> Result<(), wasmi::Error>;

    #[doc = " The name for this host. You can assume that this is unique"]
    fn get_name(&self) -> Result<String, wasmi::Error>;

    fn set_leds(&mut self, lux: &[u16]) -> Result<(), wasmi::Error>;
    fn set_rgb(&mut self, color: &LedColor, lux: u32) -> Result<(), wasmi::Error>;
    fn led_count(&mut self) -> Result<u16, wasmi::Error>;
    fn get_led_info(&mut self, id: u16) -> Result<LedInfo, wasmi::Error>;

    /// Check if this board has an ambient light sensor
    fn has_ambient_light(&mut self) -> Result<bool, wasmi::Error>;
    /// Get the ambient light in lux
    fn get_ambient_light(&mut self) -> Result<u32, wasmi::Error>;

    fn has_vibration_sensor(&mut self) -> Result<bool, wasmi::Error>;
    fn get_vibration(&mut self) -> Result<u32, wasmi::Error>;
}
