# Errors carry source spans; the CLI renders them with ariadne

All errors produced by `generate_bindings` — both parse errors and semantic
lowering errors — carry a `Span { source, start, end }` pointing at the
offending source location. The CLI renders these spans as human-readable
ariadne diagnostics; programmatic callers receive the structured `BindgenError`
values and may render them however they wish.

## Motivation

Before this change, `BindgenError` carried `Option<line>` and `Option<column>`
fields (both `None` for semantic errors) and the CLI output was a plain text
list of messages with no source excerpt. Semantic errors had no location at all
because the generator IR discarded spans after parsing.

There were two compounding problems:

1. **Synthetic locations**: semantic errors were forced to emit a fake `0:0`
   location or `None`, making it impossible for callers to distinguish "no
   location available" from "position zero".
2. **No source context**: even parse errors showed only a byte offset with no
   surrounding source text, making them hard to act on quickly.

## Decisions

### Span type

`Span` is a plain struct (`source: String, start: usize, end: usize`) in the
supported diagnostics surface of the crate. It implements `ariadne::Span` with
`SourceId = String` so it can be passed directly to ariadne without conversion.
Using a custom type rather than `chumsky::span::SimpleSpan` prevents chumsky
from leaking into the public API surface.

The same `Span` type is used internally as well. Parser declarations,
lowering errors, and `BindgenError` all carry this shared span type directly
rather than converting between a private internal span and a public API span.
That keeps the implementation simpler and avoids a fake distinction between
"internal span" and "public span" when both represent the same source
location concept.

Both `start` and `end` are byte-offsets into the source string, matching
ariadne's `IndexType::Char` (default) convention.

### Spans live on parser declaration structs

Each top-level declaration struct (`FunctionDecl`, `VariableDecl`,
`StructDecl`, `EnumDecl`) has a `pub span: Span` field populated by chumsky
`map_with` in `parse_declarations`. This is declaration-granularity — the
span covers the full declaration from the first token to the trailing semicolon.
Attribute-level sub-spans were considered and deferred; declaration-level is
already a large improvement over no location.

### Spans propagate through `LoweringError`

`LoweringError` gains `span: Option<Span>`. Every error pushed in
`Declarations::lower` is created via `LoweringError::at(message, span.clone())`
where `span` is the enclosing declaration's span. `Option` is retained because
a future lowering error might not correspond to any single declaration (e.g. a
cross-declaration consistency check), but at present all errors have a
`Some(span)`.

### `BindgenError` uses `Option<Span>` not `(line, column)`

`BindgenError` replaces `Option<line>` + `Option<column>` with a single
`Option<Span>`. This is strictly more information — callers can compute
line/col themselves from `span.start` if they need it — and removes the
distinction between "location unknown" (`None`) and "location is byte offset
zero" (`Some(span.start = 0)`).

`BindgenError` intentionally remains a high-level diagnostics type: it carries
location plus a human-readable message, but not an additional public error-kind
enum such as "parse" vs "semantic". Those categories mirror internal phases of
the implementation more than stable user-facing concepts, and exposing them
would make the public diagnostics API track internal pipeline structure too
closely.

### `generate_bindings` takes an explicit `source: &str`

The function signature becomes `generate_bindings(input, source, format)`.
`source` is the display label embedded in every `Span`. CLI callers pass the
filename; test callers pass `"<test>"` or similar. There is no overload with a
default — every caller knows what they are parsing and should say so.

`parse_declarations_checked` (the external-facing wrapper around the internal
`parse_declarations`) accepts the same `source` parameter and returns
`Vec<(Span, String)>` rather than chumsky's `Rich<char>`, so consumers never
need to import chumsky.

### ariadne rendering is exposed per error, and the CLI batches it

Public rendering convenience lives on `BindgenError` itself as an inherent
method named `render(&self, source_text: &str) -> String` that formats one
error with ariadne given the relevant source text. `generate_bindings` itself
does not call it — it returns `Vec<BindgenError>` as before. The CLI layer can
iterate those errors and concatenate their rendered forms. Programmatic callers
keep full access to the structured errors and are not forced into ariadne's
text format.

Colour is disabled (`Config::default().with_color(false)`) so the output is
stable in terminals that do not support ANSI, in CI, and in fixture test
substring checks.

## Considered options

**`Spanned<T>` newtype wrapper** (e.g. `Vec<Spanned<FunctionDecl>>`): keeps
declaration structs span-free but forces callers to destructure constantly.
Rejected — embedding span on the struct is simpler and tests can always supply
`Span::default()`.

**A separate public diagnostic span converted from a private parser span**:
would preserve a stricter internal/public boundary, but would add conversion
code and duplicated concepts without buying clearer semantics. Rejected because
the span itself is already part of the supported diagnostics API, so internal
code can use that same type directly.

**Add a public `BindgenErrorKind` enum**: would let callers branch on parse vs
semantic failures, but those categories are primarily implementation phases and
would pressure the public API to evolve with internal restructuring. Rejected in
favour of keeping `BindgenError` high-level.

**`SimpleSpan` from chumsky directly on declaration structs**: simpler initial
implementation but leaks the chumsky dependency into the public type. Rejected
to keep the parser implementation detail internal.

**`SourceId = ()` with no filename in spans**: viable but the filename in the
error header (`Error → broken.h:1:5`) is valuable user-facing information.
Rejected.

**A free `render_errors` helper over slices of errors**: works, but keeps the
public convenience API oriented around batches rather than the error type
itself. Rejected in favour of per-error inherent rendering on `BindgenError`,
with batching left to the caller.

**ariadne rendering inside `generate_bindings`**: forces all callers into
ariadne's text format and makes structured error handling impossible. Rejected.
