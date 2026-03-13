# T02: 02-auth-access-control 02

**Slice:** S02 — **Milestone:** M001

## Description

Implement the role-based access control (RBAC) engine with a static 5-role permission matrix, field-level JSON filtering for FHIR resources, and emergency break-glass access with logging.

Purpose: HIPAA requires role-based access control where each user can only access data appropriate to their role. This plan builds the enforcement layer that wraps all data commands, ensuring a Nurse cannot prescribe and a Front Desk clerk cannot read clinical notes. Break-glass provides the HIPAA-required emergency override with full accountability.

Output: RBAC module (roles, permissions, field filtering), middleware for permission enforcement on all FHIR commands, and break-glass activation/deactivation commands with database logging.

## Must-Haves

- [ ] "System enforces 5 distinct roles: SystemAdmin, Provider, NurseMa, BillingStaff, FrontDesk"
- [ ] "Each role has correct permissions per the RBAC matrix (e.g., NurseMa can read+update clinical but not create/delete)"
- [ ] "BillingStaff sees only demographics and billing codes when reading Patient resources, not clinical data"
- [ ] "FrontDesk sees only demographics when reading Patient resources"
- [ ] "Provider and SystemAdmin see all fields on all resources"
- [ ] "Break-glass activation requires a mandatory reason string"
- [ ] "Break-glass sessions are time-limited (30 minutes) and scoped to clinical read-only"
- [ ] "Break-glass activation and deactivation are recorded in the break_glass_log table"

## Files

- `src-tauri/src/rbac/mod.rs`
- `src-tauri/src/rbac/roles.rs`
- `src-tauri/src/rbac/middleware.rs`
- `src-tauri/src/rbac/field_filter.rs`
- `src-tauri/src/commands/fhir.rs`
- `src-tauri/src/commands/break_glass.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
