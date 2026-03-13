# S01: Touch ID Fix + PT Note Templates — Research

**Date:** 2026-03-13

## Summary

S01 has two distinct bodies of work: (1) replacing the `check_biometric_available()` stub that
always returns `false` with a real `LAContext` call via `objc2-local-authentication`, and
(2) building the three PT note types (Initial Evaluation, Daily Progress Note / SOAP-PT, Discharge
Summary) with co-sign/lock/addendum workflow — all following the established FHIR + index-table
pattern.

The **Touch ID fix** is the highest-risk item and the only genuine "retire the risk" task. The
`biometric.rs` stub is three lines; replacing it with a real `LAContext.canEvaluatePolicy` call
is straightforward. The harder part is the `biometric_authenticate` command: `evaluatePolicy_localizedReason_reply`
is callback-based (Objective-C block), `LAContext` is `!Send + !Sync`, and Tauri commands run on
async executor threads. The solution is a `std::sync::mpsc::channel` inside a
`tauri::async_runtime::spawn_blocking` closure — the blocking thread calls the ObjC API,
waits on the reply block, then sends the result over the channel; the async command awaits the
channel. The entitlements.plist bug (wrong key — `personal-information.location` instead of
`device.biometric-access`) also must be fixed before any LAContext call will succeed in the sandbox.

The **PT note types** extend the existing FHIR+index-table pattern from `documentation.rs`
(2,955 lines, 16 async commands). New PT notes are FHIR Composition resources in `fhir_resources`
with a `pt_note_index` table for fast queries. Migration 15 adds `pt_note_index`. The PT note
commands live in a new `commands/pt_notes.rs` module (not appended to the existing 2,955-line
file). New Rust types follow the `#[serde(rename_all = "camelCase")]` pattern; new TypeScript
types go in a new `src/types/pt.ts` file. Six commands are needed: `create_pt_note`, `get_pt_note`,
`list_pt_notes`, `update_pt_note`, `cosign_pt_note`, `lock_pt_note`. The addendum workflow reuses
the existing Task FHIR resource approach from `request_cosign` / `approve_cosign`.

No new crates are required for PT notes — only `objc2-local-authentication = "0.3.2"` (with
the `block2` feature) is new in `Cargo.toml` for Touch ID.

## Recommendation

### Touch ID

1. Fix `entitlements.plist` — replace the wrong `com.apple.security.personal-information.location`
   key with `com.apple.security.device.biometric-access = true`.
2. Replace `biometric.rs` with a real `canEvaluatePolicy_error` call using
   `LAPolicy::DeviceOwnerAuthenticationWithBiometrics`.
3. Add `biometric_authenticate` as an **async** Tauri command in `commands/mfa.rs`. Use
   `tokio::sync::oneshot` (available via Tauri's async runtime) to bridge the ObjC callback to the
   async command. The command must call into a `spawn_blocking`-style wrapper because `LAContext`
   is `!Send`. Since Tauri uses Tokio under the hood, use
   `tauri::async_runtime::spawn_blocking` or a manual `thread::spawn` + channel.
4. Wire `LockScreen.tsx`'s `handleTouchId` to call a new `commands.biometricAuthenticate()`
   wrapper that directly triggers the backend authenticate command; on success call `auth.unlock`
   with a sentinel path (or add a `biometricUnlock` to `useAuth` that skips the password).

### PT Note Types

1. Add `src-tauri/src/commands/pt_notes.rs` — new module, keep `documentation.rs` untouched.
2. Declare Rust input/output types for all three note shapes; use FHIR Composition for storage.
3. Add Migration 15 (`pt_note_index` table); no existing migrations touched.
4. Register six new commands in `lib.rs` invoke_handler; add to `src/lib/tauri.ts`.
5. Add `src/types/pt.ts` with full TypeScript shapes for all three note types.
6. Add PT note UI pages/components as thin shells (Provider-only, no BillingStaff/FrontDesk).
7. Verify with `cargo test --lib` (all 265 existing tests + new model tests) and `tsc --noEmit`.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| ObjC block bridging | `block2` feature of `objc2-local-authentication` | Required — `evaluatePolicy_localizedReason_reply` only exposes its async reply via an ObjC block; `block2` feature enables the binding |
| FHIR resource storage | Existing `fhir_resources` + index-table pattern from `documentation.rs` | All existing clinical commands use this pattern; deviating breaks consistency and audit log expectations |
| RBAC middleware | `middleware::require_authenticated` + `middleware::require_permission` from `rbac/middleware.rs` | Already implemented; new commands must use these two helpers before any DB access |
| Audit log writes | `write_audit_entry(&conn, AuditEntryInput {...})` from `audit.rs` | Every ePHI-touching command must call this; swallow errors with `let _ = write_audit_entry(...)` |
| UUID generation | `uuid::Uuid::new_v4().to_string()` | Consistent with all other resource IDs in the codebase |
| Timestamp | `chrono::Utc::now().to_rfc3339()` | All timestamps are RFC-3339 strings in FHIR JSON; consistent with existing commands |

## Existing Code and Patterns

- `src-tauri/src/auth/biometric.rs` — 13 lines; `check_biometric_available()` hard-codes `false`; `authenticate_biometric_reason()` returns the reason string. Both functions must be replaced with real LAContext calls. The module already exists — no new module needed for the availability check.
- `src-tauri/src/commands/mfa.rs` — `check_biometric`, `enable_touch_id`, `disable_touch_id` already exist. Add `biometric_authenticate` here following the `pub async fn` pattern of `documentation.rs`.
- `src-tauri/src/commands/session.rs` — `unlock_session` verifies a password, calls `session.unlock(&user_id)`, and updates the sessions row. The biometric unlock path calls `session.unlock(&user_id)` directly after successful LAContext evaluation — same code path, no password. Also updates the sessions row. The `user_id` for the locked session comes from `session.get_state().user_id`.
- `src-tauri/src/auth/session.rs` — `SessionManager::unlock(&user_id)` transitions `SessionState::Locked → Active`. Already used by `unlock_session`. No changes needed.
- `src-tauri/entitlements.plist` — **Bug:** line `com.apple.security.personal-information.location = false` was intended as a Touch ID placeholder but is the wrong key. Replace with `com.apple.security.device.biometric-access = true`. Without this, `canEvaluatePolicy` returns `.biometryNotAvailable` in sandbox regardless of hardware.
- `src/components/auth/LockScreen.tsx` — `handleTouchId` currently calls `onUnlock("")` (password unlock with empty string, which fails). Must call a new `commands.biometricAuthenticate()` and, on success, call a new `onBiometricUnlock()` prop (or adapt `useAuth` to expose a `biometricUnlock` that calls `biometric_authenticate` then refreshes session state).
- `src/hooks/useAuth.ts` — `unlock(password: string)` calls `commands.unlockSession(password)` then refreshes session. Add a parallel `biometricUnlock()` that calls `commands.biometricAuthenticate()` then refreshes session state (same `getSessionState()` refresh logic).
- `src-tauri/src/commands/documentation.rs` — 2,955 lines, reference for: FHIR builder pattern (`build_encounter_fhir`, `build_cosign_fhir`), SOAP note JSON structure, cosign/Task workflow, audit log pattern, RBAC middleware calls, `encounter_index` table insert pattern. **Do NOT append PT note commands here** — it is already large enough.
- `src-tauri/src/db/migrations.rs` — 14 migrations (0-indexed vector). Migration 15 (index 14) adds `pt_note_index`. Follow `M::up(...)` pattern exactly; never modify existing entries.
- `src-tauri/src/lib.rs` — 88 commands registered. New commands appended following the `commands::pt_notes::create_pt_note` naming convention. Also add `mod commands { pub mod pt_notes; }` to the mod declarations at top.
- `src/lib/tauri.ts` — flat `commands` object. PT wrappers appended at the bottom. Follow camelCase naming (e.g. `createPtNote`, `getPtNote`, `listPtNotes`, `updatePtNote`, `cosignPtNote`, `lockPtNote`) and pass all params with `?? null` fallbacks for `Option<T>` fields.
- `src/types/documentation.ts` — reference for TypeScript type conventions: `T | null` for optionals (never `T | undefined`), `Record<string, unknown>` for FHIR resource blobs, camelCase field names. New PT types go in `src/types/pt.ts`.
- `src-tauri/src/rbac/roles.rs` — RBAC matrix. PT notes are clinical documentation; use `Resource::ClinicalDocumentation` for all PT note commands (Provider = full CRUD, NurseMa = CRU, BillingStaff = Read, FrontDesk = no access). No new Resource variant needed for S01.

## Constraints

- `LAContext` is `!Send + !Sync` — cannot be moved across threads; must be created, used, and dropped on the same thread. Bridge to Tauri's async executor via `std::thread::spawn` + `std::sync::mpsc::channel` or a `tokio::sync::oneshot` channel with a `spawn_blocking` wrapper.
- `biometric-access` entitlement required in sandbox — without it LAContext returns `.biometryNotAvailable` silently; the plist bug is blocking everything.
- `evaluatePolicy_localizedReason_reply` requires the `block2` feature — add `objc2-local-authentication = { version = "0.3.2", features = ["LAContext", "block2"] }` to Cargo.toml. The `LAContext` feature flag also must be specified explicitly.
- `objc2-local-authentication` is macOS-only — wrap all biometric code in `#[cfg(target_os = "macos")]`; the Tauri command must compile on all platforms. On non-macOS, `biometric_authenticate` returns `Err(AppError::Authentication("Biometric not available".to_string()))`.
- `block2 >= 0.6.1, < 0.8.0` is the dependency range for `objc2-local-authentication 0.3.2` — Cargo will resolve; do not pin `block2` explicitly unless there is a conflict.
- Migrations are append-only — Migration 15 adds `pt_note_index`; never modify migrations 1–14.
- `pt_note_index` must use `pt_note_id` (not `id`) as the FK column name to avoid shadowing the `fhir_resources.id` column in JOINs — follow the `encounter_index.encounter_id` pattern.
- All PT note commands must write audit rows — use `Resource::ClinicalDocumentation` as the `resource_type` in `AuditEntryInput`. PT notes are ePHI; every read and write is audited.
- `tsc --noEmit` must pass after S01 — new TypeScript types and wrappers must compile; no `any` types in production paths.
- `cargo test --lib` must pass after S01 — 265 existing tests must still pass; add new tests for PT note type serialization and `pt_note_index` migration validity.
- `disable_touch_id` does NOT require password in the current implementation (unlike `enable_touch_id` which does) — this is intentional per the existing code; do not change this behavior.

## Common Pitfalls

- **Wrong entitlement key** — `com.apple.security.personal-information.location` is the current stub (location, not biometric). The correct key for Touch ID is `com.apple.security.device.biometric-access`. Fixing only the Rust code without fixing the plist means the Tauri sandbox will silently deny the LAContext call.
- **LAContext on wrong thread** — `LAContext` is `!Send`; instantiating it in an async Tokio task and then awaiting a yield point may panic. The safe pattern is: `std::thread::spawn` + create LAContext on that thread + ObjC block sends result over `mpsc::SyncSender` + join or receive on calling thread. Do NOT `Arc<Mutex<LAContext>>`.
- **Empty localizedReason panics** — `evaluatePolicy_localizedReason_reply` throws `NSInvalidArgumentException` if `localizedReason` is nil or empty. Always pass a non-empty string; use `authenticate_biometric_reason()` from `biometric.rs`.
- **Deadlock from calling `canEvaluatePolicy` inside the reply block** — The docs explicitly warn: do not call `canEvaluatePolicy` inside the reply block. Call availability check upfront before calling `evaluatePolicy`.
- **`pt_note_index.status` column must be constrained** — Use `CHECK(status IN ('draft','signed','locked'))` to prevent invalid state. The co-sign workflow transitions `draft → signed` and lock transitions `signed → locked`; these are the only valid transitions.
- **Biometric unlock does not have a `password` param** — The new `biometric_authenticate` command does NOT take a password; it calls LAContext and, on success, calls `session.unlock(&user_id)` directly. A password unlock with `""` will fail because `password::verify("", stored_hash)` will not match.
- **`handleTouchId` in LockScreen currently calls `onUnlock("")`** — this routes through the password verification path, which will always fail for an empty string. Must be changed to call a new separate `biometricAuthenticate` command directly (not via `onUnlock`).
- **PT note `note_type` must be a constrained enum** — Use `CHECK(note_type IN ('initial_eval','progress_note','discharge_summary'))` in `pt_note_index`. Unconstrained string allows invalid data.
- **FHIR Composition type coding must distinguish note types** — Use `type.coding[0].code` = `"initial-evaluation" | "progress-note" | "discharge-summary"` with system `http://medarc.local/fhir/CodeSystem/pt-note-type`. This is how `list_pt_notes(note_type?)` filters efficiently via the index table rather than JSON parsing.
- **`pub async fn` vs `pub fn`** — `biometric_authenticate` must be `pub async fn` (async command) because it bridges a callback; PT note CRUD can be sync (`pub fn`) following the `labs.rs` pattern. Match the async designation to the actual blocking behavior.
- **`addendum` workflow for locked notes** — A locked note cannot be edited. Corrections use a new note linked to the original via a FHIR DocumentReference or extension. S01 must define this linkage field in the Rust type (even if the addendum UI ships in a later slice) so the data model is correct from day one.

## Open Risks

- **`LAContext` thread bridging complexity** — The callback-to-channel bridge is correct in theory but requires careful lifetime management. The `LAContext` object must be kept alive until the reply block fires (Tauri may drop it if not retained). Using `Retained<LAContext>` from objc2 and moving it into the thread closure is the safe approach.
- **`block2` crate compatibility with existing Cargo dependency tree** — `objc2-local-authentication 0.3.2` pulls in `block2 >= 0.6.1, < 0.8.0`. If any other crate in the tree pins `block2` to a conflicting range, Cargo will fail. Likely fine since no other objc2 crates are in use.
- **App Sandbox blocking LAContext on non-Touch-ID Macs** — On Macs without Touch ID hardware (or with Touch ID disabled in System Preferences), `canEvaluatePolicy` returns an error even with the correct entitlement. The availability check already handles this gracefully (returns `available: false`). The fix is additive; it cannot regress hardware that never had Touch ID.
- **`cosign_pt_note` is the S07 visit-counter hook point** — S07 hooks into `cosign_pt_note` to increment `visits_used`. S01 must emit an audit row with enough context (patient_id, encounter_id, note_id) for S07 to JOIN without schema changes. Plan for this in the audit row `details` field.
- **Discharge Summary outcome measure placeholder** — S02 provides the outcome score data that the Discharge Summary references. S01 ships with a `outcome_comparison_placeholder: Option<String>` field in the Discharge Summary type. S02 fills this in. The field must exist in the Rust struct from S01 to avoid a breaking migration later.
- **`pt_notes.rs` module must be declared in `lib.rs` mod block** — Forgetting to add `pub mod pt_notes;` to `src-tauri/src/commands/mod.rs` (or wherever the commands module is declared) causes a compile error. Check the exact declaration pattern.
- **cargo compile time** — `objc2-local-authentication` adds ObjC framework linking; the first compile after adding it to `Cargo.toml` will be slower. Not a correctness risk but expect 60–90s build time on first compile.

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| Tauri 2.x Rust commands | (built-in GSD pattern) | n/a — covered by existing codebase patterns |
| objc2-local-authentication | (no dedicated GSD skill) | none found |
| React / TypeScript frontend | frontend-design skill available | installed |

## Sources

- LAContext is `!Send + !Sync` — trait impls confirmed from docs.rs auto trait listings for `objc2_local_authentication::struct.LAContext` (source: [LAContext docs](https://docs.rs/objc2-local-authentication/0.3.2/objc2_local_authentication/struct.LAContext.html))
- `evaluatePolicy_localizedReason_reply` requires `block2` feature; callback fires on private ObjC queue (source: [LAContext docs](https://docs.rs/objc2-local-authentication/0.3.2/objc2_local_authentication/struct.LAContext.html))
- Correct entitlement key: `com.apple.security.device.biometric-access` (source: M003 CONTEXT.md decision note + Apple Developer docs)
- `tauri-plugin-biometric` explicitly does NOT support macOS (source: M003 DECISIONS.md)
- `kLAPolicyDeviceOwnerAuthenticationWithBiometrics` = `LAPolicy(1)` — Touch ID without password fallback; `kLAPolicyDeviceOwnerAuthentication` = `LAPolicy(2)` — Touch ID OR password (source: LAPolicy constants from docs.rs)
- `canEvaluatePolicy_error` must not be called inside the reply block (source: Apple docs via docs.rs warning)
- `block2` version range for `objc2-local-authentication 0.3.2`: `>= 0.6.1, < 0.8.0` (source: crate metadata on crates.io/docs.rs)
- Existing `biometric.rs` stub confirmed at 13 lines returning `false` (source: codebase read)
- Entitlements bug confirmed: `personal-information.location` key with `false` value is wrong placeholder (source: codebase read of `entitlements.plist`)
- 265 existing `cargo test --lib` tests confirmed (source: DECISIONS.md S08 entry)
- `commands/documentation.rs` is 2,955 lines (source: `wc -l` codebase read)
- `session.unlock(&user_id)` is the correct method for biometric unlock bypass (source: `session.rs` read)
