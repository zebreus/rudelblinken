use rudelblinken_sdk::{
    export, exports, get_name, log, time, yield_now, Advertisement, BleGuest, Guest, LogLevel,
};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

struct TestLogging;
impl Guest for TestLogging {
    fn run() {
        log(LogLevel::Info, &format!("This is a info message"));
        log(LogLevel::Warning, &format!("This is a warn message"));
        log(LogLevel::Error, &format!("This is a error message"));
        log(LogLevel::Debug, &format!("This is a debug message"));
        log(LogLevel::Trace, &format!("This is a trace message"));
    }
}
impl BleGuest for TestLogging {
    fn on_advertisement(_advertisement: Advertisement) {}
}

export! {TestLogging}
