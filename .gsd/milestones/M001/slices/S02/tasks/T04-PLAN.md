# T04: 02-auth-access-control 04

**Slice:** S02 — **Milestone:** M001

## Description

Build the complete frontend authentication UI: login form, registration form, lock screen, MFA setup/prompt, auth state management hook, and idle timer hook. Wire into App.tsx so the app requires authentication before showing any content.

Purpose: The backend auth layer from Plans 01-02 is useless without a frontend that actually enforces login, handles session states, and provides the UI for MFA enrollment. This plan creates the user-facing authentication experience.

Output: Auth TypeScript types, invoke wrappers for all auth commands, useAuth and useIdleTimer hooks, five auth UI components, and App.tsx rewired to gate all content behind authentication.

## Must-Haves

- [ ] "User sees a login form when not authenticated"
- [ ] "User can create an account on first launch (no existing users)"
- [ ] "User sees the main app content only after successful authentication"
- [ ] "Lock screen overlay appears after inactivity timeout"
- [ ] "User can unlock the session with password (or Touch ID if enabled)"
- [ ] "MFA setup shows QR code and requires code verification before enabling"
- [ ] "Login flow prompts for TOTP code when MFA is enabled on the account"
- [ ] "Idle timer resets on mouse, keyboard, click, scroll, and touch events"

## Files

- `src/types/auth.ts`
- `src/lib/tauri.ts`
- `src/hooks/useAuth.ts`
- `src/hooks/useIdleTimer.ts`
- `src/components/auth/LoginForm.tsx`
- `src/components/auth/RegisterForm.tsx`
- `src/components/auth/LockScreen.tsx`
- `src/components/auth/MfaSetup.tsx`
- `src/components/auth/MfaPrompt.tsx`
- `src/App.tsx`
- `package.json`
