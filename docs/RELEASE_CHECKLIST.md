# TerminalVision Release Checklist

This document details the step-by-step procedure required before publishing a public release of TerminalVision.

---

## Pre-Release Validation

Run the complete verification suite locally:

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

All commands **must** pass with zero errors and zero warnings.

---

## Release Checklist

- [ ] **Working Directory Clean**: Confirm `git status` reports no untracked files or uncommitted changes.
- [ ] **Version Bump**: Verify `version` in `Cargo.toml` follows Semantic Versioning (`X.Y.Z`).
- [ ] **Documentation Sync**: Confirm `README.md`, `docs/ARCHITECTURE.md`, and `docs/ROADMAP.md` accurately reflect current features.
- [ ] **License Check**: Verify `LICENSE` (MIT) is present and copyright year/authors are correct.
- [ ] **Binary Inspection**: Build `./target/release/TerminalVision` and test interactive startup and `q` exit.
- [ ] **Clean Git Tag**: Create a Git tag corresponding to the version (e.g. `v0.1.0`).

---

## Manual GitHub Release Steps

The project maintainer performs the following steps to publish a release on GitHub:

1. **Tag the Release**:
   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   ```

2. **Push Branch and Tags**:
   ```bash
   git push origin main
   git push origin v0.1.0
   ```

3. **Create Release on GitHub**:
   - Navigate to **Releases** -> **Draft a new release** on GitHub.
   - Select tag `v0.1.0`.
   - Set title to `TerminalVision v0.1.0`.
   - Copy key release notes from `README.md`.
   - Attach compiled binaries (e.g., `TerminalVision-macOS-arm64`).
   - Click **Publish release**.
