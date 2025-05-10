use rudelblinken_sdk::{
    export,
    exports::{self},
    get_ambient_light, set_advertisement_data, set_leds, time, yield_now, Advertisement, BleGuest,
    Guest,
};
use std::sync::{LazyLock, Mutex};
use talc::{ClaimOnOom, Span, Talc, Talck};

const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array((&raw const HEAP).cast_mut())) }).lock();

const SINE_TABLE: [u8; 256] = [
    0x80, 0x83, 0x86, 0x89, 0x8C, 0x90, 0x93, 0x96, 0x99, 0x9C, 0x9F, 0xA2, 0xA5, 0xA8, 0xAB, 0xAE,
    0xB1, 0xB3, 0xB6, 0xB9, 0xBC, 0xBF, 0xC1, 0xC4, 0xC7, 0xC9, 0xCC, 0xCE, 0xD1, 0xD3, 0xD5, 0xD8,
    0xDA, 0xDC, 0xDE, 0xE0, 0xE2, 0xE4, 0xE6, 0xE8, 0xEA, 0xEB, 0xED, 0xEF, 0xF0, 0xF1, 0xF3, 0xF4,
    0xF5, 0xF6, 0xF8, 0xF9, 0xFA, 0xFA, 0xFB, 0xFC, 0xFD, 0xFD, 0xFE, 0xFE, 0xFE, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xFE, 0xFE, 0xFD, 0xFD, 0xFC, 0xFB, 0xFA, 0xFA, 0xF9, 0xF8, 0xF6,
    0xF5, 0xF4, 0xF3, 0xF1, 0xF0, 0xEF, 0xED, 0xEB, 0xEA, 0xE8, 0xE6, 0xE4, 0xE2, 0xE0, 0xDE, 0xDC,
    0xDA, 0xD8, 0xD5, 0xD3, 0xD1, 0xCE, 0xCC, 0xC9, 0xC7, 0xC4, 0xC1, 0xBF, 0xBC, 0xB9, 0xB6, 0xB3,
    0xB1, 0xAE, 0xAB, 0xA8, 0xA5, 0xA2, 0x9F, 0x9C, 0x99, 0x96, 0x93, 0x90, 0x8C, 0x89, 0x86, 0x83,
    0x80, 0x7D, 0x7A, 0x77, 0x74, 0x70, 0x6D, 0x6A, 0x67, 0x64, 0x61, 0x5E, 0x5B, 0x58, 0x55, 0x52,
    0x4F, 0x4D, 0x4A, 0x47, 0x44, 0x41, 0x3F, 0x3C, 0x39, 0x37, 0x34, 0x32, 0x2F, 0x2D, 0x2B, 0x28,
    0x26, 0x24, 0x22, 0x20, 0x1E, 0x1C, 0x1A, 0x18, 0x16, 0x15, 0x13, 0x11, 0x10, 0x0F, 0x0D, 0x0C,
    0x0B, 0x0A, 0x08, 0x07, 0x06, 0x06, 0x05, 0x04, 0x03, 0x03, 0x02, 0x02, 0x02, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x03, 0x03, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x0A,
    0x0B, 0x0C, 0x0D, 0x0F, 0x10, 0x11, 0x13, 0x15, 0x16, 0x18, 0x1A, 0x1C, 0x1E, 0x20, 0x22, 0x24,
    0x26, 0x28, 0x2B, 0x2D, 0x2F, 0x32, 0x34, 0x37, 0x39, 0x3C, 0x3F, 0x41, 0x44, 0x47, 0x4A, 0x4D,
    0x4F, 0x52, 0x55, 0x58, 0x5B, 0x5E, 0x61, 0x64, 0x67, 0x6A, 0x6D, 0x70, 0x74, 0x77, 0x7A,
    0x7D,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0,
];

fn calc_bright(progress: u16) -> u32 {
    // This whole function is really hacky and should be replaced
    //
    // All *2 multipliers are hacked on, to make it a bit brighter
    let fraction = (progress / 256) as u8;
    // log(LogLevel::Error, format!("Progress: {}", progress).as_str());
    // log(LogLevel::Error, format!("Fraction: {}", fraction).as_str());
    // Related to PWM frequency
    const MAX_VALUE: u32 = 2500;
    // relative brightness to use in bright ambient conditions (>= MAX_AMBIENT); 0-255
    const MAX_BRIGHTNESS_MULTIPLIER: u32 = (0.8 * MAX_VALUE as f32) as u32;
    const MIN_BRIGHTNESS_MULTIPLIER: u32 = (0.2 * MAX_VALUE as f32) as u32;

    const BRIGHTNESS_MULTIPLIER_RANGE: u32 = MAX_BRIGHTNESS_MULTIPLIER - MIN_BRIGHTNESS_MULTIPLIER;
    let ambient_reading = get_ambient_light();
    let ambient_multiplier = ((ambient_reading * BRIGHTNESS_MULTIPLIER_RANGE as u32) * 2 / 2500)
        + MIN_BRIGHTNESS_MULTIPLIER as u32;

    // map fraction to sine wave and apply ambient light multiplier
    let brightness: u64 = (SINE_TABLE[fraction as usize] as u64 * ambient_multiplier as u64) / 255;

    let adjusted_brightness = (brightness * brightness) / MAX_VALUE as u64;

    adjusted_brightness as u32
}

// How much nudges are attenuated
const NUDGE_ATTENUATION: i32 = 50;
// A cycle is 65536 steps. Adjust this to change the speed of a cycle
const US_PER_STEP: u64 = 20;
// Mark peers as outdated if they are older than this
const MAX_PING_AGE: u64 = 1_000_000 * 5; // 10 seconds
                                         // Delay between nudges
const NUDGE_DELAY: u64 = 200_000; // 200ms

#[derive(Debug, Clone)]
struct ReceivedPing {
    /// Source address
    address: u64,
    /// Received at this timestamp
    received_at: u64,
    /// Received offset
    offset: i16,
}

#[derive(Debug, Clone)]
struct CycleState {
    /// Progress in the cycle, 0-65536
    progress: u16,
    /// Timestamp of the last progress increase
    update_time: u64,
    /// Timestamp of the last nudge
    ///
    /// Nudging is done every NUDGE_DELAY
    nudge_time: u64,
    /// Peers received
    peers: Vec<ReceivedPing>,
    // TODO: Account for nudge remainder.
}

impl CycleState {
    fn new() -> Self {
        Self {
            progress: 0,
            update_time: time(),
            nudge_time: time(),
            peers: Vec::with_capacity(40),
        }
    }

    fn progress_at(&self, timestamp: u64) -> u16 {
        // Difference between
        let dt = self.update_time - timestamp;
        let steps = (dt / US_PER_STEP) as u16;
        self.progress.wrapping_add(steps)
    }

    /// This function gets called when a nudge is received
    /// from another device. It is called with the timestamp
    ///
    /// received_at: when the ping was received
    /// progress: progress of the other device
    /// source_address: address of the other device
    fn register_nudge(&mut self, received_at: u64, progress: u16, source_address: u64) {
        let progress_at_receive = self.progress_at(received_at);
        let offset = progress.wrapping_sub(progress_at_receive) as i16;

        let already_there = self
            .peers
            .iter_mut()
            .find(|peer| peer.address == source_address);
        match already_there {
            Some(peer) => {
                peer.received_at = received_at;
                peer.offset = (((peer.offset as i32) + (offset as i32)) / 2) as i16;
            }
            None => {
                self.peers.push(ReceivedPing {
                    address: source_address,
                    received_at,
                    offset: offset,
                });
            }
        }
    }

    /// This function gets called every tick
    fn update_progress(&mut self) {
        let now = time();
        let step_duration = now - self.update_time;
        self.update_time = now;

        // Apply nudges to progress based on received offsets
        let since_last_nudge = now - self.nudge_time;
        if since_last_nudge > NUDGE_DELAY {
            self.nudge_time = self.nudge_time + NUDGE_DELAY;
            // Get the average offset of all peers that were recently heard from
            let average_offset = self
                .peers
                .iter()
                .filter(|peer| peer.received_at > (now.saturating_sub(MAX_PING_AGE)))
                .map(|peer| peer.offset as i32)
                .sum::<i32>();

            let nudge: i32 = average_offset / NUDGE_ATTENUATION;
            self.progress = self.progress.wrapping_add_signed(nudge as i16);
        }

        // Add the appropriate number of steps based on time passed
        let steps = step_duration / US_PER_STEP;
        self.progress = self.progress.wrapping_add(steps as u16);
    }
}

static CYCLE_STATE: LazyLock<Mutex<CycleState>> = LazyLock::new(|| Mutex::new(CycleState::new()));

/// Advance a tick, updating the cycle state and setting the advertisement data
///
/// Returns the progress of the cycle state
fn tick() -> u16 {
    let progress = loop {
        yield_now(1);

        let Ok(mut state) = CYCLE_STATE.try_lock() else {
            continue;
        };
        state.update_progress();
        break state.progress;
    };

    let progress_bytes = progress.to_le_bytes();
    set_advertisement_data(&vec![
        0x00,
        0x00,
        0xca,
        0x7e,
        0xa2,
        progress_bytes[0],
        progress_bytes[1],
    ]);
    progress
}

struct Test;
impl Guest for Test {
    fn run() {
        // let mut table = [0u32; 256];
        // for i in 0..255 {
        //     let brightness = calc_bright(i * 256);
        //     table[i as usize] = brightness;
        //     yield_now(0);
        // }
        // log(
        //     LogLevel::Error,
        //     format!("Brightness table: {:?}", table).as_str(),
        // );
        loop {
            let progress = tick();

            // TODO: Add high-level API for setting led
            set_leds(0, &[calc_bright(progress) as u16]);
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
        let [0xca, 0x7e, 0xa2, other_progress_0, other_progress_1] = slice else {
            return;
        };
        let other_progress = u16::from_le_bytes([*other_progress_0, *other_progress_1]);

        if let Ok(mut state) = CYCLE_STATE.try_lock() {
            state.register_nudge(
                advertisement.received_at,
                other_progress,
                advertisement.address,
            );
        }
    }
}

/// Main is required for `cargo run`
#[allow(dead_code)]
fn main() {}

export! {Test}
