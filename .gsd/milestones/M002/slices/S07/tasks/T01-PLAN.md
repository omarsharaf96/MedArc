---
estimated_steps: 5
estimated_files: 3
---

# T01: Add backup types, invoke wrappers, and delete duplicate file

**Slice:** S07 — Settings, Cleanup & End-to-End Verification
**Milestone:** M002

## Description

Create `src/types/backup.ts` with the three TypeScript types that mirror the Rust backup structs (`BackupResult`, `RestoreResult`, `BackupLogEntry`). Add `import` and three invoke wrappers (`createBackup`, `restoreBackup`, `listBackups`) to `src/lib/tauri.ts` after the existing `verifyDocumentIntegrity` entry. Delete `src/components/scheduling/CalendarPage 2.tsx` (the exact duplicate of `CalendarPage.tsx`). Verify with `tsc --noEmit` and `cargo test --lib`.

This task is the prerequisite for T02 (SettingsPage cannot import backup types that don't exist) and the M002 DoD requirement that the duplicate file be removed.

## Steps

1. Create `src/types/backup.ts` — three interfaces with exact camelCase field names matching the Rust `#[serde(rename_all = "camelCase")]` structs:
   - `BackupResult`: `backupId: string`, `filePath: string`, `fileSizeBytes: number`, `sha256Digest: string`, `completedAt: string`
   - `RestoreResult`: `restoreId: string`, `sourcePath: string`, `completedAt: string`, `integrityVerified: boolean`
   - `BackupLogEntry`: `id: string`, `operation: string`, `initiatedBy: string`, `startedAt: string`, `completedAt: string | null`, `status: string`, `filePath: string | null`, `fileSizeBytes: number | null`, `sha256Digest: string | null`, `errorMessage: string | null`
   - Note: `BackupResult.fileSizeBytes` is `u64` in Rust → `number` (always present); `BackupLogEntry.fileSizeBytes` is `Option<i64>` → `number | null`

2. In `src/lib/tauri.ts`, add the import for the new types after the existing documentation/labs import block:
   ```typescript
   import type { BackupResult, RestoreResult, BackupLogEntry } from "../types/backup";
   ```

3. In `src/lib/tauri.ts`, add a `// ─── Backup commands ───` section after the `verifyDocumentIntegrity` entry (before the closing `};`):
   ```typescript
   // ─── Backup commands ───────────────────────────────────────────────────────

   /** Create an encrypted backup of the database at the given destination directory. */
   createBackup: (destinationPath: string) =>
     invoke<BackupResult>("create_backup", { destination_path: destinationPath }),

   /** Restore a backup from the given source path (SystemAdmin only). */
   restoreBackup: (sourcePath: string, expectedSha256?: string | null) =>
     invoke<RestoreResult>("restore_backup", {
       source_path: sourcePath,
       expected_sha256: expectedSha256 ?? null,
     }),

   /** List all backup log entries (most recent first, limit 100). */
   listBackups: () => invoke<BackupLogEntry[]>("list_backups"),
   ```
   Critical: Rust param is `destination_path` (snake_case). `expected_sha256` key must always be present (even as `null`) or Tauri silently fails deserialization.

4. Delete the duplicate file using shell quotes (filename contains a space):
   ```bash
   rm "src/components/scheduling/CalendarPage 2.tsx"
   ```

5. Run verification:
   ```bash
   npx tsc --noEmit
   cargo test --lib 2>&1 | tail -5
   ls "src/components/scheduling/CalendarPage 2.tsx" 2>&1
   ```

## Must-Haves

- [ ] `src/types/backup.ts` created with all three interfaces exported
- [ ] `BackupLogEntry.fileSizeBytes` typed as `number | null` (not `number`) — Rust `Option<i64>`
- [ ] `BackupResult.fileSizeBytes` typed as `number` (not nullable) — Rust `u64`
- [ ] Import added to `tauri.ts` — `BackupResult`, `RestoreResult`, `BackupLogEntry` all imported
- [ ] `createBackup` wrapper uses `destination_path` (snake_case) as the Rust param key
- [ ] `restoreBackup` wrapper always passes `expected_sha256` key (even as `null`)
- [ ] All three wrappers typed with correct return types matching their Rust `Result<T, AppError>`
- [ ] `CalendarPage 2.tsx` deleted (file no longer exists on disk)
- [ ] `tsc --noEmit` exits 0
- [ ] `cargo test --lib` passes 265+ tests

## Verification

- `npx tsc --noEmit` — must exit 0 with no output (zero errors)
- `cargo test --lib 2>&1 | grep -E "test result|FAILED"` — must show "test result: ok" with 265+ passed
- `ls "src/components/scheduling/CalendarPage 2.tsx" 2>&1` — must return "No such file or directory"
- `grep -n "createBackup\|restoreBackup\|listBackups" src/lib/tauri.ts` — must show all three wrappers

## Observability Impact

- Signals added/changed: Three new IPC entry points become available (`create_backup`, `restore_backup`, `list_backups`); TypeScript compiler will surface any mismatched param names at compile time
- How a future agent inspects this: `grep "createBackup\|listBackups\|restoreBackup" src/lib/tauri.ts` to confirm wrappers exist; `npx tsc --noEmit` to confirm they type-check
- Failure state exposed: If `destination_path` key is wrong, Tauri returns a runtime error (not a TypeScript error) — the exact snake_case key is the only subtle correctness requirement here

## Inputs

- `src-tauri/src/commands/backup.rs` lines 353–410 — Rust struct definitions with `#[serde(rename_all = "camelCase")]` and field types (`u64` vs `Option<i64>`)
- `src/lib/tauri.ts` lines 480–484 — `verifyDocumentIntegrity` entry; new backup section goes immediately after
- `src/components/scheduling/CalendarPage 2.tsx` — 530-line exact duplicate; not imported anywhere; safe to delete

## Expected Output

- `src/types/backup.ts` — new file with three exported interfaces
- `src/lib/tauri.ts` — updated with import and three backup wrappers
- `src/components/scheduling/CalendarPage 2.tsx` — deleted (no longer exists)
- `tsc --noEmit` exits 0 (zero TypeScript errors across the whole project)
- `cargo test --lib` passes 265+ tests (no regressions)
