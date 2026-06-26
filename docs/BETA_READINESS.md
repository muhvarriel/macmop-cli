# Beta Readiness Checklist

## v0.2.0-beta.1 Gates ✅

### 1. Code Quality & Linting
- [x] Code formatting passes `cargo fmt --check`.
- [x] No clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- [x] All tests pass locally and in CI.
- [x] Clean Architecture: Modules separate from executor mutations.

### 2. Safety Posture
- [x] `--apply` always defaults to moving to `.Trash`.
- [x] `--permanent` requires `--force` and is blocked on startup/protect/privacy/maintenance.
- [x] No sudo is required or used in core commands.
- [x] Revalidation of target paths is performed right before moves (to prevent race condition symlink attacks).

### 3. Core Capabilities
- [x] `cleanup` (Cache, Logs, Derived Data).
- [x] `startup` (LaunchAgent Enable/Disable).
- [x] `protect` (LaunchAgent Quarantine/Restore).
- [x] `privacy` (Browser Cache / Recent items only, shell history preserved).
- [x] `maintenance` (flush_dns only).
- [x] `rollback` (grouped Transaction-level restoration).
- [x] `status` (sampled debug metadata).

### 4. Release Engineering
- [x] GitHub Actions release workflow configured for `v*` tags.
- [x] Tarball packaging scripts tested and functional.
- [x] SHA256 checksums automatically generated on tag release.
- [x] Homebrew local tap validation verified (`brew test` and `brew audit` PASS).

### 5. Documentation
- [x] README safety model, setup commands, and install guides are up-to-date.
- [x] Security policy (`SECURITY.md`) is documented.
- [x] Issue templates (Bug Report, Feature Request) are in place.

---

## v0.2.0-beta.2 Gates

### 1. Release Engineering
- [ ] Matrix release workflow: `aarch64-apple-darwin` + `x86_64-apple-darwin` both build and package.
- [ ] Source archive packaged once in publish job (canonical SHA).
- [ ] x86_64 archive layout verified via `file` smoke test.
- [ ] Release workflow verified on fresh tag push.

### 2. TUI UX
- [ ] `Esc` in detail view backs to sidebar (not quit).
- [ ] Footer hint in detail view shows `[Esc/Backspace] Back`.
- [ ] `q` always quits from any view.

### 3. Documentation
- [ ] README install section uses validated `brew tap muhvarriel/macmop` + `brew install muhvarriel/macmop/macmop` command.
- [ ] Binary download table lists both aarch64 and x86_64 artifacts.
- [ ] `cargo install --git` source install documented.

### 4. Formula (Phase 2)
- [ ] Formula URL updated to beta.2 Release asset (after publish).
- [ ] SHA256 pinned to real downloaded artifact.
- [ ] `brew reinstall --build-from-source` + `brew test` + `brew audit --strict` PASS.
