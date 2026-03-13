---
estimated_steps: 6
estimated_files: 4
---

# T01: Fix Touch ID entitlement, biometric.rs, and biometric_authenticate command

**Slice:** S01 — Touch ID Fix + PT Note Templates
**Milestone:** M003

## Description

The Touch ID entitlement in `entitlements.plist` has the wrong key (`com.apple.security.personal-information.location = false`), which causes macOS to silently deny any LAContext call in the sandboxed app regardless of hardware. `biometric.rs` is a 13-line stub that hardcodes `false`. This task fixes both, adds the `objc2-local-authentication` crate, and implements the `biometric_authenticate` Tauri command that bridges the ObjC callback to the async executor safely.

This is the highest-risk task in S01 because `LAContext` is `!Send + !Sync` and must be created and destroyed on the same thread. The safe pattern is `std::thread::spawn` + `std::sync::mpsc::channel` — the spawned thread creates `LAContext`, calls `evaluatePolicy_localizedReason_reply`, waits for the ObjC block callback to fire (which delivers the result over the channel), then the calling side receives the result and either calls `session.unlock` or returns an error.

## Steps

1. **Fix `entitlements.plist`**: Replace the `com.apple.security.personal-information.location` key and its `<false/>` value with `<key>com.apple.security.device.biometric-access</key><true/>`. Update the inline comment to reference AUTH-04-FIX.

2. **Add `objc2-local-authentication` to `Cargo.toml`**: Under `[target.'cfg(target_os = "macos")'.dependencies]` (create this section if it doesn't exist), add:
   ```toml
   objc2-local-authentication = { version = "0.3.2", features = ["LAContext", "block2"] }
   ```
   Do NOT add a `[target.'cfg(target_os = "macos")'.dependencies]` section in `[dependencies]` — only in the cfg block.

3. **Rewrite `src-tauri/src/auth/biometric.rs`**:
   - Wrap the entire real implementation in `#[cfg(target_os = "macos")]`.
   - Provide a fallback `#[cfg(not(target_os = "macos"))]` block where `check_biometric_available()` returns `false`.
   - macOS implementation: `check_biometric_available()` creates `LAContext::new()`, calls `canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics, &mut ptr::null_mut())`, returns the `bool` result.
   - Keep `authenticate_biometric_reason()` unchanged (returns `"Unlock MedArc session".to_string()`).
   - Use `use objc2_local_authentication::{LAContext, LAPolicy};` and `use objc2::rc::Retained;`.

4. **Add `biometric_authenticate` command to `src-tauri/src/commands/mfa.rs`**:
   - Signature: `pub async fn biometric_authenticate(db: State<'_, Database>, session: State<'_, SessionManager>, device_id: State<'_, DeviceId>) -> Result<(), AppError>`.
   - First check: call `biometric::check_biometric_available()` — if `false`, return `Err(AppError::Authentication("Touch ID is not available on this device".to_string()))`.
   - Get `user_id` and `session_id` from `session.get_state()` — if no locked session, return `Err(AppError::Authentication("No locked session to unlock".to_string()))`.
   - Spawn a `std::thread::spawn` closure: inside the thread, create `Retained<LAContext>` (so it stays alive until the ObjC block fires), create a `std::sync::mpsc::sync_channel::<Result<(), String>>(1)`, call `evaluatePolicy_localizedReason_reply` passing a block that sends `Ok(())` or `Err(error_description)` over the channel, then `rx.recv()` to block until the callback fires.
   - `thread::spawn` returns a `JoinHandle`; call `.join().map_err(|_| AppError::Authentication("Thread panic".to_string()))?` then `?` the inner result.
   - On success: call `session.unlock(&user_id)` and update the sessions row (`UPDATE sessions SET state = 'active', last_activity = datetime('now') WHERE id = ?1`).
   - Write audit entry with `action = "auth.biometric.unlock"`, `success = true` on success; write `action = "auth.biometric.failed"`, `success = false` on any failure path.
   - Add `#[cfg(not(target_os = "macos"))]` variant that always returns `Err(AppError::Authentication("Biometric not available".to_string()))` without compiling any ObjC code.

5. **Register in `lib.rs`**: Add `commands::mfa::biometric_authenticate` to the `invoke_handler!` macro (after `commands::mfa::disable_touch_id`).

6. **Add unit tests in `mfa.rs`**: In the `#[cfg(test)]` block, add a test `biometric_check_available_returns_bool` that calls `biometric::check_biometric_available()` and asserts it returns a `bool` (the test passes on any platform — it just verifies the function is callable and returns the right type). Optionally add a compile-time cfg test comment verifying the non-macOS fallback compiles.

## Must-Haves

- [ ] `entitlements.plist` has `com.apple.security.device.biometric-access = true` (not `false`, not the wrong key)
- [ ] `Cargo.toml` has `objc2-local-authentication = { version = "0.3.2", features = ["LAContext", "block2"] }` under macOS-only target cfg
- [ ] `biometric.rs` has `#[cfg(target_os = "macos")]` guard on all ObjC imports and the real implementation
- [ ] `check_biometric_available()` does NOT hardcode `false` on macOS — calls real `canEvaluatePolicy_error`
- [ ] `biometric_authenticate` command is `pub async fn` (Tauri async command) in `commands/mfa.rs`
- [ ] `LAContext` is created inside `std::thread::spawn` — never moved across threads or placed in Arc/Mutex
- [ ] `evaluatePolicy_localizedReason_reply` is called with a non-empty reason string (from `authenticate_biometric_reason()`)
- [ ] Both success and failure paths write an audit row
- [ ] `session.unlock(&user_id)` is called only after successful LAContext evaluation
- [ ] The sessions table row is updated to `state = 'active'` on successful biometric unlock
- [ ] Command registered in `lib.rs` invoke_handler
- [ ] `cargo build` exits 0

## Verification

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | wc -l` → must print `0`
- `cd src-tauri && cargo test --lib 2>&1 | tail -5` → must show all tests passing (no regressions)
- Inspect `entitlements.plist` — confirm `com.apple.security.device.biometric-access` key with value `<true/>` is present
- Inspect `lib.rs` — confirm `biometric_authenticate` appears in the invoke_handler list

## Observability Impact

- Signals added/changed: Two new audit action strings: `"auth.biometric.unlock"` (success) and `"auth.biometric.failed"` (failure). Both are queryable via the Audit Log page.
- How a future agent inspects this: `SELECT * FROM audit_log WHERE action LIKE 'auth.biometric.%' ORDER BY timestamp DESC LIMIT 10;` in the SQLite DB shows all recent biometric unlock attempts with success/failure.
- Failure state exposed: `biometric_authenticate` returns a typed `AppError::Authentication` on every failure path (device unavailable, no locked session, user cancelled, touch ID failed). The error message is surfaced in the LockScreen error display (wired in T02).

## Inputs

- `src-tauri/src/auth/biometric.rs` — 13-line stub to be replaced
- `src-tauri/entitlements.plist` — wrong entitlement key to be fixed
- `src-tauri/src/commands/mfa.rs` — existing MFA commands; `biometric_authenticate` appended here
- `src-tauri/src/commands/session.rs` — reference for the `unlock_session` pattern (how to call `session.unlock` and update the sessions row)

## Expected Output

- `src-tauri/entitlements.plist` — `com.apple.security.device.biometric-access = true`
- `src-tauri/Cargo.toml` — `objc2-local-authentication` in macOS-only target dep section
- `src-tauri/src/auth/biometric.rs` — real `canEvaluatePolicy_error` call behind `#[cfg(target_os = "macos")]`
- `src-tauri/src/commands/mfa.rs` — `biometric_authenticate` command with thread-spawned LAContext bridging
- `src-tauri/src/lib.rs` — `biometric_authenticate` in invoke_handler
