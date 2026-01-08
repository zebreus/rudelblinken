use clap::{Parser, ValueEnum};
use log::{debug, info};
use std::fs;
use std::path::PathBuf;

/// C declaration parser and bindgen tool
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// C header file to parse
    #[arg(short, long)]
    input: PathBuf,

    /// Output file (defaults to stdout if not provided)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::CGuest)]
    format: OutputFormat,
}

/// Supported output formats
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// Generate C header file
    CGuest,
}

fn main() {
    env_logger::init();

    let args = Args::parse();

    debug!("Reading input file: {:?}", args.input);
    let input = fs::read_to_string(&args.input).unwrap_or_else(|err| {
        eprintln!("Error reading file {:?}: {}", args.input, err);
        std::process::exit(1);
    });

    debug!("Parsing declarations");
    match rudelblinken_bindgen::parse_declarations(&input) {
        Ok(parser_declarations) => {
            info!(
                "Parsed {} structs, {} functions, {} variables",
                parser_declarations.structs.len(),
                parser_declarations.functions.len(),
                parser_declarations.variables.len()
            );

            // Convert to generator representation
            debug!("Converting to generator representation");
            let gen_declarations: rudelblinken_bindgen::generator::Declarations =
                parser_declarations.into();

            // Generate output based on format
            debug!("Generating output in {:?} format", args.format);
            let output_content = match args.format {
                OutputFormat::CGuest => {
                    rudelblinken_bindgen::generator::c_guest::generate(&gen_declarations)
                }
            };

            // Write to output or stdout
            if let Some(output_path) = &args.output {
                debug!("Writing output to {:?}", output_path);
                fs::write(output_path, output_content).unwrap_or_else(|err| {
                    eprintln!("Error writing to {:?}: {}", output_path, err);
                    std::process::exit(1);
                });
                info!("Successfully wrote output to {:?}", output_path);
            } else {
                debug!("Writing output to stdout");
                print!("{}", output_content);
            }
        }
        Err(errors) => {
            eprintln!("Parse errors in {:?}:", args.input);
            for error in errors {
                eprintln!("  {:?}", error);
            }
            std::process::exit(1);
        }
    }
}
