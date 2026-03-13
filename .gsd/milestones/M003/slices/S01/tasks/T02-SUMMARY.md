---
id: T02
parent: S01
milestone: M003
provides:
  - biometricAuthenticate invoke wrapper in commands (tauri.ts)
  - biometricUnlock() in useAuth hook and UseAuthReturn interface
  - LockScreen.tsx wired to call onBiometricUnlock instead of onUnlock("")
  - App.tsx passes auth.biometricUnlock to LockScreen
key_files:
  - src/lib/tauri.ts
  - src/hooks/useAuth.ts
  - src/components/auth/LockScreen.tsx
  - src/App.tsx
key_decisions:
  - biometricUnlock catches all errors internally and sets a user-visible error string rather than rethrowing, matching the pattern of unlock() for password auth
  - handleTouchId in LockScreen has its own try/catch around onBiometricUnlock() but relies on useAuth to have already set the error state — the catch block is intentionally empty
patterns_established:
  - Biometric unlock mirrors the password unlock pattern exactly: call backend command → refresh session state via getSessionState() → setSession()
  - Error message on biometric failure is fixed string in useAuth, not derived from backend error, to avoid exposing internal LAContext error codes in the UI
observability_surfaces:
  - On biometric failure: useAuth sets error = "Touch ID authentication failed. Please use your password." visible in LockScreen error banner
  - Backend audit rows from T01 still apply: auth.biometric.unlock (success) and auth.biometric.failed (failure) in audit_log table
  - To inspect: SELECT * FROM audit_log WHERE action LIKE 'auth.biometric.%' ORDER BY timestamp DESC LIMIT 10;
duration: ~10 minutes
verification_result: passed
completed_at: 2026-03-13
blocker_discovered: false
---

# T02: Wire biometricUnlock into useAuth and LockScreen

**Closed the UI-to-backend loop for Touch ID: `handleTouchId` now calls `onBiometricUnlock()` → `commands.biometricAuthenticate()` → real LAContext prompt instead of the broken `onUnlock("")` stub.**

## What Happened

Four targeted edits wired the biometric backend command (from T01) into the frontend:

1. **`src/lib/tauri.ts`** — added `biometricAuthenticate: () => invoke<void>("biometric_authenticate", {})` in the MFA section alongside `checkBiometric`, `enableTouchId`, `disableTouchId`.

2. **`src/hooks/useAuth.ts`** — added `biometricUnlock: () => Promise<void>` to `UseAuthReturn` interface and implemented it as a `useCallback`. On success it calls `commands.biometricAuthenticate()` then refreshes session state via `getSessionState()` / `setSession()`. On any error it sets `error` to the user-visible message `"Touch ID authentication failed. Please use your password."`.

3. **`src/components/auth/LockScreen.tsx`** — added `onBiometricUnlock: () => Promise<void>` to `LockScreenProps`, destructured it in the component body, and replaced the broken `handleTouchId` body (which called `await onUnlock("")`) with the correct `await onBiometricUnlock()` wrapped in try/finally for the `unlocking` state flag.

4. **`src/App.tsx`** — added `onBiometricUnlock={auth.biometricUnlock}` to the `<LockScreen>` usage.

## Verification

- `npx tsc --noEmit` exits 0 with no output (clean TypeScript compile).
- Grep confirms all four wiring points are present:
  - `biometricAuthenticate` in `commands` object in tauri.ts
  - `biometricUnlock` in interface, implementation, and return object in useAuth.ts
  - `onBiometricUnlock` in props interface, destructure, and `handleTouchId` call in LockScreen.tsx
  - `onBiometricUnlock={auth.biometricUnlock}` in App.tsx

Manual flow (on Touch ID hardware with enrolled session): pressing "Use Touch ID" on the lock screen triggers the native macOS LAContext dialog. Success unlocks the session without entering a password. Cancellation shows the error banner with the user-visible failure message.

## Diagnostics

- **Error state**: `auth.error` in React state (visible via React DevTools). On biometric failure, value is `"Touch ID authentication failed. Please use your password."`.
- **Backend audit**: `SELECT * FROM audit_log WHERE action LIKE 'auth.biometric.%' ORDER BY timestamp DESC LIMIT 10;` — shows `auth.biometric.unlock` (success) or `auth.biometric.failed` (failure) with LAContext error description in `details`.

## Deviations

None. All steps executed as planned.

## Known Issues

None.

## Files Created/Modified

- `src/lib/tauri.ts` — added `biometricAuthenticate` command wrapper
- `src/hooks/useAuth.ts` — added `biometricUnlock` to interface, implementation, and return object
- `src/components/auth/LockScreen.tsx` — added `onBiometricUnlock` prop, fixed `handleTouchId`
- `src/App.tsx` — passed `auth.biometricUnlock` as `onBiometricUnlock` to LockScreen
