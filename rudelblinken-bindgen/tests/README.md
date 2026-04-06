# Integration test fixtures

Each test case lives in its own directory under `tests/cases/`.

Required files:

- `input.c`: the C input for the case
- `output_c_guest.c`: the expected C guest output, or an error substring

If `output_c_guest.c` starts with `ERROR:`, the remainder of the file is treated as a substring that must appear in the error returned by the bindgen run.

The integration test runner prints a unified diff when output comparison fails.
