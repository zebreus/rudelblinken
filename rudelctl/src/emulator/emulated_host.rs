use rudelblinken_runtime::{
    host::{
        AdvertisementSettings, AmbientLightType, BleEvent, Host, LedColor, LedInfo, LogLevel,
        VibrationSensorType, VoltageSensorType,
    },
    linker::linker::WrappedCaller,
};
use std::{
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub enum WasmEvent {
    SetAdvertismentSettings(AdvertisementSettings),
    SetAdvertismentData(Vec<u8>),
}

pub enum HostEvent {
    BleEvent(BleEvent),
}

pub struct EmulatedHost {
    pub start_time: Instant,
    pub host_events: Receiver<HostEvent>,
    pub wasm_events: Sender<WasmEvent>,
    // TODO: Actually use this
    #[allow(dead_code)]
    pub address: [u8; 6],
    // TODO: Actually use this
    #[allow(dead_code)]
    pub name: String,
}

impl EmulatedHost {
    pub fn new(address: [u8; 6], name: String) -> (Sender<HostEvent>, Receiver<WasmEvent>, Self) {
        let (host_sender, host_receiver) = channel::<HostEvent>(20);
        let (wasm_sender, wasm_receiver) = channel::<WasmEvent>(20);
        return (
            host_sender,
            wasm_receiver,
            EmulatedHost {
                start_time: Instant::now(),
                host_events: host_receiver,
                wasm_events: wasm_sender,
                address,
                name,
            },
        );
    }
}

impl Host for EmulatedHost {
    fn yield_now(
        caller: &mut WrappedCaller<'_, Self>,
        micros: u64,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        let end_time = Instant::now()
            .checked_add(Duration::from_micros(micros))
            .unwrap();
        loop {
            while let Ok(event) = caller.data_mut().host_events.try_recv() {
                match event {
                    HostEvent::BleEvent(ble_event) => {
                        caller.on_event(ble_event)?;
                    }
                }
            }
            if end_time <= Instant::now() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        caller.inner().set_fuel(999_999).unwrap();
        return Ok(999_999);
    }

    fn sleep(
        _caller: &mut WrappedCaller<'_, Self>,
        micros: u64,
    ) -> Result<(), rudelblinken_runtime::Error> {
        std::thread::sleep(Duration::from_micros(micros));
        return Ok(());
    }

    fn time(caller: &mut WrappedCaller<'_, Self>) -> Result<u64, rudelblinken_runtime::Error> {
        return Ok(caller.data().start_time.elapsed().as_micros() as u64);
    }

    fn log(
        _caller: &mut WrappedCaller<'_, Self>,
        level: LogLevel,
        message: &str,
    ) -> Result<(), rudelblinken_runtime::Error> {
        log::log!(
            match level {
                LogLevel::Error => log::Level::Error,
                LogLevel::Warn => log::Level::Warn,
                LogLevel::Info => log::Level::Info,
                LogLevel::Debug => log::Level::Debug,
                LogLevel::Trace => log::Level::Trace,
            },
            "{}",
            message
        );
        return Ok(());
    }

    fn get_name(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<String, rudelblinken_runtime::Error> {
        return Ok("EmulatedHost".to_string());
    }

    fn get_config(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<Vec<u8>, rudelblinken_runtime::Error> {
        return Ok(vec![]);
    }

    fn set_leds(
        _caller: &mut WrappedCaller<'_, Self>,
        _first_id: u16,
        _lux: &[u16],
    ) -> Result<u32, rudelblinken_runtime::Error> {
        Ok(0)
    }

    fn set_rgb(
        _caller: &mut WrappedCaller<'_, Self>,
        _color: &LedColor,
        _lux: u32,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        Ok(0)
    }

    fn led_count(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u16, rudelblinken_runtime::Error> {
        return Ok(500);
    }

    fn get_led_info(
        _caller: &mut WrappedCaller<'_, Self>,
        _id: u16,
    ) -> Result<LedInfo, rudelblinken_runtime::Error> {
        return Ok(LedInfo {
            color: LedColor::new(0, 0, 0),
            max_lux: 0,
        });
    }

    fn get_ambient_light_type(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<AmbientLightType, rudelblinken_runtime::Error> {
        Ok(AmbientLightType::None)
    }

    fn get_ambient_light(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        return Ok(0);
    }

    fn get_vibration_sensor_type(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<VibrationSensorType, rudelblinken_runtime::Error> {
        Ok(VibrationSensorType::None)
    }

    fn get_vibration(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        return Ok(0);
    }

    fn configure_advertisement(
        caller: &mut WrappedCaller<'_, Self>,
        settings: AdvertisementSettings,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        caller
            .data_mut()
            .wasm_events
            .blocking_send(WasmEvent::SetAdvertismentSettings(settings))
            .map_err(|error| rudelblinken_runtime::Error::new(error.to_string()))?;
        Ok(0)
    }

    fn set_advertisement_data(
        caller: &mut WrappedCaller<'_, Self>,
        data: &[u8],
    ) -> Result<u32, rudelblinken_runtime::Error> {
        caller
            .data_mut()
            .wasm_events
            .blocking_send(WasmEvent::SetAdvertismentData(data.into()))
            .map_err(|error| rudelblinken_runtime::Error::new(error.to_string()))?;
        Ok(0)
    }

    fn get_voltage_sensor_type(
        _context: &mut WrappedCaller<'_, Self>,
    ) -> Result<VoltageSensorType, rudelblinken_runtime::Error> {
        Ok(VoltageSensorType::None)
    }

    fn get_voltage(
        _context: &mut WrappedCaller<'_, Self>,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        Ok(0)
    }
}
