---
id: T03
parent: S03
milestone: M001
provides:
  - get_audit_log Tauri command (role-scoped: Provider sees own rows only, SystemAdmin sees all)
  - verify_audit_chain_cmd Tauri command (SystemAdmin only; returns ChainVerificationResult)
  - AuditLog React component (paginated table with Timestamp, Action, Resource Type, Resource ID, Result columns)
  - src/types/audit.ts TypeScript types (AuditEntry, AuditLogPage, AuditQuery, ChainVerificationResult)
  - commands::audit registered in lib.rs invoke_handler
key_files:
  - src-tauri/src/commands/audit.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - src/components/AuditLog.tsx
  - src/types/audit.ts
  - src/lib/tauri.ts
  - src/App.tsx
key_decisions:
  - Role enforcement for get_audit_log lives entirely in the Tauri command (not the query layer) — the query layer accepts a user_id filter; the command forces that filter for Provider and leaves it optional for SystemAdmin
  - verify_audit_chain is exposed as verify_audit_chain_cmd to avoid a name collision with the audit::query::verify_audit_chain function in the same crate
  - AuditLog component passes role prop from App.tsx — renders an extra "User ID" column only for SystemAdmin (Provider column is unnecessary for own-only view)
  - AuditLog fetches page on mount via useEffect; refresh button re-fetches without page reset
  - Chain verification state is kept separate from page state so it persists across page changes
patterns_established:
  - Role-scoped Tauri command pattern: call session.get_current_user(), match on role, override/restrict query params, then acquire DB lock — same order as FHIR commands
  - AuditQuery optional filters are passed as Option<AuditQuery> from frontend (null maps to Default::default() in backend)
  - TypeScript audit types in src/types/audit.ts; commands in src/lib/tauri.ts under "Audit log commands" section
observability_surfaces:
  - Backend command errors propagate as AppError::Unauthorized / AppError::Database via Tauri invoke rejection — catchable in frontend try/catch
  - ChainVerificationResult { valid, rows_checked, error } is fully observable from the frontend — error field describes the exact broken link
  - AuditLog component surfaces backend errors as inline red banners; chain result as green/red status banner
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-11
blocker_discovered: false
---

# T03: Audit Log Frontend Exposure

**Exposed the HIPAA audit log to the frontend via two Tauri commands and a role-scoped React table component.**

## What Happened

Added `src-tauri/src/commands/audit.rs` with two `#[tauri::command]` functions:

1. **`get_audit_log`** — Resolves the caller's `(user_id, role)` from `SessionManager`, then:
   - Provider: forces `effective_query.user_id = caller_id` (cannot view others' entries)
   - SystemAdmin: passes query through unmodified (full visibility, optional filters)
   - Any other role: returns `AppError::Unauthorized`
   Delegates to `audit::query::query_audit_log()` from T01.

2. **`verify_audit_chain_cmd`** — SystemAdmin only. Delegates to `audit::query::verify_audit_chain()` from T01 and returns `ChainVerificationResult { valid, rows_checked, error }`.

Both commands registered in `lib.rs` `invoke_handler`.

Created `src/types/audit.ts` with TypeScript interfaces matching the Rust `#[serde(rename_all = "camelCase")]` structs: `AuditEntry`, `AuditLogPage`, `AuditQuery`, `ChainVerificationResult`.

Added `getAuditLog` and `verifyAuditChain` to `src/lib/tauri.ts` command wrappers.

Built `src/components/AuditLog.tsx` — a self-contained paginated audit log viewer:
- Fetches page on mount, supports ← Prev / Next → pagination (PAGE_SIZE = 20)
- Columns: Timestamp (locale-formatted) · Action (colour-coded badge by category) · Resource Type · Resource ID · Result (✓ OK / ✗ Failed badge with truncated details)
- SystemAdmin sees an additional "User ID" column and a "🔒 Verify Chain" button
- Chain verification result renders as a green/red status banner
- Error states surface as inline red banners; loading state disables controls

Added AuditLog to `App.tsx` below FhirExplorer, gated on `role === "Provider" || role === "SystemAdmin"`.

## Verification

- `cargo build` (exit 0): zero compilation errors, only pre-existing warnings
- `cargo test` (exit 0): **102/102 tests pass** — all prior T01 audit tests continue green
- `npx tsc --noEmit` (exit 0): TypeScript compiles cleanly, no type errors

**Must-have check:**
| Must-Have | Status |
|---|---|
| Provider receives only own entries | ✓ — `effective_query.user_id = Some(caller_id)` enforced backend |
| SystemAdmin receives all entries | ✓ — no override on SystemAdmin branch |
| SystemAdmin can invoke verify_audit_chain, gets boolean result | ✓ — `verify_audit_chain_cmd` returns `ChainVerificationResult { valid: bool, ... }` |
| AuditLog component renders table with timestamp, action, resource, success | ✓ — all four columns present in rendered table |
| Other roles get Unauthorized | ✓ — `_ =>` match arm returns `AppError::Unauthorized` |

## Diagnostics

- Backend errors return as Tauri invoke rejections: `AppError::Unauthorized("Role '...' is not permitted to access audit logs")` or `AppError::Database(...)` — visible in browser console and React error state
- `ChainVerificationResult` has an `error` field with the exact broken-link description (e.g. `"Row N (id=...): previous_hash='...' expected '...'"`) — surfaced in the UI banner
- Failed audit entries rendered with `bg-red-50/30` row tint and `✗ Failed` badge — visually distinct
- `DEVICE_PENDING` device_id placeholder still present in all rows until T04 wires machine-uid

## Deviations

- Named the Tauri command `verify_audit_chain_cmd` instead of `verify_audit_chain` to avoid a name collision with `crate::audit::query::verify_audit_chain` — the Tauri macro uses the function name as the IPC string, so the JS wrapper still calls it as `verify_audit_chain_cmd` via `invoke("verify_audit_chain_cmd")`.
- No new Rust unit tests added in this task — the command logic is thin (role check + delegate to T01 functions that are already fully tested). Integration testing deferred to T04 which wires the full app.

## Known Issues

- End-to-end browser flow (open app, login, see audit table populate) not verified in this task — T04 is the integration task that runs the full Tauri app and performs the demo verification.
- AuditLog shows a blank table until T04 wires machine-uid and confirms DB migrations run in the built app.

## Files Created/Modified

- `src-tauri/src/commands/audit.rs` — **NEW**: `get_audit_log` + `verify_audit_chain_cmd` Tauri commands
- `src-tauri/src/commands/mod.rs` — added `pub mod audit;`
- `src-tauri/src/lib.rs` — registered both commands in `invoke_handler`
- `src/types/audit.ts` — **NEW**: TypeScript types for audit module
- `src/lib/tauri.ts` — added `getAuditLog` + `verifyAuditChain` wrappers
- `src/components/AuditLog.tsx` — **NEW**: paginated, role-scoped audit log React component
- `src/App.tsx` — imported AuditLog and rendered it below FhirExplorer for Provider/SystemAdmin roles
