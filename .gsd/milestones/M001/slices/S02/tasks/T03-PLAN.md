# T03: 02-auth-access-control 03

**Slice:** S02 — **Milestone:** M001

## Description

Add TOTP-based multi-factor authentication and Touch ID biometric support for session unlock.

Purpose: MFA is a HIPAA security best practice for ePHI access. TOTP provides a second authentication factor during login, while Touch ID provides convenient biometric unlock after session lock (not replacing password for initial login, per Pitfall 7 in RESEARCH.md).

Output: TOTP module with secret generation/QR codes/verification, Touch ID integration module, and Tauri commands for MFA enrollment and biometric authentication.

## Must-Haves

- [ ] "TOTP secret generation produces a valid base32-encoded secret, otpauth URL, and QR code base64 image"
- [ ] "TOTP verification accepts valid 6-digit codes within a 90-second window (1 step skew)"
- [ ] "TOTP uses SHA-1 algorithm for maximum authenticator app compatibility"
- [ ] "A user can enable MFA by verifying an initial TOTP code during setup"
- [ ] "TOTP secret is stored in the users table only after successful verification during setup"
- [ ] "Touch ID availability can be checked on the current hardware"
- [ ] "Touch ID authentication can be invoked for session unlock (not initial login)"

## Files

- `src-tauri/src/auth/totp.rs`
- `src-tauri/src/auth/biometric.rs`
- `src-tauri/src/auth/mod.rs`
- `src-tauri/src/commands/mfa.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/capabilities/default.json`
