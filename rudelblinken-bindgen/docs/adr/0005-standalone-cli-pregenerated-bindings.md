# Bindings are pre-generated and checked in

Crates that need bindings from `rudelblinken-bindgen` (for example `rudelblinken-sdk`) check the generated output into version control and ship it directly, rather than regenerating bindings during normal consumer builds.

This decision depends on `rudelblinken-bindgen` existing as a normal tool that can be run deliberately when the Host/Guest Linkage changes. The shape of that tool and library surface is recorded separately in ADR-0007.

## Why

Checking in pre-generated bindings means consumers of `rudelblinken-sdk` get a normal Rust crate with no bindgen dependency at all — no tool installation, no `build.rs`, no extra build step. The Host/Guest Linkage is stable enough that regeneration is a deliberate, infrequent act when the C header changes, not something that needs to happen on every build.

It also keeps the generated output reviewable in version control: diffs on the C header and diffs on the generated bindings are both visible and auditable.

## Future build.rs integration

A `build.rs`-based integration (opt-in, automatically regenerate when the header changes) is planned for cases where regenerating on each build is preferable. Because the bindgen tool exists independently of consumer builds, that can be added later without changing the checked-in-by-default strategy.

## Considered options

**proc-macro / derive-based generation**: would run at compile time inside the Rust compiler, but proc-macros cannot read arbitrary files from disk and are harder to test and debug. Rejected.

**build.rs only, no standalone CLI**: would require every consumer to run `rudelblinken-bindgen` at build time. Rejected because it forces a tool dependency on SDK consumers and makes the generated output invisible in version control.

**Check in generated bindings but still regenerate them automatically in every consumer build**: would preserve checked-in output for review, but it would reintroduce the tool dependency and build-time coupling this ADR is avoiding. Rejected.
