use rudelblinken_sdk::{
    get_ambient_light, get_voltage, log, set_leds, time, yield_now, Advertisement, LogLevel,
};
use spin::Once;
use std::sync::{Arc, LazyLock, Mutex};

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
static AMBIENT_WORKING: Once<bool> = Once::new();

static RESULT_PRINTED: Once<bool> = Once::new();

const BLE_WORKING_DURATION: u64 = 10 * 1000 * 1000;
const BLE_WORKING_THRESHOLD: u32 = 10;

const AMBIENT_PHASE_DURATION: u32 = 3;
static AMBIENT_DURATION_UNTIL_PRINTING: LazyLock<Arc<Mutex<u32>>> =
    LazyLock::new(|| Arc::new(Mutex::new(45)));
static AMBIENT_TEST_STATE_: LazyLock<Arc<Mutex<AmbientTestState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(AmbientTestState::Low(0))));
enum AmbientTestState {
    Low(u32),
    High(u32),
    LowAgain(u32),
}

fn test_ambient() {
    let ambient = get_ambient_light();
    if AMBIENT_WORKING.get().is_some() {
        return;
    }

    let mut state = AMBIENT_TEST_STATE_.lock().unwrap();
    let new_state = match &mut *state {
        AmbientTestState::Low(counter) => {
            if ambient < 5 {
                if *counter == AMBIENT_PHASE_DURATION {
                    log(
                        LogLevel::Info,
                        "[1/3] Please shine light on the sensor to start the test",
                    );
                }
                AmbientTestState::Low(*counter + 1)
            } else {
                if *counter >= AMBIENT_PHASE_DURATION {
                    AmbientTestState::High(0)
                } else {
                    AmbientTestState::Low(0)
                }
            }
        }
        AmbientTestState::High(counter) => {
            if ambient >= 5 {
                if *counter == AMBIENT_PHASE_DURATION {
                    log(
                        LogLevel::Info,
                        "[2/3] Cover the sensor again to finish the test",
                    );
                }
                if *counter > 2 * AMBIENT_PHASE_DURATION {
                    log(
                        LogLevel::Info,
                        "Sensor not covered fast enough. Ambient light sensor test failed, restarting",
                    );
                    AmbientTestState::Low(0)
                } else {
                    AmbientTestState::High(*counter + 1)
                }
            } else {
                if *counter >= AMBIENT_PHASE_DURATION {
                    AmbientTestState::LowAgain(0)
                } else {
                    log(
                        LogLevel::Info,
                        "Ambient light sensor test failed, restarting",
                    );
                    AmbientTestState::Low(0)
                }
            }
        }
        AmbientTestState::LowAgain(counter) => {
            if *counter >= AMBIENT_PHASE_DURATION {
                AMBIENT_WORKING.call_once(|| {
                    log(LogLevel::Info, "✅: Ambient light sensor working");
                    true
                });
            }

            if ambient < 5 {
                AmbientTestState::LowAgain(*counter + 1)
            } else {
                log(
                    LogLevel::Info,
                    "Ambient light sensor test failed, restarting",
                );
                AmbientTestState::Low(0)
            }
        }
    };

    *state = new_state;

    let mut counter = AMBIENT_DURATION_UNTIL_PRINTING.lock().unwrap();
    if *counter == 0 {
        log(LogLevel::Info, &format!("Ambient light: {}", ambient));
    } else {
        *counter -= 1;
    }
}

// fn test_microphone() {
//     // let ambient = ();

//     log(LogLevel::Info, &format!("Ambient: {}", ambient));
// }

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
        test_ambient();

        set_leds(0, &[255]);
        yield_now(1000 * 300);

        test_ambient();

        set_leds(0, &[0]);
        yield_now(1000 * 300);

        if let (Some(ble), Some(ambient), Some(voltage)) = (
            BLE_WORKING.get(),
            AMBIENT_WORKING.get(),
            USB_SUPPLY_WORKING.get(),
        ) {
            if RESULT_PRINTED.get().is_some() {
                continue;
            }
            RESULT_PRINTED.call_once(|| {
                if *ble && *ambient && *voltage {
                    log(LogLevel::Info, "🎉 All automated tests passed!");
                    log(LogLevel::Info, "(You need to test the LED strip manually)");
                } else {
                    log(
                        LogLevel::Info,
                        "Some tests failed, please see above for details",
                    );
                    log(
                        LogLevel::Info,
                        "You can restart the test by resetting the board",
                    );
                }
                true
            });
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
