# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-alpha.13] - 2026-06-26

### Added
- Workflow triggers for tag pushes (`v*`) and manual runs (`workflow_dispatch`).
- Explicitly pinned runner image to `macos-15` for CI validation.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.12] - 2026-06-26

### Added
- GitHub Actions CI workflow configuration in `.github/workflows/ci.yml`.
- Enabled caching using `Swatinem/rust-cache` and explicit component installations (`rustfmt`, `clippy`).
- Isolated test environment variables in the CI environment.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.11] - 2026-06-26

### Added
- Release validation script at `scripts/release/check.sh`.
- Release archive and checksum script at `scripts/release/package.sh`.
- README release checklist and Homebrew SHA guidance.

### Changed
- Updated Homebrew formula draft placeholders for the alpha.11 tagged source archive.
- No CLI behavior, JSON schema, module logic, or runtime dependency changes.

## [0.1.0-alpha.10] - 2026-06-26

### Added
- Completed Cargo package metadata for repository, homepage, keywords, and categories.
- Added MIT `LICENSE` file.
- Added release archive and draft Homebrew formula guidance.

### Changed
- Updated Homebrew formula draft placeholders for the alpha.10 tagged source archive.
- No CLI behavior, JSON schema, module logic, or runtime dependency changes.

## [0.1.0-alpha.9] - 2026-06-26

### Added
- **`status` command** (read-only): support/debug summary for version, schema, configured paths, audit/rollback state, available modules, and sampled home storage metadata.
- **New core types**: `StatusSummary` and `HomeSummary`.
- Bounded home traversal for status output: max depth 3 and max 10,000 visited entries.

### Changed
- No executor, audit write, rollback write, command behavior, or JSON schema changes.

## [0.1.0-alpha.8] - 2026-06-26

### Added
- Source install documentation for `cargo install --path .`.
- Release build and version verification documentation.
- Temp-root install verification workflow for safe local distribution checks.
- Draft Homebrew formula at `Formula/macmop.rb` with explicit placeholder URL and SHA256.

### Changed
- No CLI behavior, JSON schema, command surface, or runtime dependency changes.

## [0.1.0-alpha.7] - 2026-06-26

### Changed
- Split the large internal `src/modules.rs` implementation into per-module files under `src/modules/`.
- Preserved public paths such as `macmop::modules::cleanup::run`.
- No CLI behavior, JSON schema, command surface, or safety-policy changes.

## [0.1.0-alpha.6] - 2026-06-26

### Added
- **`maintenance` module** (report-only): safe maintenance task catalog and preflight metadata.
  - `maintenance list`: list supported future maintenance tasks without probing or execution.
  - `maintenance check`: lightweight read-only availability checks for future tasks.
- **New core type**: `MaintenanceTask` with `id`, `category`, `name`, `description`, `risk`, `requires_sudo`, `available`, `reason`, `future_action`, `execution_supported`, `action = report_only`.
- Supported catalog entries: `flush_dns`, `rebuild_spotlight`, `thin_time_machine_snapshots`, `rotate_logs`.
- Safety guarantees:
  - No system commands are executed.
  - No sudo is requested.
  - No audit or rollback files are created by maintenance preflight.
  - `execution_supported = false` for every maintenance task in alpha.6.
- README privacy note now explicitly states shell history contents are never read or emitted.

## [0.1.0-alpha.5] - 2026-06-26

### Added
- **`privacy` module** (report-only): read-only privacy artifact metadata inventory.
  - `privacy scan`: scan all categories — browser caches, recent items, QuickLook cache, and shell history.
  - `privacy browsers`: scan browser cache directories only (Safari, Chrome, Firefox, Firefox profile `cache2`).
  - `privacy recent`: scan recent items artifacts and shell history files only.
- **New core type**: `PrivacyFinding` with `id`, `category`, `path`, `size_bytes`, `count`, `detail`, `action = report_only`.
- **Test seam**: `quicklook_dirs: Vec<PathBuf>` added to `AppPaths` for injecting QuickLook cache directories in tests.
- **Safety guarantees**:
  - Shell history is detected as metadata only (path, size, existence) — **content is never read or emitted**.
  - Recent items artifacts reported as metadata only — **contents not parsed or printed**.
  - Permission-denied errors yield warnings, not crashes.
- **7 new integration tests** in `tests/integration_privacy.rs` (47 total).

## [0.1.0-alpha.4] - 2026-06-26

### Added
- **`protect` module** (report-only): read-only suspicious persistence findings analysis.
  - `protect scan` & `protect startup`: scan LaunchAgents and LaunchDaemons startup items and run security heuristic analysis.
  - `protect inspect <finding-id>`: display detailed threat score and evidence for a specific persistence finding.
- **Deterministic Finding IDs**: `protect_startup_<short_hash>` based on label, path, and rule name for reliability across runs.
- **Threat Heuristic Scoring**:
  - Shell interpreters as launcher binaries (`sh`, `bash`, `zsh`, `python`, etc.) -> `medium` or `high` severity findings.
  - Missing executable paths (validated against absolute paths) -> `high` severity findings.
  - Normal system items -> `low` severity baseline findings to separate threat risk from protection status.
  - Benign items -> `0` findings (silent by default).
- **Warnings Propagation**: Propagates scan-level malformed plist warnings to the JSON/NDJSON envelopes.
- **7 integration tests** in `tests/integration_protect.rs` (40 total).

## [0.1.0-alpha.3] - 2026-06-26

### Added
- **`startup` module** (report-only): LaunchAgent and LaunchDaemon inventory.
  - `startup list`: scan `~/Library/LaunchAgents`, `/Library/LaunchAgents`, `/Library/LaunchDaemons`.
  - `startup inspect <label>`: deep report for a single startup item by its Label.
- **New core type**: `StartupItem` with `label`, `program`, `program_arguments`, `run_at_load`, `keep_alive`, `source`, `is_system_item`, `risk`, `warnings`, `action`.
- **`startup_dirs: Vec<(PathBuf, String)>`** added to `AppPaths` with source labels for test injection.
- **Resilient plist parsing**: malformed plists become scan-level warnings; missing `Label` key falls back to filename stem with item-level warning.
- **KeepAlive polymorphism**: handles both `bool` and condition-dict variants.
- **Risk model**: `system_launch_daemons`/`system_launch_agents` → `critical`; user agents with `run_at_load=true` → `medium`; others → `low`. All `action = report_only`.
- **7 new integration tests** in `tests/integration_startup.rs` (33 total).

## [0.1.0-alpha.2] - 2026-06-26

### Added
- **`apps` module** (report-only): read-only app inventory and leftover detection.
  - `apps list`: discover installed `.app` bundles from `/Applications` and `~/Applications`.
  - `apps inspect <app>`: read `Info.plist` metadata and enumerate associated files across 8 standard Library paths.
  - `apps leftovers`: report likely orphaned files in `~/Library` where the source `.app` no longer exists.
- **New core types**: `AppBundle`, `AppAssociation`, `AppLeftover`, `LeftoverConfidence`.
- **Test seam**: `MACMOP_APPS_DIRS` (colon-separated) overrides app discovery directories when `MACMOP_TEST_MODE=1`.
- **plist** crate added for binary/XML `Info.plist` parsing.
- **5 new integration tests** in `tests/integration_apps.rs` (26 total).
- Safety: all `apps` actions are `report_only`; system/Apple apps (`com.apple.*`) marked `is_system_app=true`, `risk=critical`.

## [0.1.0-alpha.1] - 2026-06-26

### Added
- **Core Engine**: Safe cleanup path, ActionPlan boundary validation, executor mutation isolation.
- **Safety-First CLI Architecture**: Dry-run mode by default, trash-by-default logic, audit logging, and rollback entries.
- **Modules**:
  - `cleanup`: junk cleanup for caches, logs, Xcode, temp files with allowlisted roots and protected path constraints.
  - `duplicates`: staged duplicate hashing to detect exact duplicate groups.
  - `clutter`: find large files and Downloads clutter.
  - `disk`: storage space usage map.
  - `scan`: smart scanning (forced dry-run).
  - `report last`: view audit logs.
  - `rollback list`/`apply`: restore trashed items.
- **Test Seams**: Thread-safe isolated test environment variable overrides enabled by `MACMOP_TEST_MODE=1`.
- **Validation**: 21 unit and integration tests passing in parallel.
