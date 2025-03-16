//! Test wasm files on an emulated rudelblinken device.
use clap::{Args, Parser};
use espflash::cli::{
    config::Config, connect, erase_partitions, flash_elf_image, make_flash_data, monitor::monitor,
    print_board_info, EspflashProgress,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FlashError {}

#[derive(Args, Debug)]
pub struct FlashCommand {
    /// Monitor the device after flashing
    #[clap(short, long, default_value = "false")]
    monitor: bool,
}

/// Wraps espflash to flash the rudelblinken firmware.
pub struct Flasher {
    monitor: bool,
}

impl Flasher {
    pub async fn new(command: FlashCommand) -> Result<Self, FlashError> {
        Ok(Flasher {
            monitor: command.monitor,
        })
    }

    // This function is in large parts copied from espflash::bin::flash
    pub async fn flash(&self) {
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
        let Commands::Flash(args) = mock_cli.subcommand;

        let config = Config::load().unwrap();
        let mut flasher = connect(
            &args.connect_args,
            &config,
            args.flash_args.no_verify,
            args.flash_args.no_skip,
        )
        .unwrap();
        flasher
            .verify_minimum_revision(args.flash_args.image.min_chip_rev)
            .unwrap();

        if let Some(flash_size) = args.flash_config_args.flash_size {
            flasher.set_flash_size(flash_size);
        } else if let Some(flash_size) = config.flash.size {
            flasher.set_flash_size(flash_size);
        }

        let chip = flasher.chip();
        let target = chip.into_target();
        let target_xtal_freq = target.crystal_freq(flasher.connection()).unwrap();

        // Read the ELF data from the build path and load it to the target.
        let elf_data_bytes: &[u8] = include_bytes!(
            "../../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/rudelblinken-firmware"
        );
        let elf_data = Vec::from(elf_data_bytes);

        print_board_info(&mut flasher).unwrap();

        let mut flash_config = args.flash_config_args;
        flash_config.flash_size = flash_config
            .flash_size // Use CLI argument if provided
            .or(config.flash.size) // If no CLI argument, try the config file
            // .or_else(|| flasher.flash_detect().ok().flatten()) // Try detecting flash size next
            .or_else(|| Some(espflash::flasher::FlashSize::_4Mb)); // Otherwise, use a reasonable default value

        if args.flash_args.ram {
            flasher
                .load_elf_to_ram(&elf_data, Some(&mut EspflashProgress::default()))
                .unwrap();
        } else {
            let mut flash_data =
                make_flash_data(args.flash_args.image, &flash_config, &config, None, None).unwrap();
            let partition_table_bytes =
                include_bytes!("../../rudelblinken-firmware/partition_table.csv");
            let bootloader_bytes = include_bytes!(
                "../../rudelblinken-firmware/target/riscv32imc-esp-espidf/release/bootloader.bin"
            );
            flash_data.partition_table =
                esp_idf_part::PartitionTable::try_from(Vec::from(partition_table_bytes)).ok();
            flash_data.bootloader = Some(Vec::from(bootloader_bytes));

            if args.flash_args.erase_parts.is_some() || args.flash_args.erase_data_parts.is_some() {
                erase_partitions(
                    &mut flasher,
                    flash_data.partition_table.clone(),
                    args.flash_args.erase_parts,
                    args.flash_args.erase_data_parts,
                )
                .unwrap();
            }

            flash_elf_image(&mut flasher, &elf_data, flash_data, target_xtal_freq).unwrap();
        }

        if self.monitor {
            let pid = flasher.get_usb_pid().unwrap();
            let port = flasher.into_serial();
            let elf = Some(elf_data_bytes);
            let baud = 115_200;
            let log_format = espflash::cli::monitor::LogFormat::Serial;
            let interactive_mode = true;
            let processors = None;
            let elf_file = None;

            monitor(
                port,
                elf,
                pid,
                baud,
                log_format,
                interactive_mode,
                processors,
                elf_file,
            )
            .unwrap();
        }
    }
}
