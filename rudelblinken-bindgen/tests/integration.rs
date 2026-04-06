use rudelblinken_bindgen::{Args, OutputFormat, run};
use std::cmp::max;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
        input: case.input.clone(),
        output: None,
        format: OutputFormat::CGuest,
    };

    let mut stdin = io::empty();
    let result = run(args, &mut stdin);

    match (&case.expected, result) {
        (Expected::Success, Ok(_actual)) => {}
        (Expected::Success, Err(err)) => {
            panic!("expected success for {} but got error\n{}", case.name, err);
        }
        (Expected::Output(expected), Ok(actual)) => {
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
                case.name, expected, err
            );
        }
        (Expected::ErrorSubstring(expected), Err(err)) => {
            assert!(
                err.contains(expected),
                "error mismatch for {}\nexpected substring: {:?}\nactual error: {:?}",
                case.name,
                expected,
                err
            );
        }
        (Expected::ErrorSubstring(expected), Ok(actual)) => {
            panic!(
                "expected error for {} but got output\nexpected substring: {:?}\nactual output:\n{}",
                case.name, expected, actual
            );
        }
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
