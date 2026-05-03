# Bindgen exposes one supported public surface for CLI and Rust callers

`rudelblinken-bindgen` is both a standalone CLI binary and an embeddable Rust library. The library API is not just a test hook for the CLI binary. This ADR defines one supported public surface for Rust callers that want either of two modes.

## Supported modes

- **CLI-shaped embedding**: pass argument-like values, stdin, stdout, and stderr from another Rust CLI and receive an exit code, without spawning the `rudelblinken-bindgen` binary.
- **Idiomatic high-level generation**: call generation functions directly with Rust values and receive generated output or structured errors.

The separate decision that generated bindings are checked in rather than regenerated in normal consumer builds is recorded in ADR-0005.

## Execution Layers

The public surface is organised as three layers:

1. **`generate_bindings`** is the high-level generation API. It exposes bindgen functionality independent of the CLI: caller-provided source text and generation options go in; generated output or structured `BindgenError`s come out. It performs no CLI argument parsing and should not own terminal I/O policy.

   There is a single high-level generation entry point: `generate_bindings(input, source, format)`. We do not add format-specific wrapper functions such as `generate_c_guest_bindings` or `generate_rust_guest_bindings`; `OutputFormat` is the shared vocabulary for both CLI and library use.

   `generate_bindings` takes `OutputFormat` by value, not by reference. `OutputFormat` is a small `Copy` enum, and the by-value form is the more ergonomic high-level API.

2. **`run`** is the ergonomic Rust representation of the exact CLI operation. It uses already-parsed Rust values, but its behaviour should match the CLI semantics: same input/output handling, same output format defaults, and same diagnostic rendering policy where applicable. Because it owns the CLI operation, it receives both `stdin` and `stdout` handles: it reads from stdin when the input path is `-`, writes to stdout when no output path or `--output -` is requested, and writes to the configured file otherwise. It should not also return generated text after writing it; success is represented as `Result<(), RunError>`.

3. **`run_cli`** is the exact CLI-shaped adapter. It accepts argument-like values plus environment variables and stdin/stdout/stderr handles, parses CLI arguments, runs the operation, writes user-facing output, and returns a process-style exit code. The environment map is retained even though current bindgen behaviour does not consume it yet, because environment-sensitive CLI behaviour may be added later and embedders that want the full process-shaped interface should not need a breaking signature change when that happens.

Both `run` and `run_cli` take `&mut dyn Read` / `&mut dyn Write` at the I/O boundary rather than being generic over `Read` / `Write`. These are boundary APIs for already-existing streams, and the trait-object form keeps the public signatures compact and straightforward for embedders.

Error output remains the responsibility of `run_cli`. `run` returns a structured public `RunError` to its caller rather than receiving `stderr`, so embedding callers can decide whether to render errors to a terminal, collect them for tests, branch on failure kind programmatically, convert them to their own error type, or display them in another UI.

`RunError::Generate` should carry both the structured `BindgenError` list and the original input text. That keeps diagnostic rendering single-pass: `run()` already has the source text in memory after reading stdin or a file, so `run_cli()` can render errors without re-reading the source and without special-casing stdin.

`RunError::ReadInput` and `RunError::WriteOutput` should likewise stay structured. They carry the explicit input/output endpoint plus the original `std::io::Error`, rather than preformatted strings. That preserves `io::ErrorKind` and path information for embedders while leaving `run_cli()` free to render terminal-friendly messages.

## Canonical options

`Args` is the single canonical options type for the application. It is used by the binary, by `run_cli`, and by embedders calling `run` directly from Rust. We do not introduce a second `RunOptions`-style type for the same operation. If the operation needs to become more ergonomic for Rust callers, that ergonomics should be improved within `Args` itself rather than by creating a parallel configuration type.

`Args` remains the clap-annotated public type. The same type that describes the operation internally also defines CLI parsing, defaults, and help text. We do not split clap parsing into a separate wrapper type, because that would duplicate option semantics and create drift between the CLI and embedded Rust use.

That includes the stdin/stdout cases: `Args` should represent them explicitly instead of relying on raw `PathBuf` fields with the CLI `"-"` sentinel embedded in their meaning. The CLI parser can still accept `-`, but it should translate that into explicit `Args` values so Rust callers do not need to smuggle streams through fake filesystem paths. Concretely, `Args::input` should distinguish stdin from a real path, and `Args::output` should distinguish stdout from a real path while keeping stdout as the default destination.

Those endpoint types should be public top-level enums named `InputSource` and `OutputTarget`. Because `Args` is the canonical options type and is passed directly to `run`, embedders must be able to construct those explicit input/output values directly from Rust. `RunError` can also reuse the same endpoint vocabulary.

## Supported surface

The parser IR and generator IR remain internal layers behind that high-level surface. They are useful implementation concepts, but the library's top-level API should not present parser declarations as “what bindgen is”. The internal parser/generator split remains governed by ADR-0001.

Concretely, the `parser` and `generator` modules are implementation details, not supported public entry points. Crate-internal tests may exercise them directly, but external callers should depend on the high-level API surface rather than low-level IR types or parser/generator internals.

The supported public API stays flat at the crate root. We do not introduce public `cli`, `diagnostics`, or similar facade submodules for this surface. The root is the single supported entry area; `parser` and `generator` staying internal reinforces that public surface.

At the root, the supported public surface is the high-level API and its shared types: `Args`, `InputSource`, `OutputTarget`, `OutputFormat`, `RunError`, `BindgenError`, `Span`, `generate_bindings`, `run`, `run_cli`, and `BindgenError::render`. Existing parser IR re-exports are not part of that supported surface and should be removed.

## Why

Keeping the CLI implementation routed through the library avoids duplicated behaviour between the binary and Rust callers. It also makes CLI behaviour easy to test without process-spawning infrastructure: tests can call the same CLI-shaped function an embedding application would call.

The three-layer structure gives callers leverage without exposing parser or generator implementation details. High-level callers can just generate bindings; CLI-shaped embedders can reuse full process semantics; maintainers keep locality because input handling, output handling, and diagnostic policy live behind one supported interface instead of being reconstructed across callers.

## Considered options

### Execution Layers

**CLI-only library internals**: would expose just enough Rust functions to test the binary, but not treat them as a supported embedding surface. Rejected because downstream Rust tools may reasonably want bindgen behaviour without shelling out to a child process.

**Generic `Read` / `Write` parameters on `run` and `run_cli`**: would make the public surface more verbose without adding much leverage, because these operations are designed around already-existing streams rather than type-driven adapter selection. Rejected in favour of `&mut dyn Read` / `&mut dyn Write`.

### Canonical options

**Split clap parsing from the canonical options type**: would create a second public shape for the same operation and make the public surface shallower by duplicating semantics between CLI-facing and Rust-facing types. Rejected.

### Supported surface

**Expose parser IR as the primary library interface**: would let advanced callers inspect raw C syntax directly, but would make the syntax model appear to be the core product and weaken the supported high-level surface. Rejected in favour of a small high-level API over the full parse-lower-generate pipeline.

**Keep parser/generator modules public as an unofficial escape hatch**: would avoid a breaking visibility change today, but would still make low-level implementation details look intentionally supported. Rejected because accidental public reachability is still reachability callers may start depending on.

**Public facade submodules such as `cli` or `diagnostics`**: would spread one supported surface across multiple entry points even though callers are using the same shared vocabulary and types. Rejected in favour of one flat crate-root public surface.