use rudelblinken_sdk::{export, exports, get_name, log, time, yield_now, Guest, LogLevel};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

struct TestGuest;
impl Guest for TestGuest {
    fn run() {
        loop {
            log(LogLevel::Info, "Hello, world from WASM!");
            yield_now();
        }
    }
}

export! {TestGuest}
