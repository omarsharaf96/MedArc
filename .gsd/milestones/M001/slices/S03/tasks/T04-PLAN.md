# T04: 03-audit-logging 04

**Slice:** S03 — **Milestone:** M001

## Description

Wire everything together in lib.rs: register DeviceId state from machine-uid, add audit commands to invoke_handler, and verify the full end-to-end flow works in the running application.

Purpose: Plans 01-03 built all the pieces; this plan makes them reachable at runtime. Without this, FHIR commands can't receive device_id_state and audit commands aren't callable from the frontend.
Output: A fully wired, running application where every ePHI operation produces an auditable hash-chain entry viewable in the AuditLog UI.

## Must-Haves

- [ ] "App starts successfully and DeviceId state is registered before any command handler runs"
- [ ] "All 9 instrumented commands (create/get/list/update/delete resource, login, logout, activate_break_glass, deactivate_break_glass) are registered in invoke_handler"
- [ ] "get_audit_log and verify_audit_chain Tauri commands are registered in invoke_handler"
- [ ] "Performing a FHIR create in the running app produces a visible audit row in the AuditLog UI"
- [ ] "The AuditLog UI shows the Provider's own entries after login and a FHIR operation"

## Files

- `src-tauri/src/lib.rs`
