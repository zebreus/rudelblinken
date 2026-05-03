use rudelblinken_bindgen::{OutputFormat, generate_bindings};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
struct CompileCase {
    name: String,
    bindgen_input: PathBuf,
    guest_impl: PathBuf,
    object_output: PathBuf,
    generated_header: PathBuf,
    wasm_output: PathBuf,
    wat_output: PathBuf,
}

#[test]
fn compile_fixture_cases() {
    if !command_exists("clang") || !command_exists("wasm-ld") || !command_exists("wasm-tools") {
        eprintln!(
            "Skipping compile fixture cases: `clang`, `wasm-ld`, and/or `wasm-tools` not found"
        );
        return;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compile_cases");
    let cases = collect_compile_cases(&root);
    assert!(
        !cases.is_empty(),
        "no compile fixture cases found in {:?}",
        root
    );

    for case in cases {
        run_compile_case(&case);
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn collect_compile_cases(root: &Path) -> Vec<CompileCase> {
    let mut cases = Vec::new();

    let entries = fs::read_dir(root).expect("read compile_cases directory");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let bindgen_input = path.join("bindgen_input.c");
        let guest_impl = path.join("guest_impl.c");

        if bindgen_input.is_file() && guest_impl.is_file() {
            let name = path
                .file_name()
                .expect("case directory name")
                .to_string_lossy()
                .to_string();

            cases.push(CompileCase {
                name,
                bindgen_input,
                guest_impl,
                object_output: path.join("module.o"),
                generated_header: path.join("generated_c_guest.h"),
                wasm_output: path.join("module.wasm"),
                wat_output: path.join("module.wat"),
            });
        }
    }

    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

fn run_compile_case(case: &CompileCase) {
    let input = fs::read_to_string(&case.bindgen_input)
        .unwrap_or_else(|err| panic!("{}: failed reading bindgen_input.c: {}", case.name, err));

    let generated =
        generate_bindings(&input, &case.name, OutputFormat::CGuest).unwrap_or_else(|errors| {
            panic!(
                "{}: bindgen generation failed with {} parse errors: {:?}",
                case.name,
                errors.len(),
                errors
            )
        });

    fs::write(&case.generated_header, generated)
        .unwrap_or_else(|err| panic!("{}: failed writing generated header: {}", case.name, err));

    let clang_output = Command::new("clang")
        .env("NIX_HARDENING_ENABLE", "")
        .args(["--target=wasm32-unknown-unknown", "-c", "-o"])
        .arg(&case.object_output)
        .arg(&case.guest_impl)
        .output()
        .unwrap_or_else(|err| panic!("{}: failed to spawn clang: {}", case.name, err));

    if !clang_output.status.success() {
        panic!(
            "{}: clang compilation failed\nstdout:\n{}\nstderr:\n{}",
            case.name,
            String::from_utf8_lossy(&clang_output.stdout),
            String::from_utf8_lossy(&clang_output.stderr)
        );
    }

    let linker_output = Command::new("wasm-ld")
        .args(["--no-entry", "--export=main", "-o"])
        .arg(&case.wasm_output)
        .arg(&case.object_output)
        .output()
        .unwrap_or_else(|err| panic!("{}: failed to spawn wasm-ld: {}", case.name, err));

    if !linker_output.status.success() {
        panic!(
            "{}: wasm-ld linking failed\nstdout:\n{}\nstderr:\n{}",
            case.name,
            String::from_utf8_lossy(&linker_output.stdout),
            String::from_utf8_lossy(&linker_output.stderr)
        );
    }

    let wat_output = Command::new("wasm-tools")
        .arg("print")
        .arg(&case.wasm_output)
        .output()
        .unwrap_or_else(|err| panic!("{}: failed to spawn wasm-tools: {}", case.name, err));

    if !wat_output.status.success() {
        panic!(
            "{}: wasm-tools print failed\nstdout:\n{}\nstderr:\n{}",
            case.name,
            String::from_utf8_lossy(&wat_output.stdout),
            String::from_utf8_lossy(&wat_output.stderr)
        );
    }

    fs::write(&case.wat_output, &wat_output.stdout)
        .unwrap_or_else(|err| panic!("{}: failed writing wat output: {}", case.name, err));

    let _ = fs::remove_file(&case.object_output);

    assert!(
        case.wasm_output.exists(),
        "{}: expected wasm output {:?} to exist",
        case.name,
        case.wasm_output
    );
    assert!(
        case.wat_output.exists(),
        "{}: expected wat output {:?} to exist",
        case.name,
        case.wat_output
    );
}
