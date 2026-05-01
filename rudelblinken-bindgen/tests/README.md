# Integration test fixtures

Each test case lives in its own directory under `tests/cases/`.

Required files:

- `input.c`: the C input for the case
- `output_c_guest.c`: the expected C guest output, an error substring, or `SUCCESS`

If `output_c_guest.c` is exactly `SUCCESS`, the case only asserts that `run()` succeeds.

If `output_c_guest.c` starts with `ERROR:`, the remainder of the file is treated as a substring that must appear in the error returned by the bindgen run.

The integration test runner prints a unified diff when output comparison fails.

## Compile fixtures

Compile fixtures are a separate test set under `tests/compile_cases/` and are exercised by
`tests/compile_integration.rs`.

Each compile case directory contains:

- `bindgen_input.c`: input declarations for bindgen
- `guest_impl.c`: C guest source that includes `generated_c_guest.h`

Per case, the compile test does:

1. Generate `generated_c_guest.h` via bindgen
2. Compile `guest_impl.c` to `module.wasm` using `clang`
3. Convert `module.wasm` to `module.wat` using `wasm-tools print`

If `clang`, `wasm-ld`, or `wasm-tools` is unavailable, compile fixtures are skipped.

`generated_c_guest.h` is preserved for debugging. `*.wasm` and `*.wat` are gitignored
via `tests/compile_cases/.gitignore`.
