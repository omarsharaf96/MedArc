# S07: Settings, Cleanup & End-to-End Verification

**Goal:** Replace the 12-line SettingsPage placeholder with a full three-tab panel (Backup | Security | Account); add the three missing backup TypeScript types and invoke wrappers; delete the duplicate `CalendarPage 2.tsx`; confirm `tsc --noEmit` exits 0; and exercise the complete patient-visit workflow end-to-end in the running Tauri app with both Provider and FrontDesk accounts.

**Demo:** A Provider can open Settings, list backup history, create a backup (native folder picker), see the success result, switch to the Security tab (MFA setup or disable), and read session info in the Account tab — all in the live Tauri app. A FrontDesk user who logs in sees only "Schedule" in the sidebar.

## Must-Haves

- `src/types/backup.ts` exists with `BackupResult`, `RestoreResult`, `BackupLogEntry` — all fields match Rust structs exactly
- `commands.createBackup`, `commands.restoreBackup`, `commands.listBackups` added to `src/lib/tauri.ts`
- `src/components/scheduling/CalendarPage 2.tsx` is permanently deleted
- `SettingsPage.tsx` fully replaced — three-tab layout (Backup | Security | Account) renders without errors
- Backup tab: native folder picker via `open({ directory: true })`, create-backup button, backup history table with status column
- Security tab: embeds `MfaSetup` component (default-import) for TOTP enrollment; shows disable TOTP button when already enabled
- Account tab: displays `user.role`, `user.displayName`, `session.state`, `session.lastActivity` (null-safe)
- "Restore" button shown only for SystemAdmin role (prevents confusing Unauthorized error for Provider)
- `tsc --noEmit` exits 0 with zero TypeScript errors after T01 and again after T02
- `cargo test --lib` passes 265+ tests (no Rust changes — serves as regression gate)
- End-to-end Provider workflow verified in running Tauri app: login → patient search → encounter → vitals → medication → schedule → settings backup list → logout
- FrontDesk RBAC verified: sidebar shows only Schedule; Patients and Settings are absent

## Proof Level

- This slice proves: final-assembly
- Real runtime required: yes (T03 exercises the live Tauri app)
- Human/UAT required: yes (T03 UAT walkthrough is the milestone completion gate)

## Verification

- `npx tsc --noEmit` — exits 0 after T01 (type layer) and after T02 (Settings UI)
- `cargo test --lib` — passes 265+ tests (run after T01 as regression gate; no Rust changes expected)
- `ls "src/components/scheduling/CalendarPage 2.tsx"` — must return "No such file or directory" after T01
- T03 UAT: full workflow exercised in live Tauri app; `S07-UAT.md` written with pass/fail per step

## Observability / Diagnostics

- Runtime signals: backup `status` column in the history table ("in_progress" | "completed" | "failed") makes backup outcome visible at a glance; toast messages surface success/error on create-backup
- Inspection surfaces: `backup_log` SQLite table (queryable via DevTools or audit log); browser console logs for Tauri IPC errors
- Failure visibility: `errorMessage` field in `BackupLogEntry` displayed in the history table when status = "failed"; `open({ directory: true })` cancellation is silent (no error shown)
- Redaction constraints: backup file path is a filesystem path — safe to display; no PHI in backup metadata fields

## Integration Closure

- Upstream surfaces consumed:
  - `src/components/auth/MfaSetup.tsx` — default export embedded in Security tab
  - `commands.getSessionState`, `useAuth()` — Account tab session display
  - `@tauri-apps/plugin-dialog` `open({ directory: true })` — folder picker for backup destination
  - All S01–S06 components — exercised in T03 end-to-end walkthrough
- New wiring introduced in this slice:
  - `src/types/backup.ts` → imported by `src/lib/tauri.ts`
  - `commands.createBackup / restoreBackup / listBackups` → called by `SettingsPage`
  - `SettingsPage` fully replaces placeholder — `ContentArea`'s `case "settings"` already routes to it (no shell change needed)
- What remains before the milestone is truly usable end-to-end: **nothing** — this slice is the final assembly and verification gate for M002

## Tasks

- [x] **T01: Add backup types, invoke wrappers, and delete duplicate file** `est:30m`
  - Why: SettingsPage cannot be built without TypeScript types and invoke wrappers for the three backup commands; the duplicate CalendarPage 2.tsx must be gone before T03 verification
  - Files: `src/types/backup.ts` (new), `src/lib/tauri.ts`, `src/components/scheduling/CalendarPage 2.tsx` (delete)
  - Do: Create `src/types/backup.ts` with `BackupResult`, `RestoreResult`, `BackupLogEntry` (exact field mapping from Rust structs). Add `import type { BackupResult, RestoreResult, BackupLogEntry } from "../types/backup"` to `tauri.ts`. Add three wrappers in a `// ─── Backup commands ───` section after `verifyDocumentIntegrity`. Use `destination_path` (snake_case) for `create_backup`. Include `expected_sha256: expectedSha256 ?? null` in `restore_backup` even when not supplying a digest. Delete `"src/components/scheduling/CalendarPage 2.tsx"` (filename has a space — use quotes). Run `tsc --noEmit` and `cargo test --lib`.
  - Verify: `npx tsc --noEmit` exits 0; `cargo test --lib` passes 265+ tests; `ls "src/components/scheduling/CalendarPage 2.tsx"` returns error
  - Done when: tsc clean, cargo tests pass, duplicate file gone

- [x] **T02: Build SettingsPage with Backup, Security, and Account tabs** `est:1h`
  - Why: Fulfils UI-06 — the Settings panel with backup management, MFA, and account info
  - Files: `src/pages/SettingsPage.tsx` (full replacement)
  - Do: Replace the placeholder entirely. Three-tab layout using local `activeTab` state. **Backup tab**: `open({ directory: true })` from `@tauri-apps/plugin-dialog` for folder selection (returns `string | null`; handle cancellation silently); "Create Backup" button calls `commands.createBackup(destinationPath)` and shows toast on success; backup history table renders `BackupLogEntry[]` from `commands.listBackups()` with columns: Operation, Status, Date, File Size, SHA-256 (truncated); "Restore" button shown only when `user?.role === "SystemAdmin"` (passes `source_path`, `expected_sha256: null`). **Security tab**: reads `checkBiometric()` for Touch ID status; conditionally renders `MfaSetup` (default import) in an inline section when user clicks "Set up TOTP" — `onComplete`/`onCancel` callbacks reset to idle state; "Disable TOTP" button calls `commands.disableTotp(password)` with a password confirmation input when TOTP appears active (infer from `setupTotp` error or track local state). **Account tab**: displays `user?.displayName`, `user?.role`, `session?.state`, `session?.lastActivity` (formatted as locale string or "Never" if null); Sign Out button calls `logout()` from `useAuth()`. Use `useAuth()` for `user`, `session`, `logout`. Use `commands.getSessionState()` on mount for fresh session data. All `T | null` fields handled — no unchecked null access. Run `tsc --noEmit`.
  - Verify: `npx tsc --noEmit` exits 0
  - Done when: tsc clean; SettingsPage renders all three tabs with real data wired to backend commands

- [x] **T03: End-to-end verification in live Tauri app and write UAT** `est:1h`
  - Why: M002 milestone definition of done requires the full patient visit workflow verified in the running app with Provider and FrontDesk accounts; this is the final assembly gate
  - Files: `.gsd/milestones/M002/slices/S07/S07-UAT.md` (new)
  - Do: Launch `npm run tauri dev`. Exercise the Provider workflow in order: (1) log in as Provider, (2) navigate to Patients → search for existing patient, (3) open patient detail — confirm demographics, ClinicalSidebar, LabResults, Documents visible, (4) click "Start Encounter" → EncounterWorkspace opens, (5) write SOAP note (Subjective + Plan) → save, (6) record vitals (BP, HR, Temp) → save, (7) ClinicalSidebar → add a medication, (8) navigate to Schedule → create a follow-up appointment, (9) navigate to Settings → list backups (table renders) → select a directory → create backup (confirm success toast), (10) log out. Then exercise FrontDesk RBAC: log in as FrontDesk → confirm sidebar shows only "Schedule" (no Patients, no Settings) → navigate to Schedule → confirm calendar renders → log out. Write `S07-UAT.md` with pass/fail result per step and overall verdict. Note any deviations.
  - Verify: `S07-UAT.md` exists with all steps marked pass or fail with notes; at minimum steps 1–10 (Provider) and the FrontDesk RBAC check must pass
  - Done when: UAT doc written; milestone completion verified in the live app

## Files Likely Touched

- `src/types/backup.ts` — new file with BackupResult, RestoreResult, BackupLogEntry
- `src/lib/tauri.ts` — add import + three backup wrappers
- `src/components/scheduling/CalendarPage 2.tsx` — deleted
- `src/pages/SettingsPage.tsx` — full replacement (three-tab Settings panel)
- `.gsd/milestones/M002/slices/S07/S07-UAT.md` — new UAT results document
