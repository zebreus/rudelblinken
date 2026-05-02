# Rust codegen uses syn AST construction, not string concatenation

The Rust guest backend builds output by constructing a `syn::File` AST in memory and formatting it with `prettyplease::unparse()`, rather than generating Rust source by concatenating strings or using ad-hoc templates.

## Why

String concatenation is fragile: it is easy to produce syntactically invalid Rust (missing semicolons, mismatched braces, wrong identifier escaping) with no feedback until the output is compiled. Building a typed `syn` AST means the structure of the output is enforced by Rust's type system — an ill-formed item simply cannot be constructed. `prettyplease` then handles all formatting, so the backend never needs to manage indentation or whitespace.

## Considered options

**String concatenation / format! templates**: simplest to write, but provides no structural guarantees. Rejected because generator bugs would produce silently broken output that only fails at the SDK's compile step, far from the source of the error.

**`quote!` macro**: generates a `TokenStream` directly, which is idiomatic for proc-macros but gives less structure than a full `syn` AST and requires more manual effort for formatting. Considered but `syn` + `prettyplease` was preferred for the explicit, inspectable AST it produces.
