use core::slice::from_ref;
use esp32_nimble::utilities::mutex::Mutex;
use esp_idf_hal::gpio::Gpio10;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::config::{DriverConfig, MODE_0};
use esp_idf_hal::spi::{SpiBusDriver, SpiDriver};
use smart_leds::{gamma, hsv::hsv2rgb, hsv::Hsv, RGB8};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

const LED_NUM: usize = 100;

pub struct LedState {
    // ws2812: Ws2812<SpiBusDriver<'static, SpiDriver<'static>>>,
    bus_driver: SpiBusDriver<'static, SpiDriver<'static>>,
    state: [smart_leds::RGB<u8>; 50],
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
        // .data_mode()
        // .allow_pre_post_delays(true)
        .polling(true)
        .data_mode(MODE_0);

    let driver_config = DriverConfig {
        // dma: esp_idf_hal::spi::Dma::Channel2(4000),
        dma: esp_idf_hal::spi::Dma::Auto(4 * 3 * LED_NUM),
        ..Default::default()
    };

    let spidriver =
        SpiDriver::new_without_sclk(spi, out, Option::<Gpio10>::None, &driver_config).unwrap();
    let bus_driver = SpiBusDriver::new(spidriver, &config).unwrap();

    // let ws = Ws2812::new(bus_driver);

    println!("Initialized");

    Mutex::new(LedState {
        // ws2812: ws,
        bus_driver,
        progress: 0,
        state: [RGB8::default(); 50],
        brightness: 255,
    })
});

impl LedState {
    // Delta is the elapsed time in millisec
    pub fn update_leds(&mut self, delta: &Duration) {
        let mut stuff: [smart_leds::RGB<u8>; 100] = [RGB8::default(); 100];

        self.progress += delta.as_millis() as u64;
        // let ws2812 = WS2812.lock();
        let j = self.progress as usize;
        for i in 0..LED_NUM {
            // rainbow cycle using HSV, where hue goes through all colors in circle
            // value sets the brightness
            let hsv = Hsv {
                hue: ((i * 3 + ((j / 10) % 256)) % 256) as u8,
                sat: 255,
                val: if self.brightness < 80 {
                    (self.brightness as f32 * 1.5f32) as u8
                } else if self.brightness > 120 {
                    (160 + (self.brightness - 120) / 3) as u8
                } else {
                    (120 + (self.brightness - 80)) as u8
                },
            };

            stuff[i] = hsv2rgb(hsv);
        }
        // before writing, apply gamma correction for nicer rainbow
        // self.ws2812.write(gamma(stuff.iter().cloned())).unwrap();

        let rgb_iter = gamma(stuff.iter().cloned());
        let bytes_iter = rgb_iter
            .flat_map(|c| {
                let r_bytes = byte_to_data(c.r);
                let g_bytes = byte_to_data(c.g);
                let b_bytes = byte_to_data(c.b);
                [
                    g_bytes[0], g_bytes[1], g_bytes[2], g_bytes[3], r_bytes[0], r_bytes[1],
                    r_bytes[2], r_bytes[3], b_bytes[0], b_bytes[1], b_bytes[2], b_bytes[3],
                ]
            })
            .chain([0u8; 140]); // reset sequence
                                // use byte_to_data
                                // let data_iter = bytes_iter.flat_map(|b| byte_to_data(b));
        let data: Vec<u8> = bytes_iter.collect::<Vec<u8>>();
        self.bus_driver.write(data.as_slice()).unwrap();
        // for chunk in data.chunks(32) {
        //     self.bus_driver.write(chunk).unwrap();
        // }

        // for item in gamma(stuff.iter().cloned()) {
        //     write_byte(&mut self.bus_driver, item.g);
        //     write_byte(&mut self.bus_driver, item.r);
        //     write_byte(&mut self.bus_driver, item.b);
        // }

        // for item in gamma(stuff.iter().cloned()) {
        //     let bytes = byte_to_data(item.g);
        //     self.bus_driver.write(&bytes).unwrap();
        //     let bytes = byte_to_data(item.r);
        //     self.bus_driver.write(&bytes).unwrap();
        //     let bytes = byte_to_data(item.b);
        //     self.bus_driver.write(&bytes).unwrap();
        // }

        // for item in gamma(stuff.iter().cloned()) {
        //     let bytes = byte_to_data(item.g);
        //     self.bus_driver.write(from_ref(&bytes[0])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[1])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[2])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[3])).unwrap();
        //     let bytes = byte_to_data(item.r);
        //     self.bus_driver.write(from_ref(&bytes[0])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[1])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[2])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[3])).unwrap();
        //     let bytes = byte_to_data(item.b);
        //     self.bus_driver.write(from_ref(&bytes[0])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[1])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[2])).unwrap();
        //     self.bus_driver.write(from_ref(&bytes[3])).unwrap();
        // }

        // for _ in 0..140 {
        //     self.bus_driver.write(from_ref(&0)).unwrap();
        // }
        // self.ws2812.

        // for a in self.state.iter() {
        //     println!("LED: {:}", a);
        // }
        // println!("{:?}", self.state);
        println!("Updated at {:?}", Instant::now());
    }
    pub fn set_duty(&mut self, duty: u32) {
        println!("Set brightness to {}", duty);
        self.brightness = duty;
    }
    pub fn get_max_duty(&self) -> u32 {
        255
    }
}

/// Write a single byte for ws2812 devices
fn write_byte(spi: &mut SpiBusDriver<'static, SpiDriver<'static>>, mut data: u8) -> () {
    // Send two bits in one spi byte. High time first, then the low time
    // The maximum for T0H is 500ns, the minimum for one bit 1063 ns.
    // These result in the upper and lower spi frequency limits
    let patterns = [0b1000_1000, 0b1000_1110, 0b11101000, 0b11101110];
    for _ in 0..4 {
        let bits = (data & 0b1100_0000) >> 6;
        spi.write(from_ref(&patterns[bits as usize])).unwrap();
        data <<= 2;
    }
    return ();
}

/// Write a single byte for ws2812 devices
fn byte_to_data(mut data: u8) -> [u8; 4] {
    // Send two bits in one spi byte. High time first, then the low time
    // The maximum for T0H is 500ns, the minimum for one bit 1063 ns.
    // These result in the upper and lower spi frequency limits
    let patterns: [u8; 4] = [0b1000_1000, 0b1000_1110, 0b11101000, 0b11101110];
    let mut result = [0u8; 4];
    for i in 0..4 {
        let bits = (data & 0b1100_0000) >> 6;
        result[i] = patterns[bits as usize];
        data <<= 2;
    }
    return result;
}
