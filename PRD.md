# PRD — MacMop CLI

**Product Name:** MacMop CLI
**CLI Command:** `macmop`
**Product Type:** Terminal-based macOS maintenance, cleanup, security, storage analysis, and optimization utility
**Target Platform:** macOS 11+
**Primary Interface:** CLI + optional TUI
**Document Owner:** L7 Software Engineer / Product Engineering
**Status:** Draft v1.1
**Last Updated:** 2026-06-26

---

## 1. Revision Notes

### v1.1 Changes

* Renamed product codename from `tidymac` to **MacMop CLI**.
* Renamed binary command from `tidymac` to `macmop`.
* Updated config path from `~/.config/tidymac` to `~/.config/macmop`.
* Updated all CLI examples.
* Added naming safety note.
* Preserved original product scope: terminal-based Mac maintenance inspired by modern Mac cleaner apps.
* Reinforced clean-room implementation requirement.

---

## 2. Executive Summary

**MacMop CLI** adalah aplikasi terminal-based untuk membantu pengguna macOS membersihkan storage, menemukan file besar/duplikat, menghapus cache aman, menganalisis penggunaan disk, mengelola aplikasi, mengecek startup/background items, menjalankan maintenance tasks, serta melakukan basic malware/privacy scanning.

Produk ini ditujukan untuk power user, software engineer, IT support, macOS admin, dan user teknis yang ingin workflow pembersihan Mac yang transparan, scriptable, dan aman.

MacMop CLI mengambil inspirasi dari kategori produk Mac maintenance modern, tetapi harus dibangun dengan pendekatan **clean-room implementation**. Produk tidak boleh menyalin brand, UI, copywriting, aset, signature database, logic proprietary, atau behavior internal aplikasi lain.

---

## 3. Problem Statement

Pengguna macOS sering mengalami:

* Storage penuh karena cache, logs, Downloads, Xcode junk, simulator data, old iOS backups, dan duplicate files.
* Sulit mengetahui folder mana yang paling banyak memakan storage.
* App uninstall sering menyisakan leftover files.
* Login items dan background services menumpuk.
* Developer machine penuh artifacts dari Xcode, Docker, Node.js, pnpm, npm, Gradle, CocoaPods, Homebrew, dan simulator.
* Existing GUI cleaner kurang cocok untuk automation, remote SSH, scripting, dan workflow terminal.
* Banyak cleaner terlalu agresif dan tidak transparan soal file yang dihapus.

---

## 4. Product Goals

### 4.1 User Goals

* Membersihkan storage Mac dengan aman.
* Mengetahui file/folder mana yang paling besar.
* Menghapus cache, logs, temp files, dan dev artifacts dengan kontrol penuh.
* Menemukan duplicate files.
* Menghapus aplikasi dan leftover files.
* Melihat startup/background items.
* Menjalankan maintenance task macOS dari terminal.
* Mendapat laporan cleanup yang bisa dibaca manusia dan mesin.

### 4.2 Engineering Goals

* Semua destructive action harus mendukung `--dry-run`.
* Default action untuk delete adalah move to Trash, bukan permanent delete.
* Permanent delete hanya boleh dengan `--permanent --force`.
* Semua action harus menghasilkan audit log.
* Semua scanner harus modular.
* Output harus mendukung `table`, `json`, dan `ndjson`.
* Core features harus bisa berjalan tanpa network.
* Root/sudo hanya digunakan jika benar-benar diperlukan.
* Produk harus aman untuk automation di script dan CI machine.

---

## 5. Non-Goals

MacMop CLI bukan:

* Full antivirus replacement.
* Tool untuk bypass SIP, Gatekeeper, sandbox, atau macOS privacy protection.
* Tool untuk menghapus file sistem kritikal.
* GUI clone dari aplikasi cleaner lain.
* Clone brand, logo, copywriting, UI, atau proprietary database aplikasi lain.
* Tool cloud backup.
* Enterprise MDM replacement.
* Aggressive “speed booster” yang membuat klaim palsu.
* Tool yang menghapus user documents tanpa explicit opt-in.

---

## 6. Target Users

| Persona           | Need                                                         |
| ----------------- | ------------------------------------------------------------ |
| Software Engineer | Bersihkan Xcode, Docker, node_modules, package manager cache |
| IT Support        | Jalankan cleanup script di banyak Mac                        |
| macOS Admin       | Audit, report, standardized cleanup                          |
| Power User        | Terminal-first maintenance                                   |
| Designer/Creator  | Cari file besar, media lama, dan duplicate files             |
| Casual Mac User   | Gunakan interactive TUI tanpa perlu hafal command            |

---

## 7. Product Principles

1. **Safety-first**
   Tidak ada destructive action tanpa preview atau explicit confirmation.

2. **Explainable cleanup**
   Setiap file yang direkomendasikan untuk dihapus harus punya alasan.

3. **Dry-run everywhere**
   Semua module yang destructive wajib mendukung `--dry-run`.

4. **Trash by default**
   File dipindahkan ke Trash, bukan langsung dihapus permanen.

5. **Local-first**
   Tidak upload file content, metadata, atau report tanpa opt-in.

6. **Scriptable**
   Output bisa diproses dengan `jq`, shell script, cron, launchd, dan CI.

7. **Respect macOS boundaries**
   Tidak mencoba bypass SIP, TCC, sandbox, atau permission system.

8. **No scareware behavior**
   Tidak boleh menggunakan copywriting yang menakut-nakuti user.

---

# 8. Product Scope

## 8.1 MVP Modules

MVP terdiri dari:

1. `scan` — Smart Scan terminal workflow
2. `cleanup` — Safe junk cleanup
3. `clutter` — Large files, old files, downloads
4. `duplicates` — Duplicate file detection
5. `disk` — Terminal-based storage map
6. `apps` — App inventory and leftover detection
7. `startup` — Login/background item visibility
8. `privacy` — Browser/recent items cleanup
9. `protect` — Basic suspicious item scanner
10. `maintenance` — Safe macOS maintenance tasks
11. `report` — Cleanup and scan report
12. `rollback` — Restore trashed/quarantined/disabled items

---

# 9. Feature Requirements

---

## 9.1 Smart Scan

### Description

Single command untuk menjalankan scan lintas module: junk, clutter, duplicates, app leftovers, startup items, disk usage, privacy artifacts, protection findings, dan health checks.

### Commands

```bash
macmop scan
macmop scan --json
macmop scan --profile developer
macmop scan --include cleanup,clutter,apps
macmop scan --exclude duplicates,privacy
```

### Functional Requirements

* Menampilkan total reclaimable space.
* Menampilkan kategori cleanup.
* Menampilkan item count.
* Menampilkan risk level.
* Menampilkan recommended action.
* Tidak menghapus apa pun saat scan.
* Support `table`, `json`, dan `ndjson`.

### Scan Profiles

| Profile     | Purpose                                                    |
| ----------- | ---------------------------------------------------------- |
| `safe`      | Hanya low-risk cleanup candidates                          |
| `developer` | Xcode, Docker, Node.js, Homebrew, simulator, package cache |
| `creator`   | Large media, screenshots, downloads, duplicate files       |
| `privacy`   | Browser cache, recent items, app history                   |
| `deep`      | Scan lebih luas, tetap butuh confirmation                  |

### Acceptance Criteria

* `macmop scan` tidak melakukan delete.
* Output minimal berisi category, size, item count, risk, recommended action.
* Scan home directory dapat berjalan tanpa sudo.
* Exit code `0` jika scan sukses.
* Exit code non-zero jika terjadi fatal permission/config error.

---

## 9.2 Cleanup Module

### Description

Membersihkan file yang relatif aman dihapus: cache, logs, temporary files, trash, broken downloads, browser cache, mail attachments, dan developer artifacts.

### Commands

```bash
macmop cleanup
macmop cleanup --dry-run
macmop cleanup --apply
macmop cleanup --category cache,logs,temp
macmop cleanup --older-than 30d
macmop cleanup --trash
macmop cleanup --permanent --force
```

### Cleanup Categories

| Category         | Examples                                  | Default Risk |
| ---------------- | ----------------------------------------- | ------------ |
| User Cache       | `~/Library/Caches`                        | Low          |
| System Cache     | `/Library/Caches`                         | Medium       |
| Logs             | `~/Library/Logs`, diagnostic logs         | Low          |
| Temp Files       | `/tmp`, app temp folders                  | Low          |
| Trash            | User Trash                                | Low          |
| Mail Attachments | Local mail downloads                      | Medium       |
| Browser Cache    | Safari, Chrome, Firefox cache             | Low          |
| Broken Downloads | `.download`, `.crdownload`, partial files | Low          |
| Xcode Junk       | DerivedData, Archives, iOS DeviceSupport  | Medium       |
| Simulator Junk   | unavailable runtimes, logs, temp data     | Medium       |
| Node Artifacts   | npm/pnpm/yarn cache                       | Medium       |
| Docker Artifacts | stopped containers, build cache           | High         |
| Homebrew Cache   | bottles/cache                             | Low          |

### Safety Rules

* Default mode is `--dry-run`.
* `--apply` moves files to Trash.
* `--permanent` requires `--force`.
* System-owned files require explicit permission and explanation.
* Never delete protected paths by default.
* Never delete credentials, keychains, SSH keys, password manager data, or user documents.

### Acceptance Criteria

* User can preview every cleanup candidate.
* Cleanup result includes before/after size.
* Failed deletion does not abort the full run unless `--strict` is enabled.
* App writes audit log for every attempted destructive action.

---

## 9.3 Clutter Module

### Description

Mencari file besar, file lama, Downloads clutter, screenshots lama, disk images, installers, archives, dan file yang kemungkinan tidak dibutuhkan.

### Commands

```bash
macmop clutter
macmop clutter --large
macmop clutter --old --older-than 180d
macmop clutter --downloads
macmop clutter --installers
macmop clutter --screenshots
macmop clutter --min-size 500MB
```

### Functional Requirements

* Sort by size, age, extension, folder, dan last opened date.
* User bisa select item untuk move to Trash.
* Support interactive picker.
* Support exclude pattern.
* Support export report.

### Acceptance Criteria

* Bisa scan home directory tanpa root.
* Tidak follow symlink secara default.
* Permission denied harus ditampilkan sebagai warning, bukan crash.
* JSON output harus mencantumkan path, size, modified date, last opened date jika tersedia.

---

## 9.4 Duplicate Finder

### Description

Mendeteksi file duplikat menggunakan staged hashing agar cepat dan hemat resource.

### Commands

```bash
macmop duplicates
macmop duplicates ~/Downloads ~/Documents
macmop duplicates --strategy fast
macmop duplicates --strategy strict
macmop duplicates --min-size 10MB
macmop duplicates --apply
```

### Detection Strategy

1. Group by file size.
2. Group by partial hash.
3. Group by full hash.
4. Optional metadata comparison.
5. Suggest keep/delete candidates.

### Functional Requirements

* Default tidak menghapus apa pun.
* Selalu menyisakan minimal satu original.
* Auto-select duplicate candidates hanya jika confidence tinggi.
* Support ignore patterns.
* Support extension filter.
* Support cloud folder report-only mode.

### Safety Rules

Do not auto-delete files inside:

```text
~/Pictures/Photos Library.photoslibrary
~/Library/Mobile Documents
Dropbox
Google Drive
OneDrive
Git repositories
App bundles
```

### Acceptance Criteria

* Duplicate detection akurat untuk binary-identical files.
* Tidak ada duplicate group yang semua filenya terpilih untuk delete.
* Untuk 100k files, scanner harus streaming dan tidak load semua file content ke memory.

---

## 9.5 Disk Map Module

### Description

Terminal-based storage visualization untuk melihat folder dan file paling besar.

### Commands

```bash
macmop disk
macmop disk ~
macmop disk --depth 3
macmop disk --tree
macmop disk --json
macmop disk --top 50
```

### Example Output

```text
~/Library
├── Developer                 82.4 GB
│   ├── Xcode                 51.2 GB
│   └── CoreSimulator         22.7 GB
├── Caches                    18.9 GB
├── Application Support       14.2 GB
└── Logs                       1.1 GB
```

### Functional Requirements

* Interactive drill-down in TUI.
* Sort by size.
* Configurable max depth.
* Skip restricted folders unless permission granted.
* Show apparent size and disk usage if available.

### Acceptance Criteria

* `macmop disk ~ --depth 3` works without admin permission.
* Output readable in 80-column terminal.
* JSON output includes path, size, file count, folder count.

---

## 9.6 Application Manager

### Description

Mengelola aplikasi terinstall, mendeteksi leftover files, app size, app metadata, login helper, dan related app data.

### Commands

```bash
macmop apps
macmop apps list
macmop apps leftovers
macmop apps uninstall "Example.app"
macmop apps inspect "Example.app"
```

### Functional Requirements

* List apps from:

  * `/Applications`
  * `~/Applications`
  * custom paths
* Detect app bundle size.
* Detect bundle ID and version.
* Detect associated files:

  * Preferences
  * Caches
  * Application Support
  * Logs
  * Containers
  * Saved Application State
  * LaunchAgents
  * Login items
* Uninstall flow interactive by default.
* App removal moves `.app` and associated files to Trash.

### Safety Rules

* Do not uninstall Apple system apps.
* Do not remove shared frameworks without explicit review.
* Do not delete files used by multiple apps unless confidence is high.
* Show all paths before deletion.

### Acceptance Criteria

* `macmop apps leftovers` can find orphaned files after app deletion.
* `macmop apps inspect` shows bundle ID, version, size, and associated files.
* Uninstall requires confirmation unless `--yes` is passed.

---

## 9.7 Startup & Background Items

### Description

Menampilkan dan mengelola startup/login/background items.

### Commands

```bash
macmop startup
macmop startup list
macmop startup disable <item-id>
macmop startup enable <item-id>
macmop startup rollback <rollback-id>
```

### Sources

* Login Items
* LaunchAgents
* LaunchDaemons
* Background helper apps
* User-level startup scripts

### Functional Requirements

* Show item name, path, vendor, status, source, risk.
* Disable should prefer reversible method.
* Create backup before modifying plist or launch item.
* Do not disable Apple/system critical services.

### Acceptance Criteria

* User can list startup items without root.
* Disabling user-level LaunchAgent creates rollback entry.
* `macmop startup rollback <id>` restores previous state.

---

## 9.8 Privacy Cleanup

### Description

Membersihkan privacy artifacts seperti browser cache, recent files, app recent documents, QuickLook cache, dan shell history report.

### Commands

```bash
macmop privacy
macmop privacy --browser safari,chrome,firefox
macmop privacy --recent-items
macmop privacy --dry-run
macmop privacy --apply
```

### Functional Requirements

* Browser cache cleanup.
* Browser download history cleanup if supported and permission granted.
* Recent items cleanup.
* App recent documents cleanup.
* QuickLook cache cleanup if safe.
* Shell history report, but no deletion by default.

### Safety Rules

* Do not delete saved passwords.
* Do not delete cookies by default.
* Do not delete browser profile.
* Shell history deletion requires explicit category selection.

### Acceptance Criteria

* Browser cleanup works only when browser is closed or user confirms force mode.
* App explains privacy impact before applying action.
* No credential stores are modified.

---

## 9.9 Protection Module

### Description

Basic suspicious item scanner untuk mendeteksi adware-like behavior, suspicious persistence, unsigned launch items, browser hijacker indicators, dan known suspicious path patterns.

### Commands

```bash
macmop protect
macmop protect scan
macmop protect scan --quick
macmop protect scan --deep
macmop protect quarantine <finding-id>
macmop protect restore <quarantine-id>
```

### Functional Requirements

* Scan common persistence locations:

  * LaunchAgents
  * LaunchDaemons
  * Browser extensions
  * Login items
  * `/Applications`
  * `~/Library/Application Support`
* Detect suspicious traits:

  * Unknown developer
  * Unsigned binary
  * Recently added persistence
  * Obfuscated launch command
  * Known adware path pattern
  * Browser hijacker indicators
* Quarantine instead of delete.
* Generate explainable findings.

### Non-Claims

* Product must not claim to be a full antivirus.
* Product must not guarantee complete malware removal.
* Product should describe this module as basic protection and suspicious item scanning.

### Acceptance Criteria

* Every finding includes severity, evidence, path, and recommended action.
* Quarantine is reversible.
* No system binary is quarantined without explicit override.

---

## 9.10 Maintenance Module

### Description

Menjalankan maintenance tasks yang aman dan familiar untuk macOS.

### Commands

```bash
macmop maintenance
macmop maintenance run flush-dns
macmop maintenance run rebuild-spotlight
macmop maintenance run purge-memory
macmop maintenance run thin-snapshots
```

### Supported Tasks

| Task                      | Description                                  | Risk   |
| ------------------------- | -------------------------------------------- | ------ |
| `flush-dns`               | Flush DNS cache                              | Low    |
| `rebuild-spotlight`       | Reindex Spotlight selected path              | Medium |
| `thin-snapshots`          | Thin local Time Machine snapshots            | Medium |
| `purge-memory`            | Ask system to reclaim memory where supported | Low    |
| `repair-home-permissions` | Detect/fix selected user permission issues   | Medium |
| `rotate-logs`             | Trigger safe log cleanup                     | Low    |

### Safety Rules

* Tasks requiring sudo must explain why.
* Do not run undocumented destructive commands.
* Do not disable system protection.
* Do not change kernel parameters.

### Acceptance Criteria

* Each maintenance task has pre-check and post-check.
* User sees exact command or API action before execution.
* Failures include actionable remediation.

---

## 9.11 Cloud Storage Analyzer

### Description

Analyze local synced folders for cloud providers. Default behavior is report-only.

### Commands

```bash
macmop cloud
macmop cloud scan
macmop cloud scan --provider icloud,dropbox,gdrive,onedrive
macmop cloud offload-candidates
```

### Functional Requirements

* Detect local cloud sync folders.
* Report large synced files.
* Report duplicate files within cloud folder.
* Report local-only vs cloud-only status where possible.
* Never delete cloud files by default.
* Provide offload suggestions where macOS supports it.

### Safety Rules

* Cloud cleanup default is read-only.
* Any delete/offload action requires explicit provider confirmation.
* Warn user that deleting local synced file may delete cloud copy.

### Acceptance Criteria

* Cloud module never deletes files in default mode.
* Report clearly marks synced folder risk.
* User must type provider name to confirm destructive action.

---

## 9.12 Status & Monitor Mode

### Description

Karena produk terminal-based, pengganti menu bar adalah optional local monitor daemon dan CLI status command.

### Commands

```bash
macmop status
macmop monitor start
macmop monitor stop
macmop monitor config
```

### Monitored Metrics

* Disk free space
* Trash size
* Cache growth
* Memory pressure
* CPU load
* Battery health summary
* Startup item changes
* New persistence item detection

### Requirements

* Daemon disabled by default.
* User must explicitly enable.
* No telemetry upload by default.
* Writes local alerts to terminal notification or log.

### Acceptance Criteria

* `macmop status` runs without daemon.
* Daemon has clear install/uninstall command.
* User can inspect all files created by daemon.

---

# 10. CLI UX Specification

## 10.1 Global Command Shape

```bash
macmop <module> <action> [options]
```

## 10.2 Global Flags

```bash
--dry-run              Preview only
--apply                Apply recommended action
--yes                  Skip confirmation for script mode
--json                 JSON output
--ndjson               Stream newline-delimited JSON
--table                Human-readable table output
--verbose              Detailed logs
--quiet                Minimal output
--log-file <path>      Custom log path
--config <path>        Use custom config
--profile <name>       Use scan profile
--exclude <pattern>    Exclude path or category
--include <pattern>    Include path or category
```

## 10.3 Interactive TUI

Launch TUI:

```bash
macmop
```

TUI sections:

1. Smart Scan
2. Cleanup
3. Clutter
4. Duplicates
5. Disk Map
6. Apps
7. Startup
8. Privacy
9. Protection
10. Maintenance
11. Reports
12. Rollback
13. Settings

### TUI Requirements

* Keyboard-first navigation.
* Optional Vim-style keybindings.
* Preview pane for selected file/action.
* Confirm destructive actions.
* Show total selected reclaimable space.
* Show risk level.
* Show rollback availability.

---

# 11. Information Architecture

## 11.1 ScanFinding

```json
{
  "id": "finding_01H...",
  "module": "cleanup",
  "category": "user_cache",
  "path": "/Users/alex/Library/Caches/com.example",
  "size_bytes": 104857600,
  "risk": "low",
  "confidence": 0.98,
  "action": "trash",
  "reason": "User cache older than 30 days",
  "requires_sudo": false
}
```

## 11.2 ActionPlan

```json
{
  "id": "plan_01H...",
  "created_at": "2026-06-26T10:00:00Z",
  "total_items": 1282,
  "total_size_bytes": 18253611008,
  "actions": [
    {
      "finding_id": "finding_01H...",
      "action": "move_to_trash",
      "rollback_supported": true
    }
  ]
}
```

## 11.3 AuditLog

```json
{
  "id": "audit_01H...",
  "timestamp": "2026-06-26T10:01:00Z",
  "user": "alex",
  "command": "macmop cleanup --apply",
  "action": "move_to_trash",
  "path": "/Users/alex/Library/Caches/example",
  "size_bytes": 2048,
  "status": "success",
  "rollback_id": "rollback_01H..."
}
```

---

# 12. System Architecture

## 12.1 High-Level Components

```text
CLI/TUI Frontend
    ↓
Command Router
    ↓
Module Registry
    ↓
Scanner Engine
    ↓
Policy Engine
    ↓
Action Planner
    ↓
Executor
    ↓
Audit Logger / Rollback Manager
```

## 12.2 Core Packages

| Package               | Responsibility                           |
| --------------------- | ---------------------------------------- |
| `cli`                 | Argument parsing, command routing        |
| `tui`                 | Interactive terminal interface           |
| `core`                | Shared types, config, module registry    |
| `scanner`             | File traversal, metadata collection      |
| `policy`              | Safety rules and risk scoring            |
| `planner`             | Builds action plans                      |
| `executor`            | Executes trash/delete/quarantine actions |
| `audit`               | Logs all operations                      |
| `rollback`            | Restore trashed/quarantined items        |
| `modules/cleanup`     | Cleanup scanner                          |
| `modules/clutter`     | Large/old file scanner                   |
| `modules/duplicates`  | Duplicate detection                      |
| `modules/disk`        | Disk map                                 |
| `modules/apps`        | App manager                              |
| `modules/startup`     | Startup items                            |
| `modules/privacy`     | Privacy cleanup                          |
| `modules/protect`     | Protection scan                          |
| `modules/maintenance` | Maintenance tasks                        |
| `modules/cloud`       | Cloud folder analyzer                    |

---

# 13. Recommended Tech Stack

## Preferred Stack

* Language: Rust
* CLI: `clap`
* TUI: `ratatui`
* Async runtime: `tokio`
* Serialization: `serde`
* Hashing: `blake3` for fast duplicate detection
* Strict hash option: SHA-256
* Packaging: Homebrew tap, signed binary, notarized installer

## Why Rust

* Memory safety untuk file traversal dan destructive operations.
* Fast hashing untuk duplicate finder.
* Good static binary distribution.
* Strong type system untuk memisahkan scanner, policy, planner, dan executor.
* Cocok untuk terminal tooling dengan performance tinggi.

## Alternative Stack

| Language       | Pros                                   | Cons                                       |
| -------------- | -------------------------------------- | ------------------------------------------ |
| Go             | Simple static binary, fast development | Less native macOS integration              |
| Swift          | Native macOS APIs                      | CLI ecosystem lebih kecil                  |
| Python         | Fast prototype                         | Distribution dan permission lebih sulit    |
| TypeScript/Bun | Developer-friendly                     | Kurang ideal untuk deep filesystem utility |

---

# 14. Safety Architecture

## 14.1 Risk Levels

| Risk     | Meaning                                   | Default Behavior           |
| -------- | ----------------------------------------- | -------------------------- |
| Low      | Safe temporary/cache/log files            | Can be selected by default |
| Medium   | Potentially useful generated data         | Requires review            |
| High     | Could affect app/system behavior          | Never auto-selected        |
| Critical | Credentials, user documents, system files | Never delete               |

## 14.2 Delete Policy

Default hierarchy:

1. Report only
2. Dry-run
3. Move to Trash
4. Quarantine
5. Permanent delete only with `--force --permanent`

## 14.3 Protected Paths

Never delete by default:

```text
~/.ssh
~/Documents
~/Desktop
~/Pictures/Photos Library.photoslibrary
~/Library/Keychains
~/Library/Mobile Documents
~/Library/Group Containers/*password*
~/Library/Application Support/1Password
~/Library/Application Support/Bitwarden
~/Library/Application Support/iCloud
/System
/bin
/sbin
/usr/bin
/usr/sbin
```

## 14.4 Rollback

Rollback supported for:

* Move to Trash
* Quarantine
* Disabled startup items
* Modified plist backup
* App uninstall if files remain in Trash

Rollback not guaranteed for:

* Permanent delete
* External drive after unmount
* Third-party cloud sync side effects
* Files changed by other processes after cleanup

---

# 15. macOS Permissions

## 15.1 Permission Matrix

| Feature                        | Permission                       |
| ------------------------------ | -------------------------------- |
| Home folder scan               | Normal user permission           |
| Full disk scan                 | Full Disk Access recommended     |
| Mail attachments               | Full Disk Access may be required |
| Safari privacy cleanup         | Full Disk Access may be required |
| System logs                    | Sudo may be required             |
| LaunchDaemons modification     | Sudo required                    |
| Time Machine snapshot thinning | Sudo may be required             |
| System-path quarantine         | Sudo required                    |

## 15.2 Permission UX

Example:

```text
MacMop needs Full Disk Access to scan Mail attachments.

How to enable:
System Settings → Privacy & Security → Full Disk Access → Add macmop

Continue with limited scan? [Y/n]
```

---

# 16. Privacy & Security Requirements

## 16.1 Data Privacy

* No file content upload.
* No telemetry by default.
* No cloud sync by default.
* No third-party analytics in CLI.
* Reports stored locally.
* User can delete all MacMop data via:

```bash
macmop self clean-data
```

## 16.2 Security Requirements

* Binary should be signed and notarized.
* Update channel must verify signatures.
* Logs should support redaction.
* Reports should support `--redact`.
* Quarantine database should not expose sensitive paths in shared reports unless user opts in.

Command:

```bash
macmop report generate --redact
```

## 16.3 Threat Model

| Threat                      | Mitigation                                    |
| --------------------------- | --------------------------------------------- |
| Accidental deletion         | Dry-run, Trash default, rollback              |
| Malicious plugin            | Signed plugin policy, disabled by default     |
| Path traversal bugs         | Canonical path validation                     |
| Symlink attack              | Do not follow symlink by default              |
| TOCTOU delete issue         | Validate inode before execution               |
| Credential deletion         | Protected path denylist                       |
| Cloud data loss             | Cloud destructive actions disabled by default |
| Privilege escalation misuse | Minimal sudo scope, explicit prompts          |

---

# 17. Configuration

Default config path:

```bash
~/.config/macmop/config.toml
```

Example config:

```toml
[general]
default_output = "table"
trash_by_default = true
telemetry = false

[scan]
profile = "safe"
follow_symlinks = false
max_depth = 8

[cleanup]
older_than_days = 30
include_xcode = true
include_docker = false

[duplicates]
min_size = "10MB"
strategy = "strict"

[protected_paths]
paths = [
  "~/.ssh",
  "~/Documents",
  "~/Pictures/Photos Library.photoslibrary"
]
```

---

# 18. Reporting

## 18.1 Report Commands

```bash
macmop report last
macmop report generate --format markdown
macmop report generate --format json
macmop report generate --format html
macmop report diff <before> <after>
```

## 18.2 Report Content

* Machine info summary
* Scan date
* Modules scanned
* Reclaimable space
* Files moved to Trash
* Files quarantined
* Failed actions
* Permission limitations
* Rollback instructions
* Redacted mode

---

# 19. MVP Command Surface

```bash
macmop
macmop scan
macmop cleanup --dry-run
macmop cleanup --apply
macmop clutter
macmop duplicates
macmop disk
macmop apps list
macmop apps leftovers
macmop startup list
macmop privacy --dry-run
macmop protect scan
macmop maintenance
macmop report last
macmop rollback list
macmop rollback apply <id>
```

---

# 20. Example User Flows

## 20.1 Developer Cleanup

```bash
macmop scan --profile developer
macmop cleanup --category xcode,node,homebrew --dry-run
macmop cleanup --category xcode,node,homebrew --apply
macmop report last
```

Expected result:

* User sees reclaimable space from Xcode DerivedData, simulator logs, package manager cache, and Homebrew cache.
* User confirms deletion.
* Files move to Trash.
* Audit log is generated.

## 20.2 Find Storage Hog

```bash
macmop disk ~ --depth 3 --top 30
macmop clutter --large --min-size 1GB
```

Expected result:

* User sees top folders by size.
* User can inspect large files.
* No deletion happens unless explicitly applied.

## 20.3 Remove App Leftovers

```bash
macmop apps leftovers
macmop apps inspect "OldApp.app"
macmop apps uninstall "OldApp.app"
```

Expected result:

* App bundle and related files are displayed.
* User reviews associated files.
* Files move to Trash after confirmation.

## 20.4 Basic Security Check

```bash
macmop protect scan --quick
macmop protect quarantine finding_123
```

Expected result:

* Suspicious persistence items are listed.
* User can quarantine item.
* Restore is available.

---

# 21. Success Metrics

## 21.1 Product Metrics

| Metric                                       | Target                    |
| -------------------------------------------- | ------------------------- |
| Successful scan completion                   | >95%                      |
| Cleanup action success                       | >98% for user-owned files |
| False positive deletion reports              | <0.1%                     |
| User rollback usage                          | <3% of cleanups           |
| Median reclaimed space for developer profile | >5GB                      |
| Time to first useful result                  | <30 seconds               |
| Repeat weekly usage                          | >30% of active users      |

## 21.2 Engineering Metrics

| Metric                                | Target              |
| ------------------------------------- | ------------------- |
| Crash-free sessions                   | >99.9%              |
| Unit test coverage for policy engine  | >90%                |
| Integration test coverage for modules | >75%                |
| P95 scan time for 1M files            | <5 minutes          |
| Memory usage during scan              | <500MB              |
| Duplicate scan streaming correctness  | 100% on test corpus |

---

# 22. MVP Milestones

## Phase 0 — Research & Spike

Duration: 1–2 weeks

Deliverables:

* macOS path inventory
* Safety policy draft
* File traversal benchmark
* Trash/rollback prototype
* TUI feasibility prototype
* Permission behavior matrix

## Phase 1 — Core CLI

Duration: 3–4 weeks

Deliverables:

* `macmop scan`
* `macmop cleanup --dry-run`
* `macmop cleanup --apply`
* Audit log
* Config file
* JSON output
* Protected path policy

## Phase 2 — Storage Intelligence

Duration: 3–4 weeks

Deliverables:

* `macmop clutter`
* `macmop duplicates`
* `macmop disk`
* Interactive selection
* Report generation

## Phase 3 — App & Startup Management

Duration: 3–4 weeks

Deliverables:

* `macmop apps list`
* `macmop apps inspect`
* `macmop apps leftovers`
* `macmop startup list`
* `macmop startup disable`
* `macmop startup rollback`

## Phase 4 — Privacy, Protection, Maintenance

Duration: 4–6 weeks

Deliverables:

* `macmop privacy`
* `macmop protect scan`
* `macmop protect quarantine`
* `macmop maintenance`
* Permission helper UX

## Phase 5 — TUI & Packaging

Duration: 3–4 weeks

Deliverables:

* Full TUI
* Homebrew formula
* Signed binary
* Notarized binary
* Installer
* Documentation
* Release candidate

---

# 23. Risks

| Risk                          | Impact   | Mitigation                                        |
| ----------------------------- | -------- | ------------------------------------------------- |
| Accidental file deletion      | Critical | Trash default, dry-run, protected paths, rollback |
| macOS permission limitations  | High     | Clear permission UX, partial scan mode            |
| False malware positives       | High     | Quarantine not delete, explainable findings       |
| Slow scan on large disks      | Medium   | Streaming traversal, concurrency limits           |
| Cloud sync side effects       | High     | Report-only default, explicit confirmation        |
| User distrust of cleaner apps | High     | Transparent output, no scare copy, audit logs     |
| Breaking app data             | High     | Conservative policy engine                        |
| Sudo misuse                   | Critical | Minimal privileged operations                     |

---

# 24. Launch Criteria

MVP can launch when:

* Cleanup has zero known critical deletion bugs.
* Protected path policy has full test coverage.
* Trash and rollback are verified on APFS.
* Scan and cleanup work without sudo for user-owned files.
* JSON output is stable and documented.
* Homebrew install works.
* Binary is signed and notarized.
* Documentation includes safety model.
* At least 20 real-world beta machines tested.
* At least 5 developer-heavy macOS profiles tested.
* All destructive commands support `--dry-run`.

---

# 25. Definition of Done

A feature is done when:

* CLI command implemented.
* Dry-run supported.
* JSON output supported.
* Unit tests added.
* Integration tests added using fixture directories.
* Audit log written.
* Permission errors handled gracefully.
* Documentation updated.
* Safety review completed.
* Manual QA completed on clean macOS account and developer-heavy macOS account.

---

# 26. Naming & Legal Notes

The name **MacMop CLI** is selected as a working product name and command namespace.

Before public launch:

* Run trademark clearance.
* Check Homebrew formula availability.
* Check GitHub org/repo availability.
* Check npm/crates.io/package namespace if relevant.
* Avoid using competitor names in marketing copy.
* Avoid “CleanMyMac clone” language publicly.
* Use positioning such as:

  * “Terminal-based Mac maintenance toolkit”
  * “Safety-first macOS cleanup CLI”
  * “Developer-focused Mac cleanup utility”

---

# 27. Suggested README Snippet

```bash
# Install
brew install macmop

# Preview cleanup
macmop cleanup --dry-run

# Apply safe cleanup
macmop cleanup --apply

# Developer cleanup
macmop scan --profile developer

# Find duplicate files
macmop duplicates ~/Downloads ~/Documents

# See disk usage
macmop disk ~ --depth 3

# Inspect app leftovers
macmop apps leftovers
```

MacMop CLI is safety-first. It previews before deleting, moves files to Trash by default, and keeps an audit log for every destructive action.

---

# 28. Appendix: Module Name Mapping

| Product Concept  | CLI Module    |
| ---------------- | ------------- |
| Smart Scan       | `scan`        |
| Cleanup          | `cleanup`     |
| File Clutter     | `clutter`     |
| Duplicate Finder | `duplicates`  |
| Disk Map         | `disk`        |
| Applications     | `apps`        |
| Startup Items    | `startup`     |
| Privacy          | `privacy`     |
| Protection       | `protect`     |
| Maintenance      | `maintenance` |
| Cloud Analyzer   | `cloud`       |
| Reports          | `report`      |
| Rollback         | `rollback`    |
| Status           | `status`      |
| Monitor Daemon   | `monitor`     |
