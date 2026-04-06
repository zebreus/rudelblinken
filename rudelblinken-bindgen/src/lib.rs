pub mod generator;
pub mod parser;

pub use parser::{
    Attribute, C23Attributes, Declarations, Directive, EnumDecl, EnumVariant, Field, FunctionDecl,
    Parameter, StructDecl, Type, TypedefDecl, VariableDecl, parse_declarations,
};

use clap::{Parser, ValueEnum};
use log::{debug, error, info};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// C declaration parser and bindgen tool
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// C header file to parse
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (defaults to stdout if not provided)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::CGuest)]
    pub format: OutputFormat,
}

/// Supported output formats
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// Generate C header file
    CGuest,
}

/// High-level function to parse C headers and generate bindings
pub fn generate_bindings<'a>(
    input: &'a str,
    format: &OutputFormat,
) -> Result<String, Vec<chumsky::error::Rich<'a, char>>> {
    let parsed = parser::parse_declarations(input)?;
    let gen_declarations: generator::Declarations = parsed.into();
    let output = match format {
        OutputFormat::CGuest => generator::c_guest::generate(&gen_declarations),
    };
    Ok(output)
}

/// Entrypoint that handles CLI arguments and environment variables
/// Does not read from the global environment.
pub fn run_cli<I, T>(
    args: I,
    _env_vars: &HashMap<String, String>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(e) => {
            let _ = write!(stderr, "{}", e);
            return 1;
        }
    };

    match run(args, stdin) {
        Ok(output_content) => {
            let _ = write!(stdout, "{}", output_content);
            0
        }
        Err(err) => {
            let _ = write!(stderr, "{}", err);
            1
        }
    }
}

pub fn run(args: Args, stdin: &mut dyn Read) -> Result<String, String> {
    let input = if args.input.as_os_str() == "-" {
        debug!("Reading from stdin");
        let mut buf = String::new();
        if let Err(err) = stdin.read_to_string(&mut buf) {
            error!("Error reading from stdin: {}", err);
            return Err(format!("Error reading from stdin: {}", err));
        }
        buf
    } else {
        debug!("Reading input file: {:?}", args.input);
        match fs::read_to_string(&args.input) {
            Ok(s) => s,
            Err(err) => {
                error!("Error reading file {:?}: {}", args.input, err);
                return Err(format!("Error reading file {:?}: {}", args.input, err));
            }
        }
    };

    debug!("Parsing declarations");
    let parser_declarations = match parser::parse_declarations(&input) {
        Ok(decls) => decls,
        Err(errors) => {
            error!("Parse errors in {:?}:", args.input);
            for error in &errors {
                error!("  {:?}", error);
            }
            return Err(format!("Parse errors in {:?}", args.input));
        }
    };

    info!(
        "Parsed {} structs, {} functions, {} variables",
        parser_declarations.structs.len(),
        parser_declarations.functions.len(),
        parser_declarations.variables.len()
    );

    debug!("Converting to generator representation");
    let gen_declarations: generator::Declarations = parser_declarations.into();

    debug!("Generating output in {:?} format", args.format);
    let output_content = match args.format {
        OutputFormat::CGuest => generator::c_guest::generate(&gen_declarations),
    };

    if let Some(output_path) = &args.output {
        if output_path.as_os_str() != "-" {
            debug!("Writing output to {:?}", output_path);
            if let Err(err) = fs::write(output_path, &output_content) {
                error!("Error writing to {:?}: {}", output_path, err);
                return Err(format!("Error writing to {:?}: {}", output_path, err));
            }
            info!("Successfully wrote output to {:?}", output_path);
        }
    }

    Ok(output_content)
}
