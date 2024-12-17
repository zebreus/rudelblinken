use rudelblinken_sdk::{
    export,
    exports::{self},
    get_name, led_count, log, sleep, time, yield_now, Advertisment, BleGuest, Guest, LogLevel,
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
            log(LogLevel::Debug, "Looping");
            sleep(1000 * 200);
            yield_now();
        }
    }
}

impl BleGuest for Test {
    fn on_advertisment(advertisment: Advertisment) {
        log(
            LogLevel::Debug,
            format!("Received advertisment at: {}", advertisment.received_at).as_str(),
        );
        log(
            LogLevel::Debug,
            format!("Address bytes: {:?}", advertisment.get_address()).as_str(),
        );
        log(
            LogLevel::Debug,
            format!("Data bytes: {:?}", advertisment.get_data()).as_str(),
        );
    }
}

/// Main is required for `cargo run`
#[allow(dead_code)]
fn main() {}

export! {Test}
