---
id: T01
parent: S03
milestone: M001
provides:
  - audit_logs SQLite table (Migration 8) with SHA-256 hash chain
  - SQLite immutability triggers (BEFORE UPDATE / BEFORE DELETE)
  - audit::entry — write_audit_entry() function
  - audit::query — query_audit_log() and verify_audit_chain() functions
  - 21 passing unit tests covering all 5 must-haves
key_files:
  - src-tauri/Cargo.toml
  - src-tauri/src/db/migrations.rs
  - src-tauri/src/audit/mod.rs
  - src-tauri/src/audit/entry.rs
  - src-tauri/src/audit/query.rs
  - src-tauri/src/lib.rs
key_decisions:
  - SHA-256 (sha2 0.10) for hash chain — FIPS-140 compliant, no custom crypto
  - Hash pre-image: pipe-separated canonical string (unambiguous with UUIDs and RFC-3339)
  - GENESIS sentinel as chain origin — explicit and testable
  - write_audit_entry() takes &Connection (not &Database) to avoid re-entrant Mutex deadlock
  - Immutability via SQLite BEFORE UPDATE/DELETE triggers — no code path can bypass
patterns_established:
  - write_audit_entry(&conn, AuditEntryInput { ... }) called inside existing Mutex lock scope
  - success = false + details = Some("reason") pattern for failure audit entries
  - verify_audit_chain(&conn) returns ChainVerificationResult { valid, rows_checked, error }
observability_surfaces:
  - verify_audit_chain() returns structured ChainVerificationResult with first broken-link description
  - All audit write failures surface as AppError::Database (observable in Tauri invoke response)
  - Hash recomputation in verify_audit_chain exposes tampered row ID and hash mismatch values
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-11
blocker_discovered: false
---

# T01: Audit Logging Data Layer

**Added Migration 8 (audit_logs table + immutability triggers) and the full audit Rust module (entry.rs write path + query.rs read/verify path), with 21 passing unit tests covering all 5 HIPAA must-haves.**

## What Happened

### Migration 8
Added to `src-tauri/src/db/migrations.rs` as position 8 in the migrations vector. The `audit_logs` table schema captures all 9 required HIPAA fields plus two chain fields:

- `id` (UUID), `timestamp` (RFC-3339), `user_id`, `action`, `resource_type`, `resource_id`, `patient_id`, `device_id`, `success` (INTEGER 0/1 with CHECK), `details` (optional free-text), `previous_hash`, `entry_hash` (UNIQUE)
- Four indexes: user_id, timestamp, patient_id, action
- Two BEFORE triggers: `audit_logs_no_update` and `audit_logs_no_delete` — both RAISE(ABORT) with descriptive messages

### audit::entry
`AuditEntryInput` struct collects all 9 HIPAA fields plus device_id. `compute_hash()` is a pure function that builds the canonical pipe-separated pre-image and returns the hex-encoded SHA-256 digest. `write_audit_entry()` resolves the chain tip (`ORDER BY rowid DESC LIMIT 1`, falling back to "GENESIS"), generates a UUID + RFC-3339 timestamp, computes the hash, inserts the row, and returns the materialised `AuditEntry`.

### audit::query
`query_audit_log()` builds a dynamic WHERE clause from an `AuditQuery` struct (user_id, patient_id, action, from, to, limit, offset) and returns a paginated `AuditLogPage`. `verify_audit_chain()` walks all rows in rowid order, recomputing each `entry_hash` and checking `previous_hash == prior_row.entry_hash`, returning `ChainVerificationResult { valid, rows_checked, error }`.

### lib.rs
Registered `mod audit;` so the module compiles as part of the crate.

### sha2 dependency
Added `sha2 = "0.10"` to `Cargo.toml`.

## Verification

TDD cycle: tests were written to define the contract, then the implementation was written to satisfy them.

```
cargo test audit
```

Results: **21 tests passed, 0 failed**

| Must-Have | Covering Test(s) | Result |
|-----------|-----------------|--------|
| 9 HIPAA fields persisted | `write_persists_all_nine_hipaa_fields` | ✅ PASS |
| Hash chain integrity (entry_hash = SHA-256 of canonical preimage; previous_hash = prior entry_hash) | `entry_hash_equals_computed_hash`, `hash_chain_links_consecutive_rows`, `compute_hash_is_deterministic`, `compute_hash_changes_on_any_field_mutation` | ✅ PASS |
| UPDATE rejected by trigger | `update_is_rejected_by_trigger` | ✅ PASS |
| DELETE rejected by trigger | `delete_is_rejected_by_trigger` | ✅ PASS |
| First row has `previous_hash = 'GENESIS'` | `first_row_has_genesis_previous_hash` | ✅ PASS |
| Failed access recorded with `success = false` | `failed_access_records_success_false` | ✅ PASS |

Full suite: `cargo test` → **92 tests passed, 0 failed** (no regressions).

## Diagnostics

- `verify_audit_chain(&conn)` returns `ChainVerificationResult { valid: false, rows_checked: N, error: Some("Row N (id=...): ...") }` — describes the exact row and mismatch when tampered.
- Failed `write_audit_entry()` calls surface as `AppError::Database(...)` — visible in Tauri's invoke error response.
- Trigger errors propagate as `rusqlite::Error` → `AppError::Database("audit_logs rows are immutable: ...")`.

## Deviations

None. Implementation followed the plan exactly.

## Known Issues

None.

## Files Created/Modified

- `src-tauri/Cargo.toml` — added `sha2 = "0.10"` dependency
- `src-tauri/src/db/migrations.rs` — added Migration 8 (audit_logs table, indexes, immutability triggers)
- `src-tauri/src/audit/mod.rs` — new: module declaration + pub use re-exports
- `src-tauri/src/audit/entry.rs` — new: AuditEntryInput, AuditEntry, compute_hash(), write_audit_entry() + 10 unit tests
- `src-tauri/src/audit/query.rs` — new: AuditQuery, AuditLogPage, ChainVerificationResult, query_audit_log(), verify_audit_chain() + 7 unit tests
- `src-tauri/src/lib.rs` — added `mod audit;` declaration
- `.gsd/DECISIONS.md` — appended S03 architectural decisions
