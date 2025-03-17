use std::hint::black_box;

use rudelblinken_sdk::{export, exports, log, yield_now, BleEvent, BleGuest, Guest, LogLevel};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

struct TestGuest;
impl Guest for TestGuest {
    fn run() {
        log(LogLevel::Info, "Hello, world from WASM!");
        let mut counter: u32 = 0;
        loop {
            yield_now(0);
            let mut funny = black_box("funny".to_string());
            black_box(funny.push_str("toast"));
            let allocated_bytes = ALLOCATOR.lock().get_counters().allocated_bytes;
            yield_now(0);

            log(
                LogLevel::Info,
                &format!(
                    "Allocated: {} in {} allocations : {}",
                    allocated_bytes, 9, funny
                ),
            );
            drop(funny);

            log(LogLevel::Info, &format!("Counter {}", counter));
            yield_now(100000);
            counter += 1;
        }
    }
}
impl BleGuest for TestGuest {
    fn on_event(event: BleEvent) {
        // yield_now(0);
        // log(LogLevel::Info, "alpha");
        let BleEvent::Advertisement(advertisement) = event;
        // log(LogLevel::Info, "beta");
        let data = black_box(advertisement.manufacturer_data);
        let Some(data) = data else {
            log(LogLevel::Info, "Data is None");
            return;
        };
        // log(LogLevel::Info, "Data is Some");

        // let cool_string = "cool string".to_string() + &data.manufacturer_id.to_string();

        // log(LogLevel::Info, "gamma");

        // log(LogLevel::Info, &cool_string);
        // log(LogLevel::Info, "delta");
        let cool_string2 =
            "cool string22: ".to_string() + &data.data.first().unwrap_or(&99).to_string();
        log(LogLevel::Info, &cool_string2);
        // log(LogLevel::Info, "delta");

        // // log(
        // //     LogLevel::Info,
        // //     format!("Received BLE event with data {:?}", data).as_str(),
        // // );
        // log(LogLevel::Info, "epsilon");
    }
}

export! {TestGuest}
