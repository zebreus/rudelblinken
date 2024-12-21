use esp32_nimble::{utilities::mutex::Mutex, BLEAdvertisementData};
use esp_idf_hal::{
    gpio::{self, PinDriver},
    ledc::{self, config::TimerConfig, LedcDriver, LedcTimerDriver},
    units::FromValueType,
};
use rudelblinken_runtime::{
    host::{AdvertisementSettings, Event, Host, LedColor, LedInfo, LogLevel},
    linker::linker::WrappedCaller,
};
use std::{
    cell::RefCell,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};
use std::{
    rc::Rc,
    sync::mpsc::{channel, Receiver, Sender},
};

use crate::{config::device_name::get_device_name, BLE_DEVICE};

pub static LED_PIN: LazyLock<Mutex<LedcDriver<'static>>> = LazyLock::new(|| {
    Mutex::new(
        LedcDriver::new(
            unsafe { ledc::CHANNEL0::new() },
            LedcTimerDriver::new(
                unsafe { ledc::TIMER0::new() },
                &TimerConfig::new().frequency(25.kHz().into()),
            )
            .expect("timer init failed"),
            unsafe { gpio::Gpio8::new() },
        )
        .expect("ledc driver init failed"),
    )
});

pub enum WasmEvent {
    SetAdvertismentSettings(AdvertisementSettings),
    SetAdvertismentData(Vec<u8>),
}

#[derive(Clone)]
pub struct WasmHost {
    pub host_events: Arc<Mutex<Receiver<Event>>>,
    pub wasm_events: Sender<WasmEvent>,
}

impl WasmHost {
    pub fn new() -> (Sender<Event>, Receiver<WasmEvent>, Self) {
        LazyLock::force(&LED_PIN);
        let (host_sender, host_receiver) = channel::<Event>();
        let (wasm_sender, wasm_receiver) = channel::<WasmEvent>();
        return (
            host_sender,
            wasm_receiver,
            WasmHost {
                host_events: Arc::new(Mutex::new(host_receiver)),
                wasm_events: wasm_sender,
            },
        );
    }
}

impl Host for WasmHost {
    fn yield_now(
        caller: &mut WrappedCaller<'_, Self>,
        micros: u64,
    ) -> Result<u32, rudelblinken_runtime::Error> {
        // Sleep for 1 freeRTOS tick to force yielding
        tracing::error!("YIELD CALLED");
        std::thread::sleep(Duration::from_millis(1));

        loop {
            let receiver = caller.data().host_events.lock();
            let Ok(event) = receiver.try_recv() else {
                break;
            };
            drop(receiver);
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
        let time = unsafe { esp_idf_sys::esp_timer_get_time() };
        return Ok(time as u64);
    }

    fn log(
        _caller: &mut WrappedCaller<'_, Self>,
        level: LogLevel,
        message: &str,
    ) -> Result<(), rudelblinken_runtime::Error> {
        match level {
            LogLevel::Error => ::tracing::error!(msg = &message),
            LogLevel::Warn => ::tracing::warn!(msg = &message),
            LogLevel::Info => ::tracing::info!(msg = &message),
            LogLevel::Debug => ::tracing::debug!(msg = &message),
            LogLevel::Trace => ::tracing::trace!(msg = &message),
        }
        return Ok(());
    }

    fn get_name(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<String, rudelblinken_runtime::Error> {
        let mut name = get_device_name();
        let closest = name.floor_char_boundary(16);
        let name = name.split_off(closest);
        return Ok(name);
    }

    fn set_leds(
        _caller: &mut WrappedCaller<'_, Self>,
        _lux: &[u16],
    ) -> Result<(), rudelblinken_runtime::Error> {
        todo!();
        // return Ok(());
    }

    fn set_rgb(
        _caller: &mut WrappedCaller<'_, Self>,
        _color: &LedColor,
        lux: u32,
    ) -> Result<(), rudelblinken_runtime::Error> {
        LED_PIN.lock().set_duty(lux);
        Ok(())
    }

    fn led_count(
        _caller: &mut WrappedCaller<'_, Self>,
    ) -> Result<u16, rudelblinken_runtime::Error> {
        return Ok(1);
    }

    fn get_led_info(
        _caller: &mut WrappedCaller<'_, Self>,
        _id: u16,
    ) -> Result<LedInfo, rudelblinken_runtime::Error> {
        return Ok(LedInfo {
            color: LedColor::new(0, 0, 0),
            max_lux: LED_PIN.lock().get_max_duty() as u16,
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
        let min_interval = settings.min_interval.clamp(400, 1000);
        let max_interval = settings.max_interval.clamp(min_interval, 1500);

        let ble_device = unsafe { BLE_DEVICE.get_mut().unwrap() };
        let mut ble_advertising = ble_device.get_advertising().lock();
        ble_advertising
            .min_interval(min_interval)
            .max_interval(max_interval);
        ble_advertising.stop().unwrap();
        ble_advertising.start().unwrap();
        return Ok(());
    }

    fn set_advertisement_data(
        caller: &mut WrappedCaller<'_, Self>,
        data: &[u8],
    ) -> Result<(), rudelblinken_runtime::Error> {
        let ble_device = unsafe { BLE_DEVICE.get_mut().unwrap() };
        let mut ble_advertising = ble_device.get_advertising().lock();
        ble_advertising
            .set_data(
                BLEAdvertisementData::new()
                    .name(&Host::get_name(caller).unwrap())
                    .manufacturer_data(&data),
            )
            .unwrap();
        ble_advertising.stop().unwrap();
        ble_advertising.start().unwrap();

        return Ok(());
    }
}
