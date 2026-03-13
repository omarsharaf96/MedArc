# T02: 03-audit-logging 02

**Slice:** S03 — **Milestone:** M001

## Description

Inject write_audit_entry() calls into every ePHI-touching command: all 5 FHIR commands (create, get, list, update, delete), login, logout, activate_break_glass, and deactivate_break_glass.

Purpose: This is the integration wire that makes AUDT-01 real — every ePHI access, including failures, lands in the audit_logs table with a valid hash chain entry.
Output: 9 commands fully instrumented; both success and failure paths produce audit rows.

## Must-Haves

- [ ] "Every FHIR create/read/list/update/delete call writes an audit row, even when the operation fails"
- [ ] "Login and logout events write audit rows with action_type LOGIN/LOGOUT"
- [ ] "Break-glass activate and deactivate write audit rows with action_type BREAK_GLASS_ACTIVATE/BREAK_GLASS_DEACTIVATE"
- [ ] "Audit writes happen within the same Mutex lock hold as the FHIR operation (no double-lock)"
- [ ] "Denied permission attempts (Unauthorized errors before DB lock) write audit rows with success = false"

## Files

- `src-tauri/src/commands/fhir.rs`
- `src-tauri/src/commands/auth.rs`
- `src-tauri/src/commands/break_glass.rs`
