mod generator;
mod parser;

use ariadne::{Config, Label, Report, ReportKind, Source};
use clap::{Parser, ValueEnum};
use log::{debug, error, info};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::str::FromStr;

/// A byte-offset span within a named source file.
///
/// Implements [`ariadne::Span`] so it can be passed directly to ariadne
/// for diagnostic rendering.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub source: String,
    pub start: usize,
    pub end: usize,
}

impl ariadne::Span for Span {
    type SourceId = String;

    fn source(&self) -> &String {
        &self.source
    }

    fn start(&self) -> usize {
        self.start
    }

    fn end(&self) -> usize {
        // ariadne requires end >= start; guard against a zero-length sentinel
        self.end.max(self.start)
    }
}

/// Source for the C header input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    Path(PathBuf),
}

impl InputSource {
    fn display_name(&self) -> String {
        match self {
            InputSource::Stdin => "<stdin>".to_string(),
            InputSource::Path(path) => path.display().to_string(),
        }
    }
}

impl FromStr for InputSource {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "-" {
            Ok(InputSource::Stdin)
        } else {
            Ok(InputSource::Path(PathBuf::from(value)))
        }
    }
}

/// Destination for generated bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputTarget {
    Stdout,
    Path(PathBuf),
}

impl Default for OutputTarget {
    fn default() -> Self {
        OutputTarget::Stdout
    }
}

impl FromStr for OutputTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "-" {
            Ok(OutputTarget::Stdout)
        } else {
            Ok(OutputTarget::Path(PathBuf::from(value)))
        }
    }
}

/// C declaration parser and bindgen tool
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// C header file to parse
    #[arg(short, long)]
    pub input: InputSource,

    /// Output file (defaults to stdout if not provided)
    #[arg(short, long, default_value = "-")]
    pub output: OutputTarget,

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
    /// The source span of the offending declaration, if available.
    ///
    /// Pass this to ariadne for pretty diagnostic rendering.  Programmatic
    /// callers that only need a human-readable message can use `Display`.
    pub span: Option<Span>,
    pub message: String,
}

impl BindgenError {
    /// Render this error into a human-readable string using ariadne.
    ///
    /// `source_text` is the full content of the parsed input, required for the source excerpt.
    pub fn render(&self, source_text: &str) -> String {
        let mut buf: Vec<u8> = Vec::new();
        match &self.span {
            Some(span) => {
                let _ = Report::build(ReportKind::Error, span.source.clone(), span.start)
                    .with_config(Config::default().with_color(false))
                    .with_message(&self.message)
                    .with_label(Label::new(span.clone()).with_message(&self.message))
                    .finish()
                    .write((span.source.clone(), Source::from(source_text)), &mut buf);
            }
            None => {
                buf.extend_from_slice(format!("Error: {}\n", self.message).as_bytes());
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }
}

impl std::fmt::Display for BindgenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.span {
            Some(span) => write!(f, "{}:{}: {}", span.source, span.start, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// An error returned by [`run`].
#[derive(Debug)]
pub enum RunError {
    ReadInput {
        source: InputSource,
        error: io::Error,
    },
    Generate {
        errors: Vec<BindgenError>,
        input: String,
    },
    WriteOutput {
        target: OutputTarget,
        error: io::Error,
    },
}

/// Parse a C header and generate bindings in the requested format.
///
/// `source` is the display name of the input (e.g. a filename or `"<stdin>"`).  It
/// appears in the [`Span`] of every [`BindgenError`] so that ariadne can render
/// source-file headers in diagnostics.
pub fn generate_bindings(
    input: &str,
    source: &str,
    format: OutputFormat,
) -> Result<String, Vec<BindgenError>> {
    let parsed = parser::parse_declarations_checked(input, source).map_err(|errs| {
        errs.into_iter()
            .map(|(span, message)| BindgenError {
                span: Some(span),
                message,
            })
            .collect::<Vec<_>>()
    })?;
    let gen_declarations = generator::Declarations::lower(parsed).map_err(|errs| {
        errs.into_iter()
            .map(|e| BindgenError {
                span: e.span,
                message: e.message,
            })
            .collect::<Vec<_>>()
    })?;
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

    match run(args, stdin, stdout) {
        Ok(()) => 0,
        Err(err) => {
            let _ = write!(stderr, "{}", render_run_error(&err));
            1
        }
    }
}

pub fn run(args: Args, stdin: &mut dyn Read, stdout: &mut dyn Write) -> Result<(), RunError> {
    let input = if args.input == InputSource::Stdin {
        debug!("Reading from stdin");
        let mut buf = String::new();
        if let Err(err) = stdin.read_to_string(&mut buf) {
            error!("Error reading from stdin: {}", err);
            return Err(RunError::ReadInput {
                source: args.input,
                error: err,
            });
        }
        buf
    } else {
        match &args.input {
            InputSource::Path(path) => {
                debug!("Reading input file: {:?}", path);
                match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(err) => {
                        error!("Error reading file {:?}: {}", path, err);
                        return Err(RunError::ReadInput {
                            source: args.input,
                            error: err,
                        });
                    }
                }
            }
            InputSource::Stdin => unreachable!(),
        }
    };

    let source_name = args.input.display_name();

    debug!("Generating output in {:?} format", args.format);
    let output_content = generate_bindings(&input, &source_name, args.format).map_err(|errs| {
        error!("Errors in {:?}:", args.input);
        for e in &errs {
            error!("  {}", e);
        }
        RunError::Generate {
            errors: errs,
            input: input.clone(),
        }
    })?;

    match &args.output {
        OutputTarget::Stdout => {
            stdout
                .write_all(output_content.as_bytes())
                .map_err(|err| RunError::WriteOutput {
                    target: args.output,
                    error: err,
                })?;
        }
        OutputTarget::Path(output_path) => {
            debug!("Writing output to {:?}", output_path);
            fs::write(output_path, &output_content).map_err(|err| {
                error!("Error writing to {:?}: {}", output_path, err);
                RunError::WriteOutput {
                    target: args.output.clone(),
                    error: err,
                }
            })?;
            info!("Successfully wrote output to {:?}", output_path);
        }
    }

    Ok(())
}

fn render_run_error(error: &RunError) -> String {
    match error {
        RunError::ReadInput { source, error } => {
            format!("Error reading input {:?}: {}", source, error)
        }
        RunError::Generate { errors, input } => {
            errors.iter().map(|err| err.render(input)).collect()
        }
        RunError::WriteOutput { target, error } => {
            format!("Error writing output {:?}: {}", target, error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Cycle 1: parse errors are owned BindgenErrors ---

    #[test]
    fn parse_errors_are_owned_and_have_location() {
        // "struct Broken { int x;" is missing the closing brace
        let result = generate_bindings("struct Broken { int x;", "<test>", OutputFormat::CGuest);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
        let err = &errors[0];
        let span = err.span.as_ref().expect("parse errors must have a span");
        assert!(span.start <= span.end, "span start should be <= end");
        assert!(!err.message.is_empty(), "message should not be empty");
    }

    #[test]
    fn parse_error_display_shows_location() {
        let result = generate_bindings("struct Bad {", "<test>", OutputFormat::CGuest);
        let errors = result.unwrap_err();
        let display = format!("{}", errors[0]);
        // Should be "source:offset: message"
        assert!(
            display.contains(':'),
            "display should contain ':', got: {display}"
        );
    }

    // --- Cycle 2: RustGuest is a real adapter ---

    #[test]
    fn rust_guest_generates_repr_c_struct() {
        let result = generate_bindings(
            "struct Point { int x; int y; };",
            "<test>",
            OutputFormat::RustGuest,
        );
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("pub struct Point"), "output:\n{output}");
        assert!(output.contains("pub x: i32"), "output:\n{output}");
        assert!(output.contains("pub y: i32"), "output:\n{output}");
    }

    #[test]
    fn rust_guest_generates_extern_block_for_imported_function() {
        let input = r#"[[clang::import_module("env"), clang::import_name("log")]] void host_log(char *message);"#;
        let result = generate_bindings(input, "<test>", OutputFormat::RustGuest);
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("extern \"C\""), "output:\n{output}");
        assert!(output.contains("pub fn host_log"), "output:\n{output}");
    }

    #[test]
    fn rejects_function_declared_as_both_import_and_export() {
        let input =
            r#"[[clang::import_name("host_run"), clang::export_name("guest_run")]] int run();"#;
        let result = generate_bindings(input, "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("cannot be both a host import and a guest export")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn rejects_named_types_until_typedef_semantics_are_defined() {
        let result = generate_bindings("rudel_word counter;", "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|err| err.message.contains("unsupported named type `rudel_word`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn rejects_enum_values_outside_i32_range() {
        let result = generate_bindings(
            "enum Status { TOO_LARGE = 2147483648, };",
            "<test>",
            OutputFormat::CGuest,
        );
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("enum value `TOO_LARGE` is outside the supported i32 range")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn rejects_conflicting_ordinary_declarations() {
        let input = r#"
            int status;
            int status();
        "#;
        let result = generate_bindings(input, "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|err| err.message.contains("declaration `status` conflicts")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn rejects_void_parameters() {
        let result = generate_bindings("int bad(void value);", "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("parameter `value` cannot have type void")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn treats_void_parameter_list_as_no_parameters() {
        let result = generate_bindings("int main(void);", "<test>", OutputFormat::CGuest);
        assert!(result.is_ok(), "expected ok, got: {result:?}");
        assert_eq!(
            result.unwrap(),
            "int main() __attribute__((import_name(\"main\")));\n"
        );
    }

    #[test]
    fn rejects_duplicate_linkage_attributes() {
        let input = r#"[[clang::import_name("one"), clang::import_name("two")]] int run();"#;
        let result = generate_bindings(input, "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("function `run` repeats attribute `clang::import_name`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn rejects_variable_linkage_attributes() {
        let input = r#"[[clang::import_name("counter")]] int counter;"#;
        let result = generate_bindings(input, "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("variable `counter` cannot use Host/Guest Linkage attributes")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn semantic_errors_have_source_spans() {
        let result = generate_bindings("void bad;", "<test>", OutputFormat::CGuest);
        assert!(result.is_err(), "expected semantic error, got: {result:?}");
        let errors = result.unwrap_err();
        let span = errors[0]
            .span
            .as_ref()
            .expect("semantic errors should have a span");
        assert_eq!(span.source, "<test>");
        assert!(span.start <= span.end, "span start should be <= end");
    }
}
