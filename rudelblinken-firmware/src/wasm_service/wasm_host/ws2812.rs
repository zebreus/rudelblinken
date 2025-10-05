use esp32_nimble::utilities::mutex::Mutex;
use esp_idf_hal::gpio::Gpio10;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::config::{DriverConfig, MODE_0};
use esp_idf_hal::spi::{SpiBusDriver, SpiDriver};
use std::sync::LazyLock;
use std::time::Duration;

const LED_NUM: usize = 64;
const PATTERNS: [u8; 4] = [0b1000_1000, 0b1000_1110, 0b11101000, 0b11101110];

pub struct LedState {
    bus_driver: SpiBusDriver<'static, SpiDriver<'static>>,
    progress: u64,
    brightness: u32,
}
unsafe impl Sync for LedState {}
unsafe impl Send for LedState {}

pub static WS2812: LazyLock<Mutex<LedState>> = LazyLock::new(|| {
    println!("Initializing");

    let peripherals = Peripherals::take().unwrap();

    let out = peripherals.pins.gpio0;
    let spi = peripherals.spi2;

    let config = esp_idf_hal::spi::config::Config::new()
        .baudrate(3000.kHz().into())
        .write_only(true)
        .polling(true)
        .data_mode(MODE_0);

    let driver_config = DriverConfig {
        dma: esp_idf_hal::spi::Dma::Auto(4 * 3 * 100),
        ..Default::default()
    };
    let spidriver =
        SpiDriver::new_without_sclk(spi, out, Option::<Gpio10>::None, &driver_config).unwrap();
    let bus_driver = SpiBusDriver::new(spidriver, &config).unwrap();

    println!("Initialized");

    Mutex::new(LedState {
        bus_driver,
        progress: 0,
        brightness: 0,
    })
});

impl LedState {
    // Delta is the elapsed time in millisec
    pub fn update_leds(&mut self, delta: &Duration) {
        self.progress += delta.as_millis() as u64;
        let mut led_data: [u8; 12 * LED_NUM + 1] = [0; 12 * LED_NUM + 1];

        for led_num in 0..LED_NUM {
            let color: [u8; 3] = rainbow(led_num, self.progress, self.brightness);

            let mut num_byte = 0;
            for color_num in 0..3 {
                let mut color_byte =
                    (color[color_num] as u16 * (self.brightness as u16 + 1) / 256) as u8;

                for _ in 0..4 {
                    led_data[12 * led_num + num_byte] =
                        PATTERNS[((color_byte & 0b1100_0000) >> 6) as usize];
                    color_byte <<= 2;
                    num_byte += 1;
                }
            }
        }

        led_data[12 * LED_NUM] = 140;
        self.bus_driver.write(&led_data).unwrap();
    }

    pub fn set_duty(&mut self, duty: u32) {
        println!("Set brightness to {}", duty);
        self.brightness = duty;
    }

    pub fn get_max_duty(&self) -> u32 {
        255
    }
}

pub fn rainbow(led: usize, j: u64, v: u32) -> [u8; 3] {
    let h: usize = (led * 3 + ((j as usize / 10) % 256)) % 256;
    let f: u16 = (h as u16 * 2 % 85) * 3;
    let q: u16 = v as u16 * (255 - (255 * f) / 255) / 255;
    let t: u16 = v as u16 * (255 - (255 * (255 - f)) / 255) / 255;

    match h as u8 {
        0..=42 => [v as u8, t as u8, 0],
        43..=84 => [q as u8, v as u8, 0],
        85..=127 => [0, v as u8, t as u8],
        128..=169 => [0, q as u8, v as u8],
        170..=212 => [t as u8, 0, v as u8],
        213..=254 => [v as u8, 0, q as u8],
        255 => [v as u8, t as u8, 0],
    }
}
