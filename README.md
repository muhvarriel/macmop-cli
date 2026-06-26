# MacMop CLI

Safety-first macOS cleanup and maintenance utility.

Repository: https://github.com/muhvarriel/macmop-cli

## Safety-First Architecture

MacMop CLI is designed from the ground up to prevent accidental data loss and system instability:
- **Dry-run by Default**: No mutations occur unless explicitly requested via the `--apply` flag.
- **Trash-first Model**: All deletions flow through reversible moves to the macOS native `.Trash` folder.
- **Transaction Rollback Engine**: Actions are grouped under a shared `RollbackId` per execution. Trashed items can be restored cleanly back to their exact original locations using the rollback CLI.
- **Strict Allowlisting & Validation**: Destructive actions are restricted to specific user directories (e.g., `~/Library/Caches`). Arbitrary paths and critical system paths (`/System`, `/Library`, `/Applications`) are blocked.
- **No Sudo Requirement**: MacMop operates entirely in user space. It never requests privilege escalation or executes commands as root.
- **Audit Logging**: Every filesystem mutation is logged sequentially with a timestamp, action name, target path, and outcome.

---

## Installation

### 1. Via Homebrew (Local Formula)

Install using the verified local tap:

```bash
brew tap local/macmop
# Point Formula/macmop.rb to the target release asset and install
brew install --build-from-source Formula/macmop.rb
```

### 2. From GitHub Release Binaries

Download the compiled binary for Apple Silicon macOS (`aarch64-apple-darwin`):

1. Download `macmop-v0.1.0-alpha.27-aarch64-apple-darwin.tar.gz` and its `.sha256` checksum from the GitHub Releases page.
2. Extract the archive:
   ```bash
   tar -xzf macmop-v0.1.0-alpha.27-aarch64-apple-darwin.tar.gz
   ```
3. Move the binary into your PATH (e.g., `/usr/local/bin` or `~/bin`).

### 3. From Source

If you have Rust installed, install the latest revision directly:

```bash
cargo install --path .
```

---

## Command Examples

### Dry-run Scans (Safe Previews)

```bash
# Scan system junk (caches, logs, derived data)
macmop cleanup

# Scan user LaunchAgents for security persistence issues
macmop protect scan

# Inspect privacy-related browser caches and recent items
macmop privacy scan
```

### Applying Mutations (Reversible Moves)

```bash
# Clean up caches and logs (moves to Trash)
macmop cleanup --apply

# Disable a user LaunchAgent (renames plist deterministically)
macmop startup disable com.example.helper

# Quarantine a suspicious agent
macmop protect quarantine protect_startup_abc123

# Clean recent files list and browser caches
macmop privacy recent --apply
macmop privacy browsers --apply

# Flush macOS DNS resolver cache (not reversible)
macmop maintenance run flush_dns --apply
```

### Rollbacks (Undo Actions)

```bash
# List all reversible transactions in the database
macmop rollback list

# Restore all files from a specific transaction
macmop rollback apply <rollback-id> --apply
```

---

## Known Limitations (Beta Readiness)

- **macOS only**: Relies on macOS directory layout (`~/Library`) and commands (e.g., `dscacheutil`). Not supported on Linux or Windows.
- **User space only**: Directories requiring root permission or system files (e.g., `/System`) are skipped or treated as report-only.
- **Non-reversible commands**: `maintenance run flush_dns` executes system DNS cache flushing and does not support rollback.
- **Duplicates module**: Finding duplicates is supported in read-only mode; deletion is not available.
- **TUI dashboard**: Interactive dashboard (`macmop tui`) is currently read-only.

---

## Security Policy

Please read [SECURITY.md](SECURITY.md) to report security vulnerabilities.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
