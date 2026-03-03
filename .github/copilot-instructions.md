## Development Environment

This is a Rust project that uses `xtask` for task automation.

## Before Pushing Any Changes

Always run the following command before committing or pushing changes:

```bash
cargo xtask test
```

This command runs:
- `cargo fmt` - Ensures code is properly formatted
- `cargo clippy` - Runs lints to catch common mistakes
- `cargo test` - Runs the test suite
- `cargo xtask sql check` - Verifies database migrations are in sync with schema.sql

**Do not push changes if any of these checks fail.** Fix all formatting issues, clippy warnings, and test failures first.

## Code Style

- Follow Rust idioms and best practices
- All public items should have documentation
- Use `#[must_use]` where appropriate
- Prefer `expect()` over `unwrap()` with descriptive messages

## Code Quality

- Extract loops inside conditionals into their own functions
- Limit indentation to 3 levels; extract deeper logic into helper functions
- Split `if`/`else` blocks where each branch exceeds 60 lines into separate functions
- Replace exhaustive `if let..else` (e.g., `if let Some(..) { .. } else { .. }`) with `match` blocks
- Keep one primary struct/enum per file; split large files accordingly (small helpers exempted)

For detailed guidelines, examples, and a review workflow see the `code-quality` skill.

## Database Schema Changes

When changing the database schema, edit `crates/wasm-package-manager/src/storage/schema.sql`
then run `cargo xtask sql migrate --name <description>`. Never hand-write migration files.

Run `cargo xtask sql check` (or `cargo xtask test`) to verify migrations are in sync
before pushing.

## Spec-Driven Development

This project follows spec-driven development: **specs first, then tests, then
implementation**. Requirements live in `spec/*.md`, traceability is tracked with
[Tracey](https://github.com/bearcove/tracey) (config: `.config/tracey/config.styx`),
and the annotation prefix is `r`.

When implementing features, follow the `spec-driven-development` skill workflow.
For Tracey annotation details, see the `tracey` skill.
