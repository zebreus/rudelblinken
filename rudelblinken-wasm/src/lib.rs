use rudelblinken_sdk::{
    export,
    exports::{self},
    get_name, led_count, log, set_advertisement_data, sleep, time, yield_now, Advertisement,
    BleGuest, Guest, LogLevel,
};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

struct Test;
impl Guest for Test {
    fn run() {
        let name = get_name();
        let time_a = time();
        log(
            LogLevel::Info,
            &format!("Hello, world from WASM! I am running on {}", name),
        );
        let time_b = time();

        yield_now();
        let data = b"Hello World!";
        set_advertisement_data(&data.into());

        log(
            LogLevel::Info,
            &format!("Printing took {} micros", time_b - time_a),
        );

        let time_a = time();
        sleep(20_000);
        let time_b = time();
        log(
            LogLevel::Info,
            &format!("Sleeping 20.000 micros took {} micros", time_b - time_a),
        );

        log(LogLevel::Info, &format!("I have {} leds", led_count()));
        loop {
            // log(LogLevel::Debug, "Looping");
            sleep(1000 * 200);
            yield_now();
        }
    }
}

impl BleGuest for Test {
    fn on_advertisement(advertisement: Advertisement) {
        let data = unsafe {
            std::mem::transmute::<[u32; 8], [u8; 32]>(
                advertisement.data.try_into().unwrap_unchecked(),
            )
        };
        let slice = &data[0..(advertisement.data_length as usize)];
        let string = String::from_utf8_lossy(slice);

        log(LogLevel::Debug, format!("Received: '{}'", string).as_str());
    }
}

/// Main is required for `cargo run`
#[allow(dead_code)]
fn main() {}

export! {Test}
