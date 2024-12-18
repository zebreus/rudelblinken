use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::{
    host::{AdvertisementSettings, Event, Host, LedColor, LedInfo, LogLevel},
    linker::linker::WrappedCaller,
};

pub struct EmulatedHost {
    pub start_time: Instant,
    pub events: Receiver<Event>,
}

impl EmulatedHost {
    pub fn new() -> (Sender<Event>, Self) {
        let (sender, receiver) = channel::<Event>();
        return (
            sender,
            EmulatedHost {
                start_time: Instant::now(),
                events: receiver,
            },
        );
    }
}

impl Host for EmulatedHost {
    fn yield_now(caller: &mut WrappedCaller<'_, Self>) -> Result<(), wasmi::Error> {
        //YIELD here
        // callbacks = return Ok(());
        // todo!();
        while let Ok(event) = caller.data_mut().events.try_recv() {
            match event {
                Event::AdvertisementReceived(advertisement) => {
                    caller.on_advertisement(advertisement)?;
                }
            }
            // Self::log(caller, LogLevel::Warn, "Got something").unwrap();
        }
        return Ok(());
    }

    fn sleep(_caller: &mut WrappedCaller<'_, Self>, micros: u64) -> Result<(), wasmi::Error> {
        std::thread::sleep(Duration::from_micros(micros));
        return Ok(());
    }

    fn time(caller: &mut WrappedCaller<'_, Self>) -> Result<u64, wasmi::Error> {
        return Ok(caller.data().start_time.elapsed().as_micros() as u64);
    }

    fn log(
        _caller: &mut WrappedCaller<'_, Self>,
        level: LogLevel,
        message: &str,
    ) -> Result<(), wasmi::Error> {
        println!("{}: {}", level, message);
        return Ok(());
    }

    fn get_name(_caller: &mut WrappedCaller<'_, Self>) -> Result<String, wasmi::Error> {
        return Ok("EmulatedHost".to_string());
    }

    fn set_leds(_caller: &mut WrappedCaller<'_, Self>, _lux: &[u16]) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn set_rgb(
        _caller: &mut WrappedCaller<'_, Self>,
        _color: &crate::host::LedColor,
        _lux: u32,
    ) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn led_count(_caller: &mut WrappedCaller<'_, Self>) -> Result<u16, wasmi::Error> {
        return Ok(500);
    }

    fn get_led_info(
        _caller: &mut WrappedCaller<'_, Self>,
        _id: u16,
    ) -> Result<crate::host::LedInfo, wasmi::Error> {
        return Ok(LedInfo {
            color: LedColor::new(0, 0, 0),
            max_lux: 0,
        });
    }

    fn has_ambient_light(_caller: &mut WrappedCaller<'_, Self>) -> Result<bool, wasmi::Error> {
        return Ok(false);
    }

    fn get_ambient_light(_caller: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn has_vibration_sensor(_caller: &mut WrappedCaller<'_, Self>) -> Result<bool, wasmi::Error> {
        return Ok(false);
    }

    fn get_vibration(_caller: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn configure_advertisement(
        _context: &mut WrappedCaller<'_, Self>,
        _settings: AdvertisementSettings,
    ) -> Result<(), wasmi::Error> {
        return Ok(());
    }

    fn set_advertisement_data(
        _context: &mut WrappedCaller<'_, Self>,
        _data: &[u8],
    ) -> Result<(), wasmi::Error> {
        return Ok(());
    }
}
