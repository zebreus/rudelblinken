# The Host/Guest interface is defined in C headers, not WIT or WITX

The interface between the Host firmware and Guest Programs is defined using C headers annotated with GNU-style `__attribute__((import_module(...), import_name(...)))` directives. `rudelblinken-bindgen` parses these headers and generates guest bindings (C headers and Rust).

WIT was considered and rejected because it is designed for the WASM Component Model, which this project does not use — the runtime is plain `wasmi`, not a component runtime. WITX (the predecessor IDL used in early versions of this repo) was also rejected as it is poorly maintained and has a poor developer experience. C headers with attribute annotations give us a language-neutral, toolchain-agnostic source of truth that maps directly to the raw WASM import/export ABI we actually use.
