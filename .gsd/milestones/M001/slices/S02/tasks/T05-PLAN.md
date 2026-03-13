# T05: 02-auth-access-control 05

**Slice:** S02 — **Milestone:** M001

## Description

Final integration wiring and human verification of the complete auth system. Ensure the login flow integrates MFA check, the first-run setup works, and all 8 AUTH requirements are met end-to-end.

Purpose: Plans 01-04 built the pieces independently. This plan ensures they work together as a coherent auth system and gets human confirmation that all requirements are met.

Output: Fully integrated authentication system verified against all 8 AUTH requirements.

## Must-Haves

- [ ] "User can create a system admin account on first launch"
- [ ] "User can log in with correct password (Argon2id hashed)"
- [ ] "Invalid credentials show generic error without revealing which field is wrong"
- [ ] "Session locks automatically after configured inactivity timeout"
- [ ] "Locked session can be unlocked with password"
- [ ] "MFA setup displays QR code and only enables after code verification"
- [ ] "Login prompts for TOTP code when user has MFA enabled"
- [ ] "RBAC prevents unauthorized actions (e.g., FrontDesk cannot create clinical records)"
- [ ] "Break-glass provides time-limited elevated access with mandatory reason"
- [ ] "Touch ID is available for session unlock on supported hardware"

## Files

- `src-tauri/src/commands/auth.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/App.tsx`
- `src/hooks/useAuth.ts`
- `src/lib/tauri.ts`
