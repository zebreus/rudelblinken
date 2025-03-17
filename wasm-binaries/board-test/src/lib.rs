use rudelblinken_sdk::{
    get_voltage, log, set_leds, time, yield_now, Advertisement, BleEvent, LogLevel,
};
use spin::Once;
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

static USB_SUPPLY_WORKING: Once<bool> = Once::new();
static BATTERY_SUPPLY_WORKING: Once<bool> = Once::new();
static BLE_WORKING: Once<bool> = Once::new();

const BLE_WORKING_DURATION: u64 = 10 * 1000 * 1000;
const BLE_WORKING_THRESHOLD: u32 = 10;

fn test_voltage() {
    let voltage = measure_voltage();
    if voltage > 4900 && voltage < 5100 {
        if USB_SUPPLY_WORKING.get().is_some() {
            return;
        }
        USB_SUPPLY_WORKING.call_once(|| {
            log(
                LogLevel::Info,
                &format!("✅: 5V power supply detected at {}", voltage),
            );
            true
        });
        return;
    }
    if voltage > 3000 && voltage < 4300 {
        if BATTERY_SUPPLY_WORKING.get().is_some() {
            return;
        }
        if !USB_SUPPLY_WORKING.get().unwrap_or(&false) {
            BATTERY_SUPPLY_WORKING.call_once(|| {
                log(
                    LogLevel::Info,
                    &format!(
                        "❌: Battery power supply detected before 5V power supply at {}",
                        voltage
                    ),
                );
                false
            });
            return;
        }
        BATTERY_SUPPLY_WORKING.call_once(|| {
            log(
                LogLevel::Info,
                &format!("✅: Battery power supply working at {}", voltage),
            );
            true
        });
        return;
    }

    log(LogLevel::Info, &format!("Voltage: {}", voltage));
}

fn test_ble() {
    let now = time();
    if BLE_WORKING.get().is_some() {
        return;
    }
    if now < BLE_WORKING_DURATION {
        return;
    }
    if let Ok(counter) = ADVERTISMENT_COUNTER.try_lock() {
        if *counter < BLE_WORKING_THRESHOLD {
            BLE_WORKING.call_once(|| {
                log(
                    LogLevel::Warning,
                    &format!(
                        "❌: BLE not working (received only {} of {} advertisments in {} seconds)",
                        counter,
                        BLE_WORKING_THRESHOLD,
                        BLE_WORKING_DURATION / 1000 / 1000
                    ),
                );
                false
            });
            return;
        }
        BLE_WORKING.call_once(|| {
            log(
                LogLevel::Info,
                &format!(
                    "✅: BLE working (received {} advertisments in {} seconds)",
                    counter,
                    BLE_WORKING_DURATION / 1000 / 1000
                ),
            );
            true
        });
        return;
    } else {
        log(LogLevel::Warning, "Failed to lock advertisement counter");
    }
}

#[rudelblinken_sdk_macro::main]
fn main() {
    loop {
        yield_now(0);

        test_voltage();
        test_ble();

        set_leds(0, &[255]);
        yield_now(1000 * 200);
        set_leds(0, &[0]);
        yield_now(1000 * 300);
    }
}

#[rudelblinken_sdk_macro::on_event]
fn on_event(_: BleEvent) {
    let Ok(mut counter) = ADVERTISMENT_COUNTER.try_lock() else {
        return;
    };
    *counter += 1;
}
