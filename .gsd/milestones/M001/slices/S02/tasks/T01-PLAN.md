# T01: 02-auth-access-control 01

**Slice:** S02 — **Milestone:** M001

## Description

Build the authentication foundation: user account creation with Argon2id password hashing, login/logout flow, and a session state machine with configurable inactivity timeout.

Purpose: This is the security gate for the entire application. No other phase (audit logging, patient data, clinical docs) can function without authenticated users and sessions. This plan creates the backend auth infrastructure that all subsequent plans build on.

Output: Database schema for users/sessions/settings, Rust auth module (password hashing, session management), and Tauri commands for register/login/logout/session operations.

## Must-Haves

- [ ] "A user account can be created with a unique username, display name, role, and Argon2id-hashed password"
- [ ] "Duplicate usernames are rejected at the database constraint level"
- [ ] "Passwords shorter than 12 characters are rejected before hashing"
- [ ] "A user can log in with correct credentials and receives an active session"
- [ ] "Invalid credentials return an authentication error without revealing which field is wrong"
- [ ] "Session state transitions correctly between Unauthenticated, Active, Locked, and back"
- [ ] "Session timeout is configurable via app_settings table (default 15 minutes)"
- [ ] "A locked session can be unlocked by re-entering the correct password"

## Files

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
