# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
