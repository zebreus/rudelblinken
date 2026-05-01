# Domain Docs

How the engineering skills should consume this repo's domain documentation.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — shared domain glossary for the entire project.
- **`docs/adr/`** at the repo root — system-wide architectural decisions.
- **`<crate>/docs/adr/`** — crate-internal implementation decisions (e.g. `rudelblinken-bindgen/docs/adr/`). Read only when working inside that crate.

If any of these files don't exist, proceed silently. Don't flag their absence or
suggest creating them upfront — `/grill-with-docs` creates them lazily as terms
and decisions are resolved.

## File structure

Single-context repo with tiered ADRs:

```
/
├── CONTEXT.md                          ← shared domain glossary (single context)
├── docs/adr/                           ← system-wide decisions
│   └── 0001-*.md
├── rudelblinken-bindgen/
│   └── docs/adr/                       ← bindgen-internal decisions
├── rudelblinken-filesystem/
│   └── docs/adr/                       ← filesystem-internal decisions
└── ...
```

There is **one** `CONTEXT.md` at the repo root. No per-crate context files.
Per-crate `docs/adr/` directories record crate-internal implementation decisions
that are not relevant to consumers of those crates.

## Use the glossary's vocabulary

When naming domain concepts (in issue titles, refactor proposals, hypotheses,
test names), use the term as defined in `CONTEXT.md`. Don't drift to synonyms
the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either
you're inventing language the project doesn't use (reconsider) or there's a real
gap (note it for `/grill-with-docs`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than
silently overriding:

> _Contradicts ADR-0002 (WIT-defined host/guest interface) — but worth reopening
because…_
