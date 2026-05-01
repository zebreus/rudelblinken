# Agent Instructions

## Agent skills

This repo uses the [mattpocock/skills](https://github.com/mattpocock/skills) engineering skills.
Configuration for those skills lives in `docs/agents/`:

- **Issue tracker**: `docs/agents/issue-tracker.md`
- **Triage labels**: `docs/agents/triage-labels.md`
- **Domain docs**: `docs/agents/domain.md`

Before working on any issue or feature, read these files so the right vocabulary,
issue-tracker commands, and domain glossary are available.

## Commit messages

Use standard Git commit style: imperative mood, no conventional-commit prefixes. Use whatever verb fits — Add, Fix, Implement, Remove, Unfuck, Clean up, Attempt fixing, Refactor, Update, Rename, Extract, Debug, Write, etc.

- `Add fuel metering documentation`
- `Fix advertisement callback not firing on upload`
- `Implement refuel() as replacement for yield_now()`

Not: `feat: add fuel metering` or `docs(context): update glossary`.
