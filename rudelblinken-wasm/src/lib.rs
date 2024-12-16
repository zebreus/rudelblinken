use rudelblinken_sdk::{export, exports, get_name, led_count, log, sleep, time, Guest, LogLevel};
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
    }
}

fn main() {}

export! {Test}
