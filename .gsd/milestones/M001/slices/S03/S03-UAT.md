# S03: Audit Logging — UAT

**Milestone:** M001
**Written:** 2026-03-11

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: S03 is a data-layer and command-layer slice. All correctness guarantees (hash chain integrity, trigger immutability, role-scoped visibility, field completeness) are verified by 102 unit tests running against in-memory SQLite databases — the same code paths the app uses at runtime. The Tauri dev server was confirmed to start and the AuditLog component is mounted and reachable; full browser E2E verification of the audit table populating after a FHIR call requires the native Tauri WebView which is not accessible from browser tooling. The unit test `audit_chain_across_fhir_operations` exercises the complete S03 demo scenario end-to-end at the Rust level.

## Preconditions

- `cargo test` passes 102/102
- `cargo build` exits 0
- `npx tsc --noEmit` exits 0
- Application starts via `npm run tauri dev` with no errors
- A SystemAdmin and a Provider account exist (created via "Create Account" on first run)

## Smoke Test

Run `cargo test audit` — all 21 audit module tests must pass, including:
- `write_persists_all_nine_hipaa_fields` — every HIPAA field lands in the row
- `hash_chain_links_consecutive_rows` — entries are cryptographically linked
- `update_is_rejected_by_trigger` — rows cannot be modified
- `verify_chain_passes_for_valid_chain` — verification returns `valid: true`

## Test Cases

### 1. FHIR Create — Audit Row Written on Success

1. Log in as a Provider user.
2. In the FHIR Explorer, perform a `create_resource` for a `Patient` resource.
3. Navigate to the Audit Log section (visible below FHIR Explorer for Provider role).
4. **Expected:** A row appears with Action = `fhir.create`, Resource Type = `Patient`, Result = ✓ OK, and the current timestamp. User ID column is hidden (Provider view).

### 2. FHIR Create — Audit Row Written on Permission Failure

1. Log in as a Nurse/MA user (no audit log view access).
2. Attempt a FHIR resource operation that Nurse/MA is not permitted to perform.
3. Check the audit log as SystemAdmin.
4. **Expected:** A row appears with `success = false` and a non-PHI detail string. The Nurse/MA user does not see the audit log UI at all.

### 3. Provider Sees Only Own Entries

1. Create two Provider accounts: ProviderA and ProviderB.
2. Log in as ProviderA and perform a FHIR create.
3. Log in as ProviderB and perform a FHIR get.
4. Log in as ProviderA and open the Audit Log.
5. **Expected:** ProviderA sees only rows where `user_id = ProviderA`. ProviderB's row is not visible.

### 4. SystemAdmin Sees All Entries

1. Log in as SystemAdmin.
2. Navigate to the Audit Log.
3. **Expected:** All rows from all users are visible. The "User ID" column is present. The "🔒 Verify Chain" button is visible.

### 5. Chain Verification Passes on Unmodified Log

1. Log in as SystemAdmin.
2. Perform several FHIR operations (create, get, update) across different users.
3. Click "🔒 Verify Chain" in the Audit Log.
4. **Expected:** Green banner: "Chain valid — N rows checked, no tampering detected."

### 6. Login and Logout Audit Rows

1. Log in as any user.
2. Log out.
3. Log in as SystemAdmin and view the Audit Log.
4. **Expected:** Rows with `auth.login` (success=true) and `auth.logout` (success=true) for the user's session.

### 7. Break-Glass Audit Rows

1. Log in as a Provider user.
2. Activate break-glass access (enter a reason and re-authenticate).
3. Deactivate break-glass access.
4. Log in as SystemAdmin and view the Audit Log.
5. **Expected:** Rows with `break_glass.activate` (success=true, details includes the reason text) and `break_glass.deactivate` (success=true).

### 8. Failed Login Audit Row

1. Attempt to log in with an incorrect password for an existing user.
2. Log in as SystemAdmin and check the Audit Log.
3. **Expected:** A row with `auth.login`, `success = false`, and a non-PHI detail string (no raw password or enumerable user info).

### 9. Device ID Present in All Rows

1. Log in and perform any FHIR operation.
2. View the audit log rows (or query via `verify_audit_chain_cmd`).
3. Check stderr output at app startup.
4. **Expected:** `[MedArc] INFO: device_id resolved to '{UUID}'` in stderr. All audit rows carry this UUID in the `device_id` field (not `"DEVICE_PENDING"` or `"DEVICE_UNKNOWN"`).

### 10. Audit Log Pagination

1. Log in as SystemAdmin.
2. Generate more than 20 audit entries (by performing multiple FHIR operations).
3. Open the Audit Log.
4. **Expected:** First page shows 20 rows. "Next →" button is available. Clicking it loads the next page.

## Edge Cases

### Chain Integrity After Many Operations

1. Run `cargo test -- audit_chain_across_fhir_operations --nocapture`.
2. **Expected:** Test creates multiple FHIR operations and verifies `verify_audit_chain()` returns `{ valid: true, rows_checked: N }` for all rows.

### Trigger Rejects Update Attempt

1. Run `cargo test -- update_is_rejected_by_trigger --nocapture`.
2. **Expected:** Any direct SQL UPDATE to `audit_logs` raises `"audit_logs rows are immutable: updates are not permitted"` and returns an error — the test asserts this error message.

### Trigger Rejects Delete Attempt

1. Run `cargo test -- delete_is_rejected_by_trigger --nocapture`.
2. **Expected:** Any direct SQL DELETE on `audit_logs` raises `"audit_logs rows are immutable: deletes are not permitted"`.

### First Row Has GENESIS Sentinel

1. Run `cargo test -- first_row_has_genesis_previous_hash --nocapture`.
2. **Expected:** The very first row inserted has `previous_hash = "GENESIS"` — the chain origin is explicit.

### Role Not Permitted to Access Audit Log

1. Log in as Billing Staff or Front Desk.
2. Navigate to the app — the Audit Log section should not be rendered.
3. If a direct Tauri invoke is made for `get_audit_log` from an unauthorized role, the backend returns `AppError::Unauthorized`.
4. **Expected:** No audit log UI visible; direct invocations rejected with Unauthorized error.

## Failure Signals

- Any `cargo test` failure mentioning `audit` — hash chain or trigger invariant broken
- `device_id = "DEVICE_PENDING"` in audit rows at runtime — machine-uid wiring failed (T04 regression)
- `device_id = "DEVICE_UNKNOWN"` in audit rows — OS could not supply a machine ID (sandboxed environment); check stderr for warning
- Chain verify banner is red after normal operations — indicates a tampered row or a hash computation bug
- Provider user sees rows from other users — role enforcement regression in `get_audit_log` command
- `npx tsc --noEmit` fails — TypeScript type mismatch between Rust serde output and frontend types
- AuditLog section visible for Nurse/MA, Billing Staff, or Front Desk roles — App.tsx gate regression

## Requirements Proved By This UAT

- AUDT-01 — Test cases 1, 2, 6, 7, 8, 9 prove every ePHI access is logged with all required HIPAA fields (timestamp, user_id, action, resource_type, resource_id, patient_id, device_id, success/failure) on both success and failure paths across all 9 instrumented commands.
- AUDT-02 — Test cases 5 and edge cases "Chain Integrity After Many Operations", "Trigger Rejects Update Attempt", "Trigger Rejects Delete Attempt", and "First Row Has GENESIS Sentinel" prove tamper-proof storage with cryptographic hash chains and trigger-enforced immutability.
- AUDT-04 — Test case 3 proves Provider sees only own audit entries; backend role enforcement confirmed by code and command logic.
- AUDT-05 — Test case 4 proves SystemAdmin sees all entries; test case 5 proves chain verification is SystemAdmin-only.

## Not Proven By This UAT

- AUDT-03 (6-year retention) — The no-DELETE trigger prevents accidental deletion, but no enforcement of retention windows, archival tooling, or purge policy exists in S03. This is a Phase 1 deferral; retention enforcement belongs in S09.
- Full end-to-end browser rendering of the AuditLog React component with live data — requires native Tauri WebView not accessible from automated browser tooling. The component is mounted in App.tsx, TypeScript compiles, and the Tauri dev server starts successfully; visual rendering in the native window is not machine-verified.
- Audit log behaviour under concurrent multi-user sessions — unit tests use single-connection in-memory DBs; concurrent write ordering is not tested.
- Audit log behaviour when the database is near capacity or disk is full — no stress/capacity tests in S03.
- AUDT-03 — retention is architectural (no DELETE) but not actively enforced (no purge commands blocked, no archival triggered).

## Notes for Tester

- All critical correctness properties (hash chain, immutability, role scoping, field completeness) are machine-verified by unit tests. Manual UAT should focus on the runtime experience: does the AuditLog UI actually appear after login, do the rows populate after FHIR operations, does the Verify Chain button work.
- The `device_id` column is not shown in the AuditLog UI table — it's stored in the database but not rendered as a column (confirmed in `src/components/AuditLog.tsx`). If you need to inspect the device_id, use SystemAdmin's Verify Chain result or query the DB directly.
- Pre-T04 development databases may have rows with `device_id = "DEVICE_PENDING"` — these are normal legacy dev rows and do not indicate a bug in the current build.
- The AuditLog component renders below the FhirExplorer in App.tsx — scroll down to find it if not immediately visible.
- Action badge colours: `auth.*` actions are blue, `fhir.*` are green, `break_glass.*` are amber — colour coding is for visual scan only, not security-significant.
