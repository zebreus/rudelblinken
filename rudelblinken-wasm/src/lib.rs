use std::sync::{LazyLock, Mutex};

use rudelblinken_sdk::{
    export, exports, get_ambient_light, get_config, get_led_info, get_name, get_vibration,
    led_count, log, set_advertisement_data, set_rgb, sleep, time, yield_now, BleEvent, BleGuest,
    Guest, LedColor, LogLevel,
};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

const NUDGE_STRENGHT: u8 = 20;
const MS_PER_STEP: u32 = 16;

#[derive(Debug, Clone)]
struct CycleState {
    progress: u8,
    prog_time: u32,
    off_sum: i32,
    off_cnt: u16,
    nudge_rem: i8,
}

impl CycleState {
    fn new() -> Self {
        Self {
            progress: 0,
            prog_time: (time() / 1000) as u32,
            off_sum: 0,
            off_cnt: 0,
            nudge_rem: 0,
        }
    }

    fn update_progress(&mut self, timestamp: u32) {
        if self.off_cnt != 0 {
            let div = self.off_cnt as i32 * NUDGE_STRENGHT as i32;
            let nudge_base = self.off_sum + self.nudge_rem as i32;
            let nudge = nudge_base / div;
            self.nudge_rem = (nudge_base % div) as i8;

            self.progress = self.progress.wrapping_add(nudge as u8);
            self.off_sum = 0;
            self.off_cnt = 0;
        }

        let dt = self.prog_time - timestamp;
        let t_off = dt % MS_PER_STEP;
        self.prog_time = timestamp - t_off;

        let steps = dt / MS_PER_STEP;
        self.progress = self.progress.wrapping_add(steps as u8);
    }
}

static CYCLE_STATE: LazyLock<Mutex<CycleState>> = LazyLock::new(|| Mutex::new(CycleState::new()));

// relative brightness to use in bright ambient conditions (>= MAX_AMBIENT); 0-255
const MAX_BRIGHT: u8 = 192;
// relative brightness to use in dark ambient conditions (<= MIN_AMBIENT); 0-255
const MIN_BRIGHT: u8 = 32;
const BRIGHT_RANGE: u32 = (MAX_BRIGHT - MIN_BRIGHT) as u32;

const MAX_AMBIENT: u32 = 2_000;
const MIN_AMBIENT: u32 = 0;
const AMBIENT_RANGE: u32 = MAX_AMBIENT - MIN_AMBIENT;

fn calc_bright(ambient: u32, fraction: u8, max_lux: u32) -> u32 {
    // calculate brightness factor based on ambient light (0-255)
    let max_bright = if ambient <= MIN_AMBIENT {
        MIN_BRIGHT as u16
    } else if MAX_AMBIENT <= ambient {
        MAX_BRIGHT as u16
    } else {
        let rel_ambient = ambient - MIN_AMBIENT;
        let rel_bright = (BRIGHT_RANGE * rel_ambient) / AMBIENT_RANGE;
        (MIN_BRIGHT as u16) + (rel_bright as u16)
    };
    // calculate target fraction of maximum brightness (0-65535)
    let target_bright = max_bright * (fraction as u16);
    // scale max_lux based on target brightness
    let target_lux = ((max_lux as u64) * (target_bright as u64) / 0x10000) as u32;
    /* log(
        LogLevel::Info,
        &format!(
            "ambient={}, fraction={}, max_lux={}, \
             max_bright={}, target_bright={}, target_lux={}",
            ambient, fraction, max_lux, max_bright, target_bright, target_lux
        ),
    ); */
    target_lux
}

// number of update cycles between logging internal status
const STATUS_LOG_PERIOD: usize = 127;

struct Test;
impl Guest for Test {
    fn run() {
        let name = get_name();
        let time_a = time();
        let x = format!("Hello, world from WASM! I am running on {}", name);
        let time_b = time();
        log(LogLevel::Info, &x);

        yield_now(1000);
        let data = b"Hello World!";
        set_advertisement_data(&data.into());

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

        let config = get_config();
        log(LogLevel::Info, &format!("Configuration: {:?}", config));

        let led_info = get_led_info(0);
        let max_lux = led_info.max_lux as u32;

        log(
            LogLevel::Info,
            &format!("I have {} leds; led 0 info: {:?}", led_count(), led_info),
        );

        let mut ambient = 0u32;
        let mut vibrate = 0u32;
        let mut next_status_log = STATUS_LOG_PERIOD;
        let mut prog = 0u8;
        loop {
            yield_now(1_000);
            {
                let l = get_ambient_light();
                if l != u32::MAX {
                    ambient = (31 * ambient + l) / 32;
                }
            }
            {
                let v = get_vibration();
                if v != u32::MAX {
                    vibrate = (15 * vibrate + v) / 16;
                }
            }
            if let Ok(mut state) = CYCLE_STATE.try_lock() {
                let t = (time() / 1000) as u32;

                state.update_progress(t);
                prog = state.progress;
                drop(state);
                set_advertisement_data(&vec![0x00, 0x00, 0xca, 0x7e, 0xa2, prog]);
                set_rgb(
                    LedColor {
                        red: 0xff,
                        green: 0xff,
                        blue: 0xff,
                    },
                    if 192 <= prog {
                        calc_bright(ambient, 255, max_lux)
                    } else {
                        calc_bright(ambient, 0, max_lux)
                    },
                );
            };
            next_status_log -= 1;
            if next_status_log == 0 {
                next_status_log = STATUS_LOG_PERIOD;
                log(
                    LogLevel::Info,
                    &format!("prog={:3}, ambient={}, vibrate={}", prog, ambient, vibrate),
                );
            }
        }
    }
}

impl BleGuest for Test {
    fn on_event(event: BleEvent) {
        let BleEvent::Advertisement(advertisement) = event;
        let Some(data) = advertisement.manufacturer_data else {
            return;
        };
        let slice = data.data.as_slice();
        if slice.len() == 4 && slice[0] == 0x0ca && slice[1] == 0x7e && slice[2] == 0xa2 {
            if let Ok(mut state) = CYCLE_STATE.try_lock() {
                state.off_cnt += 1;
                state.off_sum += slice[3].wrapping_sub(state.progress) as i8 as i32;
                state.update_progress((advertisement.received_at / 1000) as u32)
            }
        }
    }
}

// We need a main function to be able to `cargo run` this project
#[allow(dead_code)]
fn main() {}

export! {Test}
