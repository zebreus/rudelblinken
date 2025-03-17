use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::{
    host::{
        AdvertisementSettings, AmbientLightType, BleEvent, Host, LedColor, LedInfo, LogLevel,
        VibrationSensorType, VoltageSensorType,
    },
    linker::linker::WrappedCaller,
};

#[derive(Clone, Debug)]
pub enum Event {
    EventReceived(BleEvent),
}

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
    fn yield_now(caller: &mut WrappedCaller<'_, Self>, micros: u64) -> Result<u32, wasmi::Error> {
        std::thread::sleep(Duration::from_micros(micros));
        while let Ok(event) = caller.data_mut().events.try_recv() {
            match event {
                Event::EventReceived(ble_event) => {
                    caller.on_event(ble_event)?;
                }
            }
        }
        caller.inner().set_fuel(999_999).unwrap();
        return Ok(999_999);
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

    fn get_config(_caller: &mut WrappedCaller<'_, Self>) -> Result<Vec<u8>, wasmi::Error> {
        return Ok(vec![]);
    }

    fn set_leds(
        _caller: &mut WrappedCaller<'_, Self>,
        _first_id: u16,
        _lux: &[u16],
    ) -> Result<u32, wasmi::Error> {
        Ok(0)
    }

    fn set_rgb(
        _caller: &mut WrappedCaller<'_, Self>,
        _color: &crate::host::LedColor,
        _lux: u32,
    ) -> Result<u32, wasmi::Error> {
        return Ok(0);
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

    fn get_ambient_light_type(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<AmbientLightType, wasmi::Error> {
        Ok(AmbientLightType::None)
    }

    fn get_ambient_light(_caller: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn get_vibration_sensor_type(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<VibrationSensorType, wasmi::Error> {
        Ok(VibrationSensorType::None)
    }

    fn get_vibration(_caller: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn get_voltage_sensor_type(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<VoltageSensorType, wasmi::Error> {
        Ok(VoltageSensorType::None)
    }

    fn get_voltage(_caller: &mut WrappedCaller<'_, Self>) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn configure_advertisement(
        _context: &mut WrappedCaller<'_, Self>,
        _settings: AdvertisementSettings,
    ) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }

    fn set_advertisement_data(
        _context: &mut WrappedCaller<'_, Self>,
        _data: &[u8],
    ) -> Result<u32, wasmi::Error> {
        return Ok(0);
    }
}
