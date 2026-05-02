# Pure-Rust parser (chumsky) over libclang

`rudelblinken-bindgen` uses a hand-rolled `chumsky`-based parser rather than binding to `libclang` or any other C compiler frontend.

Three reasons drove this:

1. **No external system dependencies.** `libclang` requires a Clang installation on the host. Keeping the tool pure-Rust means it builds anywhere `cargo` builds without extra setup.

2. **Only a restricted declaration subset is needed.** The input is a narrow, well-defined slice of C — interface declarations, not implementation code. A full C compiler frontend is far more than required and adds complexity without benefit.

3. **Better error messages for unsupported constructs.** Because the parser only accepts the canonical supported subset, it can give precise, actionable errors when the input uses something outside that subset. `libclang` would silently accept constructs the generator cannot handle.

## Considered options

**libclang / clang-sys**: would understand all of C and handle every edge case in preprocessing and type resolution. Rejected because it introduces a system dependency and accepts far more input than we want — making it harder to enforce the canonical-only input model (see ADR-0003).

**tree-sitter-c**: grammar-based approach, also pure-Rust, but produces a CST that still requires significant lowering and does not give us control over which constructs to reject. Rejected in favour of a combinator parser that can precisely express the accepted subset and reject everything else at parse time.
