# Input headers use a restricted, canonical subset of C

`rudelblinken-bindgen` accepts only a strict subset of C declarations and rejects everything else before code generation. Pure syntax errors are rejected by the parser; parse-valid declarations whose meaning is unsupported or contradictory are rejected by semantic lowering. Within the accepted subset, each construct maps to exactly one semantic meaning in the generator IR.

## Why restrict

C provides multiple syntactically valid ways to express semantically similar things. If all of them were accepted and mapped to the same IR node, the input format would become unconstrained and tooling (linters, formatters, future backends) would need to defend against many equivalent representations. By accepting only one canonical form per concept, the input stays predictable and the generator stays simple.

Example: named structs (`struct Name { ... };`) are supported; `typedef`-anonymous structs (`typedef struct { ... } Name;`) are not. Both produce a named struct type in C, but they are kept syntactically distinct because they *might* carry different semantics for future target languages (e.g. one becomes a value type, the other a reference type). Accepting only one form now reserves the distinction for later rather than conflating them.

The parser and lowerer intentionally split the work: the parser recognises a small C-shaped grammar, while lowering decides whether that syntax has supported rudelblinken-bindgen semantics. For example, a bare identifier used as a type is syntactically recognisable as a named C type, but until typedef semantics are defined it is rejected during lowering rather than being passed to backends as ambiguous generator IR. This keeps canonicality a property of the full parse-and-lower pipeline, not only of the parser grammar.

## C23 preferred over legacy conventions

Where C23 provides a modern form, only the modern form is accepted. Legacy equivalents (e.g. `_Static_assert` vs `static_assert`) are either unsupported or deprecated in the input format. This reduces the surface area of the parser and keeps the input consistent.

### WASM linkage attributes

WASM linkage is expressed exclusively through C23 `[[...]]` attribute specifiers in **prefix** position (before the return type):

| Meaning                   | Canonical form                                                   |
|---------------------------|------------------------------------------------------------------|
| Import from host          | `[[clang::import_module("mod"), clang::import_name("name")]]`    |
| Import from host ("env")  | `[[clang::import_name("name")]]` (module defaults to `"env"`)    |
| Export from guest         | `[[clang::export_name("name")]]`                                 |

Every function declaration is definitively one of these two directions. There is no neutral/unannotated third kind. A function with no WASM linkage attribute is implicitly a host import (clang's default WASM import module is `"env"` and the import name defaults to the C function name). There is no implicit guest export — clang has no convention for it; an `[[clang::export_name(...)]]` attribute is always required to mark a function as a guest export.

The generator IR stores the resolved `"env"` module explicitly (it is always present after lowering), but the C backend omits the `import_module` attribute from output when the module is `"env"`. This is a deliberate readability choice: `[[clang::import_name("log")]]` is cleaner than repeating `[[clang::import_module("env"), clang::import_name("log")]]` on every function in a typical rudel header where all imports come from `"env"`. The IR stays unambiguous; the human-readable output stays concise.

GNU `__attribute__((import_module(...), import_name(...)))` and `__attribute__((export_name(...)))` suffix forms are **not accepted**. There is no backwards-compatibility shim. Backwards compatibility with the GNU form is explicitly out of scope — the canonical form is the only form, and old inputs must be migrated.

The suffix form (`void f() [[clang::...]]`) is also rejected: clang itself rejects it for function declarations ("cannot be applied to types"), so it cannot be used portably even in valid C23.

The bare (no-namespace) form — e.g. `[[import_name("log")]]` — is also not accepted; clang silently ignores unknown unscoped attributes, making it impossible to distinguish a mistyped name from intentional use. The `clang::` namespace is required for all WASM linkage attributes.

The C23 attribute forms for WASM linkage (`[[clang::import_module(...)]]`, `[[clang::import_name(...)]]`, `[[clang::export_name(...)]]`) were verified to be supported by clang before being adopted as the canonical input and output form. The `compile_cases/c23_export_name` test fixture is the ongoing proof of this: it compiles a header using `[[clang::export_name(...)]]` through clang to WASM and verifies the result. If clang ever stops supporting these C23 attribute forms the fixture will fail and the canonical form must be revisited.

## Annotation comments as an escape hatch

Special structured comments attached to a declaration may modify its semantic meaning in the generator IR. This is the only supported mechanism for expressing semantics that can't be derived from the C syntax alone. (Currently unspecified; reserved for future use.)

## Considered options

**Accept all valid C and normalise in the generator**: would allow any C header as input but would push ambiguity downstream. Rejected because it makes it impossible to reject inputs that the generator cannot cleanly handle, and removes the ability to reserve syntactic distinctions for future semantic use.

**Accept only what is needed right now, no explicit canonicality rule**: simpler short-term but would allow ad-hoc additions over time that silently break the one-form-per-concept invariant. Rejected in favour of making the invariant explicit.
