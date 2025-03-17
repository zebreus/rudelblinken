use rudelblinken_sdk::{
    export, exports, log, set_rgb, yield_now, BleEvent, BleGuest, Guest, LedColor, LogLevel,
};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

struct TestGuest;
impl Guest for TestGuest {
    fn run() {
        let mut on = false;
        loop {
            on = !on;
            log(
                LogLevel::Info,
                &format!("Turning LED {}", if on { "on" } else { "off" }),
            );

            yield_now(500_000);
            set_rgb(
                LedColor {
                    red: 0xff,
                    green: 0xff,
                    blue: 0xff,
                },
                if on { 255 } else { 0 },
            );
        }
    }
}
impl BleGuest for TestGuest {
    fn on_event(_event: BleEvent) {}
}

export! {TestGuest}
