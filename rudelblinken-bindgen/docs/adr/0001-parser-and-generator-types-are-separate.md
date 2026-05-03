# Parser and generator types are intentionally separate

The `parser` module and `generator` module each define their own type hierarchies and serve distinct purposes. They look similar today but have different semantic roles and will diverge as backends evolve.

## What each IR is for

**Parser IR** (`parser` module): models _C syntax_ faithfully. Its job is to represent what was written in the input header — including raw attributes (`__attribute__((...))`, `[[...]]`), legacy forms, and anything else the parser accepts. It is a structural reflection of the source text. The parsing step produces parser IR from source text.

**Generator IR** (`generator` module): models _semantics_ for code generation. Its job is to represent what each declaration _means_ — stripped of syntactic noise and attribute syntax. Concrete example: where the parser IR has C23 attribute data, the generator IR has a `Linkage` enum — `HostImport { module, name }` or `GuestExport { name }` — with all defaults already resolved. A backend never inspects raw attribute syntax; it reads resolved semantic fields.

Once parsing has produced parser IR, rudelblinken-bindgen first validates it semantically and then lowers it to generator IR. The parser may accept C syntax that is structurally valid inside the restricted grammar but not meaningful for rudelblinken-bindgen yet: unsupported named types/typedef status, conflicting declarations, invalid C ABI object types, enum values outside the supported ABI range, or contradictory WASM linkage attributes. Those are semantic validation errors in the validation step before lowering, not parser errors. Internally, this happens in two steps: the validation step first proves that parser declarations have supported rudelblinken-bindgen semantics, then the lowering step converts that validated parser IR into generator IR. Keeping validation and lowering together here makes `generate_bindings` a useful test surface for parse-valid-but-semantically-invalid headers and prevents backends from defending against ambiguous generator IR.

The generator IR also maps cleanly to the WASM C ABI that the input C header implies. Backends generate idiomatic code for their target language, but the generated code must produce the same ABI layout and import/export linkage as the original C declarations.

The generator IR models the full bidirectional host/guest contract — both directions, resolved to concrete linkage — even when not all backends have a consumer for both directions yet. The SDK and runtime currently use WIT-generated bindings rather than rudelblinken-bindgen output; they will migrate to rudelblinken-bindgen once it is ready. Until then, the GuestExport path in the generator IR and backends is implemented and tested in anticipation of that migration, not dead code.

## Lowering Parser IR to Generator IR

The full internal pipeline is: Input -> parsing step -> parser IR -> validation step -> lowering step -> generator IR -> generation step -> Output. `generator::Declarations::validate` is the validation step: it validates parser IR into an internal validated-declarations wrapper. `generator::Declarations::lower` is the lowering step: it translates that validated parser IR into generator IR. Semantic validation happens in the validation step. Attribute-flattening, default resolution, and syntax normalisation are completed before backends run — represented in parser IR, resolved during validation and lowering, invisible to backends.

## Why keep them separate

Even though the types look nearly identical today, collapsing them would couple the C AST representation to the generator IR. Backends would then either be forced to handle raw attribute syntax, or the parser would have to understand backend-specific semantics. Keeping the parser and generator separate lets each side evolve independently.

## Considered options

**Collapsed into one type set**: re-export parser types from the generator module and skip the `From<>` indirection. Rejected because it would couple C syntax representation to generator semantics, forcing backends to deal with raw attribute tokens and making it harder to add backend-specific fields without touching the parser.
