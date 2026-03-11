# S03: Audit Logging

**Goal:** Every access to electronic protected health information is logged with tamper-proof cryptographic hash chains, viewable by authorized users
**Demo:** Perform a FHIR CRUD operation and confirm the audit log records timestamp, user ID, action, patient ID, device ID, and hash-chain integrity; verify Provider can view own entries and System Admin can view all

## Must-Haves


## Tasks

- [x] **T01: 03-audit-logging 01**
  - Create the audit logging data layer: Migration 8 (audit_logs table + immutability triggers) and the audit Rust module (entry.rs for writing, query.rs for reading and chain verification).

Purpose: Every subsequent FHIR command in Plan 02 will call write_audit_entry() from this module. The hash chain and trigger immutability established here are the cryptographic backbone of HIPAA compliance.
Output: Tested audit module with TDD cycle — tests written RED first, implementation makes them GREEN.
- [x] **T02: 03-audit-logging 02**
  - Inject write_audit_entry() calls into every ePHI-touching command: all 5 FHIR commands (create, get, list, update, delete), login, logout, activate_break_glass, and deactivate_break_glass.

Purpose: This is the integration wire that makes AUDT-01 real — every ePHI access, including failures, lands in the audit_logs table with a valid hash chain entry.
Output: 9 commands fully instrumented; both success and failure paths produce audit rows.
- [x] **T03: 03-audit-logging 03**
  - Expose the audit log to the frontend: two Tauri backend commands (get_audit_log, verify_audit_chain) and a React AuditLog component that renders a role-scoped table of entries.

Purpose: AUDT-04 and AUDT-05 require Provider and SystemAdmin to be able to view audit entries — the backend enforces the role filter, the frontend renders the data.
Output: Working audit log view accessible from the React app, backed by the role-scoped query layer from Plan 01.
- [x] **T04: 03-audit-logging 04**
  - Wire everything together in lib.rs: register DeviceId state from machine-uid, add audit commands to invoke_handler, and verify the full end-to-end flow works in the running application.

Purpose: Plans 01-03 built all the pieces; this plan makes them reachable at runtime. Without this, FHIR commands can't receive device_id_state and audit commands aren't callable from the frontend.
Output: A fully wired, running application where every ePHI operation produces an auditable hash-chain entry viewable in the AuditLog UI.

## Files Likely Touched

- `src-tauri/Cargo.toml`
- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/audit/mod.rs`
- `src-tauri/src/audit/entry.rs`
- `src-tauri/src/audit/query.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/commands/fhir.rs`
- `src-tauri/src/commands/auth.rs`
- `src-tauri/src/commands/break_glass.rs`
- `src-tauri/src/commands/audit.rs`
- `src-tauri/src/commands/mod.rs`
- `src/components/AuditLog.tsx`
- `src/App.tsx`
- `src-tauri/src/lib.rs`
