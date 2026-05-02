# Input headers use a restricted, canonical subset of C

`rudelblinken-bindgen` accepts only a strict subset of C declarations and rejects everything else at parse time. Within that subset, each construct maps to exactly one semantic meaning in the generator IR.

## Why restrict

C provides multiple syntactically valid ways to express semantically similar things. If all of them were accepted and mapped to the same IR node, the input format would become unconstrained and tooling (linters, formatters, future backends) would need to defend against many equivalent representations. By accepting only one canonical form per concept, the input stays predictable and the generator stays simple.

Example: named structs (`struct Name { ... };`) are supported; `typedef`-anonymous structs (`typedef struct { ... } Name;`) are not. Both produce a named struct type in C, but they are kept syntactically distinct because they *might* carry different semantics for future target languages (e.g. one becomes a value type, the other a reference type). Accepting only one form now reserves the distinction for later rather than conflating them.

## C23 preferred over legacy conventions

Where C23 provides a modern form, only the modern form is accepted. Legacy equivalents (e.g. `_Static_assert` vs `static_assert`, GNU `__attribute__` vs `[[...]]` C23 attributes) are either unsupported or deprecated in the input format. This reduces the surface area of the parser and keeps the input consistent.

## Annotation comments as an escape hatch

Special structured comments attached to a declaration may modify its semantic meaning in the generator IR. This is the only supported mechanism for expressing semantics that can't be derived from the C syntax alone. (Currently unspecified; reserved for future use.)

## Considered options

**Accept all valid C and normalise in the generator**: would allow any C header as input but would push ambiguity downstream. Rejected because it makes it impossible to reject inputs that the generator cannot cleanly handle, and removes the ability to reserve syntactic distinctions for future semantic use.

**Accept only what is needed right now, no explicit canonicality rule**: simpler short-term but would allow ad-hoc additions over time that silently break the one-form-per-concept invariant. Rejected in favour of making the invariant explicit.
