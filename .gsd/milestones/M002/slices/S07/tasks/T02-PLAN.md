---
estimated_steps: 7
estimated_files: 1
---

# T02: Build SettingsPage with Backup, Security, and Account tabs

**Slice:** S07 — Settings, Cleanup & End-to-End Verification
**Milestone:** M002

## Description

Replace the 12-line `SettingsPage.tsx` placeholder with a full three-tab panel. The component calls `useAuth()` for user/session/logout, `commands.listBackups()` on mount, and `commands.getSessionState()` for fresh session info. Tabs: **Backup** (folder picker + create + history table), **Security** (MFA setup/disable), **Account** (session info + sign-out). No shell changes needed — `ContentArea` already routes `case "settings"` to `<SettingsPage />` with no props.

This fulfils UI-06 and is the last user-facing feature task of M002.

## Steps

1. **Structure and imports** — Replace the file header. Import: `useState`, `useEffect`, `useCallback` from react; `open` from `@tauri-apps/plugin-dialog`; `commands` from `../../lib/tauri` (adjust relative path from `src/pages/`); `useAuth` from `../hooks/useAuth`; `MfaSetup` as default import from `../components/auth/MfaSetup`; types `BackupResult`, `BackupLogEntry` from `../types/backup`. Export a named `SettingsPage` function (no props).

2. **State declarations** — Inside `SettingsPage`:
   - `activeTab: "backup" | "security" | "account"` — default `"backup"`
   - Backup tab: `backupDir: string | null`, `backupEntries: BackupLogEntry[]`, `backupLoading: boolean`, `backupError: string | null`, `creating: boolean`, `lastResult: BackupResult | null`
   - Security tab: `showMfaSetup: boolean`, `disablingTotp: boolean`, `totpPassword: string`, `mfaError: string | null`
   - Account tab: `sessionLoading: boolean`
   - Call `const { user, session, logout } = useAuth()`

3. **Backup data fetch** — `useEffect` on mount (+ a `reloadKey` counter state for refresh after create): call `commands.listBackups()` → set `backupEntries`; handle errors with `backupError`. Follow the mounted-guard pattern from `useSchedule.ts`.

4. **Backup tab UI** — Three-section layout:
   - *Destination picker*: A read-only text field showing `backupDir ?? "No folder selected"` + "Choose Folder" button. On click: `const dir = await open({ directory: true }); if (dir) setBackupDir(dir as string);` — handle cancellation (null return) silently, no error shown.
   - *Create backup button*: disabled when `!backupDir || creating`. On click: call `commands.createBackup(backupDir)` → set `lastResult` → increment `reloadKey` to refresh history; show inline success banner with `lastResult.filePath` and `lastResult.sha256Digest.slice(0, 12)…`. Show inline error on failure.
   - *History table*: columns Operation | Started | Status | File Size | SHA-256. Render `backupEntries.map(...)`. Status cell: colour-code `completed` (green), `failed` (red), `in_progress` (yellow). File size: format as KB/MB when not null. SHA-256: show first 12 chars + `…` when not null. `errorMessage` shown as a sub-row when status = "failed".
   - *Restore button*: render only when `user?.role === "SystemAdmin"`. On click: prompt for `source_path` via a small inline form (text input) + confirm button → call `commands.restoreBackup(sourcePath, null)` → show result. This prevents Provider users from seeing a confusing Unauthorized error.

5. **Security tab UI** — Two sections:
   - *TOTP section*: "Set up TOTP" button that sets `showMfaSetup = true`. When `showMfaSetup`, render `<MfaSetup onComplete={() => setShowMfaSetup(false)} onCancel={() => setShowMfaSetup(false)} />` inline. Separately, a "Disable TOTP" section with a password input (`totpPassword`) + "Disable" button that calls `commands.disableTotp(totpPassword)` → shows success/error inline; clears password on success. Show `mfaError` in red when set.
   - *Touch ID section*: Call `commands.checkBiometric()` on mount → display availability status. "Enable Touch ID" / "Disable Touch ID" buttons wired to `commands.enableTouchId(password)` / `commands.disableTouchId()`. Keep simple — graceful degradation if unavailable.

6. **Account tab UI** — Read-only info grid:
   - Display Name: `user?.displayName ?? "—"`
   - Username (role): `user?.role ?? "—"`
   - Session State: `session?.state ?? "—"`
   - Last Activity: `session?.lastActivity ? new Date(session.lastActivity).toLocaleString() : "Never"`
   - Sign Out: full-width red button calling `await logout()`. No confirmation needed — the app will navigate to login automatically via `useAuth`.

7. **Run tsc** — `npx tsc --noEmit` — fix any errors before marking done. Common pitfalls: `open()` returns `string | string[] | null` — cast to `string` after null check; `MfaSetup` must be default import; all `BackupLogEntry` nullable fields need null checks before use.

## Must-Haves

- [ ] `SettingsPage` is a named export (`export function SettingsPage`) — `ContentArea` imports it by name
- [ ] `MfaSetup` imported as default export (`import MfaSetup from "../components/auth/MfaSetup"`)
- [ ] `open({ directory: true })` returns `string | null` — null cancellation handled silently (no error shown)
- [ ] "Restore" button gated on `user?.role === "SystemAdmin"` — not shown to Provider or other roles
- [ ] `restoreBackup` called with `null` as second argument (not omitting it)
- [ ] All `BackupLogEntry` nullable fields (`completedAt`, `filePath`, `fileSizeBytes`, `sha256Digest`, `errorMessage`) checked before rendering — no unchecked null/undefined access
- [ ] `session?.lastActivity` displayed as formatted date or "Never" — never throws if null
- [ ] `tsc --noEmit` exits 0 after this task

## Verification

- `npx tsc --noEmit` — exits 0, no errors
- Visual inspection in the Tauri app (T03) confirms all three tabs render with live data — but tsc clean is the gate for this task
- `grep -n "export function SettingsPage" src/pages/SettingsPage.tsx` — must match (named export, not default)
- `grep -n "import MfaSetup from" src/pages/SettingsPage.tsx` — must be present (default import, not named)

## Observability Impact

- Signals added/changed: Backup create shows inline toast with file path and SHA-256 prefix; backup history table shows `status` and `errorMessage` for every logged operation — makes past backup health visible without querying the DB directly
- How a future agent inspects this: Open Settings → Backup tab in the running app; or query `SELECT * FROM backup_log ORDER BY started_at DESC LIMIT 10` via any SQLite tool; `npx tsc --noEmit` confirms wiring
- Failure state exposed: Failed backups surface `errorMessage` as a sub-row in the history table; network/IPC errors set `backupError` shown in red above the table

## Inputs

- `src/types/backup.ts` — from T01; provides `BackupResult`, `BackupLogEntry`
- `src/lib/tauri.ts` — from T01; `commands.createBackup`, `commands.listBackups`, `commands.restoreBackup` available
- `src/components/auth/MfaSetup.tsx` — default export with `{ onComplete, onCancel }` props; ready to embed
- `src/hooks/useAuth.ts` — `useAuth()` returns `{ user, session, logout }`; `user.role` and `session.lastActivity` needed
- `src/components/clinical/DocumentBrowser.tsx` lines 166–195 — reference pattern for `open()` + null handling
- `src/pages/SchedulePage.tsx` — reference pattern for `useAuth()` inside a page component

## Expected Output

- `src/pages/SettingsPage.tsx` — fully replaced: three-tab Settings panel with Backup, Security, Account tabs wired to real backend commands
- `tsc --noEmit` exits 0 (all new code type-checks with zero errors)
