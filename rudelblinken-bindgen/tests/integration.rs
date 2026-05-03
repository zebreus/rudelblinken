use rudelblinken_bindgen::{Args, InputSource, OutputFormat, OutputTarget, RunError, run, run_cli};
use std::cmp::max;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn fixture_cases() {
    let cases = collect_cases(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/cases")
            .as_path(),
    );
    assert!(!cases.is_empty(), "no integration test cases found");

    for case in cases {
        run_case(&case);
    }
}

#[test]
fn run_generates_from_stdin_to_stdout() {
    let args = Args {
        input: InputSource::Stdin,
        output: OutputTarget::Stdout,
        format: OutputFormat::CGuest,
    };

    let mut stdin = "int main(void);".as_bytes();
    let mut stdout = Vec::new();

    run(args, &mut stdin, &mut stdout).expect("run should succeed");

    assert_eq!(
        String::from_utf8(stdout).expect("stdout should be utf-8"),
        "[[clang::import_name(\"main\")]] int main();\n"
    );
}

#[test]
fn run_returns_structured_generation_error_with_input_text() {
    let args = Args {
        input: InputSource::Stdin,
        output: OutputTarget::Stdout,
        format: OutputFormat::CGuest,
    };

    let input = "struct Broken { int x;";
    let mut stdin = input.as_bytes();
    let mut stdout = Vec::new();

    let err = run(args, &mut stdin, &mut stdout).expect_err("run should fail");

    match err {
        RunError::Generate {
            errors,
            input: actual_input,
        } => {
            assert_eq!(actual_input, input);
            assert!(!errors.is_empty(), "expected at least one bindgen error");
            assert!(
                errors[0].render(&actual_input).contains("Error"),
                "rendered error should be human-readable"
            );
        }
        other => panic!("expected generation error, got {other:?}"),
    }
}

#[test]
fn run_writes_to_file_without_stdout_duplication() {
    let output_path = unique_temp_path("run-output", "h");
    let args = Args {
        input: InputSource::Stdin,
        output: OutputTarget::Path(output_path.clone()),
        format: OutputFormat::CGuest,
    };

    let mut stdin = "int main(void);".as_bytes();
    let mut stdout = Vec::new();

    run(args, &mut stdin, &mut stdout).expect("run should succeed");

    assert!(stdout.is_empty(), "file output should not duplicate stdout");
    assert_eq!(
        fs::read_to_string(&output_path).expect("read generated output"),
        "[[clang::import_name(\"main\")]] int main();\n"
    );

    let _ = fs::remove_file(output_path);
}

#[test]
fn run_cli_writes_to_file_without_stdout_duplication() {
    let output_path = unique_temp_path("run-cli-output", "h");
    let argv = vec![
        "rudelblinken-bindgen".to_string(),
        "--input".to_string(),
        "-".to_string(),
        "--output".to_string(),
        output_path.display().to_string(),
    ];
    let env_vars = HashMap::new();
    let mut stdin = "int main(void);".as_bytes();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = run_cli(argv, &env_vars, &mut stdin, &mut stdout, &mut stderr);

    assert_eq!(exit_code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
    assert!(stdout.is_empty(), "file output should not duplicate stdout");
    assert_eq!(
        fs::read_to_string(&output_path).expect("read generated output"),
        "[[clang::import_name(\"main\")]] int main();\n"
    );

    let _ = fs::remove_file(output_path);
}

#[test]
fn run_returns_structured_read_input_error() {
    let input_path = unique_temp_path("missing-input", "h");
    let args = Args {
        input: InputSource::Path(input_path.clone()),
        output: OutputTarget::Stdout,
        format: OutputFormat::CGuest,
    };
    let mut stdin = io::empty();
    let mut stdout = Vec::new();

    let err = run(args, &mut stdin, &mut stdout).expect_err("run should fail");

    match err {
        RunError::ReadInput { source, error } => {
            assert_eq!(source, InputSource::Path(input_path));
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
        }
        other => panic!("expected read input error, got {other:?}"),
    }
}

#[test]
fn run_returns_structured_write_output_error() {
    let output_dir = unique_temp_path("output-dir", "dir");
    fs::create_dir(&output_dir).expect("create output directory");
    let args = Args {
        input: InputSource::Stdin,
        output: OutputTarget::Path(output_dir.clone()),
        format: OutputFormat::CGuest,
    };
    let mut stdin = "int main(void);".as_bytes();
    let mut stdout = Vec::new();

    let err = run(args, &mut stdin, &mut stdout).expect_err("run should fail");

    match err {
        RunError::WriteOutput { target, error } => {
            assert_eq!(target, OutputTarget::Path(output_dir.clone()));
            assert!(
                matches!(
                    error.kind(),
                    io::ErrorKind::IsADirectory | io::ErrorKind::PermissionDenied
                ),
                "unexpected error kind: {:?}",
                error.kind()
            );
        }
        other => panic!("expected write output error, got {other:?}"),
    }

    let _ = fs::remove_dir(output_dir);
}

fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rudelblinken-bindgen-{prefix}-{}-{nanos}.{extension}",
        std::process::id()
    ))
}

#[derive(Debug)]
struct Case {
    name: String,
    input: PathBuf,
    expected: Expected,
}

#[derive(Debug)]
enum Expected {
    Success,
    Output(String),
    ErrorSubstring(String),
}

fn run_case(case: &Case) {
    let args = Args {
        input: InputSource::Path(case.input.clone()),
        output: OutputTarget::Stdout,
        format: OutputFormat::CGuest,
    };

    let mut stdin = io::empty();
    let mut stdout = Vec::new();
    let result = run(args, &mut stdin, &mut stdout);

    match (&case.expected, result) {
        (Expected::Success, Ok(())) => {}
        (Expected::Success, Err(err)) => {
            panic!(
                "expected success for {} but got error\n{:?}",
                case.name, err
            );
        }
        (Expected::Output(expected), Ok(())) => {
            let actual = String::from_utf8(stdout).expect("stdout should be utf-8");
            if actual != *expected {
                panic!(
                    "output mismatch for {}\n{}",
                    case.name,
                    unified_diff(expected, &actual)
                );
            }
        }
        (Expected::Output(expected), Err(err)) => {
            panic!(
                "unexpected error for {}:\nexpected output:\n{}\nactual error:\n{}",
                case.name,
                expected,
                format_run_error(&err)
            );
        }
        (Expected::ErrorSubstring(expected), Err(err)) => {
            let actual = format_run_error(&err);
            assert!(
                actual.contains(expected),
                "error mismatch for {}\nexpected substring: {:?}\nactual error: {:?}",
                case.name,
                expected,
                actual
            );
        }
        (Expected::ErrorSubstring(expected), Ok(())) => {
            panic!(
                "expected error for {} but got output\nexpected substring: {:?}\nactual output:\n{}",
                case.name,
                expected,
                String::from_utf8_lossy(&stdout)
            );
        }
    }
}

fn format_run_error(err: &rudelblinken_bindgen::RunError) -> String {
    match err {
        rudelblinken_bindgen::RunError::Generate { errors, input } => errors
            .iter()
            .map(|err| err.render(input))
            .collect::<String>(),
        other => format!("{other:?}"),
    }
}

fn collect_cases(root: &Path) -> Vec<Case> {
    let mut cases = Vec::new();
    collect_cases_inner(root, &mut cases);
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

fn collect_cases_inner(dir: &Path, cases: &mut Vec<Case>) {
    let input = dir.join("input.c");
    let expected = dir.join("output_c_guest.c");
    if input.is_file() && expected.is_file() {
        let expected_contents = fs::read_to_string(&expected).expect("read expected fixture");
        let expectation = if expected_contents.trim() == "SUCCESS" {
            Expected::Success
        } else if let Some(rest) = expected_contents.strip_prefix("ERROR:") {
            Expected::ErrorSubstring(rest.trim().to_string())
        } else {
            Expected::Output(expected_contents)
        };

        cases.push(Case {
            name: dir.file_name().unwrap().to_string_lossy().to_string(),
            input,
            expected: expectation,
        });
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_cases_inner(&path, cases);
            }
        }
    }
}

fn unified_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.split_inclusive('\n').collect();
    let actual_lines: Vec<&str> = actual.split_inclusive('\n').collect();
    let table = lcs_table(&expected_lines, &actual_lines);
    let mut diff = String::new();

    diff.push_str("--- expected\n+++ actual\n");
    let mut chunks = Vec::new();
    backtrack(
        &expected_lines,
        &actual_lines,
        &table,
        expected_lines.len(),
        actual_lines.len(),
        &mut chunks,
    );
    for chunk in chunks {
        diff.push_str(&chunk);
        if !chunk.ends_with('\n') {
            diff.push('\n');
        }
    }
    diff
}

fn backtrack(
    expected: &[&str],
    actual: &[&str],
    table: &[Vec<usize>],
    i: usize,
    j: usize,
    chunks: &mut Vec<String>,
) {
    if i > 0 && j > 0 && expected[i - 1] == actual[j - 1] {
        backtrack(expected, actual, table, i - 1, j - 1, chunks);
        chunks.push(format!(" {}", expected[i - 1]));
    } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
        backtrack(expected, actual, table, i, j - 1, chunks);
        chunks.push(format!("+{}", actual[j - 1]));
    } else if i > 0 {
        backtrack(expected, actual, table, i - 1, j, chunks);
        chunks.push(format!("-{}", expected[i - 1]));
    }
}

fn lcs_table(expected: &[&str], actual: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; actual.len() + 1]; expected.len() + 1];
    for i in 0..expected.len() {
        for j in 0..actual.len() {
            if expected[i] == actual[j] {
                table[i + 1][j + 1] = table[i][j] + 1;
            } else {
                table[i + 1][j + 1] = max(table[i + 1][j], table[i][j + 1]);
            }
        }
    }
    table
}
