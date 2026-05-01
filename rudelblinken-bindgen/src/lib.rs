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
    /// Generate C header file for WebAssembly guests
    CGuest,
    /// Generate Rust guest bindings
    RustGuest,
}

/// An error returned by [`generate_bindings`].
#[derive(Debug, Clone, PartialEq)]
pub struct BindgenError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for BindgenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

fn offset_to_line_col(input: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(input.len());
    let prefix = &input[..clamped];
    let line = prefix.chars().filter(|&c| c == '\n').count() + 1;
    let col = prefix.rfind('\n').map(|nl| clamped - nl - 1).unwrap_or(clamped) + 1;
    (line, col)
}

/// Parse a C header and generate bindings in the requested format.
pub fn generate_bindings(input: &str, format: &OutputFormat) -> Result<String, Vec<BindgenError>> {
    let parsed = parser::parse_declarations(input).map_err(|errs| {
        errs.into_iter()
            .map(|e| {
                let offset = e.span().start;
                let (line, col) = offset_to_line_col(input, offset);
                BindgenError {
                    line,
                    column: col,
                    message: format!("{}", e.reason()),
                }
            })
            .collect::<Vec<_>>()
    })?;
    let gen_declarations: generator::Declarations = parsed.into();
    let output = match format {
        OutputFormat::CGuest => generator::c_guest::generate(&gen_declarations),
        OutputFormat::RustGuest => generator::rust_guest::generate(&gen_declarations),
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

    debug!("Generating output in {:?} format", args.format);
    let output_content = generate_bindings(&input, &args.format).map_err(|errs| {
        error!("Parse errors in {:?}:", args.input);
        for e in &errs {
            error!("  {}", e);
        }
        format!("Parse errors in {:?}", args.input)
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Cycle 1: parse errors are owned BindgenErrors ---

    #[test]
    fn parse_errors_are_owned_and_have_location() {
        // "struct Broken { int x;" is missing the closing brace
        let result = generate_bindings("struct Broken { int x;", &OutputFormat::CGuest);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
        let err = &errors[0];
        assert!(err.line >= 1, "line should be >= 1, got {}", err.line);
        assert!(err.column >= 1, "column should be >= 1, got {}", err.column);
        assert!(!err.message.is_empty(), "message should not be empty");
    }

    #[test]
    fn parse_error_display_shows_location() {
        let result = generate_bindings("struct Bad {", &OutputFormat::CGuest);
        let errors = result.unwrap_err();
        let display = format!("{}", errors[0]);
        // Should be "line:col: message"
        assert!(display.contains(':'), "display should contain ':', got: {display}");
    }

    // --- Cycle 2: RustGuest is a real adapter ---

    #[test]
    fn rust_guest_generates_repr_c_struct() {
        let result = generate_bindings("struct Point { int x; int y; };", &OutputFormat::RustGuest);
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("pub struct Point"), "output:\n{output}");
        assert!(output.contains("pub x: i32"), "output:\n{output}");
        assert!(output.contains("pub y: i32"), "output:\n{output}");
    }

    #[test]
    fn rust_guest_generates_extern_block_for_imported_function() {
        let input = r#"void host_log(char *message) __attribute__((import_module("env"), import_name("log")));"#;
        let result = generate_bindings(input, &OutputFormat::RustGuest);
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("extern \"C\""), "output:\n{output}");
        assert!(output.contains("pub fn host_log"), "output:\n{output}");
    }
}
