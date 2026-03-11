# T01: 03-audit-logging 01

**Slice:** S03 — **Milestone:** M001

## Description

Create the audit logging data layer: Migration 8 (audit_logs table + immutability triggers) and the audit Rust module (entry.rs for writing, query.rs for reading and chain verification).

Purpose: Every subsequent FHIR command in Plan 02 will call write_audit_entry() from this module. The hash chain and trigger immutability established here are the cryptographic backbone of HIPAA compliance.
Output: Tested audit module with TDD cycle — tests written RED first, implementation makes them GREEN.

## Must-Haves

- [ ] "Every ePHI access produces a persisted audit row with all 9 required HIPAA fields"
- [ ] "The entry_hash of row N equals SHA-256 of (previous_hash|id|timestamp|...), and previous_hash of row N equals entry_hash of row N-1"
- [ ] "Attempting to UPDATE or DELETE any audit_logs row aborts with a SQLite trigger error"
- [ ] "The first audit row has previous_hash = 'GENESIS'"
- [ ] "A failed ePHI access is recorded with success = false"

## Files

- `src-tauri/Cargo.toml`
- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/audit/mod.rs`
- `src-tauri/src/audit/entry.rs`
- `src-tauri/src/audit/query.rs`
- `src-tauri/src/lib.rs`
