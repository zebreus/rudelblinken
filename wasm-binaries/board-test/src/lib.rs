use rudelblinken_sdk::{
    get_ambient_light, get_voltage, log, set_leds, time, yield_now, Advertisement, LogLevel,
};

static mut ADVERTISEMENT_COUNTER: u32 = 0;

fn measure_voltage() -> u16 {
    const SAMPLES: u32 = 10;
    let mut voltages = 0;
    for _ in 0..SAMPLES {
        voltages += get_voltage();
        yield_now(1000);
    }
    (voltages / SAMPLES) as u16
}

static mut USB_SUPPLY_WORKING: Option<bool> = None;
static mut BATTERY_SUPPLY_WORKING: Option<bool> = None;
static mut BLE_WORKING: Option<bool> = None;
static mut AMBIENT_WORKING: Option<bool> = None;

static mut RESULT_PRINTED: Option<bool> = None;

const BLE_WORKING_DURATION: u64 = 10 * 1000 * 1000;
const BLE_WORKING_THRESHOLD: u32 = 10;

const AMBIENT_PHASE_DURATION: u32 = 3;
static mut AMBIENT_DURATION_UNTIL_PRINTING: u32 = 45;
static mut AMBIENT_TEST_STATE_: AmbientTestState = AmbientTestState::Low(0);
enum AmbientTestState {
    Low(u32),
    High(u32),
    LowAgain(u32),
}

fn test_ambient() {
    unsafe {
        let ambient = get_ambient_light();
        if AMBIENT_WORKING.is_some() {
            return;
        }

        let new_state = match AMBIENT_TEST_STATE_ {
            AmbientTestState::Low(counter) => {
                if ambient < 5 {
                    if counter == AMBIENT_PHASE_DURATION {
                        log(
                            LogLevel::Info,
                            "[1/3] Please shine light on the sensor to start the test",
                        );
                    }
                    AmbientTestState::Low(counter + 1)
                } else {
                    if counter >= AMBIENT_PHASE_DURATION {
                        AmbientTestState::High(0)
                    } else {
                        AmbientTestState::Low(0)
                    }
                }
            }
            AmbientTestState::High(counter) => {
                if ambient >= 5 {
                    if counter == AMBIENT_PHASE_DURATION {
                        log(
                            LogLevel::Info,
                            "[2/3] Cover the sensor again to finish the test",
                        );
                    }
                    if counter > 2 * AMBIENT_PHASE_DURATION {
                        log(
                        LogLevel::Info,
                        "Sensor not covered fast enough. Ambient light sensor test failed, restarting",
                    );
                        AmbientTestState::Low(0)
                    } else {
                        AmbientTestState::High(counter + 1)
                    }
                } else {
                    if counter >= AMBIENT_PHASE_DURATION {
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
                if counter >= AMBIENT_PHASE_DURATION {
                    if AMBIENT_WORKING.is_none() {
                        log(LogLevel::Info, "✅: Ambient light sensor working");
                        AMBIENT_WORKING = Some(true);
                    }
                }

                if ambient < 5 {
                    AmbientTestState::LowAgain(counter + 1)
                } else {
                    log(
                        LogLevel::Info,
                        "Ambient light sensor test failed, restarting 2",
                    );
                    AmbientTestState::Low(0)
                }
            }
        };

        AMBIENT_TEST_STATE_ = new_state;

        if AMBIENT_DURATION_UNTIL_PRINTING == 0 {
            if ambient == 0 {
                log(LogLevel::Info, "Ambient light: 0");
            } else if ambient == 1 {
                log(LogLevel::Info, "Ambient light: 1");
            } else if ambient == 2 {
                log(LogLevel::Info, "Ambient light: 2");
            } else if ambient == 3 {
                log(LogLevel::Info, "Ambient light: 3");
            } else if ambient == 4 {
                log(LogLevel::Info, "Ambient light: 4");
            } else if ambient == 5 {
                log(LogLevel::Info, "Ambient light: 5");
            } else if ambient > 5 && ambient < 50 {
                log(LogLevel::Info, "Ambient light: 5-50");
            } else if ambient >= 50 {
                log(LogLevel::Info, "Ambient light: >50");
            } else {
                log(
                    LogLevel::Warning,
                    "Ambient light: {} (too high, please cover the sensor)",
                );
            }
        } else {
            AMBIENT_DURATION_UNTIL_PRINTING -= 1;
        }
    }
}

// fn test_microphone() {
//     // let ambient = ();

//     log(LogLevel::Info, "Ambient: {}", ambient));
// }

fn testget_voltage() {
    unsafe {
        let voltage = measure_voltage();
        if voltage > 4900 && voltage < 5100 {
            if USB_SUPPLY_WORKING.is_some() {
                return;
            }
            if USB_SUPPLY_WORKING.is_none() {
                log(LogLevel::Info, "✅: 5V power supply detected at {}");
                USB_SUPPLY_WORKING = Some(true);
            }
            return;
        }
        if voltage > 3000 && voltage < 4300 {
            if BATTERY_SUPPLY_WORKING.is_some() {
                return;
            }
            if !USB_SUPPLY_WORKING.unwrap_or(false) {
                if BATTERY_SUPPLY_WORKING.is_none() {
                    log(
                        LogLevel::Info,
                        "❌: Battery power supply detected before 5V power supply at {}",
                    );
                    BATTERY_SUPPLY_WORKING = Some(false);
                }
                return;
            }
            if BATTERY_SUPPLY_WORKING.is_none() {
                log(LogLevel::Info, "✅: Battery power supply working at {}");
                BATTERY_SUPPLY_WORKING = Some(true);
            }
            return;
        }

        log(LogLevel::Info, "Voltage: {}");
    }
}

fn test_ble() {
    unsafe {
        let now = time();
        if BLE_WORKING.is_some() {
            return;
        }
        if now < BLE_WORKING_DURATION {
            return;
        }
        let counter = &mut ADVERTISEMENT_COUNTER;
        if *counter < BLE_WORKING_THRESHOLD {
            if BLE_WORKING.is_none() {
                log(
                    LogLevel::Warning,
                    "❌: BLE not working (received only {} of {} advertisements in {} seconds)",
                );
                BLE_WORKING = Some(false);
            }
            return;
        }
        if BLE_WORKING.is_none() {
            log(
                LogLevel::Info,
                "✅: BLE working (received {} advertisements in {} seconds)",
            );
            BLE_WORKING = Some(true);
        }
        return;
    }
}

#[rudelblinken_sdk_macro::main]
fn main() {
    unsafe {
        loop {
            yield_now(0);

            testget_voltage();
            test_ble();
            test_ambient();

            set_leds(0, &[255]);
            yield_now(1000 * 300);

            test_ambient();

            set_leds(0, &[0]);
            yield_now(1000 * 300);

            if let (Some(ble), Some(ambient), Some(voltage)) =
                (BLE_WORKING, AMBIENT_WORKING, USB_SUPPLY_WORKING)
            {
                if RESULT_PRINTED.is_some() {
                    continue;
                }
                if ble && ambient && voltage {
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
                RESULT_PRINTED = Some(true);
            }
        }
    }
}

#[rudelblinken_sdk_macro::on_advertisement]
fn on_advertisement(_: Advertisement) {
    unsafe {
        let counter = &mut ADVERTISEMENT_COUNTER;
        *counter += 1;
    }
}
