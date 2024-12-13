use std::time::{Duration, Instant};

use crate::host::{Host, LogLevel, SemanticVersion};

pub struct EmulatedHost {
    start_time: Instant,
}

impl EmulatedHost {
    pub fn new() -> Self {
        return EmulatedHost {
            start_time: Instant::now(),
        };
    }
}

impl Host for EmulatedHost {
    fn sleep(&mut self, micros: u64) -> () {
        std::thread::sleep(Duration::from_micros(micros));
    }

    fn time(&mut self) -> u64 {
        return self.start_time.elapsed().as_micros() as u64;
    }

    fn get_base_version(&mut self) -> SemanticVersion {
        return SemanticVersion::new(0, 1, 0);
    }

    fn log(&mut self, level: LogLevel, message: String) -> () {
        println!("{}: {}", level, message);
    }

    fn get_name(&self) -> String {
        return "emulated".to_string();
    }
}
