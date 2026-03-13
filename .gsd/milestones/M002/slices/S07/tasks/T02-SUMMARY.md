---
id: T02
parent: S07
milestone: M002
provides:
  - src/pages/SettingsPage.tsx — full three-tab Settings panel (Backup | Security | Account)
key_files:
  - src/pages/SettingsPage.tsx
key_decisions:
  - Restore button is gated on user?.role === "SystemAdmin" to prevent confusing Unauthorized errors for Provider/FrontDesk roles
  - open({ directory: true }) cancellation (null return) handled silently with no error state set, matching the plan and DocumentBrowser pattern
  - restoreBackup always called with null as second argument (not omitted) to match Tauri IPC contract established in T01
  - biometric (Touch ID) check failure on non-macOS platforms silently ignored (no error state set) — graceful degradation
  - History table uses React fragment key pattern (<> with key on each tr) to allow optional error sub-row per failed entry
patterns_established:
  - mounted-guard useEffect pattern (let mounted = true / return () => { mounted = false }) reused from useSchedule.ts for backup list fetch
  - reloadKey counter state triggers re-fetch after create backup without requiring a custom event system
  - statusBadge() helper returns a colour-coded <span> for backup status strings — reusable pattern for any status column
observability_surfaces:
  - Inline success toast on createBackup: shows filePath and sha256Digest prefix (12 chars + …)
  - backupError banner above history table when listBackups IPC call fails
  - Failed history entries surface errorMessage as a coloured sub-row in the history table
  - mfaError / mfaSuccess banners in Security tab for TOTP disable operations
  - biometricError banner for Touch ID enable/disable failures
duration: ~30 min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Build SettingsPage with Backup, Security, and Account tabs

**Replaced 12-line SettingsPage placeholder with a full three-tab panel (Backup | Security | Account) wired to real Tauri backend commands; `tsc --noEmit` exits 0.**

## What Happened

Wrote `src/pages/SettingsPage.tsx` from scratch, replacing the stub from the S06 placeholder. The component:

- **Backup tab**: native folder picker via `open({ directory: true })`, create backup button (disabled until folder chosen), inline success toast showing `filePath` and `sha256Digest` prefix, history table with status colour-coding and failed-entry error sub-rows, SystemAdmin-only restore form.
- **Security tab**: inline `<MfaSetup>` TOTP setup flow, Disable TOTP section with password confirmation, Touch ID status display with enable/disable buttons (gracefully degrades if unavailable).
- **Account tab**: read-only info grid (Display Name, Role, Session State, Last Activity, Session ID) + full-width sign-out button.

All nullable fields in `BackupLogEntry` (`completedAt`, `filePath`, `fileSizeBytes`, `sha256Digest`, `errorMessage`) are null-checked before use. `session?.lastActivity` is formatted via `toLocaleString()` or "Never". `open()` null return is handled silently.

Fixed import paths immediately after writing — file lives in `src/pages/` so paths are `../lib/tauri`, `../types/backup`, `../types/auth` (not `../../`).

## Verification

```
$ npx tsc --noEmit
# exits 0, no output

$ grep -n "export function SettingsPage" src/pages/SettingsPage.tsx
59:export function SettingsPage() {

$ grep -n "import MfaSetup from" src/pages/SettingsPage.tsx
21:import MfaSetup from "../components/auth/MfaSetup";
```

All must-haves confirmed:
- Named export `SettingsPage` ✓
- `MfaSetup` imported as default export ✓
- `open({ directory: true })` null handled silently ✓
- Restore gated on `user?.role === "SystemAdmin"` ✓
- `restoreBackup` called with `null` as second argument ✓
- All `BackupLogEntry` nullable fields checked ✓
- `session?.lastActivity` displayed as formatted date or "Never" ✓
- `tsc --noEmit` exits 0 ✓

## Diagnostics

- Open Settings → Backup tab in running app to see folder picker, create button, and history table
- Open Settings → Security tab to test TOTP setup/disable and Touch ID
- Open Settings → Account tab to see session info
- `SELECT * FROM backup_log ORDER BY started_at DESC LIMIT 10` — inspect backup history via SQLite
- Browser DevTools console: IPC errors surface on failed Tauri commands
- `backupError` banner above history table when `listBackups` fails

## Deviations

- Import paths corrected from `../../lib/tauri` / `../../types/backup` to `../lib/tauri` / `../types/backup` — the task plan described paths relative to an assumed `src/pages/` location but used wrong depth; fixed immediately after writing.

## Known Issues

None.

## Files Created/Modified

- `src/pages/SettingsPage.tsx` — replaced: full three-tab Settings panel with Backup, Security, Account tabs wired to real backend commands via `commands.*` in `src/lib/tauri.ts`
