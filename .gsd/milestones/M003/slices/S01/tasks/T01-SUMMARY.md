---
id: T01
parent: S01
milestone: M003
provides:
  - entitlements.plist with correct biometric entitlement (com.apple.security.device.biometric-access = true)
  - biometric.rs with real canEvaluatePolicy_error call behind #[cfg(target_os = "macos")]
  - biometric_authenticate Tauri async command with thread-spawned LAContext bridging
  - objc2-local-authentication, objc2, objc2-foundation, block2 under macOS-only target cfg
key_files:
  - src-tauri/entitlements.plist
  - src-tauri/Cargo.toml
  - src-tauri/src/auth/biometric.rs
  - src-tauri/src/commands/mfa.rs
  - src-tauri/src/lib.rs
key_decisions:
  - Used tauri::async_runtime::spawn_blocking to avoid blocking the async executor, with std::thread::spawn inside it to own the LAContext for its full lifetime
  - Added objc2, objc2-foundation, block2 as explicit direct macOS-only dependencies since they are transitive deps of objc2-local-authentication but must be declared to use directly
  - evaluatePolicy_localizedReason_reply reply block captures SyncSender<Result<(), String>> which is Send; rx.recv() blocks the spawned thread until the ObjC callback fires
patterns_established:
  - ObjC LAContext bridging pattern: spawn_blocking -> thread::spawn -> mpsc channel -> ObjC block callback -> rx.recv() -> join
  - Audit writes both on success (auth.biometric.unlock) and failure (auth.biometric.failed) paths
  - cfg-gated macOS/fallback command pair for Tauri async commands using ObjC APIs
observability_surfaces:
  - "SELECT * FROM audit_log WHERE action LIKE 'auth.biometric.%' ORDER BY timestamp DESC LIMIT 10;" in SQLite shows all biometric attempts
  - Audit action strings: auth.biometric.unlock (success), auth.biometric.failed (failure with localizedDescription in details)
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-13
blocker_discovered: false
---

# T01: Fix Touch ID entitlement, biometric.rs, and biometric_authenticate command

**Fixed wrong biometric entitlement, replaced hardcoded-false stub with real `canEvaluatePolicy_error` call, and implemented thread-safe `biometric_authenticate` Tauri command bridging ObjC LAContext callback to async Rust.**

## What Happened

**Step 1 — entitlements.plist:** Replaced the wrong `com.apple.security.personal-information.location = false` key with `com.apple.security.device.biometric-access = true`. This was the root cause of macOS silently denying all LAContext calls in the sandboxed app.

**Step 2 — Cargo.toml:** Added `objc2-local-authentication = { version = "0.3.2", features = ["LAContext", "block2"] }` under `[target.'cfg(target_os = "macos")'.dependencies]`. Also added `objc2`, `objc2-foundation`, and `block2` as explicit macOS-only direct dependencies (they are transitive deps but must be declared directly for `use` statements to resolve).

**Step 3 — biometric.rs:** Rewrote the 13-line stub. The macOS implementation wraps all ObjC code in `#[cfg(target_os = "macos")]`, creates a `Retained<LAContext>`, and calls `canEvaluatePolicy_error(DeviceOwnerAuthenticationWithBiometrics)` returning the `is_ok()` result. Non-macOS fallback always returns `false`. Two unit tests added.

**Step 4 — mfa.rs:** Implemented `biometric_authenticate` as a `pub async fn` Tauri command. The macOS implementation:
- Pre-flight: calls `check_biometric_available()`, fails fast if no Touch ID
- Pre-flight: reads `session.get_state()`, fails if not in "locked" state
- Calls `tauri::async_runtime::spawn_blocking` to move blocking work off the async executor
- Inside `spawn_blocking`, spawns a dedicated `std::thread::spawn` that owns the `LAContext` for its full lifetime
- Creates `mpsc::sync_channel::<Result<(), String>>(1)`, creates `RcBlock` capturing the `SyncSender`, calls `evaluatePolicy_localizedReason_reply`, then `rx.recv()` blocks until the ObjC callback fires
- Back on the calling side: on success calls `session.unlock(&user_id)`, updates sessions row to `state = 'active'`, writes `auth.biometric.unlock` audit entry
- On failure: writes `auth.biometric.failed` audit entry with the `NSError.localizedDescription()` and returns `AppError::Authentication`
- Non-macOS fallback always returns `Err(AppError::Authentication("Biometric not available"))`

**Step 5 — lib.rs:** Added `commands::mfa::biometric_authenticate` to invoke_handler after `disable_touch_id`.

**Step 6 — Tests:** Added `biometric_check_available_returns_bool` test in both `auth::biometric::tests` and `commands::mfa::tests`.

## Verification

```
cd src-tauri && cargo build 2>&1 | grep -E "^error" | wc -l
# → 0

cd src-tauri && cargo test --lib 2>&1 | tail -5
# → test result: ok. 268 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

grep "biometric-access" src-tauri/entitlements.plist
# → <key>com.apple.security.device.biometric-access</key> / <true/>

grep "biometric_authenticate" src-tauri/src/lib.rs
# → commands::mfa::biometric_authenticate,
```

All must-haves confirmed:
- ✅ entitlements.plist has `com.apple.security.device.biometric-access = true`
- ✅ Cargo.toml has objc2-local-authentication under macOS-only target cfg
- ✅ biometric.rs has `#[cfg(target_os = "macos")]` guard on all ObjC imports and real implementation
- ✅ check_biometric_available() calls real canEvaluatePolicy_error on macOS
- ✅ biometric_authenticate is pub async fn in commands/mfa.rs
- ✅ LAContext is created inside std::thread::spawn (never moved across threads)
- ✅ evaluatePolicy_localizedReason_reply called with non-empty reason string
- ✅ Both success and failure paths write audit rows
- ✅ session.unlock(&user_id) called only after successful LAContext evaluation
- ✅ sessions row updated to state = 'active' on success
- ✅ Command registered in lib.rs invoke_handler
- ✅ cargo build exits 0

## Diagnostics

- Biometric attempts are auditable: `SELECT * FROM audit_log WHERE action LIKE 'auth.biometric.%' ORDER BY timestamp DESC LIMIT 10;`
- On success: `action = "auth.biometric.unlock"`, `success = 1`, `details = NULL`
- On failure: `action = "auth.biometric.failed"`, `success = 0`, `details = NSError.localizedDescription()` (e.g. "User canceled" for LAErrorUserCancel)
- `biometric_authenticate` returns `AppError::Authentication` with human-readable message on every failure path — surfaces in LockScreen error display (wired in T02)

## Deviations

**Added explicit objc2/block2/objc2-foundation direct dependencies:** The task plan only listed `objc2-local-authentication`, but Rust requires direct dependency declaration to use crate paths. Added `objc2 = "0.6.4"`, `objc2-foundation = "0.3.2"` (with NSString, NSError features), and `block2 = "0.6.2"` as macOS-only deps. Versions match the already-locked transitive deps — no version conflicts.

**Used `tauri::async_runtime::spawn_blocking` wrapper:** The task plan described `std::thread::spawn` + `.join()` directly in the async command. Since `.join()` blocks synchronously inside an async fn (which would block the Tauri async executor), the implementation wraps the entire blocking section in `spawn_blocking` which is the correct non-blocking pattern. `std::thread::spawn` is still used inside `spawn_blocking` to own the LAContext for its entire lifetime.

## Known Issues

None. Touch ID hardware verification requires a physical macOS device with enrolled Touch ID — wired in T02 LockScreen integration.

## Files Created/Modified

- `src-tauri/entitlements.plist` — Fixed biometric entitlement key and value
- `src-tauri/Cargo.toml` — Added objc2-local-authentication + objc2/objc2-foundation/block2 under macOS target cfg
- `src-tauri/src/auth/biometric.rs` — Full rewrite with real canEvaluatePolicy_error + cfg guards + tests
- `src-tauri/src/commands/mfa.rs` — Added biometric_authenticate async command + imports + unit test
- `src-tauri/src/lib.rs` — Registered biometric_authenticate in invoke_handler
