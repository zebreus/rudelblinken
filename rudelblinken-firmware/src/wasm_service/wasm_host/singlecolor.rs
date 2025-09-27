use esp32_nimble::utilities::mutex::Mutex;
use esp_idf_hal::{
    gpio::{self},
    ledc::{self, config::TimerConfig, LedcDriver, LedcTimerDriver},
    units::FromValueType,
};
use std::sync::LazyLock;

pub static LED_PIN: LazyLock<Mutex<LedcDriver<'static>>> = LazyLock::new(|| {
    Mutex::new(
        LedcDriver::new(
            unsafe { ledc::CHANNEL0::new() },
            LedcTimerDriver::new(
                unsafe { ledc::TIMER0::new() },
                &TimerConfig::new()
                    .frequency(6.kHz().into())
                    .resolution(ledc::Resolution::Bits13),
            )
            .expect("timer init failed"),
            unsafe { gpio::Gpio8::new() },
        )
        .expect("ledc driver init failed"),
    )
});
