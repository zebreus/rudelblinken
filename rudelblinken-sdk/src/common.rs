use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct Log {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<&LogLevel> for tracing::Level {
    fn from(value: &LogLevel) -> Self {
        match value {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct LEDBrightnessSettings {
    pub rgb: [u8; 3],
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct BLEAdvSettings {
    pub min_interval: u16,
    pub max_interval: u16,
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct BLEAdvData {
    pub data: Vec<u8>,
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct BLEAdvNotification {
    pub mac: [u8; 6],
    pub data: Vec<u8>,
}

#[derive(Debug, Archive, Deserialize, Serialize)]
pub struct TestResult {
    pub min_interval: u32,
    pub max_interval: u32,
    pub test_string: String,
}

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Region {
    pub(crate) ptr: u32,
    pub(crate) len: u32,
    pub(crate) cap: u32,
}
