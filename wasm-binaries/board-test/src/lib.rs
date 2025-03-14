use rudelblinken_sdk::{get_voltage, log, yield_now, Advertisement, LogLevel};
use std::sync::{LazyLock, Mutex};

static ADVERTISMENT_COUNTER: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));

fn measure_voltage() -> u16 {
    const SAMPLES: u32 = 10;
    let mut voltages = 0;
    for _ in 0..SAMPLES {
        voltages += get_voltage();
        yield_now(1000);
    }
    (voltages / SAMPLES) as u16
}

#[rudelblinken_sdk_macro::main]
fn main() {
    loop {
        yield_now(0);

        let voltage = measure_voltage();
        log(LogLevel::Info, &format!("Voltage: {}", voltage));

        if let Ok(counter) = ADVERTISMENT_COUNTER.try_lock() {
            log(
                LogLevel::Info,
                &format!("Advertisements received: {}", *counter),
            );
        }
    }
}

#[rudelblinken_sdk_macro::on_advertisement]
fn on_advertisement(_: Advertisement) {
    let Ok(mut counter) = ADVERTISMENT_COUNTER.try_lock() else {
        return;
    };
    *counter += 1;
}
