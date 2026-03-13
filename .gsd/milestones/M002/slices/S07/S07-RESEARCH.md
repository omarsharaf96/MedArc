# S07: Settings, Cleanup & End-to-End Verification — Research

**Date:** 2026-03-12
**Milestone:** M002
**Requirements:** UI-06 (Settings panel), M002 milestone definition of done (all slices complete, clean TypeScript, end-to-end workflow verified)

## Summary

S07 is a three-part slice: (1) build the Settings panel with backup management, MFA status, and account/session info; (2) remove the one duplicate file `CalendarPage 2.tsx` and add missing backup TypeScript types + invoke wrappers to `tauri.ts`; (3) run `tsc --noEmit` to confirm zero errors, then exercise the full patient visit workflow end-to-end in the running Tauri app and verify RBAC with Provider and FrontDesk accounts.

The backend is fully ready. All three backup commands (`create_backup`, `restore_backup`, `list_backups`) are registered in `lib.rs` and the `Backup` RBAC resource is defined. The frontend currently has a 12-line placeholder (`SettingsPage.tsx`) and is missing: (a) TypeScript types for backup (no `BackupResult`, `RestoreResult`, `BackupLogEntry`), (b) invoke wrappers (`createBackup`, `restoreBackup`, `listBackups`), and (c) the actual Settings UI. The `tauri-plugin-dialog` with `directory: true` provides the native folder picker for the backup destination — the same plugin already used by `DocumentBrowser.tsx`.

The duplicate `CalendarPage 2.tsx` (an exact copy of `CalendarPage.tsx`, 530 lines each) must be deleted. It is not imported anywhere but its presence in the `scheduling/` directory causes a dev-server warning and is a milestone DoD requirement. No Rust duplicate files exist.

S07 is deliberately low-risk and integrative — no new Rust commands, no schema changes. The primary risk is the backup folder picker: `open({ directory: true })` from `tauri-plugin-dialog` is confirmed available (the plugin's TypeScript types expose `directory?: boolean` on `OpenDialogOptions`), but it has not been exercised yet in this codebase. The document upload flow used `open({ multiple: false, filters: [...] })` — directory selection is a slightly different code path.

## Recommendation

Build Settings in three tasks:
- **T01** — Type layer: add `BackupResult`, `RestoreResult`, `BackupLogEntry` to a new `src/types/backup.ts`; add `createBackup`, `restoreBackup`, `listBackups` to `commands` in `src/lib/tauri.ts`; delete `CalendarPage 2.tsx`; verify with `tsc --noEmit`.
- **T02** — Settings UI: replace `SettingsPage.tsx` placeholder with a three-tab panel (Backup | Security | Account). Backup tab: folder picker (native dialog), create-backup button, backup history table. Security tab: MFA status + setup/disable links (reuse existing `MfaSetup` default-export component). Account tab: session info (role, session state, timeout), sign-out button. Verify with `tsc --noEmit`.
- **T03** — End-to-end verification: run the full patient visit workflow in the live Tauri app (`npm run tauri dev`) with Provider account, then verify RBAC with FrontDesk account. Write `S07-UAT.md`. Mark slice complete.

The Settings page can call `useAuth()` directly (same pattern as `SchedulePage.tsx`) to get `user.role`, `user.id`, and `session` data for display. No new props are needed — `ContentArea` already renders `<SettingsPage />` with no arguments.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Native folder picker for backup destination | `open({ directory: true })` from `@tauri-apps/plugin-dialog` | Plugin already wired in `lib.rs` and installed in `package.json`; proven by `DocumentBrowser.tsx` using `open()` for file selection |
| MFA setup flow | `MfaSetup` default-export component in `src/components/auth/MfaSetup.tsx` | Full TOTP setup with QR code display and verification already implemented; just mount it in a modal/section within the Security tab |
| Auth/session state display | `useAuth()` hook + `commands.getSessionState()` | Hook already returns `user.role`, `user.id`, `session.state`, `session.lastActivity`; no additional backend calls needed |

## Existing Code and Patterns

- `src/pages/SettingsPage.tsx` — 12-line placeholder; **replace entirely** in T02
- `src/components/auth/MfaSetup.tsx` — Complete TOTP setup component (default export). Props: `{ onComplete: () => void; onCancel: () => void }`. Mount inside a modal or a collapsible section in the Security tab.
- `src/components/clinical/DocumentBrowser.tsx` — Reference implementation for `open()` from `tauri-plugin-dialog`. Lines 24-25 show the import; lines 166-195 show the call pattern. **For folder picker, pass `{ directory: true }` instead of `{ filters: [...] }`. Returns `string | null`.**
- `src/lib/tauri.ts` — All 88+ invoke wrappers. **Missing**: `createBackup`, `restoreBackup`, `listBackups`. Add after the existing `verifyDocumentIntegrity` entry, in a new `// ─── Backup commands ───` section. Import new types from `../types/backup`.
- `src/hooks/useSchedule.ts` — Pattern for a data-fetching hook with mounted guard, refreshCounter, per-domain error isolation. `useSettings` (if extracted to a hook) should follow this pattern.
- `src/pages/SchedulePage.tsx` — Pattern for calling `useAuth()` inside a page component and reading `user?.id`, `user?.role`. `SettingsPage` should follow the same pattern (no props, self-sufficient).
- `src/components/shell/ContentArea.tsx` — Already has `case "settings": return <SettingsPage />;`. No changes needed here.
- `src/components/shell/Sidebar.tsx` — Provider, BillingStaff, and SystemAdmin all have Settings in their nav items. NurseMa and FrontDesk do not — this is the existing RBAC gate. SettingsPage itself does not need a secondary role check for the basic S07 scope (backup is gated at the backend RBAC layer).

### Backup command signatures (from `src-tauri/src/commands/backup.rs`)

```rust
pub fn create_backup(destination_path: String, ...) -> Result<BackupResult, AppError>
pub fn restore_backup(source_path: String, expected_sha256: Option<String>, ...) -> Result<RestoreResult, AppError>
pub fn list_backups(...) -> Result<Vec<BackupLogEntry>, AppError>
```

### Rust → TypeScript type mapping

```typescript
// src/types/backup.ts
export interface BackupResult {
  backupId: string;
  filePath: string;
  fileSizeBytes: number;     // Rust u64 → number (safe for file sizes ≤ 64MB)
  sha256Digest: string;
  completedAt: string;       // RFC-3339
}

export interface RestoreResult {
  restoreId: string;
  sourcePath: string;
  completedAt: string;
  integrityVerified: boolean;
}

export interface BackupLogEntry {
  id: string;
  operation: string;         // "backup" | "restore"
  initiatedBy: string;
  startedAt: string;
  completedAt: string | null;
  status: string;            // "in_progress" | "completed" | "failed"
  filePath: string | null;
  fileSizeBytes: number | null;   // Rust Option<i64> → number | null
  sha256Digest: string | null;
  errorMessage: string | null;
}
```

### Invoke wrappers to add

```typescript
// In src/lib/tauri.ts, after verifyDocumentIntegrity:
createBackup: (destinationPath: string) =>
  invoke<BackupResult>("create_backup", { destination_path: destinationPath }),

restoreBackup: (sourcePath: string, expectedSha256?: string | null) =>
  invoke<RestoreResult>("restore_backup", {
    source_path: sourcePath,
    expected_sha256: expectedSha256 ?? null,
  }),

listBackups: () => invoke<BackupLogEntry[]>("list_backups"),
```

**Critical**: Rust param is `destination_path` (snake_case), not `destinationPath`. Same rule as all other wrappers.

## Constraints

- No new Rust commands or schema changes — S07 is frontend-only
- TypeScript strict mode: `T | null` for all optional fields (not `T | undefined`); no `any`
- `tsc --noEmit` must exit 0 before T03 (end-to-end verification)
- `cargo test --lib` must continue passing 265+ tests (no Rust changes so this is a no-op gate)
- The `calendarPage 2.tsx` duplicate must be deleted (not just ignored) — it's a DoD requirement
- MfaSetup is a default export — import it as `import MfaSetup from "../../components/auth/MfaSetup"` (not named import)
- RBAC for backup: Provider can `Create` and `Read` backups; only `SystemAdmin` can `restore_backup` (the command has an extra role check beyond RBAC). The UI should conditionally show "Restore" only for SystemAdmin to prevent a confusing "Unauthorized" error.
- The `open({ directory: true })` call returns `string | null` — null when user cancels; the UI must handle cancellation gracefully (no error shown, just reset loading state)
- `fileSizeBytes` in `BackupLogEntry` is `Option<i64>` in Rust → `number | null` in TypeScript (uses `i64` not `u64` because SQLite stores it as INTEGER)
- `BackupResult.fileSizeBytes` is `u64` in Rust → `number` in TypeScript (always present)
- The `Backup` resource is not in the current sidebar nav; the page-level RBAC check is implicit through the sidebar (Provider and SystemAdmin see Settings; FrontDesk and NurseMa do not). No explicit role guard needed in SettingsPage for S07 MVP.

## Common Pitfalls

- **Wrong Rust param name** — The invoke call must use `destination_path`, not `destinationPath`. The `directory: true` option for `open()` is in the OpenDialogOptions (not a separate function) — confirmed in `node_modules/@tauri-apps/plugin-dialog/dist-js/index.d.ts`.
- **Forgetting `expected_sha256: null`** — `restore_backup` has `expected_sha256: Option<String>`. The invoke call must include this key even when not supplying a digest: `expected_sha256: null`. Omitting it causes a silent deserialization failure.
- **Importing MfaSetup as named export** — `MfaSetup` is a default export (`export default function MfaSetup`). Named import `{ MfaSetup }` will fail TypeScript. Use `import MfaSetup from "../../components/auth/MfaSetup"`.
- **fileSizeBytes overflow** — Rust `u64` safely maps to JS `number` for files ≤ 9 petabytes; no safety issue for 64MB max document size.
- **BackupLogEntry.fileSizeBytes is i64 (nullable)** — In the backup log, `file_size_bytes` is `Option<i64>` (can be null for failed backups). The TypeScript type must be `number | null`, not `number`.
- **Displaying backup destination path** — After calling `open({ directory: true })`, the returned path string is a native macOS filesystem path. Display it in a read-only input or pill; do not attempt to manipulate or normalize it.
- **CalendarPage 2.tsx** — Filename contains a space. Must use quotes when deleting: `rm "src/components/scheduling/CalendarPage 2.tsx"`. It has 530 lines identical to `CalendarPage.tsx` — no content is lost by deletion.
- **End-to-end verification workflow order** — The milestone DoD requires a specific sequence: log in → find patient → create/update encounter → record vitals → add medication → schedule follow-up → log out. The researcher has confirmed all underlying components and commands exist and are wired. The verification must exercise them in order, not in isolation.
- **FrontDesk RBAC verification** — Log in with a FrontDesk-role account. Confirm the sidebar shows only "Schedule". Attempt to navigate to "Patients" (should not be possible). Confirm the flow board and appointment creation work.

## Open Risks

- **`open({ directory: true })` on macOS App Sandbox** — The `entitlements.plist` includes `com.apple.security.files.user-selected.read-write: true` which covers user-selected directories. The folder picker should work. Risk is low but cannot be fully confirmed without a signed build — the dev build (`npm run tauri dev`) should exercise this correctly.
- **Restore path in dev build** — `restore_backup` writes the decrypted DB to the live database file path. In development, this is the Tauri app data directory. Running a restore in a dev session replaces the current database — this is intentional but the verification step must note this and restore from a known-good backup or skip the restore command in automated verification.
- **`create_backup` reads the live DB file while the app is running** — The command drops the mutex lock before reading the file (`drop(conn)` at line ~360 in backup.rs). This is a best-effort consistency snapshot, not a transactionally consistent backup. This is acceptable for MVP but should be noted in the UAT.
- **SessionInfo.lastActivity format** — `lastActivity` in `SessionInfo` is `Option<String>` serialized from a chrono datetime. The format is implementation-defined (Rust uses RFC-3339 or ISO-8601 depending on how it's stored). The Settings Account tab should display this as a formatted string but handle `null` gracefully.

## Duplicate File Inventory

Only one duplicate found:
- **`src/components/scheduling/CalendarPage 2.tsx`** — 530 lines, identical to `CalendarPage.tsx`. Not imported anywhere. Delete.
- No `* 2.rs` Rust duplicates found in `src-tauri/src/commands/` or elsewhere.

## End-to-End Verification Checklist (T03)

The following workflow must be confirmed in the running Tauri app:

**Provider account workflow:**
1. Launch app (`npm run tauri dev`)
2. Log in as Provider
3. Navigate to Patients → search for an existing patient
4. Open patient detail → confirm demographics, ClinicalSidebar, LabResults, Documents visible
5. Click "Start Encounter" → EncounterWorkspace opens
6. Write SOAP note (Subjective + Plan at minimum) → save
7. Record vitals (BP, HR, temp) → save
8. Navigate back → ClinicalSidebar → add a medication
9. Navigate to Schedule → create a follow-up appointment for the patient
10. Navigate to Settings → list backups (confirm table renders) → create a backup (select a directory, confirm success toast)
11. Log out

**FrontDesk RBAC verification:**
1. Log in as FrontDesk
2. Confirm sidebar shows only "Schedule" — no Patients, no Settings
3. Navigate to Schedule → confirm calendar renders, appointments visible
4. Confirm Patient Flow Board accessible
5. Log out

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React / TypeScript | No specific skill needed | N/A — patterns established in S01–S06 |
| tauri-plugin-dialog | No dedicated skill | Built-in; documented in `node_modules/@tauri-apps/plugin-dialog/dist-js/index.d.ts` |

## Sources

- `src-tauri/src/commands/backup.rs` — Authoritative Rust types and command signatures for all backup operations
- `src-tauri/src/lib.rs` lines 150-152 — Confirms `create_backup`, `restore_backup`, `list_backups` are registered in `invoke_handler!`
- `src-tauri/src/rbac/roles.rs` lines 232-239 — RBAC matrix for Backup resource; Provider gets Create+Read; SystemAdmin gets all; restore is further restricted at command level
- `node_modules/@tauri-apps/plugin-dialog/dist-js/index.d.ts` — Confirms `open({ directory: true })` is the correct API for native folder picker
- `src/components/clinical/DocumentBrowser.tsx` lines 24-195 — Reference implementation for `open()` from plugin-dialog; folder selection is `directory: true` variant
- `src/components/auth/MfaSetup.tsx` — Default-export component with `{ onComplete, onCancel }` props; ready to embed in Settings Security tab
- `.gsd/DECISIONS.md` — M002/S01 RBAC nav matrix; `cargo test --lib` as verification gate established in S07–S08
