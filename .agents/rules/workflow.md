# Workflow Rules

## Development workflow

1. Read the relevant project documentation before changing code.
2. Inspect the existing implementation before modifying it.
3. Identify the smallest set of files required.
4. Implement only the requested phase or task.
5. Do not implement future phases.
6. Run formatting.
7. Run compilation.
8. Run relevant tests.
9. Review the diff, confirming the repository hygiene rules in `coding.md`.
10. Report exactly what changed.
11. Stop when the requested task is complete.

## Validation commands

```
cargo fmt --check
cargo check
cargo test
cargo clippy
```

## Honesty

- Do not hide compiler or clippy warnings.
- Do not claim a test passed unless it was actually executed.
- If a check could not be run, say so instead of implying success.
- Report pre-existing failures accurately; do not fix unrelated code to make a phase look successful.
