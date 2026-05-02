# Bindgen ships as a standalone CLI; bindings are pre-generated

`rudelblinken-bindgen` is a standalone CLI binary. Crates that need bindings (e.g. `rudelblinken-sdk`) check in the generated output and ship it directly, rather than running the generator at build time.

## Why

Checking in pre-generated bindings means consumers of `rudelblinken-sdk` get a normal Rust crate with no bindgen dependency at all — no tool installation, no `build.rs`, no extra build step. The interface is stable enough that regeneration is a deliberate, infrequent act (when the C header changes), not something that needs to happen on every build.

It also keeps the generated output reviewable in version control: diffs on the C header and diffs on the generated bindings are both visible and auditable.

## Future build.rs integration

A `build.rs`-based integration (opt-in, automatically regenerate when the header changes) is planned for cases where regenerating on each build is preferable. The CLI being a separate binary makes this straightforward to add later without changing the core tool.

## Considered options

**proc-macro / derive-based generation**: would run at compile time inside the Rust compiler, but proc-macros cannot read arbitrary files from disk and are harder to test and debug. Rejected.

**build.rs only, no standalone CLI**: would require every consumer to run `rudelblinken-bindgen` at build time. Rejected because it forces a tool dependency on SDK consumers and makes the generated output invisible in version control.
