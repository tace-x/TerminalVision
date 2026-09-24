# Coding Rules

## Language

- Rust stable only. Do not use nightly features.
- Target the Rust 2024 edition.
- Prefer the standard library when it is sufficient.
- Do not add a dependency without a clear technical reason.
- Do not use unsafe code.
- Do not introduce global mutable state.

## Code

- Use meaningful names.
- Keep functions focused on a single job.
- Keep modules focused on a single responsibility.
- Avoid giant files. Split a module when it holds unrelated responsibilities; do not work to a fixed line count.
- Avoid duplicated logic. Extract shared behaviour once it is genuinely repeated.
- Handle errors explicitly.
- Do not silently ignore errors.
- Do not panic on operations that can fail at runtime; propagate the error instead.
- Keep comments useful and minimal. Explain why, not what.
- Follow rustfmt.
- Keep clippy warnings under control. Do not leave new warnings behind.

## Project size and repository hygiene

- Keep the source repository lightweight.
- Do not commit `target/`.
- Do not commit generated binaries.
- Do not commit build artifacts.
- Do not commit caches.
- Do not commit large test fixtures unless absolutely necessary.
- Avoid dependencies that significantly increase project size without strong justification.
- Keep the final source project under the established file-count and repository-size goals.

Current targets:

- Maximum meaningful project files: 120
- Target repository size: below 15 MB
- AI workspace ceiling: 128 MB

These are constraints, not targets to intentionally fill.
