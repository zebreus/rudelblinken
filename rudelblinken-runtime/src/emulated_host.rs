use std::time::{Duration, Instant};

use crate::host::{Host, LedColor, LedInfo, LogLevel};

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
    fn yield_now(&mut self) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn sleep(&mut self, micros: u64) -> Result<(), wasmi::Error> {
        std::thread::sleep(Duration::from_micros(micros));
        return Ok(());
    }

    fn time(&mut self) -> Result<u64, wasmi::Error> {
        return Ok(self.start_time.elapsed().as_micros() as u64);
    }

    fn log(&mut self, level: LogLevel, message: &str) -> Result<(), wasmi::Error> {
        println!("{}: {}", level, message);
        return Ok(());
    }

    fn get_name(&self) -> Result<String, wasmi::Error> {
        return Ok("EmulatedHost".to_string());
    }

    fn set_leds(&mut self, _lux: &[u16]) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn set_rgb(&mut self, _color: &crate::host::LedColor, _lux: u32) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn led_count(&mut self) -> Result<u16, wasmi::Error> {
        return Ok(0);
    }

    fn get_led_info(&mut self, _id: u16) -> Result<crate::host::LedInfo, wasmi::Error> {
        return Ok(LedInfo {
            color: LedColor::new(0, 0, 0),
            max_lux: 0,
        });
    }

    fn has_ambient_light(&mut self) -> Result<bool, wasmi::Error> {
        return Ok(false);
    }

    fn get_ambient_light(&mut self) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn has_vibration_sensor(&mut self) -> Result<bool, wasmi::Error> {
        return Ok(false);
    }

    fn get_vibration(&mut self) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }
}
