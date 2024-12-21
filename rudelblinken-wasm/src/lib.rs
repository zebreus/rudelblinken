use std::sync::{LazyLock, Mutex};

use rudelblinken_sdk::{
    export,
    exports::{self},
    get_led_info, get_name, led_count, log, set_advertisement_data, set_leds, set_rgb, sleep, time,
    yield_now, Advertisement, BleGuest, Guest, LedColor, LogLevel,
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

        let max_lux = get_led_info(0).max_lux as u32;

        log(LogLevel::Info, &format!("I have {} leds", led_count()));
        loop {
            yield_now(1000);
            if let Ok(mut state) = CYCLE_STATE.try_lock() {
                state.update_progress((time() / 1000) as u32);
                let prog = state.progress;
                drop(state);
                set_advertisement_data(&vec![0x00, 0x00, 0xca, 0x7e, 0xa2, prog]);
                set_rgb(
                    LedColor {
                        red: 0xff,
                        green: 0xff,
                        blue: 0xff,
                    },
                    (max_lux >> 8) * (prog as u32),
                );
            }
        }
    }
}

impl BleGuest for Test {
    fn on_advertisement(advertisement: Advertisement) {
        let data = unsafe {
            std::mem::transmute::<[u32; 8], [u8; 32]>(
                advertisement.data.try_into().unwrap_unchecked(),
            )
        };
        let slice = &data[0..(advertisement.data_length as usize)];
        if slice.len() == 4 && slice[0] == 0x0ca && slice[1] == 0x7e && slice[2] == 0xa2 {
            if let Ok(mut state) = CYCLE_STATE.try_lock() {
                state.off_cnt += 1;
                state.off_sum += slice[3].wrapping_sub(state.progress) as i8 as i32;
            }
        }
    }
}

/// Main is required for `cargo run`
#[allow(dead_code)]
fn main() {}

export! {Test}
