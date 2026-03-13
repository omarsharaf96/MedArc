---
id: T04
parent: S03
milestone: M001
provides:
  - DeviceId::from_machine_uid() wired in lib.rs setup — all audit rows now carry the real OS hardware fingerprint instead of "DEVICE_PENDING"
  - machine-uid 0.5 dependency added to src-tauri/Cargo.toml
  - Graceful fallback to "DEVICE_UNKNOWN" with startup warning if OS cannot supply a machine ID
  - Full end-to-end runtime integration: DeviceId state registered before any command handler runs, all 9 ePHI commands + 2 audit commands reachable from frontend
key_files:
  - src-tauri/Cargo.toml
  - src-tauri/src/device_id.rs
  - src-tauri/src/lib.rs
key_decisions:
  - machine-uid 0.5 crate chosen for cross-platform hardware fingerprint — reads IOPlatformUUID on macOS, /etc/machine-id on Linux, MachineGuid registry on Windows; no elevated privileges required
  - Graceful degradation: from_machine_uid() falls back to "DEVICE_UNKNOWN" with an eprintln! warning rather than panicking or failing app startup — audit rows with DEVICE_UNKNOWN are still valid and inspectable
  - DeviceId::placeholder() preserved (dead_code allowed) for test scenarios that don't need a real machine UID
patterns_established:
  - app.manage(DeviceId::from_machine_uid()) called in .setup() before app.manage(database) — ensures device_id state is available to all Tauri command handlers at first invocation
observability_surfaces:
  - "[MedArc] INFO: device_id resolved to '...'" printed to stderr at app startup — confirms the exact UUID that will appear in all audit rows
  - "[MedArc] WARNING: could not resolve machine-uid (...)" printed to stderr if OS ID lookup fails — operator-visible without crashing the app
  - All audit rows carry the real device UUID post-T04; any rows with "DEVICE_PENDING" were written before this task and are easily identified in the AuditLog UI
duration: ~20 minutes
verification_result: passed
completed_at: 2026-03-11
blocker_discovered: false
---

# T04: 03-audit-logging 04

**Wired DeviceId::from_machine_uid() into lib.rs setup, replacing the DEVICE_PENDING placeholder with the real OS hardware fingerprint; confirmed all 9 ePHI commands and 2 audit commands are registered and the app starts cleanly.**

## What Happened

T04 is the wiring task for S03 — T01–T03 built all the pieces; this task makes them runtime-reachable.

**machine-uid dependency:** Added `machine-uid = "0.5"` to `src-tauri/Cargo.toml`. This crate reads the OS-native machine identifier without elevated privileges: `IOPlatformUUID` via ioreg on macOS (confirmed value: `01B40573-2D09-50CC-A450-BC28F1F9D0F4`), `/etc/machine-id` on Linux, `MachineGuid` registry key on Windows.

**device_id.rs rewrite:** Replaced the stub module with a full implementation:
- `DeviceId::from_machine_uid()` — calls `machine_uid::get()`, trims whitespace, logs the resolved ID to stderr as `[MedArc] INFO: device_id resolved to '...'`; on error or empty string, falls back to `"DEVICE_UNKNOWN"` with a warning log
- `DeviceId::placeholder()` preserved with `#[allow(dead_code)]` for tests that don't need a real UID
- `DeviceId::new(id)` helper kept for test injection

**lib.rs change:** Replaced `app.manage(DeviceId::placeholder())` with `app.manage(DeviceId::from_machine_uid())` — one-line change in the `.setup()` closure, before `app.manage(database)`.

**Verification of lib.rs completeness:** Confirmed all 9 ePHI-touching commands are registered in `invoke_handler`:
- `commands::fhir::create_resource`, `get_resource`, `list_resources`, `update_resource`, `delete_resource`
- `commands::auth::login`, `commands::auth::logout`
- `commands::break_glass::activate_break_glass`, `commands::break_glass::deactivate_break_glass`

And both audit commands:
- `commands::audit::get_audit_log`
- `commands::audit::verify_audit_chain_cmd`

These were registered by T03; T04 confirmed they are present and the full handler list is correct.

## Verification

- `cargo build` (exit 0): compiles cleanly; machine-uid 0.5.4 resolved and compiled; only pre-existing warnings
- `cargo test` (exit 0): **102/102 tests pass** — all prior S03 tests (T01 audit chain, T02 FHIR+auth audit, T03 command/query) continue green
- `npx tsc --noEmit` (exit 0): TypeScript compiles cleanly
- `npm run tauri dev`: app starts, Vite dev server at `http://localhost:1420`, Tauri binary `target/debug/medarc` launched successfully; app UI confirmed running (browser screenshot shows "Create Account" first-run screen)
- macOS machine-uid spot-check: `ioreg -rd1 -c IOPlatformExpertDevice | grep IOPlatformUUID` returns `"01B40573-2D09-50CC-A450-BC28F1F9D0F4"` — matches what `machine_uid::get()` returns on this machine

**Must-have check:**
| Must-Have | Status |
|---|---|
| App starts successfully and DeviceId state is registered before any command handler runs | ✓ — `app.manage(DeviceId::from_machine_uid())` is in `.setup()` before `manage(database)`; Tauri `.setup()` runs before any invoke handler can fire |
| All 9 instrumented commands registered in invoke_handler | ✓ — confirmed in lib.rs: create/get/list/update/delete, login, logout, activate_break_glass, deactivate_break_glass |
| get_audit_log and verify_audit_chain_cmd registered in invoke_handler | ✓ — confirmed in lib.rs |
| App starts successfully | ✓ — `npm run tauri dev` builds and runs without errors |
| AuditLog UI accessible after login and FHIR operation | ✓ — AuditLog component mounted in App.tsx (T03) for Provider/SystemAdmin; backend commands wired; full E2E flow requires native Tauri window interaction (screen recording unavailable in this environment, but all integration pieces are confirmed wired) |

## Diagnostics

- `[MedArc] INFO: device_id resolved to 'UUID'` in stderr at app startup — confirms which device UUID appears in audit rows
- `[MedArc] WARNING: could not resolve machine-uid (...)` if OS lookup fails — graceful degradation, app continues
- Any audit rows with `device_id = "DEVICE_PENDING"` were written before T04 (pre-existing rows in dev databases) — visually identifiable in the AuditLog table
- `DEVICE_UNKNOWN` in audit rows indicates a sandboxed/containerized environment where machine-uid failed at runtime

## Deviations

None. The plan specified replacing DeviceId::placeholder() with machine-uid; this was done exactly. The fact that T03 had already registered audit commands in lib.rs (carry-forward from T03-SUMMARY.md confirmed this) meant T04's lib.rs scope was smaller than originally estimated — only the DeviceId swap was needed.

## Known Issues

- Full browser-level E2E verification (login → FHIR create → AuditLog table shows row) requires the native Tauri WebView, which is not accessible from the browser-based verification tooling in this environment. All wiring is confirmed correct through code review, build verification, and 102 passing tests. The S03 demo scenario (FHIR CRUD → visible audit row) is exercised by unit tests in `commands::fhir::tests::audit_chain_across_fhir_operations`.

## Files Created/Modified

- `src-tauri/Cargo.toml` — added `machine-uid = "0.5"` dependency
- `src-tauri/src/device_id.rs` — **REPLACED**: full implementation using machine_uid::get() with graceful fallback; placeholder() kept as dead_code for tests
- `src-tauri/src/lib.rs` — changed `DeviceId::placeholder()` → `DeviceId::from_machine_uid()` in setup closure
- `.gsd/DECISIONS.md` — appended machine-uid crate decision
