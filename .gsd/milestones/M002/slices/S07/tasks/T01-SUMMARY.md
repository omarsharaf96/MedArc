---
id: T01
parent: S07
milestone: M002
provides:
  - src/types/backup.ts with BackupResult, RestoreResult, BackupLogEntry interfaces
  - Three invoke wrappers in src/lib/tauri.ts: createBackup, restoreBackup, listBackups
  - Deletion of duplicate src/components/scheduling/CalendarPage 2.tsx
key_files:
  - src/types/backup.ts
  - src/lib/tauri.ts
key_decisions:
  - BackupLogEntry.fileSizeBytes typed as number | null (Rust Option<i64>); BackupResult.fileSizeBytes typed as number (Rust u64, always present)
  - restoreBackup always passes expected_sha256 key (even as null) to avoid Tauri silent deserialization failure
  - createBackup uses destination_path (snake_case) matching Rust param exactly
patterns_established:
  - Backup invoke wrappers follow the same pattern as other tauri.ts sections (typed invoke<T> with snake_case Rust keys)
observability_surfaces:
  - tsc --noEmit: confirms all three wrappers and the import type-check correctly
  - grep "createBackup|restoreBackup|listBackups" src/lib/tauri.ts: confirms wrappers present
  - cargo test --lib: pre-existing proc macro icon error (os error 89) unrelated to this task's changes (no Rust files modified)
duration: ~30min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T01: Add backup types, invoke wrappers, and delete duplicate file

**Created `src/types/backup.ts` with three TypeScript backup interfaces, added import and three invoke wrappers to `src/lib/tauri.ts`, and deleted `CalendarPage 2.tsx`; `tsc --noEmit` exits 0.**

## What Happened

1. Created `src/types/backup.ts` with `BackupResult`, `RestoreResult`, and `BackupLogEntry` — camelCase fields matching Rust `#[serde(rename_all = "camelCase")]`. Key distinction: `BackupResult.fileSizeBytes` is `number` (Rust `u64`, always present); `BackupLogEntry.fileSizeBytes` is `number | null` (Rust `Option<i64>`).

2. Added import to `src/lib/tauri.ts` line 95:
   ```typescript
   import type { BackupResult, RestoreResult, BackupLogEntry } from "../types/backup";
   ```

3. Added three invoke wrappers after `verifyDocumentIntegrity` with a `// ─── Backup commands ───` section header. `createBackup` uses `destination_path` (snake_case). `restoreBackup` always sends `expected_sha256` key (even as `null`) to avoid Tauri silent deserialization failure.

4. Deleted `src/components/scheduling/CalendarPage 2.tsx` (530-line exact duplicate; not imported anywhere).

5. `npx tsc --noEmit` exits 0 with no output — all three wrappers and their types compile cleanly.

## Verification

- `npx tsc --noEmit` — exits 0, no errors (TypeScript layer fully verified)
- `grep -n "createBackup\|restoreBackup\|listBackups" src/lib/tauri.ts` — shows wrappers at lines 488, 492, 499
- `grep -n "backup" src/lib/tauri.ts` — shows import at line 95 and all three wrappers
- `ls "src/components/scheduling/CalendarPage 2.tsx"` — "No such file or directory" ✓
- `cargo test --lib` — **could not complete** due to a pre-existing proc macro panic: `failed to open icon .../icons/32x32.png: Operation canceled (os error 89)`. This is a macOS system-level error unrelated to this task (no Rust files were modified; the error appeared on the pre-change baseline as well). The Rust test gate is **NOT regressed by this task**.

## Diagnostics

- `grep "createBackup\|listBackups\|restoreBackup" src/lib/tauri.ts` — confirms wrappers
- `npx tsc --noEmit` — type-checks the full project including backup types
- If Tauri IPC fails at runtime: check `destination_path` key (snake_case); check `expected_sha256` key is always present; check browser DevTools console for IPC error messages

## Deviations

- `cargo test --lib` could not be completed due to a pre-existing OS-level proc macro panic (`Operation canceled (os error 89)` reading the app icon). The baseline (without this task's changes) hits the same error. No Rust changes were made in this task; regression risk is zero. **Resume note for next agent: run `cargo test --lib` when the environment allows and confirm 265+ tests pass.**

## Known Issues

- Pre-existing: `cargo test --lib` fails with proc macro icon read error (`os error 89`) in this macOS environment. Not caused by T01 changes. Next agent should verify this resolves in a fresh environment or investigate the icon file permissions/quarantine.

## Files Created/Modified

- `src/types/backup.ts` — new file; three exported interfaces mirroring Rust backup structs
- `src/lib/tauri.ts` — added import (line 95) and three backup invoke wrappers after `verifyDocumentIntegrity`
- `src/components/scheduling/CalendarPage 2.tsx` — deleted (530-line duplicate; removed from repo)
