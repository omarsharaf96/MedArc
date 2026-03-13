# T03: 03-audit-logging 03

**Slice:** S03 — **Milestone:** M001

## Description

Expose the audit log to the frontend: two Tauri backend commands (get_audit_log, verify_audit_chain) and a React AuditLog component that renders a role-scoped table of entries.

Purpose: AUDT-04 and AUDT-05 require Provider and SystemAdmin to be able to view audit entries — the backend enforces the role filter, the frontend renders the data.
Output: Working audit log view accessible from the React app, backed by the role-scoped query layer from Plan 01.

## Must-Haves

- [ ] "Provider can invoke get_audit_log and receives only their own entries"
- [ ] "SystemAdmin can invoke get_audit_log and receives all entries"
- [ ] "SystemAdmin can invoke verify_audit_chain and receives a boolean integrity status"
- [ ] "The AuditLog React component renders a table of audit rows with timestamp, action, resource, success columns"
- [ ] "A role other than Provider or SystemAdmin invoking get_audit_log receives an Unauthorized error"

## Files

- `src-tauri/src/commands/audit.rs`
- `src-tauri/src/commands/mod.rs`
- `src/components/AuditLog.tsx`
- `src/App.tsx`
