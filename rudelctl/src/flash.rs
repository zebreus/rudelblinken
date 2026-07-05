//! Flash a built-in copy of the rudelblinken firmware to a Board via USB.

use clap::{Args, Parser};
use espflash::{
    cli::{
        config::Config, connect, make_flash_data, monitor::monitor, print_board_info,
        EspflashProgress,
    },
    elf::{ElfFirmwareImage, RomSegment},
    flasher::ProgressCallbacks,
};
use thiserror::Error;

pub use espflash::connection::Port;

/// The firmware ELF, also needed for symbol resolution when monitoring.
pub const FIRMWARE_ELF: &[u8] = include_bytes!("../firmware/rudelblinken-firmware");
const PARTITION_TABLE: &[u8] = include_bytes!("../firmware/partition_table.csv");
const BOOTLOADER: &[u8] = include_bytes!("../firmware/bootloader.bin");
const DEFAULT_PROGRAM: &[u8] = include_bytes!("../firmware/default_program.wasm");
const TEST_PROGRAM: &[u8] = include_bytes!("../firmware/test_program.wasm");

#[derive(Error, Debug)]
#[error("{stage}: {message}")]
pub struct FlashError {
    /// Which step of the flashing process failed
    pub stage: &'static str,
    pub message: String,
}

impl FlashError {
    fn new(stage: &'static str, error: impl ToString) -> Self {
        FlashError {
            stage,
            message: error.to_string(),
        }
    }
}

#[derive(Args, Debug)]
pub struct FlashCommand {
    /// Monitor the device after flashing
    #[clap(short, long, default_value = "false")]
    monitor: bool,
    /// Flash a firmware that runs the board test instead of rudelblinken.
    #[clap(long, default_value = "false")]
    test: bool,
    /// Flash the default program
    #[clap(short, long, default_value = "true")]
    default_program: bool,
}

/// A Board that was just flashed successfully.
pub struct FlashedBoard {
    pub mac: String,
    usb_pid: u16,
    /// The serial connection, still open, for resetting and monitoring the Board.
    pub serial: Port,
}

impl FlashedBoard {
    /// Monitor the serial output interactively, like `espflash monitor`. Requires a TTY.
    pub fn monitor(self) -> Result<(), FlashError> {
        monitor(
            self.serial,
            Some(FIRMWARE_ELF),
            self.usb_pid,
            115_200,
            espflash::cli::monitor::LogFormat::Serial,
            true,
            None,
            None,
        )
        .map_err(|error| FlashError::new("monitor", error))
    }
}

/// Flash the built-in firmware to the Board at `port`, or autodetect a Board if `port` is `None`.
///
/// `test_program` selects which Program goes into the `default_program`
/// partition: the board test or the production Default Program. The firmware
/// image itself is the same for both.
///
/// This function is in large parts copied from espflash::bin::flash.
pub fn flash_board(
    port: Option<&str>,
    test_program: bool,
    flash_default_program: bool,
    print_info: bool,
    progress: Option<&mut dyn ProgressCallbacks>,
) -> Result<FlashedBoard, FlashError> {
    #[derive(Debug, Args)]
    #[non_exhaustive]
    struct FlashArgs {
        /// Connection configuration
        #[clap(flatten)]
        connect_args: espflash::cli::ConnectArgs,
        /// Flashing configuration
        #[clap(flatten)]
        pub flash_config_args: espflash::cli::FlashConfigArgs,
        /// Flashing arguments
        #[clap(flatten)]
        flash_args: espflash::cli::FlashArgs,
    }
    #[derive(Debug, clap::Subcommand)]
    enum Commands {
        Flash(FlashArgs),
    }
    #[derive(Debug, clap::Parser)]
    #[command(about, max_term_width = 100, propagate_version = true, version)]
    pub struct MockCli {
        #[command(subcommand)]
        subcommand: Commands,

        /// Do not check for updates
        #[clap(short = 'S', long, global = true, action)]
        skip_update_check: bool,
    }
    let mock_args = vec!["espflash", "flash"];
    let mut mock_cli = MockCli::parse_from(mock_args);
    mock_cli.skip_update_check = true;
    let Commands::Flash(mut args) = mock_cli.subcommand;
    args.connect_args.port = port.map(str::to_owned);

    let config = Config::load().map_err(|error| FlashError::new("load config", error))?;
    let mut flasher = connect(
        &args.connect_args,
        &config,
        args.flash_args.no_verify,
        args.flash_args.no_skip,
    )
    .map_err(|error| FlashError::new("connect", error))?;
    flasher
        .verify_minimum_revision(args.flash_args.image.min_chip_rev)
        .map_err(|error| FlashError::new("verify chip revision", error))?;

    if let Some(flash_size) = args.flash_config_args.flash_size {
        flasher.set_flash_size(flash_size);
    } else if let Some(flash_size) = config.flash.size {
        flasher.set_flash_size(flash_size);
    }

    let chip = flasher.chip();
    let target = chip.into_target();
    let target_xtal_freq = target
        .crystal_freq(flasher.connection())
        .map_err(|error| FlashError::new("read crystal frequency", error))?;

    if print_info {
        print_board_info(&mut flasher).map_err(|error| FlashError::new("board info", error))?;
    }
    let device_info = flasher
        .device_info()
        .map_err(|error| FlashError::new("device info", error))?;

    let mut flash_config = args.flash_config_args;
    flash_config.flash_size = flash_config
        .flash_size // Use CLI argument if provided
        .or(config.flash.size) // If no CLI argument, try the config file
        .or_else(|| Some(espflash::flasher::FlashSize::_4Mb)); // Otherwise, use a reasonable default value

    let mut flash_data = make_flash_data(args.flash_args.image, &flash_config, &config, None, None)
        .map_err(|error| FlashError::new("make flash data", error))?;
    flash_data.partition_table =
        esp_idf_part::PartitionTable::try_from(Vec::from(PARTITION_TABLE)).ok();
    flash_data.bootloader = Some(Vec::from(BOOTLOADER));

    let prog_seg = if flash_default_program {
        let prog_part = flash_data
            .partition_table
            .as_ref()
            .and_then(|pt| pt.find("default_program").cloned())
            .ok_or_else(|| {
                FlashError::new("partition table", "Failed to find default_program partition")
            })?;

        let program_bytes: &[u8] = if test_program {
            TEST_PROGRAM
        } else {
            DEFAULT_PROGRAM
        };
        let prog_len = program_bytes.len();

        let part_len = prog_part.size() as usize;
        let mut buf = vec![0u8; part_len];
        buf[0..prog_len].copy_from_slice(program_bytes);
        buf[part_len - 4..].copy_from_slice(&(prog_len as u32).to_le_bytes());

        Some(RomSegment {
            addr: prog_part.offset(),
            data: buf.into(),
        })
    } else {
        None
    };

    // Copy to the heap: the ELF parser needs aligned data, include_bytes! does
    // not guarantee any alignment.
    let elf_data = Vec::from(FIRMWARE_ELF);
    let image = ElfFirmwareImage::try_from(elf_data.as_slice())
        .map_err(|error| FlashError::new("parse firmware image", error))?;

    let chip_revision = Some(
        flasher
            .chip()
            .into_target()
            .chip_revision(&mut flasher.connection())
            .map_err(|error| FlashError::new("read chip revision", error))?,
    );

    let image = flasher
        .chip()
        .into_target()
        .get_flash_image(&image, flash_data, chip_revision, target_xtal_freq)
        .map_err(|error| FlashError::new("build flash image", error))?;

    let segments = image.flash_segments().chain(prog_seg).collect::<Vec<_>>();

    flasher
        .write_bins_to_flash(&segments, progress)
        .map_err(|error| FlashError::new("write flash", error))?;

    let usb_pid = flasher
        .get_usb_pid()
        .map_err(|error| FlashError::new("usb pid", error))?;

    Ok(FlashedBoard {
        mac: device_info.mac_address,
        usb_pid,
        serial: flasher.into_serial(),
    })
}

/// Wraps espflash to flash the rudelblinken firmware.
pub struct Flasher {
    monitor: bool,
    /// Flash a special test firmware instead of the normal firmware.
    board_test_firmware: bool,
    flash_default_program: bool,
}

impl Flasher {
    pub async fn new(command: FlashCommand) -> Result<Self, FlashError> {
        Ok(Flasher {
            monitor: command.monitor,
            board_test_firmware: command.test,
            flash_default_program: command.default_program,
        })
    }

    pub async fn flash(&self) {
        let flashed = flash_board(
            None,
            self.board_test_firmware,
            self.flash_default_program,
            true,
            Some(&mut EspflashProgress::default()),
        )
        .unwrap();

        if self.monitor {
            flashed.monitor().unwrap();
        }
    }
}
