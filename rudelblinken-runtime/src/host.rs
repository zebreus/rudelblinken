#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl LogLevel {
    pub fn from_i32(level: i32) -> Result<LogLevel, wasmi::Error> {
        match level {
            0 => Ok(LogLevel::Error),
            1 => Ok(LogLevel::Warn),
            2 => Ok(LogLevel::Info),
            3 => Ok(LogLevel::Debug),
            4 => Ok(LogLevel::Trace),
            _ => Err(wasmi::Error::new(format!("Invalid log level {}", level))),
        }
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

pub trait Host {
    #[doc = "You need to yield periodically, as the watchdog will kill you if you dont"]
    fn yield_now(&mut self) -> ();
    #[doc = " Sleep for a given amount of time."]
    fn sleep(&mut self, micros: u64) -> ();

    #[doc = " Returns the number of microseconds that have passed since boot"]
    fn time(&mut self) -> u64;

    #[doc = " Check if the host base module is implemented"]
    #[doc = " "]
    #[doc = " The rudelblinken runtime will mock out all functions the it can not link."]
    #[doc = " If this function returns false you should not use any of the other functions"]
    fn get_base_version(&mut self) -> SemanticVersion;

    #[doc = " Log a message"]
    fn log(&mut self, level: LogLevel, message: String) -> ();

    #[doc = " The name for this host. You can assume that this is unique"]
    fn get_name(&self) -> String;
}
