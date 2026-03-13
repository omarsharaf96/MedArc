---
estimated_steps: 4
estimated_files: 4
---

# T02: Wire biometricUnlock into useAuth and LockScreen

**Slice:** S01 — Touch ID Fix + PT Note Templates
**Milestone:** M003

## Description

T01 produced the backend `biometric_authenticate` command, but the frontend still calls `onUnlock("")` (the password path) when "Use Touch ID" is clicked — this always fails. This task closes the UI-to-backend loop: adds the `biometricAuthenticate` invoke wrapper, adds `biometricUnlock()` to `useAuth`, updates `LockScreen` to call it, and passes it through from `App.tsx`.

This is a pure frontend wiring task. No new Rust code. No DB schema changes.

## Steps

1. **Add `biometricAuthenticate` to `src/lib/tauri.ts`**: In the `commands` object, append:
   ```ts
   biometricAuthenticate: () => invoke<void>("biometric_authenticate", {}),
   ```
   Place it in the MFA section alongside `checkBiometric`, `enableTouchId`, `disableTouchId`.

2. **Add `biometricUnlock` to `src/hooks/useAuth.ts`**:
   - Add `biometricUnlock: () => Promise<void>` to the `UseAuthReturn` interface.
   - Implement as a `useCallback` (no dependencies except `commands`) that:
     1. Sets `error` to `null`.
     2. Calls `await commands.biometricAuthenticate()` — throws on failure.
     3. On success: calls `const sessionInfo = await commands.getSessionState()` and `setSession(sessionInfo)`.
     4. On catch: sets `error` to `"Touch ID authentication failed. Please use your password."`.
   - The refresh after success mirrors the existing `unlock` callback exactly.
   - Add `biometricUnlock` to the returned object.

3. **Update `src/components/auth/LockScreen.tsx`**:
   - Add `onBiometricUnlock: () => Promise<void>` to the `LockScreenProps` interface.
   - Replace the entire body of `handleTouchId` with:
     ```ts
     setUnlocking(true);
     try {
       await onBiometricUnlock();
     } catch {
       // error is already set in useAuth
     } finally {
       setUnlocking(false);
     }
     ```
   - The "Use Touch ID" button already calls `handleTouchId` — no changes needed to the JSX.

4. **Pass `onBiometricUnlock` from `src/App.tsx`**:
   - In the `LockScreen` usage in `App.tsx`, add `onBiometricUnlock={auth.biometricUnlock}` prop.
   - No other changes to `App.tsx`.

## Must-Haves

- [ ] `biometricAuthenticate` wrapper present in `commands` object in `tauri.ts`
- [ ] `biometricUnlock()` added to `UseAuthReturn` interface and implemented in `useAuth.ts`
- [ ] `biometricUnlock` refreshes session state via `commands.getSessionState()` on success
- [ ] `biometricUnlock` sets a user-visible error message on failure (does NOT throw unhandled)
- [ ] `LockScreen.tsx` props interface includes `onBiometricUnlock: () => Promise<void>`
- [ ] `handleTouchId` calls `onBiometricUnlock()` — NOT `onUnlock("")`
- [ ] `App.tsx` passes `auth.biometricUnlock` as `onBiometricUnlock` to `LockScreen`
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit 2>&1 | tail -5` → must exit 0 with no errors
- On hardware with Touch ID enabled and session locked: press "Use Touch ID" → native macOS Touch ID dialog appears → successful authentication unlocks the session without entering a password → app returns to previous screen
- On failure (e.g. cancel Touch ID): error message "Touch ID authentication failed. Please use your password." appears in the LockScreen error area; password unlock still works

## Observability Impact

- Signals added/changed: `biometricUnlock` sets the `error` state in `useAuth` on failure — this state is already displayed in `LockScreen` via the `error` prop passed from `App.tsx`. No new signals needed.
- How a future agent inspects this: Check `auth.error` state via React DevTools, or check the LockScreen error banner text in the UI. Backend audit log shows `auth.biometric.unlock` or `auth.biometric.failed` entries from T01.
- Failure state exposed: User sees an error message on failure; backend audit row records the failure with `success = false`.

## Inputs

- `src-tauri/src/commands/mfa.rs` — `biometric_authenticate` command from T01 (must be built first)
- `src/lib/tauri.ts` — existing `commands` object pattern to follow
- `src/hooks/useAuth.ts` — existing `unlock` callback to mirror for `biometricUnlock`
- `src/components/auth/LockScreen.tsx` — existing `handleTouchId` stub to replace
- `src/App.tsx` — existing `LockScreen` usage to update with new prop

## Expected Output

- `src/lib/tauri.ts` — `biometricAuthenticate` wrapper added to commands
- `src/hooks/useAuth.ts` — `biometricUnlock` function in hook and interface
- `src/components/auth/LockScreen.tsx` — `handleTouchId` calls `onBiometricUnlock`
- `src/App.tsx` — passes `auth.biometricUnlock` to `LockScreen`
