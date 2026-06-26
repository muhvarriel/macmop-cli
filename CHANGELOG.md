# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
