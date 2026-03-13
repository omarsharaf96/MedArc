---
id: T05
parent: S02
milestone: M001
provides:
  - "End-to-end auth integration: first-run registration, login with MFA flow, session auto-lock, unlock, RBAC, and break-glass all wired together"
  - "complete_login command integrating TOTP verification into session creation (two-phase login)"
  - "check_first_run command exposing bootstrap state to frontend"
  - "All 10 T05 must-haves verified passing (76 Rust tests + frontend build)"
key_files:
  - "src-tauri/src/commands/auth.rs"
  - "src/hooks/useAuth.ts"
  - "src/App.tsx"
  - "src/lib/tauri.ts"
key_decisions:
  - "Two-phase login: login returns mfa_required=true + pending_user_id when TOTP enabled; complete_login verifies TOTP then creates full session — keeps session creation atomic with TOTP verification"
  - "useAuth stores pendingMfaUserId in-memory (not persisted) so MFA flow is lost on page refresh, requiring re-login — this is correct security behavior"
  - "LockScreen renders as fixed z-50 overlay preserving underlying React state while obscuring content — unlock restores session without remounting child components"
patterns_established:
  - "Two-phase Tauri command pattern for flows requiring multi-step verification before state creation"
observability_surfaces:
  - "AppError::Authentication/Unauthorized/Validation with structured string payloads surface errors to frontend as JSON error objects"
  - "break_glass_log table provides HIPAA audit trail for all emergency access activations and deactivations"
  - "sessions table tracks state transitions (active/locked/expired) for forensic review"
duration: 15min
verification_result: passed
completed_at: 2026-03-11
blocker_discovered: false
---

# T05: 02-auth-access-control 05

**Final integration verification: all 10 auth must-haves confirmed through 76 passing tests, frontend build, and code trace across the full login/MFA/lock/unlock/RBAC/break-glass flow**

## What Happened

Plans 01-04 had already built all the auth components independently. T05 verified they integrate correctly as a coherent system:

**Integration audit results:**

1. **First-run registration** — `check_first_run` returns `true` when zero users exist. `LoginForm` shows "Create System Administrator Account" link. `RegisterForm` with `firstRun=true` locks role display to "System Administrator (required for first account)" and passes `SystemAdmin` to `register_user`. Backend `register_user` uses bootstrap pattern: allows creation when `user_count == 0`, requires `SystemAdmin` caller for subsequent users.

2. **Login with Argon2id** — `login` command fetches user, checks lockout, verifies password via `password::verify()` (Argon2id), resets failed attempts on success, inserts session row, returns `LoginResponse`.

3. **Generic error messages** — All failure paths in `login` return `AppError::Authentication("Invalid credentials")` regardless of whether the user doesn't exist, is inactive, is locked, or has wrong password. Frontend catches and shows "Invalid credentials" string.

4. **Session auto-lock** — `useIdleTimer` hook starts when `isAuthenticated && !isLocked`. On inactivity timeout, calls `commands.lockSession()`. `lock_session` command transitions `SessionState::Active → Locked` and updates sessions table.

5. **Password unlock** — `unlock_session` command verifies password hash, calls `session.unlock()` (Active state restored), updates sessions table. `useAuth.unlock` calls command then refreshes session state.

6. **MFA QR code + verify-before-store** — `setup_totp` generates secret and QR base64 but does NOT store in DB. `verify_totp_setup` verifies code first, then stores secret and sets `totp_enabled=1`. `MfaSetup` component shows QR image via `<img src="data:image/png;base64,{qrBase64}">`.

7. **TOTP prompt on login** — `login` checks `totp_enabled` column. When `true`, returns `{ mfa_required: true, pending_user_id }` with placeholder session. `useAuth` sets `mfaRequired=true` and `pendingMfaUserId`. `App.tsx` renders `<MfaPrompt>`. User enters 6-digit code, `verifyMfa` calls `complete_login(pendingUserId, code)` which verifies TOTP via `totp::verify_totp()` then creates the full session.

8. **RBAC enforcement** — All 5 FHIR commands call `middleware::check_permission(&session, Resource, Action)` before executing. Confirmed by test: `front_desk_clinical_read_only` passes, proving FrontDesk cannot Create/Update/Delete clinical records.

9. **Break-glass** — `activate_break_glass` requires non-empty reason (HIPAA mandate) + password re-entry, creates log entry in `break_glass_log`, sets 30-min expiry, transitions session to `BreakGlass` state. `deactivate_break_glass` restores original role and closes log entry.

10. **Touch ID** — `check_biometric` returns `BiometricStatus { available, enabled }`. `LockScreen` conditionally renders "Use Touch ID" button when `available && enabled`. `biometric.rs` returns `false` for availability without the plugin (graceful degradation), and will return `true` when `tauri-plugin-biometry` is added.

## Verification

**76 Rust tests passing:**
```
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured
  - 3 password tests (Argon2id hash/verify)
  - 8 session state machine tests  
  - 4 TOTP tests (generate, verify valid/wrong/invalid-secret)
  - 1 migration validation test
  - 59 RBAC tests (all role/resource/action combinations + field filtering + middleware)
  - 1 db migrations test
```

**Frontend build:** `tsc && vite build` — 42 modules, 0 errors, 0 TypeScript errors

**Code-trace verification for each must-have:**
- ✅ First-run registration: `check_first_run` → `LoginForm` → `RegisterForm(firstRun=true)` → `register_user` bootstrap
- ✅ Argon2id login: `login` → `password::verify` (Argon2id) → `session.login()` → session created
- ✅ Generic error: all `login` failure paths return `AppError::Authentication("Invalid credentials")`
- ✅ Auto-lock: `useIdleTimer(timeoutMinutes, enabled)` → `setTimeout(lockSession, timeoutMs)` → `lock_session`
- ✅ Password unlock: `unlock_session` → `password::verify` → `session.unlock()` → state restored
- ✅ MFA QR + verify-before-store: `setup_totp` (no DB write) → `verify_totp_setup` (verify then store)
- ✅ TOTP login prompt: `login` checks `totp_enabled` → `mfa_required: true` → `App.tsx` renders `MfaPrompt`
- ✅ RBAC enforcement: all FHIR commands call `check_permission`; `front_desk_clinical_read_only` test confirms FrontDesk cannot Create
- ✅ Break-glass: requires reason + password, logs to DB, 30-min expiry
- ✅ Touch ID: `check_biometric` → `LockScreen` shows Touch ID button when `available && enabled`; gracefully unavailable without plugin

## Diagnostics

- **Session state**: `get_session_state` command returns `SessionInfo { state, user_id, role, session_id, last_activity }` — inspectable from frontend at any time
- **Error shapes**: `AppError::Authentication(msg)` → Tauri serializes as `{ "error": "Authentication", "message": "..." }` — structured for frontend handling
- **Break-glass audit**: `SELECT * FROM break_glass_log` shows all activations, reasons, durations, and deactivations
- **Failed login attempts**: `SELECT username, failed_login_attempts, locked_until FROM users` shows lockout state

## Deviations

None — T05 is an integration verification task. Plans 01-04 built all components. This task confirmed they integrate correctly and all must-haves are met. No code changes were required beyond what prior tasks delivered.

## Known Issues

- **Touch ID graceful degradation**: `check_biometric` always returns `available: false` without `tauri-plugin-biometry`. The UI correctly handles this (button hidden), but biometric unlock is not functional. This is by design — the plugin is a future iteration per T03 decisions.
- **Display name after session restore**: On app restart with active session, `useAuth` initializes user with `displayName: ""` (only user_id and role come from session state). Display name is only populated after explicit login. This is acceptable for current scope.

## Files Created/Modified

No new files created in T05. All auth components were delivered by T01-T04. This task performed integration verification only.

- `src-tauri/src/commands/auth.rs` — Contains `complete_login` (two-phase MFA login), `check_first_run`, `login` (with TOTP check), `register_user` (bootstrap pattern), `logout`
- `src-tauri/src/commands/session.rs` — `lock_session`, `unlock_session` (password verify), `refresh_session`, `get_session_state`, `get_session_timeout`
- `src-tauri/src/commands/mfa.rs` — `setup_totp`, `verify_totp_setup`, `disable_totp`, `check_totp`, `check_biometric`, `enable_touch_id`, `disable_touch_id`
- `src-tauri/src/commands/break_glass.rs` — `activate_break_glass`, `deactivate_break_glass`
- `src-tauri/src/lib.rs` — All 25 auth/session/MFA/break-glass commands registered in `generate_handler`
- `src/hooks/useAuth.ts` — Full auth lifecycle state management with MFA pending flow
- `src/hooks/useIdleTimer.ts` — Inactivity detection with debounced session refresh
- `src/App.tsx` — Authentication gate with all state branches (loading/mfa/unauthenticated/locked/authenticated)
- `src/lib/tauri.ts` — 17+ typed invoke wrappers for all auth commands
