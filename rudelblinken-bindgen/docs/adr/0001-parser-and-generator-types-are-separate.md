# Parser and generator types are intentionally separate

The `parser` module models C syntax; the `generator` module models a language-neutral IR consumed by code-generation backends (C headers, Rust bindings, …). Even though the types look nearly identical today, they are defined independently in each module. The `From<>` impls in `generator.rs` are the explicit seam between the two. As backends grow their own requirements the two type sets will diverge, and keeping them separate from the start avoids a painful split later.

## Considered options

Collapsed into one: re-export parser types from the generator module and skip the `From<>` indirection. Rejected because it would couple the C AST representation to the generator IR, making it harder for backends to extend or reinterpret types without touching the parser.
