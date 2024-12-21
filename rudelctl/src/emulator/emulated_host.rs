use rudelblinken_runtime::{
    host::{AdvertisementSettings, Event, Host, LedColor, LedInfo, LogLevel},
    linker::linker::WrappedCaller,
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub enum WasmEvent {
    SetAdvertismentSettings(AdvertisementSettings),
    SetAdvertismentData(Vec<u8>),
}

pub struct EmulatedHost {
    pub start_time: Instant,
    pub host_events: Receiver<Event>,
    pub wasm_events: Sender<WasmEvent>,
    pub address: [u8; 6],
    pub name: String,
}

impl EmulatedHost {
    pub fn new(address: [u8; 6], name: String) -> (Sender<Event>, Receiver<WasmEvent>, Self) {
        let (host_sender, host_receiver) = channel::<Event>(20);
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
        while let Ok(event) = caller.data_mut().host_events.try_recv() {
            match event {
                Event::AdvertisementReceived(advertisement) => {
                    caller.on_advertisement(advertisement)?;
                }
            }
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
        println!("{}: {}", level, message);
        return Ok(());
    }

    fn get_name(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<String, rudelblinken_runtime::Error> {
        return Ok("EmulatedHost".to_string());
    }

    fn set_leds(
        _caller: &mut WrappedCaller<'_, Self>,
        _lux: &[u16],
    ) -> Result<(), rudelblinken_runtime::Error> {
        return Ok(());
    }

    fn set_rgb(
        _caller: &mut WrappedCaller<'_, Self>,
        _color: &LedColor,
        _lux: u32,
    ) -> Result<(), rudelblinken_runtime::Error> {
        return Ok(());
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

    fn has_ambient_light(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<bool, rudelblinken_runtime::Error> {
        return Ok(false);
    }

    fn get_ambient_light(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        return Ok(0);
    }

    fn has_vibration_sensor(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<bool, rudelblinken_runtime::Error> {
        return Ok(false);
    }

    fn get_vibration(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        return Ok(0);
    }

    fn configure_advertisement(
        caller: &mut WrappedCaller<'_, Self>,
        settings: AdvertisementSettings,
    ) -> Result<(), rudelblinken_runtime::Error> {
        caller
            .data_mut()
            .wasm_events
            .blocking_send(WasmEvent::SetAdvertismentSettings(settings));
        return Ok(());
    }

    fn set_advertisement_data(
        caller: &mut WrappedCaller<'_, Self>,
        data: &[u8],
    ) -> Result<(), rudelblinken_runtime::Error> {
        caller
            .data_mut()
            .wasm_events
            .blocking_send(WasmEvent::SetAdvertismentData(data.into()));
        return Ok(());
    }
}
