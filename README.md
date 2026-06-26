# MacMop CLI

Safety-first macOS cleanup CLI.

## MVP Safety Rules

- Default mode is dry-run.
- Destructive actions must flow through `ActionPlan`.
- Modules produce findings; only `executor` mutates files.
- `--apply` moves files to Trash.
- `--permanent` requires `--force`.
- No sudo in core MVP. System paths are report-only.
- JSON output uses `schema_version: "1.0"`.

## Commands

```bash
macmop cleanup --dry-run
macmop cleanup --apply
macmop disk ~ --depth 3
macmop clutter ~/Downloads
macmop duplicates ~/Downloads ~/Documents
macmop report last
macmop rollback list
macmop rollback apply <id> --apply
macmop scan
macmop apps list
macmop apps inspect "Example.app"
macmop apps leftovers
macmop startup list
macmop startup inspect com.example.helper
macmop protect scan
macmop protect startup
macmop protect inspect protect_startup_abc123
```

## First-Time Setup

This project requires Cargo dependencies to be fetched once:

```bash
cargo fetch
cargo generate-lockfile
```

After dependencies are available locally, validation can run offline:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

## Validation

Preferred local workflow:

```bash
rtk cargo fmt --check
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test --all
```

Contributor fallback:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

## Safety Model & Test Overrides

MacMop CLI enforces strict safety policies to prevent accidental data loss:
- **Trash by Default**: Items removed during `cleanup` or other subcommands are moved to `.Trash` instead of permanent deletion.
- **Rollback Engine**: Rollback entries are saved under `~/.config/macmop/rollback` to revert files back to their original locations.
- **Dry-run by Default**: All scans and subcommands do not perform mutations unless run with `--apply` or `--permanent --force`.

### Test Environment Variables
When `MACMOP_TEST_MODE=1` is set, you can override standard directories to isolate runs (useful for integration tests and manual QA):
- `MACMOP_HOME`: Overrides the home directory.
- `MACMOP_DATA_DIR`: Overrides the CLI configuration and data directory.
- `MACMOP_TRASH_DIR`: Overrides the target Trash directory.
- `MACMOP_AUDIT_FILE`: Overrides the location of the audit log JSON.
- `MACMOP_ROLLBACK_FILE`: Overrides the location of the rollback database JSON.
- `MACMOP_APPS_DIRS`: Colon-separated list of directories to search for `.app` bundles (overrides `/Applications` and `~/Applications`).

## Alpha Limitations

This version (`v0.1.0-alpha.4`) is a preview release with several limitations:
- **macOS only**: Not verified on other operating systems.
- **No sudo support**: Will skip directories requiring root access.
- **No app uninstall**: Application leftovers can be reported, but bundle removal is disabled.
- **No TUI**: The TUI dashboard is not yet implemented.
- **Disjoint Cleanup Roots**: Scans are bounded to allowlisted cache, logs, and derived data paths only.
- **Missing Modules**: `privacy` module is not yet included in this preview.
- **Apps module is report-only**: `apps list`, `apps inspect`, `apps leftovers` are read-only; no deletion or uninstall.
- **Startup module is report-only**: `startup list` and `startup inspect` are read-only; no enable/disable support yet.
- **Protect module is report-only**: `protect scan`, `protect startup`, and `protect inspect` are read-only; no quarantine or deletion.


