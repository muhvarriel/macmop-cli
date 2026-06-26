# Manual QA Checklist

This document details the test scenarios that must be verified manually before cutting a public release.

## Scenarios to Verify

### 1. Dry-run Safety Guarantee
- [ ] Run `macmop cleanup`. Verify it displays file scans but creates **no** files in `.Trash` or `~/.config/macmop/rollback`.
- [ ] Run `macmop clutter ~/Downloads`. Verify no files are mutated.
- [ ] Run `macmop privacy scan`. Verify only logs are printed and no cookies/history are wiped.

### 2. Apply Mode & Trash
- [ ] Run `macmop cleanup --apply`. Verify matching files under `~/Library/Caches` are moved to the macOS Trash directory.
- [ ] Verify the files exist in the `.Trash` folder.
- [ ] Verify `~/.config/macmop/audit/last.json` logs the transactions as `"status": "success"`.

### 3. Bulk Rollback Engine
- [ ] Run `macmop rollback list`. Copy the latest `RollbackId`.
- [ ] Run `macmop rollback apply <id> --apply`.
- [ ] Verify that all files moved to Trash are restored back to their exact original locations.
- [ ] Verify that running the rollback twice does not crash (idempotency check).

### 4. Startup Disable / Enable
- [ ] Create a fake plist in `~/Library/LaunchAgents/com.test.agent.plist`.
- [ ] Run `macmop startup disable com.test.agent`. Verify plist is moved to `~/Library/LaunchAgents/disabled_launchagents/com.test.agent__<hash>.plist`.
- [ ] Run `macmop startup enable com.test.agent`. Verify plist is restored to the original location.

### 5. Protect Quarantine / Restore
- [ ] Run `macmop protect scan` and identify a LaunchAgent finding ID.
- [ ] Run `macmop protect quarantine <finding-id> --apply`. Verify it is moved to quarantine and sidecar metadata is written.
- [ ] Run `macmop protect restore <quarantine-id> --apply`. Verify it is cleanly restored.

### 6. Privacy & Browser Cleanup
- [ ] Run `macmop privacy browsers --apply`. Verify exact browser cache folders are moved to Trash.
- [ ] Run `macmop privacy recent --apply`. Verify recent files list plist/files are cleaned up.
- [ ] Check shell history files (`~/.zsh_history`). Verify they are **untouched** and intact.

### 7. Maintenance DNS Flush
- [ ] Run `macmop maintenance run flush_dns --apply`.
- [ ] Verify command succeeds without requesting sudo.
- [ ] Check audit log: should record output lengths and status `"success"`.
- [ ] Check rollback database: should record **no** rollback entries.
