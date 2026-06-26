# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-alpha.27] - 2026-06-26

### Added
- GitHub Actions release workflow on `v*` tag push.
- Automated release artifact packaging for macOS arm64 (`aarch64-apple-darwin`).
- Automated source archive packaging.
- SHA256 checksum generation for release artifacts.
- Draft prerelease publication to GitHub.

## [0.1.0-alpha.26] - 2026-06-26

### Added
- Multi-file transaction grouping under a single rollback transaction ID.
- Partial transaction audit trail consistency: aborted/interrupted runs log pending actions as `"cancelled"`.
- Flushed audit trails and rollback database immediately after each action.

### Changed
- Improved Ctrl+C safe interrupt logic to evaluate cancellation before and after each action rather than mid-move.

## [0.1.0-alpha.25] - 2026-06-26

### Added
- Guarded execution support for maintenance tasks: `macmop maintenance run flush_dns --apply` to flush DNS caches using user-level `/usr/bin/dscacheutil -flushcache`.
- Testing seam override `MACMOP_MAINTENANCE_DSCACHEUTIL` to test DNS flush behavior reliably.
- Clean execution tracking in `audit_file` (logs exit code, stdout/stderr lengths).

### Changed
- Blocked `--permanent` execution modes for all maintenance subcommands.
- Deferred execution support for spotlight rebuilding, log rotation, and Time Machine snapshot thinning.
- Clearly recorded rollback as `"not_reversible"` without creating rollback entries.

## [0.1.0-alpha.24] - 2026-06-26

### Added
- Mutation support for `macmop privacy browsers --apply` to safely clean up browser caches.
- Mutation support for `macmop privacy recent --apply` to clean up recent items.
- Strict path allowlists (browser caches and recent item plist files/folders only) to prevent arbitrary file matching.
- Run-time validation for target paths before moving to Trash.
- Support for process running warnings during browser cache cleanup.
- Full audit log and rollback support.

### Changed
- Blocked `privacy scan --apply` to prevent overreaching mutations.
- Blocked permanent deletion mode (`--permanent`) for the privacy module.
- Retained shell history files as strictly report-only.

## [0.1.0-alpha.23] - 2026-06-26

### Added
- Subcommands `macmop protect quarantine <finding-id>` and `macmop protect restore <quarantine-id>`.
- Reversible file quarantine into a MacMop-managed quarantine folder.
- Deterministic sidecar metadata tracking for quarantined files.
- Safe restore functionality with collision checks and strict path verification.
- Revalidation checks immediately before performing quarantine/restore moves.
- Full audit log and rollback support.

### Changed
- Blocked permanent deletion mode (`--permanent`) and system path quarantine/restore actions.

## [0.1.0-alpha.22] - 2026-06-26

### Added
- Subcommands `macmop startup disable <label>` and `macmop startup enable <label>`.
- Support for safely moving user LaunchAgents plists to a MacMop-managed disabled directory.
- Revalidation safety checks immediately before plist relocation.
- Collision avoidance using deterministic hashed filenames for disabled plists.
- Overwrite conflict prevention during plist enablement.
- Rollback entry creation and rollback support.

### Changed
- Blocked system LaunchAgents/LaunchDaemons, sudo, and permanent deletions.

## [0.1.0-alpha.21] - 2026-06-26

### Added
- Mutation support for `macmop apps uninstall <app> --apply` to move files to Trash.
- Immediate revalidation of leftover and app paths against policy before execution.
- Order-deliberate uninstall moving leftovers first, app bundle last.
- Rollback entry creation and rollback support for restoring uninstalled apps.
- Running-app warnings check.

### Changed
- Explicitly blocked `--permanent` execution mode for the apps uninstall command.

## [0.1.0-alpha.20] - 2026-06-26

### Added
- Dry-run only `macmop apps uninstall <app>` subcommand to pre-scan and plan app removal.
- Precise app identity resolution order (direct path, exact name, exact bundle, fuzzy match).
- Safety blocking checks for system and user protected paths during app resolution and candidate files.
- Ambiguity checks with capped listing formatting for matching apps.

### Changed
- No mutations are performed; action plans created are strictly dry-run previews.

## [0.1.0-alpha.19] - 2026-06-26

### Added
- TUI Cloud storage integration page.
- Cloud providers detail lists and warning indicators on the dashboard.
- Consolidated warnings panel with aggregated cloud pre-scan alerts.
- Read-only module text formatting polish.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.18] - 2026-06-26

### Added
- Cloud storage analyzer module (`macmop cloud`).
- Subcommands `macmop cloud providers` and `macmop cloud scan`.
- Cloud provider detection (iCloud Drive, Dropbox, Google Drive, OneDrive).
- Bounded file search with limits (max depth 3, max entries 10,000).
- Sync-deletion warnings regarding cloud/local sync risks.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.17] - 2026-06-26

### Added
- Config file support with `~/.config/macmop/config.toml` and `--config <path>`.
- Strict CLI > Env > Config > Defaults precedence.
- Subcommands `macmop config show` and `macmop config validate`.
- Support for `custom_protected_paths` added additively to safety checks.
- Validation checks for defaults profiles and output formats.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.16] - 2026-06-26

### Added
- Interactive read-only detail views for TUI module pages.
- Navigation state transitions between `Sidebar` navigation and scrollable `Detail` lists.
- Display-oriented, type-safe `TuiDetailItem` mapping for modular lists.
- Strict display limit cap (`TUI_DETAIL_LIMIT = 100`) to prevent interface performance lag.
- Regression tests to verify no audit/rollback file creations on TUI launches.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.15] - 2026-06-26

### Added
- Wired live data into TUI pages using cached `TuiData` loaded at startup.
- Displays live statistics for Status, Cleanup, Apps, Startup, Protect, Privacy, and Maintenance.
- Resilient design: failed module scans are reported as warnings and do not crash the TUI.

### Changed
- No product behavior, schema, or logic changes.

## [0.1.0-alpha.14] - 2026-06-26

### Added
- Interactive read-only TUI Dashboard (`macmop tui`).
- Sidebar navigation for all modules (Overview, Cleanup, Clutter, Disk, Apps, Startup, Protect, Privacy, Maintenance).
- Dynamic read-only status and overview panel in the TUI.
- Safe terminal restoration and raw mode guards.

### Changed
- No product behavior, schema, or logic changes.

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
