# S02: Auth Access Control

**Goal:** Users can securely create accounts, log in with multiple authentication methods, and have their access restricted by role
**Demo:** Register as System Admin, log in, trigger session auto-lock after inactivity, unlock with Touch ID, enable TOTP MFA, confirm a Nurse/MA cannot access billing data, and verify break-glass grants time-limited elevated access

## Must-Haves


## Tasks

- [x] **T01: 02-auth-access-control 01** `est:5min`
  - Build the authentication foundation: user account creation with Argon2id password hashing, login/logout flow, and a session state machine with configurable inactivity timeout.

Purpose: This is the security gate for the entire application. No other phase (audit logging, patient data, clinical docs) can function without authenticated users and sessions. This plan creates the backend auth infrastructure that all subsequent plans build on.

Output: Database schema for users/sessions/settings, Rust auth module (password hashing, session management), and Tauri commands for register/login/logout/session operations.
- [x] **T02: 02-auth-access-control 02** `est:11min`
  - Implement the role-based access control (RBAC) engine with a static 5-role permission matrix, field-level JSON filtering for FHIR resources, and emergency break-glass access with logging.

Purpose: HIPAA requires role-based access control where each user can only access data appropriate to their role. This plan builds the enforcement layer that wraps all data commands, ensuring a Nurse cannot prescribe and a Front Desk clerk cannot read clinical notes. Break-glass provides the HIPAA-required emergency override with full accountability.

Output: RBAC module (roles, permissions, field filtering), middleware for permission enforcement on all FHIR commands, and break-glass activation/deactivation commands with database logging.
- [x] **T03: 02-auth-access-control 03** `est:4min`
  - Add TOTP-based multi-factor authentication and Touch ID biometric support for session unlock.

Purpose: MFA is a HIPAA security best practice for ePHI access. TOTP provides a second authentication factor during login, while Touch ID provides convenient biometric unlock after session lock (not replacing password for initial login, per Pitfall 7 in RESEARCH.md).

Output: TOTP module with secret generation/QR codes/verification, Touch ID integration module, and Tauri commands for MFA enrollment and biometric authentication.
- [x] **T04: 02-auth-access-control 04** `est:8min`
  - Build the complete frontend authentication UI: login form, registration form, lock screen, MFA setup/prompt, auth state management hook, and idle timer hook. Wire into App.tsx so the app requires authentication before showing any content.

Purpose: The backend auth layer from Plans 01-02 is useless without a frontend that actually enforces login, handles session states, and provides the UI for MFA enrollment. This plan creates the user-facing authentication experience.

Output: Auth TypeScript types, invoke wrappers for all auth commands, useAuth and useIdleTimer hooks, five auth UI components, and App.tsx rewired to gate all content behind authentication.
- [x] **T05: 02-auth-access-control 05**
  - Final integration wiring and human verification of the complete auth system. Ensure the login flow integrates MFA check, the first-run setup works, and all 8 AUTH requirements are met end-to-end.

Purpose: Plans 01-04 built the pieces independently. This plan ensures they work together as a coherent auth system and gets human confirmation that all requirements are met.

Output: Fully integrated authentication system verified against all 8 AUTH requirements.

## Files Likely Touched

- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/db/models/user.rs`
- `src-tauri/src/db/models/mod.rs`
- `src-tauri/src/auth/mod.rs`
- `src-tauri/src/auth/password.rs`
- `src-tauri/src/auth/session.rs`
- `src-tauri/src/commands/auth.rs`
- `src-tauri/src/commands/session.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/error.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/src/rbac/mod.rs`
- `src-tauri/src/rbac/roles.rs`
- `src-tauri/src/rbac/middleware.rs`
- `src-tauri/src/rbac/field_filter.rs`
- `src-tauri/src/commands/fhir.rs`
- `src-tauri/src/commands/break_glass.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/auth/totp.rs`
- `src-tauri/src/auth/biometric.rs`
- `src-tauri/src/auth/mod.rs`
- `src-tauri/src/commands/mfa.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/capabilities/default.json`
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
- `src-tauri/src/commands/auth.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/App.tsx`
- `src/hooks/useAuth.ts`
- `src/lib/tauri.ts`
