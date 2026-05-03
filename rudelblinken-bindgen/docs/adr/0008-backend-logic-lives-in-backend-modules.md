# Backend-specific logic lives in its backend module

## Vocabulary

**Backend**: a submodule of `generator/` that turns the generator IR into a
specific output format. Current backends: `c_guest` (C23 header files) and
`rust_guest` (Rust guest bindings). Each backend owns its own formatting logic,
type mapping, and tests.
_Avoid_: target, emitter, renderer (use backend in bindgen-internal discussions)

---

All code that is specific to a single generation target (C guest headers, Rust
guest bindings, …) lives in the corresponding submodule of `generator/`, or in
a submodule of that module. No backend-specific code belongs in `generator.rs`
itself.

## Why

`generator.rs` owns the intermediate representation: `Type`, `Declarations`,
`Function`, and friends. That is its responsibility. If C-specific string
formatting or Rust-specific `syn` construction drifted into `generator.rs`, a
reader would need to understand two backends just to understand the IR. Keeping
each backend self-contained makes the IR module readable in isolation and makes
each backend navigable without reading the others.

The same rule applies to shared utilities that serve exactly one backend: a
helper that only `c_guest.rs` calls belongs in `c_guest.rs` (or a submodule of
it), not in a shared file.

## Corollary: no shared extraction unless two backends vary

A projection or helper function should be extracted into a shared module only if
two or more backends genuinely need it and their needs diverge in a way that
warrants a shared abstraction. If only one backend uses a function, it stays
in that backend. If two backends call identical logic with no plausible future
variation, a free function in a small shared helper module inside `generator/`
is acceptable, but a trait with one implementor is not — that is indirection
without leverage.
