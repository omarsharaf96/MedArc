---
id: T02
parent: S03
milestone: M001
provides:
  - write_audit_entry() injected into all 5 FHIR commands (create, get, list, update, delete)
  - write_audit_entry() injected into login, logout, complete_login (auth commands)
  - write_audit_entry() injected into activate_break_glass and deactivate_break_glass
  - DeviceId managed state stub (returns "DEVICE_PENDING" until T04 wires machine-uid)
  - extract_patient_id() helper extracting FHIR patient references for audit metadata
  - audit_denied() helper for pre-lock permission failure audit rows
  - 10 new unit tests covering audit injection patterns
key_files:
  - src-tauri/src/commands/fhir.rs
  - src-tauri/src/commands/auth.rs
  - src-tauri/src/commands/break_glass.rs
  - src-tauri/src/device_id.rs
  - src-tauri/src/lib.rs
key_decisions:
  - DeviceId stub introduced now so T02 commands compile; T04 replaces with machine-uid
  - audit write failures swallowed (let _ = write_audit_entry) — failed audit must never block the primary operation
  - audit_denied() acquires its own DB lock when permission is rejected before the command acquires it
  - extract_patient_id() checks subject.reference, patient.reference, and Patient.id in priority order
  - All login failure sub-paths (inactive, locked, wrong password, MFA pending, invalid MFA) produce distinct audit rows with success=false
patterns_established:
  - Check permission → if denied: audit_denied() + return Err — else: acquire lock and proceed
  - lock + primary DB op + write_audit_entry inside same lock hold (no double-lock)
  - Success audit written after confirming the primary operation succeeded
  - Failure audit written at each early-return point with contextual details
observability_surfaces:
  - Every FHIR call lands an audit row; action names are stable dot-separated strings (fhir.create, fhir.get, fhir.list, fhir.update, fhir.delete, auth.login, auth.logout, break_glass.activate, break_glass.deactivate)
  - success=false rows carry safe detail strings (never raw PHI) visible via verify_audit_chain() and future query commands (T03)
  - device_id="DEVICE_PENDING" in all rows until T04 wires machine-uid — detectable via audit log query
duration: ~55 minutes
verification_result: passed
completed_at: 2026-03-11
blocker_discovered: false
---

# T02: Audit Logging Integration

**Injected `write_audit_entry()` into all 9 ePHI-touching commands (5 FHIR + login + logout + break-glass activate/deactivate); both success and failure paths produce audit rows on every call.**

## What Happened

### DeviceId stub (`src-tauri/src/device_id.rs`)

Created `DeviceId` managed state with `placeholder()` factory that returns `"DEVICE_PENDING"`. T04 will call `DeviceId::new(machine_uid::get().unwrap())` and swap this out. The stub keeps T02 compiling without adding machine-uid as a dependency prematurely.

Registered via `app.manage(DeviceId::placeholder())` in `lib.rs` alongside the existing session and database manages.

### FHIR commands (`src-tauri/src/commands/fhir.rs`)

All 5 commands (`create_resource`, `get_resource`, `list_resources`, `update_resource`, `delete_resource`) now:

1. Accept `device_id: State<'_, DeviceId>` as a new parameter
2. Call `middleware::check_permission()` first (before DB lock)
3. On permission denied: call `audit_denied()` helper (acquires its own transient lock) and return `Err`
4. Acquire DB mutex lock
5. Execute the primary DB operation
6. Write an audit entry within the same lock hold — success=true after confirmed success, success=false at each early-return failure point

Two helpers added:
- `extract_patient_id(resource_type, resource)` — extracts patient reference from FHIR JSON (`Patient.id`, `subject.reference`, `patient.reference` in priority order)
- `audit_denied(db, device_id, ...)` — writes failure audit row without holding the caller's lock

`list_resources` includes `details: Some("returned N records")` on success to aid audit review.
`delete_resource` fetches `resource_type` and `patient_id` before the DELETE so they're available for the post-delete audit row.

### Auth commands (`src-tauri/src/commands/auth.rs`)

`login`:
- Adds `device_id: State<'_, DeviceId>`
- Writes failure audit rows for: inactive account, locked account, wrong password
- Writes failure audit row when MFA is required (session not yet established: `success=false` with details `"MFA challenge required; session not yet established"`)
- Writes success audit row after full session is created (no-MFA path)

`logout`:
- Adds `device_id: State<'_, DeviceId>`
- Writes `auth.logout` success row inside the same DB lock as the `UPDATE sessions SET state = 'expired'` update

`complete_login` (MFA step 2):
- Adds `device_id: State<'_, DeviceId>`
- Writes failure rows for: MFA not enabled, invalid MFA code
- Writes success row after TOTP verified and session created (`details: Some("MFA verified")`)

### Break-glass commands (`src-tauri/src/commands/break_glass.rs`)

`activate_break_glass`:
- Adds `device_id: State<'_, DeviceId>`
- Writes failure audit rows for: empty reason (with separate lock), password verification failure
- Writes success audit row after the `break_glass_log` INSERT, with `details: Some("reason: {reason_text}")` (trimmed) and `resource_id: Some(log_id)` for cross-reference to the break_glass_log entry

`deactivate_break_glass`:
- Adds `device_id: State<'_, DeviceId>`
- Writes `break_glass.deactivate` success row inside the same DB lock as the `UPDATE break_glass_log SET deactivated_at` update

## Verification

```
cargo test
```

**102 tests passed, 0 failed** (up from 92 — 10 new tests added)

| Must-Have | Test Coverage | Result |
|-----------|--------------|--------|
| Every FHIR command writes audit row on success and failure | `audit_write_on_create_success`, `audit_write_on_create_failure`, `audit_chain_across_fhir_operations` | ✅ PASS |
| Login/logout write audit rows with auth.login/auth.logout | `audit_auth_actions` | ✅ PASS |
| Break-glass activate/deactivate write audit rows | `audit_break_glass_actions` | ✅ PASS |
| Audit writes happen within the same Mutex lock hold | Code review — all `write_audit_entry()` calls are inside the `conn` scope | ✅ PASS |
| Denied permission attempts write audit rows with success=false | `audit_permission_denied_records_failure` | ✅ PASS |

Additional tests verify `extract_patient_id()` for all FHIR reference patterns (4 tests).

## Diagnostics

- Audit rows are inspectable via `query_audit_log()` (T03 will expose this as a Tauri command)
- Action names are stable: `fhir.create`, `fhir.get`, `fhir.list`, `fhir.update`, `fhir.delete`, `auth.login`, `auth.logout`, `break_glass.activate`, `break_glass.deactivate`
- `device_id = "DEVICE_PENDING"` in all rows until T04 wires machine-uid — visually detectable in the audit log
- `success = 0` rows always carry a `details` string explaining the failure (never raw PHI)
- `user_id = "UNAUTHENTICATED"` on rows written before a valid session is resolved

## Deviations

- **DeviceId stub added to T02** (originally planned for T04). The T04 plan says "register DeviceId state from machine-uid". Since T02's FHIR commands need `device_id: State<'_, DeviceId>`, the struct and managed state registration had to exist now. T04's job is to replace `DeviceId::placeholder()` with `DeviceId::new(machine_uid::get()?)`. This is an additive forward-compatible change, not a violation of T04's scope.

## Known Issues

None.

## Files Created/Modified

- `src-tauri/src/device_id.rs` — new: DeviceId managed state stub (placeholder until T04)
- `src-tauri/src/lib.rs` — added `mod device_id`, `use device_id::DeviceId`, `app.manage(DeviceId::placeholder())`
- `src-tauri/src/commands/fhir.rs` — all 5 FHIR commands instrumented with audit writes + 10 unit tests
- `src-tauri/src/commands/auth.rs` — login, logout, complete_login instrumented with audit writes
- `src-tauri/src/commands/break_glass.rs` — activate_break_glass, deactivate_break_glass instrumented with audit writes
- `.gsd/DECISIONS.md` — appended 7 S03/T02 decisions
