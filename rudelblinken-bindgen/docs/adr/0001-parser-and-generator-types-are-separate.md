# Parser and generator types are intentionally separate

The `parser` module and `generator` module each define their own type hierarchies and serve distinct purposes. They look similar today but have different semantic roles and will diverge as backends evolve.

## What each IR is for

**Parser IR** (`parser` module): models the accepted C syntax faithfully. Its job is to represent what was written in the input header within the restricted canonical C subset, including C23 attribute data. It is a structural reflection of the source text that the parser accepts. The parsing step produces parser IR from source text.

**Generator IR** (`generator` module): models _semantics_ for code generation. Its job is to represent what each declaration _means_ — stripped of syntactic noise and attribute syntax. Concrete example: where the parser IR has C23 attribute data, the generator IR has a `Linkage` enum — `HostImport { module, name }` or `GuestExport { name }` — with all defaults already resolved. A backend never inspects raw attribute syntax; it reads resolved semantic fields.

Once parsing has produced parser IR, rudelblinken-bindgen lowers it to generator IR. That lowering step begins by validating the parser IR semantically, then translates the validated declarations into generator IR. The parser may accept C syntax that is structurally valid inside the restricted grammar but not meaningful for rudelblinken-bindgen yet: unsupported named types/typedef status, conflicting declarations, invalid C ABI object types, enum values outside the supported ABI range, or contradictory WASM linkage attributes. Those are semantic lowering errors, not parser errors. Internally, the lowering module currently exposes a validation helper that produces a validated-declarations wrapper before the final IR translation runs, but that helper is an implementation detail of lowering rather than a separately significant architectural stage. Keeping semantic validation inside lowering makes `generate_bindings` a useful test surface for parse-valid-but-semantically-invalid headers and prevents backends from defending against ambiguous generator IR.

The generator IR also maps cleanly to the WASM C ABI that the input C header implies. Backends generate idiomatic code for their target language, but the generated code must produce the same ABI layout and import/export linkage as the original C declarations.

The generator IR models the full bidirectional host/guest contract — both directions, resolved to concrete linkage — even when not all backends have a consumer for both directions yet. The SDK and runtime currently use WIT-generated bindings rather than rudelblinken-bindgen output; they will migrate to rudelblinken-bindgen once it is ready. Until then, the GuestExport path in the generator IR and backends is implemented and tested in anticipation of that migration, not dead code.

## Lowering Parser IR to Generator IR

The full internal pipeline is: Input -> parsing step -> parser IR -> lowering step -> generator IR -> generation step -> Output. The lowering step performs semantic validation and IR translation together. Today that lowering implementation is split across `generator::Declarations::validate`, which produces an internal validated-declarations wrapper, and `generator::Declarations::lower`, which translates that validated parser IR into generator IR. Those functions are internal phases of the lowering implementation, not separate architectural steps in the pipeline. Attribute-flattening, default resolution, and syntax normalisation are completed during lowering before backends run — represented in parser IR, resolved during lowering, invisible to backends.

## Why keep them separate

Even though the types look nearly identical today, collapsing them would couple the C AST representation to the generator IR. Backends would then either be forced to handle raw attribute syntax, or the parser would have to understand backend-specific semantics. Keeping the parser and generator separate lets each side evolve independently.

## Considered options

**Collapsed into one type set**: re-export parser types from the generator module and skip the `From<>` indirection. Rejected because it would couple C syntax representation to generator semantics, forcing backends to deal with raw attribute tokens and making it harder to add backend-specific fields without touching the parser.
