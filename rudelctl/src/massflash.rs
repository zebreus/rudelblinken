//! Mass-flash and test many Boards in a pipelined flow.
//!
//! Watches USB for Boards, flashes the board-test firmware, follows the test
//! output, flashes the production firmware once the self test (including the
//! ambient light dance) passes, verifies that it boots without crashing, and
//! finally asks the operator to confirm the LED strip blinking on the real
//! firmware. Every Board runs this pipeline concurrently, so the operator
//! only ever waits on their own hands.
//!
//! Rows on the dashboard are keyed by the physical USB port path (e.g.
//! "3-4.1"), not by the /dev/ttyACM* number, so re-plugs and kernel
//! renumbering never shuffle rows. Each port can be assigned the color of the
//! cable plugged into it (unassigned ports are hot pink), which also colors
//! that port's hardware events and its entry in the ports panel.
//!
//! One Board is always focused: number keys focus manually, and focus follows
//! whatever needs keyboard input otherwise. Every failure carries an exact
//! failure mode and must be reviewed: verify it (the Board is at fault),
//! dispute it with a reason (the tool judged wrong), or ignore it. Every
//! Board's complete event history is dumped to a log file. A failed Board
//! that is swapped out before being reviewed parks its failure in a review
//! list, so the port is never blocked — except connect-failed, which is
//! treated as a cable/contact issue and silently dropped on unplug.

use crate::flash::{flash_board, FlashError};
use clap::Args;
use espflash::flasher::ProgressCallbacks;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
    Frame,
};
use serialport::SerialPort;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{ErrorKind, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Args, Debug)]
pub struct MassflashCommand {
    /// Workspace directory holding results.csv, settings.json and dispute logs
    #[clap(long, default_value = ".massflash")]
    dir: PathBuf,
    /// Append per-Board test results to this CSV file instead of <dir>/results.csv
    #[clap(long)]
    results: Option<PathBuf>,
    /// Dump event histories of disputed failures here instead of <dir>/logs
    #[clap(long)]
    logs: Option<PathBuf>,
    /// Show a top message row fed from the message file, re-read twice a
    /// second — lets a remote guide direct the operator through the
    /// physical steps
    #[clap(long)]
    guided: bool,
    /// The message file for --guided, instead of <dir>/message
    #[clap(long)]
    message_file: Option<PathBuf>,
}

/// Resolved locations of everything massflash persists.
struct Paths {
    results: PathBuf,
    logs: PathBuf,
    settings: PathBuf,
    /// Message file to mirror into the top row; Some only in guided mode
    message: Option<PathBuf>,
}

impl Paths {
    fn resolve(command: &MassflashCommand) -> Paths {
        Paths {
            results: command
                .results
                .clone()
                .unwrap_or_else(|| command.dir.join("results.csv")),
            logs: command
                .logs
                .clone()
                .unwrap_or_else(|| command.dir.join("logs")),
            settings: command.dir.join("settings.json"),
            message: command.guided.then(|| {
                command
                    .message_file
                    .clone()
                    .unwrap_or_else(|| command.dir.join("message"))
            }),
        }
    }
}

const ESPRESSIF_VID: &str = "303a";
/// No valid supply voltage reading for this long means the divider is broken.
const POWER_TIMEOUT: Duration = Duration::from_secs(30);
/// The ambient test is operator-paced, but if the reading also never moved
/// the sensor itself is the prime suspect.
const SENSOR_TIMEOUT: Duration = Duration::from_secs(150);
/// Per reset attempt, when waiting for the production firmware to boot.
const BOOT_TIMEOUT: Duration = Duration::from_secs(30);
/// After the boot message, watch this long for crashes and resets before
/// trusting the firmware.
const BOOT_STABILITY_WINDOW: Duration = Duration::from_secs(8);
/// A USB device without a serial port interface after this long failed enumeration.
const NO_SERIAL_TIMEOUT: Duration = Duration::from_secs(5);
/// Connecting/flashing without any progress for this long means espflash is
/// hung on a port that neither answers nor errors.
const STALL_TIMEOUT: Duration = Duration::from_secs(120);
/// The bootloader banner arrives within a second of reset; a Board that
/// produced nothing for this long after the reset is not running at all.
const NO_OUTPUT_TIMEOUT: Duration = Duration::from_secs(20);
/// Reset banners seen during one test run before we call it a boot loop
/// (one is expected from our own reset).
const BOOT_LOOP_THRESHOLD: u32 = 3;
/// Pre-workspace locations, migrated into <dir> on startup.
const LEGACY_RESULTS_FILE: &str = "massflash-results.csv";
const LEGACY_PORT_COLORS_FILE: &str = "massflash-port-colors.csv";
const CSV_HEADER: &str =
    "timestamp,mac,usb_port,seconds,power,ble,light_sensor,result,detail,verdict,reason,log";
/// Reason recorded for rows migrated from a CSV schema without a reason field.
const MIGRATED_REASON: &str = "unknown_created_before_field_existed";

/// The color of the cable plugged into a port.
struct CableColor {
    name: &'static str,
    color: Color,
    /// Higher-value variant, for the action text in the ports panel
    bright: Color,
}

/// Twelve equidistant OKLCH hues (30° apart, L=0.75 C=0.12), five grayscale
/// tones, and hot pink — which doubles as the color of unassigned ports.
static PALETTE: LazyLock<Vec<CableColor>> = LazyLock::new(|| {
    let hue_names = [
        "rose", "red", "orange", "yellow", "lime", "green", "teal", "cyan", "sky", "blue",
        "purple", "magenta",
    ];
    let mut palette: Vec<CableColor> = hue_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let hue = index as f64 * 30.0;
            CableColor {
                name,
                color: oklch(0.75, 0.12, hue),
                bright: oklch(0.85, 0.22, hue),
            }
        })
        .collect();
    let grays: [(&'static str, f64); 5] = [
        ("white", 0.97),
        ("silver", 0.78),
        ("gray", 0.58),
        ("charcoal", 0.38),
        ("black", 0.18),
    ];
    palette.extend(grays.map(|(name, lightness)| CableColor {
        name,
        color: oklch(lightness, 0.0, 0.0),
        bright: oklch((lightness + 0.20).min(0.99), 0.0, 0.0),
    }));
    palette.push(CableColor {
        name: "hotpink",
        color: oklch(0.70, 0.22, 350.0),
        bright: oklch(0.85, 0.26, 350.0),
    });
    palette
});

/// The color for ports without an assigned cable color.
fn default_cable() -> &'static CableColor {
    PALETTE
        .iter()
        .find(|cable| cable.name == "hotpink")
        .expect("hotpink is always in the palette")
}

fn oklch(lightness: f64, chroma: f64, hue_degrees: f64) -> Color {
    let hue = hue_degrees.to_radians();
    let a = chroma * hue.cos();
    let b = chroma * hue.sin();
    let l_ = lightness + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = lightness - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = lightness - 0.0894841775 * a - 1.2914855480 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let red = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let green = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let blue = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
    let gamma = |channel: f64| {
        let channel = channel.clamp(0.0, 1.0);
        let nonlinear = if channel <= 0.0031308 {
            12.92 * channel
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        (nonlinear * 255.0).round() as u8
    };
    Color::Rgb(gamma(red), gamma(green), gamma(blue))
}

/// Every exact way a Board can fail, each with a stable label for the CSV.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FailureMode {
    /// USB device enumerated but never exposed a serial port
    NoSerialPort,
    /// Disappeared from USB before finishing
    UsbDropout,
    /// Serial port exists but the bootloader never answered
    ConnectFailed,
    /// Flashing started but writing failed
    FlashWriteFailed,
    /// Any other flashing error
    FlashOther,
    /// Test firmware reported a power supply problem
    PowerFail,
    /// Test firmware never obtained a valid supply voltage reading
    PowerNoReading,
    /// Test firmware reported BLE not working
    BleFail,
    /// Ambient light reading never reacted during the sensor test
    SensorFrozen,
    /// Test firmware reported failure in more than one test
    TestFailed,
    /// Operator judged the LED strip dead
    LedStripDead,
    /// Production firmware produced no boot message
    NoBoot,
    /// Production firmware booted but crashed or reset afterwards
    BootCrash,
    /// Connecting or flashing hung without progress or error
    FlashStalled,
    /// The Board produced no serial output at all after reset
    NoTestOutput,
    /// The Board kept resetting during the test (brownout suspect)
    BootLoop,
    /// The operator failed the Board by hand
    Manual,
    /// Bug in this tool
    InternalPanic,
}

impl FailureMode {
    fn label(&self) -> &'static str {
        match self {
            FailureMode::NoSerialPort => "no-serial-port",
            FailureMode::UsbDropout => "usb-dropout",
            FailureMode::ConnectFailed => "connect-failed",
            FailureMode::FlashWriteFailed => "flash-write-failed",
            FailureMode::FlashOther => "flash-error",
            FailureMode::PowerFail => "power-fail",
            FailureMode::PowerNoReading => "power-no-reading",
            FailureMode::BleFail => "ble-fail",
            FailureMode::SensorFrozen => "sensor-frozen",
            FailureMode::TestFailed => "test-failed",
            FailureMode::LedStripDead => "led-strip-dead",
            FailureMode::NoBoot => "no-boot",
            FailureMode::BootCrash => "boot-crash",
            FailureMode::FlashStalled => "flash-stalled",
            FailureMode::NoTestOutput => "no-test-output",
            FailureMode::BootLoop => "boot-loop",
            FailureMode::Manual => "manual",
            FailureMode::InternalPanic => "internal-panic",
        }
    }
}

#[derive(Clone, Debug)]
struct Failure {
    mode: FailureMode,
    detail: String,
}

/// How the operator currently needs to move the light source for the ambient test.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum AmbientPrompt {
    #[default]
    Cover,
    Shine,
    CoverAgain,
    Passed,
}

#[derive(Clone, Debug, Default)]
struct TestProgress {
    voltage: Option<bool>,
    ble: Option<bool>,
    ambient: Option<bool>,
    prompt: AmbientPrompt,
    /// Last ambient light reading reported by the Board
    light: Option<String>,
}

/// The operator's judgement of a recorded failure.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ReviewKind {
    /// The Board really is at fault
    Verified,
    /// The tool judged wrong; the full history was dumped with a reason
    Disputed,
    /// Not worth recording as a Board fault
    Ignored,
    /// Failed by hand; the explanation was given up front
    Manual,
}

impl ReviewKind {
    fn label(&self) -> &'static str {
        match self {
            ReviewKind::Verified => "verified",
            ReviewKind::Disputed => "disputed",
            ReviewKind::Ignored => "ignored",
            ReviewKind::Manual => "manual",
        }
    }
}

#[derive(Clone, Debug)]
enum BoardState {
    Connecting,
    Flashing { firmware: &'static str, percent: u8 },
    Testing(TestProgress),
    Verifying,
    /// Production firmware confirmed working, waiting for the operator's LED
    /// strip verdict (the real firmware blinks)
    LedCheck,
    Done,
    /// Waiting for the operator to verify, dispute or ignore the failure
    Failed(Failure),
    /// Reviewed failure; the Board can be unplugged
    Resolved {
        verdict: ReviewKind,
        log_path: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
struct BoardRow {
    dev_node: String,
    mac: Option<String>,
    state: BoardState,
    /// Set by the dashboard thread when the operator judges the LED strip
    led_verdict: Option<bool>,
    /// Ties a worker to one plug event: a stale worker must not touch the row
    /// of a Board that was re-plugged into the same port.
    generation: u64,
    test_results: TestProgress,
    started: Instant,
    /// Pipeline duration, fixed when the worker finishes
    finished_seconds: Option<u64>,
    /// Complete timestamped record of everything that happened to this Board
    history: Vec<String>,
    /// Last time the worker made observable progress; drives the stall watchdog
    last_activity: Instant,
    /// Explanation entered by the operator to fail this Board by hand;
    /// the worker picks it up at its next checkpoint
    manual_fail: Option<String>,
    /// Where this Board's event history is dumped
    log_path: Option<PathBuf>,
}

impl BoardRow {
    fn log(&mut self, message: impl AsRef<str>) {
        self.history.push(format!(
            "[{:>7.1}s] {}",
            self.started.elapsed().as_secs_f32(),
            message.as_ref()
        ));
    }
}

/// A failure whose Board is no longer on its port, still awaiting review.
#[derive(Clone, Debug)]
struct PendingReview {
    id: u64,
    mac: String,
    usb_path: String,
    failure: Failure,
    test_results: TestProgress,
    seconds: u64,
    history: Vec<String>,
    log_path: Option<PathBuf>,
}

/// One line in the hardware event log at the bottom of the screen.
struct HardwareEvent {
    /// Time since session start
    at: Duration,
    usb_path: Option<String>,
    message: String,
}

struct Dashboard {
    /// One row per physical USB port path
    rows: BTreeMap<String, BoardRow>,
    /// Unreviewed failures of Boards that already left their port
    pending: Vec<PendingReview>,
    next_pending_id: u64,
    /// USB devices without a serial port, and when we first saw them
    no_serial_since: HashMap<String, Instant>,
    /// Cable color per port: index into PALETTE
    port_colors: HashMap<String, usize>,
    /// Every port a Board has been detected on this session
    known_ports: BTreeSet<String>,
    events: Vec<HardwareEvent>,
    session_started: Instant,
    done: u32,
    failed: u32,
    /// Sum of pipeline durations of all finished Boards, for the average
    total_seconds: u64,
    /// One-line message shown under the header (e.g. where a log was dumped)
    notice: Option<String>,
}

impl Dashboard {
    fn new(paths: &Paths) -> Self {
        Dashboard {
            rows: BTreeMap::new(),
            pending: Vec::new(),
            next_pending_id: 0,
            no_serial_since: HashMap::new(),
            port_colors: load_port_colors(&paths.settings),
            known_ports: BTreeSet::new(),
            events: Vec::new(),
            session_started: Instant::now(),
            done: 0,
            failed: 0,
            total_seconds: 0,
            notice: None,
        }
    }

    fn event(&mut self, usb_path: Option<&str>, message: impl Into<String>) {
        self.events.push(HardwareEvent {
            at: self.session_started.elapsed(),
            usb_path: usb_path.map(str::to_owned),
            message: message.into(),
        });
        if self.events.len() > 1000 {
            self.events.drain(..500);
        }
    }

    fn cable_color(&self, usb_path: &str) -> &'static CableColor {
        self.port_colors
            .get(usb_path)
            .map(|index| &PALETTE[*index])
            .unwrap_or_else(default_cable)
    }

    fn push_pending(&mut self, mut entry: PendingReview) -> u64 {
        entry.id = self.next_pending_id;
        self.next_pending_id += 1;
        let id = entry.id;
        self.pending.push(entry);
        id
    }
}

/// Persistent settings, stored as JSON in the workspace directory.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Settings {
    /// Cable color name per USB port path; the cables don't move between sessions
    port_colors: HashMap<String, String>,
}

fn load_port_colors(settings_path: &Path) -> HashMap<String, usize> {
    let Ok(content) = fs::read_to_string(settings_path) else {
        return HashMap::new();
    };
    let Ok(settings) = serde_json::from_str::<Settings>(&content) else {
        return HashMap::new();
    };
    settings
        .port_colors
        .into_iter()
        .filter_map(|(path, name)| {
            let index = PALETTE.iter().position(|cable| cable.name == name)?;
            Some((path, index))
        })
        .collect()
}

fn save_port_colors(settings_path: &Path, colors: &HashMap<String, usize>) {
    let settings = Settings {
        port_colors: colors
            .iter()
            .map(|(path, index)| (path.clone(), PALETTE[*index].name.to_string()))
            .collect(),
    };
    if let Ok(content) = serde_json::to_string_pretty(&settings) {
        let _ = fs::write(settings_path, content);
    }
}

/// Create the workspace directory and pull data from pre-workspace locations
/// and older CSV schemas into the current shape. Returns notes about what was
/// migrated, for the event log.
fn prepare_workspace(command: &MassflashCommand, paths: &Paths) -> Vec<String> {
    let mut notes = Vec::new();
    let _ = fs::create_dir_all(&command.dir);

    // Results CSV from the pre-workspace location
    let legacy_results = Path::new(LEGACY_RESULTS_FILE);
    if legacy_results.exists() && !paths.results.exists() {
        if let Ok(content) = fs::read_to_string(legacy_results) {
            let migrated = migrate_csv(&content).unwrap_or(content);
            if fs::write(&paths.results, migrated).is_ok() {
                let _ = fs::rename(legacy_results, format!("{}.migrated", LEGACY_RESULTS_FILE));
                notes.push(format!(
                    "migrated {} to {}",
                    LEGACY_RESULTS_FILE,
                    paths.results.display()
                ));
            }
        }
    }

    // Results CSV written with an older schema
    if let Ok(content) = fs::read_to_string(&paths.results) {
        if let Some(migrated) = migrate_csv(&content) {
            if fs::write(&paths.results, migrated).is_ok() {
                notes.push(format!(
                    "migrated {} to the current CSV schema",
                    paths.results.display()
                ));
            }
        }
    }

    // Cable colors from the pre-settings.json format
    let legacy_colors = Path::new(LEGACY_PORT_COLORS_FILE);
    if legacy_colors.exists() && !paths.settings.exists() {
        if let Ok(content) = fs::read_to_string(legacy_colors) {
            let colors: HashMap<String, usize> = content
                .lines()
                .filter_map(|line| {
                    let (path, index) = line.split_once(',')?;
                    let index: usize = index.trim().parse().ok()?;
                    (index < PALETTE.len()).then(|| (path.trim().to_string(), index))
                })
                .collect();
            save_port_colors(&paths.settings, &colors);
            let _ = fs::rename(
                legacy_colors,
                format!("{}.migrated", LEGACY_PORT_COLORS_FILE),
            );
            notes.push(format!(
                "migrated cable colors to {}",
                paths.settings.display()
            ));
        }
    }

    notes
}

/// Rewrite CSV content from an older schema to CSV_HEADER, filling the reason
/// column with MIGRATED_REASON. Returns None when the content is current.
fn migrate_csv(content: &str) -> Option<String> {
    let mut lines = content.lines();
    let header = lines.next()?;
    if header.trim() == CSV_HEADER {
        return None;
    }
    let mut output = String::from(CSV_HEADER);
    output.push('\n');
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_csv_line(line);
        let row: Vec<String> = match fields.len() {
            // v1: timestamp,mac,usb_port,seconds,power,ble,light_sensor,result
            8 => {
                let result = fields[7].trim();
                let (result, detail) = if let Some(rest) = result.strip_prefix("fail: ") {
                    ("fail".to_string(), rest.to_string())
                } else if let Some(rest) = result.strip_prefix("gone: ") {
                    ("usb-dropout".to_string(), rest.to_string())
                } else {
                    (result.to_string(), String::new())
                };
                fields[..7]
                    .iter()
                    .cloned()
                    .chain([
                        result,
                        detail,
                        String::new(),
                        MIGRATED_REASON.to_string(),
                        String::new(),
                    ])
                    .collect()
            }
            // pre-reason: ...,result,detail,verdict,log
            11 => fields[..10]
                .iter()
                .cloned()
                .chain([MIGRATED_REASON.to_string(), fields[10].clone()])
                .collect(),
            _ => continue,
        };
        let quoted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(column, value)| {
                // detail and reason are always quoted, like append_result writes them
                if column == 8 || column == 10 {
                    format!("\"{}\"", value.replace('"', "'"))
                } else {
                    value.clone()
                }
            })
            .collect();
        output.push_str(&quoted.join(","));
        output.push('\n');
    }
    Some(output)
}

/// Split one CSV line on commas outside quotes (our writer never emits
/// escaped quotes, it replaces them with apostrophes).
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    for character in line.chars() {
        match character {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => fields.push(std::mem::take(&mut field)),
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
}

type Shared = Arc<Mutex<Dashboard>>;

/// What the dashboard's cursor points at.
#[derive(Clone, Debug, PartialEq)]
enum FocusKey {
    Port(String),
    Pending(u64),
}

/// The focused entry. Manual focus (number key) is never switched away
/// automatically; it decays back to automatic after one interaction.
#[derive(Clone)]
struct Focus {
    key: FocusKey,
    manual: bool,
}

/// What a text-entry modal is collecting.
#[derive(Clone, Copy, PartialEq)]
enum TextPurpose {
    /// Why the failure verdict on this entry is wrong
    Dispute,
    /// Why the operator is failing this Board by hand
    ManualFail,
}

/// What keyboard input currently means.
enum InputMode {
    Normal,
    /// Typing an explanation for the entry
    EnterText {
        purpose: TextPurpose,
        target: FocusKey,
        text: String,
    },
    /// Choosing the cable color for a port
    PickColor { usb_path: String, cursor: usize },
}

struct UiState {
    focus: Option<Focus>,
    quit_armed: bool,
    mode: InputMode,
    /// Content of the guided-mode message file; None when not guided
    guided_message: Option<String>,
    /// The one thing the operator should do next; sticky until completed
    next_action: NextAction,
}

/// The single most important physical step for the operator right now.
#[derive(Clone, Debug, PartialEq)]
enum NextAction {
    /// No port has ever seen a Board this session
    PlugFirst,
    /// A finished Board is blocking its port
    Unplug(String),
    /// A known port is empty
    Plug(String),
    /// The port the next action would reference has no cable color yet
    AssignColor(String),
    /// A failure waits for review on this port
    Investigate(String),
    /// The production firmware is blinking, hold the strip and press space
    CheckBlinking(String),
    /// The ambient light dance is due on this port
    TestAmbient(String),
    /// Nothing to do; placeholder that is always replaced
    Wait,
}

/// Whether a previously chosen action still makes sense. Wait is never
/// valid — it is a placeholder, not a commitment.
fn action_valid(dashboard: &Dashboard, focus: Option<&FocusKey>, action: &NextAction) -> bool {
    let row_state = |path: &String| dashboard.rows.get(path).map(|row| &row.state);
    match action {
        NextAction::PlugFirst => dashboard.known_ports.is_empty(),
        NextAction::Unplug(path) => matches!(
            row_state(path),
            Some(BoardState::Done | BoardState::Resolved { .. })
        ),
        // "plug in a board" is only completed by a connection that actually
        // came up: an immediate connect-failed means reseat and try again.
        NextAction::Plug(path) => {
            dashboard.known_ports.contains(path)
                && match row_state(path) {
                    None => true,
                    Some(BoardState::Connecting) => true,
                    Some(BoardState::Flashing { percent: 0, .. }) => true,
                    Some(BoardState::Failed(failure)) => {
                        failure.mode == FailureMode::ConnectFailed
                    }
                    _ => false,
                }
        }
        NextAction::AssignColor(path) => {
            dashboard.known_ports.contains(path) && !dashboard.port_colors.contains_key(path)
        }
        NextAction::Investigate(path) => matches!(
            row_state(path),
            Some(BoardState::Failed(failure)) if failure.mode != FailureMode::ConnectFailed
        ),
        // Only valid while the spacebar would actually confirm this Board
        NextAction::CheckBlinking(path) => {
            matches!(row_state(path), Some(BoardState::LedCheck))
                && focus == Some(&FocusKey::Port(path.clone()))
        }
        NextAction::TestAmbient(path) => matches!(
            row_state(path),
            Some(BoardState::Testing(test)) if test.ambient.is_none()
        ),
        NextAction::Wait => false,
    }
}

/// Pick the most important next action by priority: unplug finished Boards,
/// refill empty ports, investigate failures, confirm blinking, do the sensor
/// dance. A chosen action whose port has no cable color yet becomes
/// "assign a color" first, since actions are communicated by color.
fn compute_next_action(dashboard: &Dashboard, focus: Option<&FocusKey>) -> NextAction {
    if dashboard.known_ports.is_empty() {
        return NextAction::PlugFirst;
    }
    let find_row = |predicate: &dyn Fn(&BoardState) -> bool| {
        dashboard
            .rows
            .iter()
            .find(|(_, row)| predicate(&row.state))
            .map(|(path, _)| path.clone())
    };
    // The spacebar confirms the focused Board, so only it qualifies for the
    // blinking action.
    let focused_led_check = match focus {
        Some(FocusKey::Port(path)) => matches!(
            dashboard.rows.get(path).map(|row| &row.state),
            Some(BoardState::LedCheck)
        )
        .then(|| path.clone()),
        _ => None,
    };
    let action = if let Some(path) =
        find_row(&|state| matches!(state, BoardState::Done | BoardState::Resolved { .. }))
    {
        NextAction::Unplug(path)
    } else if let Some(path) = dashboard
        .known_ports
        .iter()
        .find(|path| match dashboard.rows.get(*path) {
            None => true,
            // An immediate connect-failed means the plug needs reseating
            Some(row) => matches!(
                &row.state,
                BoardState::Failed(failure) if failure.mode == FailureMode::ConnectFailed
            ),
        })
        .cloned()
    {
        NextAction::Plug(path)
    } else if let Some(path) = find_row(&|state| {
        matches!(state, BoardState::Failed(failure) if failure.mode != FailureMode::ConnectFailed)
    }) {
        NextAction::Investigate(path)
    } else if let Some(path) = focused_led_check {
        NextAction::CheckBlinking(path)
    } else if let Some(path) = find_row(
        &|state| matches!(state, BoardState::Testing(test) if test.ambient.is_none()),
    ) {
        NextAction::TestAmbient(path)
    } else {
        NextAction::Wait
    };
    match &action {
        NextAction::Unplug(path)
        | NextAction::Plug(path)
        | NextAction::Investigate(path)
        | NextAction::CheckBlinking(path)
        | NextAction::TestAmbient(path)
            if !dashboard.port_colors.contains_key(path) =>
        {
            NextAction::AssignColor(path.clone())
        }
        _ => action,
    }
}

/// Keep the shown action until it is completed or impossible, so it never
/// flickers between equally valid choices.
fn update_next_action(dashboard: &Dashboard, focus: Option<&FocusKey>, current: &mut NextAction) {
    if !action_valid(dashboard, focus, current) {
        *current = compute_next_action(dashboard, focus);
    }
}

pub fn run(command: MassflashCommand) -> std::io::Result<()> {
    // espflash logs through the global logger, which would draw over the dashboard
    let log_level = log::max_level();
    log::set_max_level(log::LevelFilter::Off);

    let paths = Paths::resolve(&command);
    let migration_notes = prepare_workspace(&command, &paths);
    let mut dashboard = Dashboard::new(&paths);
    for note in migration_notes {
        dashboard.event(None, note.clone());
        dashboard.notice = Some(note);
    }
    let shared: Shared = Arc::new(Mutex::new(dashboard));
    let mut terminal = ratatui::init();
    let result = dashboard_loop(&mut terminal, &shared, &paths);
    ratatui::restore();
    log::set_max_level(log_level);

    let dashboard = shared.lock().unwrap();
    println!(
        "massflash finished: {} Boards done, {} failed. Results in {}",
        dashboard.done,
        dashboard.failed,
        paths.results.display()
    );
    result
}

fn dashboard_loop(
    terminal: &mut ratatui::DefaultTerminal,
    shared: &Shared,
    paths: &Paths,
) -> std::io::Result<()> {
    let mut state = UiState {
        focus: None,
        quit_armed: false,
        mode: InputMode::Normal,
        guided_message: paths.message.as_ref().map(|_| String::new()),
        next_action: NextAction::Wait,
    };
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut last_message_read = Instant::now() - Duration::from_secs(10);

    loop {
        // Pause hotplug reconciliation while a modal is open, so entries
        // can't shift under the operator's fingers.
        if matches!(state.mode, InputMode::Normal)
            && last_scan.elapsed() > Duration::from_millis(500)
        {
            last_scan = Instant::now();
            scan_ports(shared, paths);
        }

        // The guide's message updates even while a modal is open
        if let Some(message_file) = &paths.message {
            if last_message_read.elapsed() > Duration::from_millis(500) {
                last_message_read = Instant::now();
                let content = fs::read_to_string(message_file).unwrap_or_default();
                state.guided_message =
                    Some(content.split_whitespace().collect::<Vec<_>>().join(" "));
            }
        }

        {
            let dashboard = shared.lock().unwrap();
            update_focus(&dashboard, &mut state.focus);
            let focus_key = state.focus.as_ref().map(|focus| focus.key.clone());
            update_next_action(&dashboard, focus_key.as_ref(), &mut state.next_action);
            terminal.draw(|frame| ui(frame, &dashboard, &state))?;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && handle_key(key, shared, paths, &mut state) {
                    return Ok(());
                }
            }
        }
    }
}

/// All focusable entries in display order: port rows, then parked reviews.
fn display_keys(dashboard: &Dashboard) -> Vec<FocusKey> {
    dashboard
        .rows
        .keys()
        .map(|path| FocusKey::Port(path.clone()))
        .chain(dashboard.pending.iter().map(|entry| FocusKey::Pending(entry.id)))
        .collect()
}

/// Whether the entry currently waits for a keypress from the operator.
fn requires_input(dashboard: &Dashboard, key: &FocusKey) -> bool {
    match key {
        FocusKey::Port(path) => matches!(
            dashboard.rows.get(path).map(|row| &row.state),
            Some(BoardState::LedCheck | BoardState::Failed(_))
        ),
        FocusKey::Pending(_) => true,
    }
}

/// Keep exactly one entry focused whenever any exist. Focus follows entries
/// that need keyboard input, but never leaves a manually focused entry and
/// never leaves an entry that itself needs input.
fn update_focus(dashboard: &Dashboard, focus: &mut Option<Focus>) {
    let keys = display_keys(dashboard);
    if keys.is_empty() {
        *focus = None;
        return;
    }
    if let Some(current) = focus.as_ref() {
        if !keys.contains(&current.key) {
            *focus = None;
        }
    }
    match focus.as_mut() {
        None => {
            let key = keys
                .iter()
                .find(|key| requires_input(dashboard, key))
                .unwrap_or(&keys[0])
                .clone();
            *focus = Some(Focus { key, manual: false });
        }
        Some(current) => {
            if !current.manual && !requires_input(dashboard, &current.key) {
                if let Some(key) = keys.iter().find(|key| requires_input(dashboard, key)) {
                    current.key = key.clone();
                }
            }
        }
    }
}

/// Returns true when the operator wants to quit.
fn handle_key(key: KeyEvent, shared: &Shared, paths: &Paths, state: &mut UiState) -> bool {
    match &mut state.mode {
        InputMode::EnterText {
            purpose,
            target,
            text,
        } => {
            match key.code {
                KeyCode::Esc => state.mode = InputMode::Normal,
                KeyCode::Enter if !text.trim().is_empty() => {
                    let mut dashboard = shared.lock().unwrap();
                    let explanation = text.trim().to_string();
                    match purpose {
                        TextPurpose::Dispute => {
                            resolve_failure(
                                &mut dashboard,
                                target,
                                Review::Dispute(explanation),
                                paths,
                            );
                        }
                        TextPurpose::ManualFail => {
                            if let FocusKey::Port(path) = target {
                                if let Some(row) = dashboard.rows.get_mut(path) {
                                    row.log(format!(
                                        "operator requested manual fail: {}",
                                        explanation
                                    ));
                                    row.manual_fail = Some(explanation.clone());
                                }
                                let path = path.clone();
                                dashboard.event(
                                    Some(&path),
                                    format!("manual fail requested: {}", explanation),
                                );
                            }
                        }
                    }
                    state.mode = InputMode::Normal;
                    consume_manual_focus(state);
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(character) => text.push(character),
                _ => {}
            }
            false
        }
        InputMode::PickColor { usb_path, cursor } => {
            match key.code {
                KeyCode::Esc => state.mode = InputMode::Normal,
                KeyCode::Up | KeyCode::Char('k') => {
                    *cursor = (*cursor + PALETTE.len() - 1) % PALETTE.len();
                }
                KeyCode::Down | KeyCode::Char('j') => *cursor = (*cursor + 1) % PALETTE.len(),
                KeyCode::Enter => {
                    let mut dashboard = shared.lock().unwrap();
                    dashboard.port_colors.insert(usb_path.clone(), *cursor);
                    save_port_colors(&paths.settings, &dashboard.port_colors);
                    let message = format!("cable color set to {}", PALETTE[*cursor].name);
                    let usb_path = usb_path.clone();
                    dashboard.event(Some(&usb_path), message);
                    state.mode = InputMode::Normal;
                    consume_manual_focus(state);
                }
                _ => {}
            }
            false
        }
        InputMode::Normal => handle_normal_key(key, shared, paths, state),
    }
}

/// Manual focus decays back to automatic after one interaction.
fn consume_manual_focus(state: &mut UiState) {
    if let Some(focus) = &mut state.focus {
        focus.manual = false;
    }
}

fn handle_normal_key(key: KeyEvent, shared: &Shared, paths: &Paths, state: &mut UiState) -> bool {
    let mut dashboard = shared.lock().unwrap();
    let quit_requested = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || matches!(key.code, KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL));

    if quit_requested {
        let busy = dashboard
            .rows
            .values()
            .any(|row| matches!(row.state, BoardState::Flashing { .. }));
        let unreviewed = !dashboard.pending.is_empty()
            || dashboard
                .rows
                .values()
                .any(|row| matches!(row.state, BoardState::Failed(_)));
        if (busy || unreviewed) && !state.quit_armed {
            state.quit_armed = true;
            return false;
        }
        return true;
    }
    state.quit_armed = false;
    dashboard.notice = None;

    let focused = state.focus.as_ref().map(|focus| focus.key.clone());

    match key.code {
        KeyCode::Char(digit @ '1'..='9') => {
            let keys = display_keys(&dashboard);
            let index = digit as usize - '1' as usize;
            if let Some(key) = keys.get(index) {
                state.focus = Some(Focus {
                    key: key.clone(),
                    manual: true,
                });
            }
        }
        // The real firmware is blinking: space/y = strip works, n = dead
        KeyCode::Char(' ') | KeyCode::Char('y') | KeyCode::Char('n') => {
            let works = key.code != KeyCode::Char('n');
            if let Some(FocusKey::Port(path)) = &focused {
                let mut applied = false;
                if let Some(row) = dashboard.rows.get_mut(path) {
                    if matches!(row.state, BoardState::LedCheck) {
                        row.log(format!(
                            "operator LED strip verdict: {}",
                            if works { "blinks" } else { "dead" }
                        ));
                        row.led_verdict = Some(works);
                        applied = true;
                    }
                }
                if applied {
                    let path = path.clone();
                    dashboard.event(
                        Some(&path),
                        format!(
                            "LED strip verdict: {}",
                            if works { "blinks ✓" } else { "dead ✗" }
                        ),
                    );
                    consume_manual_focus(state);
                }
            }
        }
        KeyCode::Char('v') => {
            if let Some(key) = &focused {
                if review_eligible(&dashboard, key) {
                    resolve_failure(&mut dashboard, key, Review::Verify, paths);
                    consume_manual_focus(state);
                }
            }
        }
        KeyCode::Char('i') => {
            if let Some(key) = &focused {
                if review_eligible(&dashboard, key) {
                    resolve_failure(&mut dashboard, key, Review::Ignore, paths);
                    consume_manual_focus(state);
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(key) = &focused {
                if review_eligible(&dashboard, key) {
                    state.mode = InputMode::EnterText {
                        purpose: TextPurpose::Dispute,
                        target: key.clone(),
                        text: String::new(),
                    };
                }
            }
        }
        KeyCode::Char('f') => {
            if let Some(key @ FocusKey::Port(path)) = &focused {
                let in_progress = matches!(
                    dashboard.rows.get(path).map(|row| &row.state),
                    Some(
                        BoardState::Connecting
                            | BoardState::Flashing { .. }
                            | BoardState::Testing(_)
                            | BoardState::LedCheck
                            | BoardState::Verifying
                    )
                );
                if in_progress {
                    state.mode = InputMode::EnterText {
                        purpose: TextPurpose::ManualFail,
                        target: key.clone(),
                        text: String::new(),
                    };
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(FocusKey::Port(path)) = &focused {
                if matches!(
                    dashboard.rows.get(path).map(|row| &row.state),
                    Some(BoardState::Failed(_))
                ) {
                    restart_board(&mut dashboard, path.clone(), shared, paths);
                    consume_manual_focus(state);
                }
            }
        }
        KeyCode::Char('c') => {
            let usb_path = focused.as_ref().map(|key| match key {
                FocusKey::Port(path) => path.clone(),
                FocusKey::Pending(id) => dashboard
                    .pending
                    .iter()
                    .find(|entry| entry.id == *id)
                    .map(|entry| entry.usb_path.clone())
                    .unwrap_or_default(),
            });
            match usb_path.filter(|path| !path.is_empty()) {
                Some(usb_path) => {
                    let cursor = dashboard.port_colors.get(&usb_path).copied().unwrap_or(0);
                    state.mode = InputMode::PickColor { usb_path, cursor };
                }
                None => {
                    dashboard.notice = Some("no port focused to set a cable color for".into())
                }
            }
        }
        _ => {}
    }
    false
}

/// Whether the entry awaits a verify/dispute/ignore review.
fn review_eligible(dashboard: &Dashboard, key: &FocusKey) -> bool {
    match key {
        FocusKey::Port(path) => matches!(
            dashboard.rows.get(path).map(|row| &row.state),
            Some(BoardState::Failed(_))
        ),
        FocusKey::Pending(_) => true,
    }
}

/// Send a failed Board through the whole pipeline again. The row, its event
/// history and its dump file are kept, so the final dump tells the full
/// story including the failed attempts.
fn restart_board(dashboard: &mut Dashboard, path: String, shared: &Shared, paths: &Paths) {
    let Some(row) = dashboard.rows.get_mut(&path) else {
        return;
    };
    if !Path::new(&row.dev_node).exists() {
        dashboard.notice = Some("Board is no longer on its port — replug it instead".into());
        return;
    }
    let BoardState::Failed(failure) = row.state.clone() else {
        return;
    };
    row.log(format!(
        "operator restarted the pipeline after {}",
        failure.mode.label()
    ));
    // The aborted attempt must not skew the average pipeline duration
    let aborted_seconds = row.finished_seconds.take().unwrap_or(0);
    row.state = BoardState::Connecting;
    row.test_results = TestProgress::default();
    row.led_verdict = None;
    row.manual_fail = None;
    row.generation += 1;
    let generation = row.generation;
    let dev_node = row.dev_node.clone();
    dashboard.total_seconds = dashboard.total_seconds.saturating_sub(aborted_seconds);
    dashboard.event(
        Some(&path),
        format!("restarting the pipeline after {}", failure.mode.label()),
    );
    let worker = Worker {
        shared: shared.clone(),
        usb_path: path,
        dev_node,
        generation,
        results: paths.results.clone(),
        logs: paths.logs.clone(),
    };
    thread::spawn(move || worker.run());
}

/// The operator's review decision for a recorded failure.
enum Review {
    Verify,
    Dispute(String),
    Ignore,
}

impl Review {
    fn kind(&self) -> ReviewKind {
        match self {
            Review::Verify => ReviewKind::Verified,
            Review::Dispute(_) => ReviewKind::Disputed,
            Review::Ignore => ReviewKind::Ignored,
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Review::Dispute(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Apply the operator's review to a failure. Ignored failures are recorded in
/// the CSV but don't count as failed Boards. Disputes rewrite the history
/// dump with the reason included.
fn resolve_failure(dashboard: &mut Dashboard, target: &FocusKey, review: Review, paths: &Paths) {
    let kind = review.kind();

    match target {
        FocusKey::Port(path) => {
            let Some(row) = dashboard.rows.get_mut(path) else {
                return;
            };
            let BoardState::Failed(failure) = row.state.clone() else {
                return;
            };
            match &review {
                Review::Dispute(reason) => {
                    row.log(format!("operator disputed the failure: {}", reason))
                }
                Review::Verify => row.log("operator verified the failure"),
                Review::Ignore => row.log("operator ignored the failure"),
            }
            let log_path = row.log_path.clone().unwrap_or_else(|| {
                history_file_path(&paths.logs, row.mac.as_deref(), failure.mode.label())
            });
            write_history_file(
                &log_path,
                row.mac.as_deref(),
                path,
                failure.mode.label(),
                &failure.detail,
                review.reason(),
                &row.history,
            );
            append_result(
                &paths.results,
                CsvRecord {
                    mac: row.mac.as_deref().unwrap_or(""),
                    usb_port: path,
                    seconds: row.finished_seconds.unwrap_or(0),
                    test: &row.test_results.clone(),
                    result: failure.mode.label(),
                    detail: &failure.detail,
                    verdict: kind.label(),
                    reason: review.reason().unwrap_or(""),
                    log: &log_path.display().to_string(),
                },
            );
            let row = dashboard.rows.get_mut(path).unwrap();
            row.state = BoardState::Resolved {
                verdict: kind,
                log_path: Some(log_path.clone()),
            };
            if kind == ReviewKind::Disputed {
                dashboard.notice =
                    Some(format!("event history dumped to {}", log_path.display()));
            }
            if kind != ReviewKind::Ignored {
                dashboard.failed += 1;
            }
            let path = path.clone();
            dashboard.event(Some(&path), format!("failure {}", kind.label()));
        }
        FocusKey::Pending(id) => {
            let Some(position) = dashboard.pending.iter().position(|entry| entry.id == *id)
            else {
                return;
            };
            let mut entry = dashboard.pending.remove(position);
            match &review {
                Review::Dispute(reason) => entry
                    .history
                    .push(format!("operator disputed the failure: {}", reason)),
                Review::Verify => entry.history.push("operator verified the failure".into()),
                Review::Ignore => entry.history.push("operator ignored the failure".into()),
            }
            let log_path = entry.log_path.clone().unwrap_or_else(|| {
                history_file_path(&paths.logs, Some(&entry.mac), entry.failure.mode.label())
            });
            write_history_file(
                &log_path,
                Some(&entry.mac),
                &entry.usb_path,
                entry.failure.mode.label(),
                &entry.failure.detail,
                review.reason(),
                &entry.history,
            );
            append_result(
                &paths.results,
                CsvRecord {
                    mac: &entry.mac,
                    usb_port: &entry.usb_path,
                    seconds: entry.seconds,
                    test: &entry.test_results,
                    result: entry.failure.mode.label(),
                    detail: &entry.failure.detail,
                    verdict: kind.label(),
                    reason: review.reason().unwrap_or(""),
                    log: &log_path.display().to_string(),
                },
            );
            if kind == ReviewKind::Disputed {
                dashboard.notice =
                    Some(format!("event history dumped to {}", log_path.display()));
            }
            if kind != ReviewKind::Ignored {
                dashboard.failed += 1;
            }
            dashboard.event(Some(&entry.usb_path), format!("failure {}", kind.label()));
        }
    }
}

/// Choose the dump path for one Board's run. Every Board gets a dump, pass or
/// fail; reviews rewrite the same file with the resolution included.
fn history_file_path(logs_dir: &Path, mac: Option<&str>, result_label: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    logs_dir.join(format!(
        "{}-{}-{}.log",
        timestamp,
        mac.unwrap_or("unknown").replace(':', ""),
        result_label
    ))
}

/// Write (or rewrite) a Board's full event history dump.
fn write_history_file(
    path: &Path,
    mac: Option<&str>,
    usb_path: &str,
    result: &str,
    detail: &str,
    dispute_reason: Option<&str>,
    history: &[String],
) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut content = format!(
        "board: {}\nusb port: {}\nresult: {}\ndetail: {}\n",
        mac.unwrap_or("unknown"),
        usb_path,
        result,
        detail
    );
    if let Some(reason) = dispute_reason {
        content.push_str(&format!("dispute reason: {}\n", reason));
    }
    content.push('\n');
    content.push_str(&history.join("\n"));
    content.push('\n');
    let _ = fs::write(path, content);
}

/// A USB device with the Espressif vendor id, possibly without a serial port.
struct DetectedUsb {
    /// USB port chain, e.g. "3-4.1" — stable per physical hub port
    usb_path: String,
    /// The USB serial number, which is the Board's MAC address
    serial_number: String,
    /// Current device node, e.g. "/dev/ttyACM0" — None when enumeration
    /// produced no serial port interface
    dev_node: Option<String>,
}

fn detect_usb() -> Vec<DetectedUsb> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Devices are named by port chain ("3-4.1"); interfaces contain ':', hubs "usb3"
        if !name.contains('-') || name.contains(':') {
            continue;
        }
        let vid = fs::read_to_string(entry.path().join("idVendor")).unwrap_or_default();
        if vid.trim() != ESPRESSIF_VID {
            continue;
        }
        let serial_number = fs::read_to_string(entry.path().join("serial"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let tty_dir = entry.path().join(format!("{}:1.0", name)).join("tty");
        let dev_node = fs::read_dir(tty_dir).ok().and_then(|mut ttys| {
            ttys.next()
                .and_then(|tty| tty.ok())
                .map(|tty| format!("/dev/{}", tty.file_name().to_string_lossy()))
        });
        found.push(DetectedUsb {
            usb_path: name,
            serial_number,
            dev_node,
        });
    }
    found
}

/// Reconcile the dashboard with the USB bus: spawn workers for new Boards,
/// surface enumeration failures, park unreviewed failures of removed Boards.
fn scan_ports(shared: &Shared, paths: &Paths) {
    let detected = detect_usb();
    let mut dashboard = shared.lock().unwrap();

    for device in &detected {
        dashboard.known_ports.insert(device.usb_path.clone());
    }

    // Boards whose USB device is present but never produced a serial port
    for device in detected.iter().filter(|device| device.dev_node.is_none()) {
        let first_seen = *dashboard
            .no_serial_since
            .entry(device.usb_path.clone())
            .or_insert_with(Instant::now);
        let already_reported = dashboard
            .pending
            .iter()
            .any(|entry| entry.mac == device.serial_number);
        if first_seen.elapsed() > NO_SERIAL_TIMEOUT && !already_reported {
            let history = vec![format!(
                "USB device {} (usb port {}) present without a serial port interface",
                device.serial_number, device.usb_path
            )];
            let log_path = history_file_path(
                &paths.logs,
                Some(&device.serial_number),
                FailureMode::NoSerialPort.label(),
            );
            write_history_file(
                &log_path,
                Some(&device.serial_number),
                &device.usb_path,
                FailureMode::NoSerialPort.label(),
                "USB device enumerated but exposed no serial port",
                None,
                &history,
            );
            dashboard.push_pending(PendingReview {
                id: 0,
                mac: device.serial_number.clone(),
                usb_path: device.usb_path.clone(),
                failure: Failure {
                    mode: FailureMode::NoSerialPort,
                    detail: "USB device enumerated but exposed no serial port".into(),
                },
                test_results: TestProgress::default(),
                seconds: first_seen.elapsed().as_secs(),
                history,
                log_path: Some(log_path),
            });
            dashboard.event(
                Some(&device.usb_path),
                format!("{} enumerated without a serial port", device.serial_number),
            );
        }
    }
    dashboard.no_serial_since.retain(|usb_path, _| {
        detected
            .iter()
            .any(|device| &device.usb_path == usb_path && device.dev_node.is_none())
    });

    for device in &detected {
        let Some(dev_node) = &device.dev_node else {
            continue;
        };
        let respawn = match dashboard.rows.get(&device.usb_path) {
            None => true,
            // A dropped-out Board is back on its port (maybe with the same
            // device node): park the unreviewed failure and start over.
            Some(row) => match &row.state {
                BoardState::Failed(failure) if failure.mode == FailureMode::UsbDropout => true,
                BoardState::Done | BoardState::Resolved { .. } | BoardState::Failed(_) => {
                    row.dev_node != *dev_node
                }
                _ => false,
            },
        };
        if !respawn {
            continue;
        }
        let generation = match dashboard.rows.remove(&device.usb_path) {
            Some(row) => {
                let generation = row.generation + 1;
                match &row.state {
                    // connect-failed is a cable/contact issue, not worth a review
                    BoardState::Failed(failure) if failure.mode == FailureMode::ConnectFailed => {
                        dashboard.event(
                            Some(&device.usb_path),
                            "connect-failed cleared by reconnect — not recorded",
                        );
                    }
                    BoardState::Failed(_) => {
                        park_failure(&mut dashboard, &device.usb_path, row);
                    }
                    _ => {}
                }
                generation
            }
            None => 0,
        };
        dashboard.event(
            Some(&device.usb_path),
            format!("Board {} plugged in ({})", device.serial_number, dev_node),
        );
        dashboard.rows.insert(
            device.usb_path.clone(),
            BoardRow {
                dev_node: dev_node.clone(),
                mac: None,
                state: BoardState::Connecting,
                led_verdict: None,
                generation,
                test_results: TestProgress::default(),
                started: Instant::now(),
                finished_seconds: None,
                history: vec![format!(
                    "detected on usb port {} as {}",
                    device.usb_path, dev_node
                )],
                last_activity: Instant::now(),
                manual_fail: None,
                log_path: None,
            },
        );
        let worker = Worker {
            shared: shared.clone(),
            usb_path: device.usb_path.clone(),
            dev_node: dev_node.clone(),
            generation,
            results: paths.results.clone(),
            logs: paths.logs.clone(),
        };
        thread::spawn(move || worker.run());
    }

    let vanished: Vec<String> = dashboard
        .rows
        .keys()
        .filter(|path| {
            !detected
                .iter()
                .any(|device| &&device.usb_path == path && device.dev_node.is_some())
        })
        .cloned()
        .collect();
    for path in vanished {
        let state = dashboard.rows.get(&path).map(|row| row.state.clone());
        match state {
            // Reviewed or successful Boards just leave
            Some(BoardState::Done) | Some(BoardState::Resolved { .. }) => {
                dashboard.rows.remove(&path);
                dashboard.event(Some(&path), "Board unplugged");
            }
            // connect-failed on an unplugged Board is a cable/contact issue:
            // drop it without a review or a CSV record
            Some(BoardState::Failed(failure)) if failure.mode == FailureMode::ConnectFailed => {
                dashboard.rows.remove(&path);
                dashboard.event(
                    Some(&path),
                    "connect-failed Board unplugged — ignored (cable/contact suspected)",
                );
            }
            // Unreviewed failures move to the review list so the port is free
            Some(BoardState::Failed(_)) => {
                let row = dashboard.rows.remove(&path).unwrap();
                park_failure(&mut dashboard, &path, row);
                dashboard.event(
                    Some(&path),
                    "Board unplugged with unreviewed failure — parked for review",
                );
            }
            // In progress: the worker will notice the dead port and fail
            // with usb-dropout on its own.
            _ => {}
        }
    }

    // Stall watchdog: espflash hung without progress or error. The worker
    // thread cannot be cancelled; it is orphaned and its late writes are
    // ignored because the row is already in a terminal state.
    let stalled: Vec<String> = dashboard
        .rows
        .iter()
        .filter(|(_, row)| {
            matches!(
                row.state,
                BoardState::Connecting | BoardState::Flashing { .. }
            ) && row.last_activity.elapsed() > STALL_TIMEOUT
        })
        .map(|(path, _)| path.clone())
        .collect();
    for path in stalled {
        let row = dashboard.rows.get_mut(&path).unwrap();
        let failure = Failure {
            mode: FailureMode::FlashStalled,
            detail: format!(
                "no flashing progress or error for {}s",
                STALL_TIMEOUT.as_secs()
            ),
        };
        row.log(format!(
            "watchdog: {} — {}",
            failure.mode.label(),
            failure.detail
        ));
        row.finished_seconds = Some(row.started.elapsed().as_secs());
        let log_path = history_file_path(&paths.logs, row.mac.as_deref(), failure.mode.label());
        write_history_file(
            &log_path,
            row.mac.as_deref(),
            &path,
            failure.mode.label(),
            &failure.detail,
            None,
            &row.history,
        );
        row.log_path = Some(log_path);
        let message = format!("❌ {} — {}", failure.mode.label(), failure.detail);
        row.state = BoardState::Failed(failure);
        dashboard.event(Some(&path), message);
    }
}

/// Move an unreviewed failure off its port row into the review list.
fn park_failure(dashboard: &mut Dashboard, usb_path: &str, row: BoardRow) {
    let BoardState::Failed(failure) = row.state else {
        return;
    };
    dashboard.push_pending(PendingReview {
        id: 0,
        mac: row.mac.unwrap_or_else(|| "unknown".into()),
        usb_path: usb_path.to_string(),
        failure,
        test_results: row.test_results,
        seconds: row.finished_seconds.unwrap_or(0),
        history: row.history,
        log_path: row.log_path,
    });
}

/// Reports flashing progress of one Board into its dashboard row.
struct RowProgress<'a> {
    worker: &'a Worker,
    firmware: &'static str,
    total: usize,
    current: usize,
}

impl ProgressCallbacks for RowProgress<'_> {
    fn init(&mut self, addr: u32, total: usize) {
        self.total = total;
        self.current = 0;
        self.worker.update(|row| {
            row.log(format!("writing segment @{:#x} ({} bytes)", addr, total));
        });
    }
    fn update(&mut self, current: usize) {
        self.current = current;
        let percent = if self.total == 0 {
            0
        } else {
            (self.current * 100 / self.total) as u8
        };
        self.worker.update(|row| {
            row.state = BoardState::Flashing {
                firmware: self.firmware,
                percent,
            }
        });
    }
    fn finish(&mut self) {
        self.worker.update(|row| row.log("segment written"));
    }
}

struct Worker {
    shared: Shared,
    usb_path: String,
    dev_node: String,
    generation: u64,
    results: PathBuf,
    logs: PathBuf,
}

enum WorkerError {
    Fail(Failure),
    /// The Board disappeared from the bus
    Gone { during: &'static str },
}

impl Worker {
    /// Update this worker's row, unless the Board was re-plugged and the row
    /// belongs to a newer worker now.
    fn update(&self, change: impl FnOnce(&mut BoardRow)) {
        let mut dashboard = self.shared.lock().unwrap();
        if let Some(row) = dashboard.rows.get_mut(&self.usb_path) {
            if row.generation == self.generation {
                change(row);
                row.last_activity = Instant::now();
            }
        }
    }

    /// A manual fail requested by the operator, to be honored at the next
    /// checkpoint.
    fn check_manual_fail(&self) -> Result<(), WorkerError> {
        let mut explanation = None;
        self.update(|row| explanation = row.manual_fail.clone());
        match explanation {
            Some(explanation) => Err(WorkerError::Fail(Failure {
                mode: FailureMode::Manual,
                detail: explanation,
            })),
            None => Ok(()),
        }
    }

    fn log(&self, message: impl AsRef<str>) {
        self.update(|row| row.log(message.as_ref()));
    }

    /// Announce a hardware event in the shared event log.
    fn event(&self, message: impl Into<String>) {
        let mut dashboard = self.shared.lock().unwrap();
        let message = message.into();
        dashboard.event(Some(&self.usb_path), message);
    }

    fn run(self) {
        let outcome = catch_unwind(AssertUnwindSafe(|| self.process())).unwrap_or_else(|panic| {
            let detail = panic
                .downcast_ref::<&str>()
                .map(|text| text.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(WorkerError::Fail(Failure {
                mode: FailureMode::InternalPanic,
                detail,
            }))
        });
        let failure = match outcome {
            Ok(()) => None,
            Err(WorkerError::Fail(failure)) => Some(failure),
            Err(WorkerError::Gone { during }) => Some(Failure {
                mode: FailureMode::UsbDropout,
                detail: format!("USB connection lost {}", during),
            }),
        };

        let mut dashboard = self.shared.lock().unwrap();
        let Some(row) = dashboard.rows.get_mut(&self.usb_path) else {
            return;
        };
        if row.generation != self.generation {
            return;
        }
        // The stall watchdog may have already recorded an outcome for this
        // row; a late worker result must not overwrite it.
        if matches!(
            row.state,
            BoardState::Done | BoardState::Failed(_) | BoardState::Resolved { .. }
        ) {
            return;
        }
        let seconds = row.started.elapsed().as_secs();
        row.finished_seconds = Some(seconds);
        dashboard.total_seconds += seconds;
        let row = dashboard.rows.get_mut(&self.usb_path).unwrap();

        let (result_label, detail) = match &failure {
            None => ("pass", String::new()),
            Some(failure) => (failure.mode.label(), failure.detail.clone()),
        };
        match &failure {
            None => row.log("pipeline finished: pass"),
            Some(failure) => row.log(format!(
                "pipeline failed: {} — {}",
                failure.mode.label(),
                failure.detail
            )),
        }
        // A restarted Board keeps its dump file so it tells the full story;
        // rename it so the filename reflects the latest outcome.
        let log_path = match row.log_path.take() {
            Some(old_path) => {
                let new_path = history_file_path(&self.logs, row.mac.as_deref(), result_label);
                if old_path != new_path {
                    let _ = fs::rename(&old_path, &new_path);
                }
                new_path
            }
            None => history_file_path(&self.logs, row.mac.as_deref(), result_label),
        };
        write_history_file(
            &log_path,
            row.mac.as_deref(),
            &self.usb_path,
            result_label,
            &detail,
            None,
            &row.history,
        );
        row.log_path = Some(log_path.clone());
        let record_mac = row.mac.clone().unwrap_or_default();
        let test = row.test_results.clone();

        match failure {
            None => {
                row.state = BoardState::Done;
                append_result(
                    &self.results,
                    CsvRecord {
                        mac: &record_mac,
                        usb_port: &self.usb_path,
                        seconds,
                        test: &test,
                        result: "pass",
                        detail: "",
                        verdict: "",
                        reason: "",
                        log: &log_path.display().to_string(),
                    },
                );
                dashboard.done += 1;
                dashboard.event(Some(&self.usb_path), format!("✅ done in {}s", seconds));
            }
            // The operator already gave the explanation; no extra review round
            Some(failure) if failure.mode == FailureMode::Manual => {
                append_result(
                    &self.results,
                    CsvRecord {
                        mac: &record_mac,
                        usb_port: &self.usb_path,
                        seconds,
                        test: &test,
                        result: failure.mode.label(),
                        detail: &failure.detail,
                        verdict: ReviewKind::Manual.label(),
                        reason: "",
                        log: &log_path.display().to_string(),
                    },
                );
                let message = format!("❌ manual — {}", failure.detail);
                row.state = BoardState::Resolved {
                    verdict: ReviewKind::Manual,
                    log_path: Some(log_path),
                };
                dashboard.failed += 1;
                dashboard.event(Some(&self.usb_path), message);
            }
            Some(failure) => {
                let message = format!("❌ {} — {}", failure.mode.label(), failure.detail);
                // The CSV row is written when the operator reviews the failure
                row.state = BoardState::Failed(failure);
                dashboard.event(Some(&self.usb_path), message);
            }
        }
    }

    fn process(&self) -> Result<(), WorkerError> {
        // 1. Flash the firmware with the board-test Program
        let flashed = self.flash_with_retry(true, "board-test")?;
        self.update(|row| {
            row.mac = Some(flashed.mac.clone());
            row.log(format!("flashed board-test firmware, mac {}", flashed.mac));
        });
        self.event(format!("flashed board-test firmware ({})", flashed.mac));

        // 2. Reset and follow the test output through the self test
        // (including the ambient light dance)
        let mut serial = flashed.serial;
        reset_board(&mut serial).map_err(|_| WorkerError::Gone {
            during: "while resetting into the board test",
        })?;
        self.update(|row| {
            row.log("reset into board test");
            row.state = BoardState::Testing(TestProgress::default());
        });
        self.follow_test(&mut serial)?;
        self.event("all automated tests passed 🎉");

        // 3. Flash the production firmware right away — its own blinking is
        // the LED strip test. The bootloader needs the port, so close our
        // serial handle first.
        drop(serial);
        let flashed = self.flash_with_retry(false, "rudelblinken")?;
        self.log("flashed production firmware");
        self.event("flashed production firmware");

        // 4. Verify that it boots and survives without crashing
        let mut serial = flashed.serial;
        self.update(|row| row.state = BoardState::Verifying);
        self.verify_boot(&mut serial)?;
        self.event("production firmware boot confirmed");

        // 5. The production firmware is blinking: LED strip verdict
        self.update(|row| {
            row.log("awaiting LED strip verdict on the production firmware");
            row.state = BoardState::LedCheck;
        });
        let verdict = self.await_led_verdict(&mut serial)?;
        if !verdict {
            return Err(WorkerError::Fail(Failure {
                mode: FailureMode::LedStripDead,
                detail: "operator judged the LED strip dead".into(),
            }));
        }
        Ok(())
    }

    /// Flash, retrying once: cable blips and stray bootloader states vanish on
    /// retry, real faults reproduce.
    fn flash_with_retry(
        &self,
        test_program: bool,
        firmware: &'static str,
    ) -> Result<crate::flash::FlashedBoard, WorkerError> {
        for attempt in 1..=2 {
            self.update(|row| {
                row.state = BoardState::Flashing {
                    firmware,
                    percent: 0,
                }
            });
            let mut progress = RowProgress {
                worker: self,
                firmware,
                total: 0,
                current: 0,
            };
            match flash_board(
                Some(&self.dev_node),
                test_program,
                true,
                false,
                Some(&mut progress),
            ) {
                Ok(flashed) => {
                    self.check_manual_fail()?;
                    return Ok(flashed);
                }
                Err(_) if !Path::new(&self.dev_node).exists() => {
                    return Err(WorkerError::Gone {
                        during: "while flashing",
                    })
                }
                Err(error) if attempt == 1 => {
                    self.log(format!(
                        "flashing {} failed ({}), retrying once",
                        firmware, error
                    ));
                    self.event(format!("flashing {} failed, retrying once", firmware));
                }
                Err(error) => return Err(WorkerError::Fail(flash_failure(error))),
            }
        }
        unreachable!()
    }

    /// Drive the row's TestProgress from the Board's log until the test
    /// Program reports an overall result or a hardware timeout is reached.
    fn follow_test(&self, serial: &mut dyn SerialPort) -> Result<(), WorkerError> {
        let mut lines = LineReader::default();
        let started = Instant::now();
        let mut current = TestProgress::default();
        let mut first_light: Option<String> = None;
        let mut light_changed = false;
        let mut got_output = false;
        let mut reset_banners = 0u32;

        loop {
            self.check_manual_fail()?;
            let line = lines.next_line(serial).map_err(|_| WorkerError::Gone {
                during: "during the board test",
            })?;
            if let Some(line) = line {
                got_output = true;
                if line.contains("rst:0x") {
                    reset_banners += 1;
                    // One banner comes from our own reset; more mean the
                    // Board keeps rebooting on its own.
                    if reset_banners >= BOOT_LOOP_THRESHOLD {
                        return Err(WorkerError::Fail(Failure {
                            mode: FailureMode::BootLoop,
                            detail: format!(
                                "board reset {} times during the test — brownout/power suspect",
                                reset_banners - 1
                            ),
                        }));
                    }
                }
                let previous = current.clone();
                self.update(|row| {
                    row.log(format!("« {}", line.trim_end()));
                    if let BoardState::Testing(test) = &mut row.state {
                        apply_test_line(&line, test);
                        row.test_results = test.clone();
                        current = test.clone();
                    }
                });
                self.announce_test_transitions(&previous, &current);
                if let Some(light) = &current.light {
                    match &first_light {
                        None => first_light = Some(light.clone()),
                        Some(first) if first != light => light_changed = true,
                        _ => {}
                    }
                }

                // Fail fast on a definitive ❌ — no point finishing the
                // sensor dance on a Board that already failed.
                if current.voltage == Some(false) {
                    return Err(WorkerError::Fail(Failure {
                        mode: FailureMode::PowerFail,
                        detail: "board test reported power supply not working".into(),
                    }));
                }
                if current.ble == Some(false) {
                    return Err(WorkerError::Fail(Failure {
                        mode: FailureMode::BleFail,
                        detail: "board test reported BLE not working".into(),
                    }));
                }

                if line.contains("🎉 All automated tests passed") {
                    return Ok(());
                }
                if line.contains("Some tests failed") {
                    return Err(WorkerError::Fail(test_failure(&current)));
                }
            }

            if !got_output && started.elapsed() > NO_OUTPUT_TIMEOUT {
                return Err(WorkerError::Fail(Failure {
                    mode: FailureMode::NoTestOutput,
                    detail: format!(
                        "no serial output within {}s of reset — board not running",
                        NO_OUTPUT_TIMEOUT.as_secs()
                    ),
                }));
            }
            if current.voltage.is_none() && started.elapsed() > POWER_TIMEOUT {
                return Err(WorkerError::Fail(Failure {
                    mode: FailureMode::PowerNoReading,
                    detail: format!(
                        "no valid supply voltage reading within {}s — broken voltage divider, \
                         or test Program not running (check the history)",
                        POWER_TIMEOUT.as_secs()
                    ),
                }));
            }
            if current.ambient.is_none() && !light_changed && started.elapsed() > SENSOR_TIMEOUT {
                return Err(WorkerError::Fail(Failure {
                    mode: FailureMode::SensorFrozen,
                    detail: format!(
                        "ambient light reading never changed from {} in {}s",
                        first_light.as_deref().unwrap_or("(none)"),
                        SENSOR_TIMEOUT.as_secs()
                    ),
                }));
            }
        }
    }

    /// Emit a hardware event whenever one of the automated tests concludes.
    fn announce_test_transitions(&self, previous: &TestProgress, current: &TestProgress) {
        let tests = [
            ("power supply", previous.voltage, current.voltage),
            ("BLE", previous.ble, current.ble),
            ("light sensor", previous.ambient, current.ambient),
        ];
        for (name, before, after) in tests {
            if before.is_none() && after.is_some() {
                let passed = after == Some(true);
                self.event(format!("{} {}", name, if passed { "✓" } else { "✗" }));
            }
        }
    }

    /// Poll for the operator's verdict while draining serial output, so an
    /// unplug or a crash of the production firmware is still noticed.
    fn await_led_verdict(&self, serial: &mut dyn SerialPort) -> Result<bool, WorkerError> {
        let mut lines = LineReader::default();
        loop {
            self.check_manual_fail()?;
            let mut verdict = None;
            self.update(|row| verdict = row.led_verdict);
            if let Some(verdict) = verdict {
                return Ok(verdict);
            }
            let line = lines.next_line(serial).map_err(|_| WorkerError::Gone {
                during: "during the LED strip check",
            })?;
            if let Some(line) = line {
                if crash_marker(&line) {
                    self.update(|row| row.log(format!("« {}", line.trim_end())));
                    return Err(WorkerError::Fail(Failure {
                        mode: FailureMode::BootCrash,
                        detail: "production firmware crashed during the LED strip check".into(),
                    }));
                }
            }
        }
    }

    /// Wait for a boot message from the production firmware, give the Board a
    /// second reset before declaring it dead, then watch a stability window
    /// for crashes.
    fn verify_boot(&self, serial: &mut crate::flash::Port) -> Result<(), WorkerError> {
        for round in 1..=2 {
            reset_board(serial).map_err(|_| WorkerError::Gone {
                during: "while resetting into the production firmware",
            })?;
            let mut lines = LineReader::default();
            let deadline = Instant::now() + BOOT_TIMEOUT;
            let mut booted = false;
            while Instant::now() < deadline {
                self.check_manual_fail()?;
                let line = lines.next_line(serial).map_err(|_| WorkerError::Gone {
                    during: "while verifying boot",
                })?;
                let Some(line) = line else { continue };
                self.update(|row| row.log(format!("« {}", line.trim_end())));
                if line.contains("Calling app_main()") || line.contains("wasm-guest") {
                    booted = true;
                    break;
                }
            }
            if !booted {
                if round == 1 {
                    self.log("no boot message yet, resetting once more");
                }
                continue;
            }
            // Booted — watch for crashes before trusting it
            let stable_until = Instant::now() + BOOT_STABILITY_WINDOW;
            while Instant::now() < stable_until {
                self.check_manual_fail()?;
                let line = lines.next_line(serial).map_err(|_| WorkerError::Gone {
                    during: "while verifying boot stability",
                })?;
                let Some(line) = line else { continue };
                self.update(|row| row.log(format!("« {}", line.trim_end())));
                if crash_marker(&line) {
                    return Err(WorkerError::Fail(Failure {
                        mode: FailureMode::BootCrash,
                        detail: format!(
                            "production firmware crashed within {}s of booting",
                            BOOT_STABILITY_WINDOW.as_secs()
                        ),
                    }));
                }
            }
            self.log("production firmware boot confirmed stable");
            return Ok(());
        }
        Err(WorkerError::Fail(Failure {
            mode: FailureMode::NoBoot,
            detail: format!(
                "no boot message within {}s after two resets",
                2 * BOOT_TIMEOUT.as_secs()
            ),
        }))
    }
}

/// A line that indicates the firmware crashed or reset unexpectedly.
fn crash_marker(line: &str) -> bool {
    line.contains("rst:0x")
        || line.contains("panicked")
        || line.contains("abort()")
        || line.contains("Guru Meditation")
}

/// Map an espflash error to the exact failure mode of its stage.
fn flash_failure(error: FlashError) -> Failure {
    let mode = match error.stage {
        "connect" => FailureMode::ConnectFailed,
        "write flash" => FailureMode::FlashWriteFailed,
        _ => FailureMode::FlashOther,
    };
    Failure {
        mode,
        detail: error.to_string(),
    }
}

/// Map the test firmware's overall failure verdict to an exact failure mode.
fn test_failure(test: &TestProgress) -> Failure {
    let mut broken = Vec::new();
    if test.voltage != Some(true) {
        broken.push((FailureMode::PowerFail, "power supply"));
    }
    if test.ble != Some(true) {
        broken.push((FailureMode::BleFail, "BLE"));
    }
    if test.ambient != Some(true) {
        broken.push((FailureMode::SensorFrozen, "ambient light sensor"));
    }
    match broken.as_slice() {
        [(mode, what)] => Failure {
            mode: *mode,
            detail: format!("board test reported {} not working", what),
        },
        _ => Failure {
            mode: FailureMode::TestFailed,
            detail: format!(
                "board test reported failures: {}",
                broken
                    .iter()
                    .map(|(_, what)| *what)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    }
}

/// Hard-reset a Board via the RTS line (wired to EN through the USB-JTAG peripheral).
fn reset_board(serial: &mut dyn SerialPort) -> Result<(), serialport::Error> {
    serial.write_data_terminal_ready(false)?;
    serial.write_request_to_send(true)?;
    thread::sleep(Duration::from_millis(100));
    serial.write_request_to_send(false)?;
    serial.set_timeout(Duration::from_millis(200))?;
    Ok(())
}

/// The Board disappeared while reading from its serial port.
struct PortGone;

/// Accumulates serial bytes into ANSI-stripped lines without blocking forever.
#[derive(Default)]
struct LineReader {
    buffer: Vec<u8>,
}

impl LineReader {
    /// Read the next line, `None` when nothing arrived before the port timeout.
    fn next_line(&mut self, serial: &mut dyn SerialPort) -> Result<Option<String>, PortGone> {
        loop {
            if let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                return Ok(Some(strip_ansi(&String::from_utf8_lossy(&line))));
            }
            let mut chunk = [0u8; 1024];
            match serial.read(&mut chunk) {
                Ok(0) => return Err(PortGone),
                Ok(count) => self.buffer.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == ErrorKind::TimedOut => return Ok(None),
                Err(_) => return Err(PortGone),
            }
        }
    }
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars();
    while let Some(character) = characters.next() {
        if character != '\x1b' {
            if character != '\r' {
                output.push(character);
            }
            continue;
        }
        // Skip over "\x1b[…letter" sequences
        if characters.next() == Some('[') {
            for terminator in characters.by_ref() {
                if terminator.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    output
}

/// Interpret one log line of the board-test Program.
fn apply_test_line(line: &str, test: &mut TestProgress) {
    if line.contains("✅: 5V power supply") {
        test.voltage = Some(true);
    } else if line.contains("❌: Battery power supply detected before") {
        test.voltage = Some(false);
    } else if line.contains("✅: BLE working") {
        test.ble = Some(true);
    } else if line.contains("❌: BLE not working") {
        test.ble = Some(false);
    } else if line.contains("✅: Ambient light sensor working") {
        test.ambient = Some(true);
        test.prompt = AmbientPrompt::Passed;
    } else if line.contains("[1/3]") {
        test.prompt = AmbientPrompt::Shine;
    } else if line.contains("[2/3]") {
        test.prompt = AmbientPrompt::CoverAgain;
    } else if line.contains("sensor test failed, restarting") {
        test.prompt = AmbientPrompt::Cover;
    } else if let Some(reading) = line.split("Ambient light: ").nth(1) {
        test.light = Some(reading.trim().trim_matches('"').to_string());
    }
}

struct CsvRecord<'a> {
    mac: &'a str,
    usb_port: &'a str,
    seconds: u64,
    test: &'a TestProgress,
    result: &'a str,
    detail: &'a str,
    verdict: &'a str,
    reason: &'a str,
    log: &'a str,
}

fn append_result(path: &PathBuf, record: CsvRecord) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let header_needed = !path.exists();
    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    if header_needed {
        let _ = writeln!(file, "{}", CSV_HEADER);
    }
    let show = |value: Option<bool>| match value {
        Some(true) => "pass",
        Some(false) => "fail",
        None => "",
    };
    let _ = writeln!(
        file,
        "{},{},{},{},{},{},{},{},\"{}\",{},\"{}\",{}",
        timestamp,
        record.mac,
        record.usb_port,
        record.seconds,
        show(record.test.voltage),
        show(record.test.ble),
        show(record.test.ambient),
        record.result,
        record.detail.replace('"', "'"),
        record.verdict,
        record.reason.replace('"', "'"),
        record.log,
    );
}

fn ui(frame: &mut Frame, dashboard: &Dashboard, state: &UiState) {
    // In guided mode a message row from the guide sits above everything
    let content_area = match &state.guided_message {
        Some(message) => {
            let [message_area, rest] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)])
                    .areas(frame.area());
            if !message.is_empty() {
                frame.render_widget(
                    Paragraph::new(format!("📢 {}", message)).bold().reversed(),
                    message_area,
                );
            }
            rest
        }
        None => frame.area(),
    };
    let ports_height = dashboard.known_ports.len().max(1) as u16 + 1;
    let [header_area, notice_area, main_area, action_area, space_area, ports_area, events_area, status_area] =
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(ports_height),
            Constraint::Percentage(30),
            Constraint::Length(1),
        ])
        .areas(content_area);

    frame.render_widget(
        Paragraph::new(format!(
            "rudelctl massflash — plug Boards into the hub to begin   done: {}   failed: {}",
            dashboard.done, dashboard.failed
        ))
        .bold(),
        header_area,
    );
    let notice_line = match &dashboard.notice {
        Some(notice) => Paragraph::new(notice.as_str()).italic(),
        None => Paragraph::new(
            "keys: space/y/n = blink verdict   v/d/i = verify/dispute/ignore   r = restart   f = fail by hand   c = cable color   1-9 = focus   q = quit",
        )
        .dim(),
    };
    frame.render_widget(notice_line, notice_area);

    render_boards(frame, main_area, dashboard, state);
    render_next_action(frame, action_area, dashboard, &state.next_action);
    render_space_hint(frame, space_area, dashboard, state);
    render_ports(frame, ports_area, dashboard);
    render_events(frame, events_area, dashboard);
    render_status(frame, status_area, dashboard, state);

    match &state.mode {
        InputMode::Normal => {}
        InputMode::EnterText { purpose, text, .. } => render_text_modal(frame, *purpose, text),
        InputMode::PickColor { usb_path, cursor } => {
            render_color_modal(frame, dashboard, usb_path, *cursor)
        }
    }
}

fn render_boards(frame: &mut Frame, area: Rect, dashboard: &Dashboard, state: &UiState) {
    if dashboard.rows.is_empty() && dashboard.pending.is_empty() {
        frame.render_widget(
            Paragraph::new("waiting for Boards… (no Espressif USB devices found)").dim(),
            area,
        );
        return;
    }
    let focused_key = state.focus.as_ref().map(|focus| &focus.key);
    let port_rows = dashboard
        .rows
        .iter()
        .enumerate()
        .map(|(index, (usb_path, row))| {
            let (color, description) = describe(row);
            table_row(
                index,
                focused_key == Some(&FocusKey::Port(usb_path.clone())),
                usb_path,
                dashboard.cable_color(usb_path),
                row.mac.as_deref().unwrap_or("…"),
                description,
                color,
            )
        });
    let review_rows = dashboard
        .pending
        .iter()
        .enumerate()
        .map(|(pending_index, entry)| {
            let index = dashboard.rows.len() + pending_index;
            table_row(
                index,
                focused_key == Some(&FocusKey::Pending(entry.id)),
                &entry.usb_path,
                dashboard.cable_color(&entry.usb_path),
                &entry.mac,
                format!(
                    "review: ❌ {} — {}   [v]/[d]/[i]",
                    entry.failure.mode.label(),
                    entry.failure.detail
                ),
                Color::Red,
            )
        });
    let table = Table::new(
        port_rows.chain(review_rows),
        [
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Fill(1),
        ],
    )
    .header(Row::new(["#", "port", "mac", "state"]).dim());
    frame.render_widget(table, area);
}

fn table_row<'a>(
    index: usize,
    focused: bool,
    port: &str,
    cable: &'static CableColor,
    mac: &str,
    description: String,
    state_color: Color,
) -> Row<'a> {
    let marker = if focused { "▶" } else { " " };
    Row::new(vec![
        Cell::from(format!("{}{}", marker, index + 1)),
        Cell::from(format!("● {}", port)).style(Style::default().fg(cable.color)),
        Cell::from(mac.to_string()),
        Cell::from(description).style(Style::default().fg(state_color)),
    ])
}

/// The framed, centered box telling the operator the single next step.
fn render_next_action(frame: &mut Frame, area: Rect, dashboard: &Dashboard, action: &NextAction) {
    let (text, color) = match action {
        NextAction::PlugFirst => ("Plug in first board".to_string(), Color::White),
        NextAction::Wait => ("wait".to_string(), Color::DarkGray),
        NextAction::Unplug(path) => {
            let cable = dashboard.cable_color(path);
            (format!("{} — unplug board", cable.name), cable.bright)
        }
        NextAction::Plug(path) => {
            let cable = dashboard.cable_color(path);
            (format!("{} — plug in a board", cable.name), cable.bright)
        }
        NextAction::AssignColor(path) => {
            let cable = default_cable();
            (
                format!("port {} — assign a cable color (c)", path),
                cable.bright,
            )
        }
        NextAction::Investigate(path) => {
            let cable = dashboard.cable_color(path);
            (format!("{} — investigate error", cable.name), cable.bright)
        }
        NextAction::CheckBlinking(path) => {
            let cable = dashboard.cable_color(path);
            (
                format!("{} — verify blinking then confirm", cable.name),
                cable.bright,
            )
        }
        NextAction::TestAmbient(path) => {
            let cable = dashboard.cable_color(path);
            (format!("{} — test ambient light", cable.name), cable.bright)
        }
    };
    let width = (text.chars().count() as u16 + 6).clamp(24, area.width);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y,
        width,
        height: area.height.min(3),
    };
    frame.render_widget(
        Paragraph::new(text)
            .centered()
            .bold()
            .style(Style::default().fg(color))
            .block(Block::bordered().border_style(Style::default().fg(color))),
        boxed,
    );
}

/// One line that always says what the spacebar does right now.
fn render_space_hint(frame: &mut Frame, area: Rect, dashboard: &Dashboard, state: &UiState) {
    let target = state.focus.as_ref().and_then(|focus| match &focus.key {
        FocusKey::Port(path) => dashboard
            .rows
            .get(path)
            .filter(|row| matches!(row.state, BoardState::LedCheck))
            .map(|_| path.clone()),
        FocusKey::Pending(_) => None,
    });
    let hint = match target {
        Some(path) => {
            let cable = dashboard.cable_color(&path);
            Paragraph::new(format!(
                "space → confirm blinking on {} port {}",
                cable.name, path
            ))
            .style(Style::default().fg(cable.color))
            .bold()
        }
        None => Paragraph::new("space → nothing").dim(),
    };
    frame.render_widget(hint, area);
}

/// Compact per-port status: every port a Board was seen on this session, in
/// its cable color, with the action the operator owes the connected Board.
fn render_ports(frame: &mut Frame, area: Rect, dashboard: &Dashboard) {
    let block = Block::new().borders(Borders::TOP).title(" ports ");
    if dashboard.known_ports.is_empty() {
        frame.render_widget(
            Paragraph::new("no ports seen yet").dim().block(block),
            area,
        );
        return;
    }
    let lines: Vec<Line> = dashboard
        .known_ports
        .iter()
        .map(|port| {
            let cable = dashboard.cable_color(port);
            let action = match dashboard.rows.get(port).map(|row| &row.state) {
                None => "plug in a board",
                Some(BoardState::Connecting)
                | Some(BoardState::Flashing { .. })
                | Some(BoardState::Verifying) => "wait",
                Some(BoardState::Testing(test)) => {
                    if test.ambient.is_none() {
                        "test ambient light"
                    } else {
                        "wait"
                    }
                }
                Some(BoardState::LedCheck) => "test LED strip",
                Some(BoardState::Done) | Some(BoardState::Resolved { .. }) => "unplug",
                Some(BoardState::Failed(failure))
                    if failure.mode == FailureMode::ConnectFailed =>
                {
                    "replug board"
                }
                Some(BoardState::Failed(_)) => "investigate problem",
            };
            let action_style = if action == "wait" {
                Style::default().fg(cable.color).dim()
            } else {
                Style::default().fg(cable.bright).bold()
            };
            Line::from(vec![
                Span::styled(
                    format!("● {:<10} {:<9} ", port, cable.name),
                    Style::default().fg(cable.color),
                ),
                Span::styled(action.to_string(), action_style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_events(frame: &mut Frame, area: Rect, dashboard: &Dashboard) {
    let block = Block::new().borders(Borders::TOP).title(" hardware events ");
    let visible = area.height.saturating_sub(1) as usize;
    let lines: Vec<Line> = dashboard
        .events
        .iter()
        .rev()
        .take(visible)
        .rev()
        .map(|event| {
            let seconds = event.at.as_secs();
            let port_color = event
                .usb_path
                .as_deref()
                .map(|path| dashboard.cable_color(path).color)
                .unwrap_or(Color::Gray);
            Line::from(vec![
                Span::styled(
                    format!("{:>3}:{:02} ", seconds / 60, seconds % 60),
                    Style::default().dim(),
                ),
                Span::styled(
                    format!("{:<8} ", event.usb_path.as_deref().unwrap_or("")),
                    Style::default().fg(port_color),
                ),
                Span::raw(event.message.clone()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_status(frame: &mut Frame, area: Rect, dashboard: &Dashboard, state: &UiState) {
    if state.quit_armed {
        frame.render_widget(
            Paragraph::new("flash in progress or unreviewed failures — press q again to quit anyway")
                .style(Style::default().fg(Color::Red)),
            area,
        );
        return;
    }
    let tested = dashboard.done + dashboard.failed;
    let elapsed = dashboard.session_started.elapsed();
    let minutes = elapsed.as_secs_f32() / 60.0;
    let rate = if minutes > 0.1 {
        tested as f32 / minutes
    } else {
        0.0
    };
    let average = if tested > 0 {
        dashboard.total_seconds / tested as u64
    } else {
        0
    };
    let in_progress = dashboard
        .rows
        .values()
        .filter(|row| {
            !matches!(
                row.state,
                BoardState::Done | BoardState::Failed(_) | BoardState::Resolved { .. }
            )
        })
        .count();
    let unreviewed = dashboard.pending.len()
        + dashboard
            .rows
            .values()
            .filter(|row| matches!(row.state, BoardState::Failed(_)))
            .count();
    frame.render_widget(
        Paragraph::new(format!(
            "tested: {} (✅ {}  ❌ {})   {:.1} boards/min   avg {}s/board   {} in progress   {} awaiting review   session {}:{:02}",
            tested,
            dashboard.done,
            dashboard.failed,
            rate,
            average,
            in_progress,
            unreviewed,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60,
        ))
        .bold()
        .reversed(),
        area,
    );
}

fn render_text_modal(frame: &mut Frame, purpose: TextPurpose, text: &str) {
    let (title, hint) = match purpose {
        TextPurpose::Dispute => (
            " dispute failure — why is this verdict wrong? ",
            "Enter = dispute & dump history   Esc = cancel",
        ),
        TextPurpose::ManualFail => (
            " fail this Board by hand — what is wrong with it? ",
            "Enter = fail the Board   Esc = cancel",
        ),
    };
    let area = centered(frame.area(), 70, 7);
    frame.render_widget(Clear, area);
    let content = Paragraph::new(format!("{}▏", text))
        .wrap(Wrap { trim: false })
        .block(Block::bordered().title(title));
    frame.render_widget(content, area);
    let hint_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        x: area.x + 2,
        width: area.width.saturating_sub(4),
    };
    frame.render_widget(Paragraph::new(hint).dim(), hint_area);
}

fn render_color_modal(frame: &mut Frame, dashboard: &Dashboard, usb_path: &str, cursor: usize) {
    let height = PALETTE.len() as u16 + 3;
    let area = centered(frame.area(), 40, height);
    frame.render_widget(Clear, area);
    let current = dashboard.port_colors.get(usb_path).copied();
    let lines: Vec<Line> = PALETTE
        .iter()
        .enumerate()
        .map(|(index, cable)| {
            let marker = if index == cursor { "▶" } else { " " };
            let assigned = if current == Some(index) {
                " (current)"
            } else {
                ""
            };
            Line::from(vec![
                Span::raw(format!("{} ", marker)),
                Span::styled("██ ", Style::default().fg(cable.color)),
                Span::styled(
                    format!("{}{}", cable.name, assigned),
                    if index == cursor {
                        Style::default().bold()
                    } else {
                        Style::default()
                    },
                ),
            ])
        })
        .chain(std::iter::once(Line::from(Span::styled(
            "↑/↓ choose   Enter = set   Esc = cancel",
            Style::default().dim(),
        ))))
        .collect();
    let block = Block::bordered().title(format!(" cable color for port {} ", usb_path));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// One-line description and color of a Board's state.
fn describe(row: &BoardRow) -> (Color, String) {
    match &row.state {
        BoardState::Connecting => (Color::Yellow, "connecting…".into()),
        BoardState::Flashing { firmware, percent } => {
            let filled = (*percent as usize) / 5;
            (
                Color::Cyan,
                format!(
                    "flashing {:<12} [{}{}] {:>3}%",
                    firmware,
                    "#".repeat(filled),
                    "-".repeat(20 - filled),
                    percent
                ),
            )
        }
        BoardState::Testing(test) => {
            let show = |value: Option<bool>| match value {
                Some(true) => "✓",
                Some(false) => "✗",
                None => "…",
            };
            let action = match test.prompt {
                AmbientPrompt::Cover => "👉 COVER the light sensor",
                AmbientPrompt::Shine => "👉 SHINE light on the sensor",
                AmbientPrompt::CoverAgain => "👉 COVER the sensor again",
                AmbientPrompt::Passed => "",
            };
            let light = test
                .light
                .as_deref()
                .map(|value| format!("  (light: {})", value))
                .unwrap_or_default();
            (
                Color::Yellow,
                format!(
                    "testing: power {} ble {} sensor {}  {}{}",
                    show(test.voltage),
                    show(test.ble),
                    show(test.ambient),
                    action,
                    light
                ),
            )
        }
        BoardState::Verifying => (Color::Cyan, "verifying production firmware boot…".into()),
        BoardState::LedCheck => (
            Color::Magenta,
            "🎉 hold the LED strip to the pads — space/y = blinks, n = dead".into(),
        ),
        BoardState::Done => (Color::Green, "✅ DONE — unplug this Board".into()),
        BoardState::Failed(failure) => (
            Color::Red,
            format!(
                "❌ {} — {}   [v]erify / [d]ispute / [i]gnore / [r]estart",
                failure.mode.label(),
                failure.detail
            ),
        ),
        BoardState::Resolved { verdict, log_path } => {
            let log = log_path
                .as_ref()
                .map(|path| format!(", log: {}", path.display()))
                .unwrap_or_default();
            let color = match verdict {
                ReviewKind::Disputed => Color::Blue,
                _ => Color::DarkGray,
            };
            (
                color,
                format!("reviewed ({}{}) — unplug this Board", verdict.label(), log),
            )
        }
    }
}
