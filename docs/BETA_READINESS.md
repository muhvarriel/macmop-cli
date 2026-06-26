# Beta Readiness Checklist

This document tracks technical and documentation gates required before graduating MacMop to `v0.2.0-beta.1`.

## Beta Gates

### 1. Code Quality & Linting
- [ ] Code formatting passes `cargo fmt --check`.
- [ ] No clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- [ ] All tests pass locally and in CI.
- [ ] Clean Architecture: Modules separate from executor mutations.

### 2. Safety Posture
- [ ] `--apply` always defaults to moving to `.Trash`.
- [ ] `--permanent` requires `--force` and is blocked on startup/protect/privacy/maintenance.
- [ ] No sudo is required or used in core commands.
- [ ] Revalidation of target paths is performed right before moves (to prevent race condition symlink attacks).

### 3. Core Capabilities
- [ ] `cleanup` (Cache, Logs, Derived Data).
- [ ] `startup` (LaunchAgent Enable/Disable).
- [ ] `protect` (LaunchAgent Quarantine/Restore).
- [ ] `privacy` (Browser Cache / Recent items only, shell history preserved).
- [ ] `maintenance` (flush_dns only).
- [ ] `rollback` (grouped Transaction-level restoration).
- [ ] `status` (sampled debug metadata).

### 4. Release Engineering
- [ ] GitHub Actions release workflow configured for `v*` tags.
- [ ] Tarball packaging scripts tested and functional.
- [ ] SHA256 checksums automatically generated on tag release.
- [ ] Homebrew local tap validation verified (`brew test` and `brew audit` PASS).

### 5. Documentation
- [ ] README safety model, setup commands, and install guides are up-to-date.
- [ ] Security policy (`SECURITY.md`) is documented.
- [ ] Issue templates (Bug Report, Feature Request) are in place.
