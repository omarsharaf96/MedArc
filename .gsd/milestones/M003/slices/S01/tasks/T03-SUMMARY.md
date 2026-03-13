---
id: T03
parent: S01
milestone: M003
provides:
  - Migration 15 (pt_note_index table) in src-tauri/src/db/migrations.rs
  - Rust types: PtNoteType, InitialEvalFields, ProgressNoteFields, DischargeSummaryFields, PtNoteFields, PtNoteInput, PtNoteRecord
  - 6 Tauri commands: create_pt_note, get_pt_note, list_pt_notes, update_pt_note, cosign_pt_note, lock_pt_note
  - 4 unit tests: type serialization, round-trip, migration validation, null serialization
key_files:
  - src-tauri/src/db/migrations.rs
  - src-tauri/src/commands/pt_notes.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
key_decisions:
  - Made MIGRATIONS pub static to allow cross-module test access (migration_15_is_valid test)
  - PtNoteFields uses serde tag+content discriminated union so TypeScript can discriminate on noteType without a wrapper type
  - cosign_pt_note audit row details includes both patient_id and encounter_id for S07 visit counter JOIN without schema changes
  - addendum_of FK ships in Migration 15 to avoid a breaking migration in S07 when addendum flow is implemented
patterns_established:
  - PT note lifecycle: draft → signed (cosign_pt_note) → locked (lock_pt_note); each transition validated at command entry, returns AppError::Validation if invalid
  - locked notes reject update_pt_note immediately with AppError::Validation("PT note {id} is locked and cannot be modified")
  - All 6 commands follow documentation.rs pattern: require_authenticated + require_permission + lock conn once + write_audit_entry
  - build_pt_note_fhir stores structured fields as a JSON-stringified extension in the Composition section (same pattern as encounter SOAP notes)
observability_surfaces:
  - "SELECT * FROM pt_note_index ORDER BY created_at DESC LIMIT 20; — all PT notes with type/status/provider"
  - "SELECT * FROM audit_logs WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC LIMIT 20; — full PT note audit trail"
  - "cosign audit row details: 'patient_id={id},encounter_id={id}' for S07 visit counter JOIN"
  - "AppError::Validation with human-readable message on invalid status transitions (locked/not-draft/not-signed)"
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-13
blocker_discovered: false
---

# T03: Add Migration 15, PT note Rust types, and pt_notes.rs commands

**Built the complete PT note data model: Migration 15 (pt_note_index), all three note shape types (InitialEval, ProgressNote, DischargeSummary), and 6 Tauri commands with full RBAC + audit coverage.**

## What Happened

1. **Migration 15 appended** to `src-tauri/src/db/migrations.rs` as the 15th `M::up` entry (index 14). The `pt_note_index` table has `pt_note_id` as PK, `CHECK` constraints on `note_type` and `status`, the `addendum_of` self-referential FK, and three covering indexes. Migrations 0–13 were not touched. `MIGRATIONS` was made `pub static` to allow the cross-module `migration_15_is_valid` test.

2. **`src-tauri/src/commands/pt_notes.rs` created** with:
   - `PtNoteType` enum (`InitialEval`, `ProgressNote`, `DischargeSummary`) with `Display` returning the snake_case DB string
   - `InitialEvalFields` (14 `Option<String>` fields), `ProgressNoteFields` (11 fields), `DischargeSummaryFields` (8 fields including `outcome_comparison_placeholder`)
   - `PtNoteFields` discriminated union with `#[serde(tag = "noteType", content = "fields", rename_all = "snake_case")]`
   - `PtNoteInput` and `PtNoteRecord` I/O types
   - `build_pt_note_fhir` FHIR Composition builder
   - 6 Tauri commands: `create_pt_note`, `get_pt_note`, `list_pt_notes`, `update_pt_note`, `cosign_pt_note`, `lock_pt_note`
   - 4 unit tests

3. **Module registered**: `pub mod pt_notes;` added to `commands/mod.rs`

4. **Commands registered**: all 6 added to `lib.rs` invoke_handler under `// M003/S01 — PT Note Templates`

## Verification

```
cd src-tauri && cargo test --lib 2>&1 | tail -5
# → test result: ok. 272 passed; 0 failed

cd src-tauri && cargo test --lib pt_note 2>&1
# → 4 tests: pt_note_type_serializes_correctly, pt_note_type_all_variants_round_trip,
#             migration_15_is_valid, initial_eval_fields_serialize_nulls — all ok

cd .. && npx tsc --noEmit 2>&1 | tail -5
# → (no output — 0 errors)
```

All must-haves verified: migration index 14 ✓, CHECK constraints ✓, `pt_note_id` PK ✓, `addendum_of` FK ✓, `outcome_comparison_placeholder` ✓, 6 commands with RBAC + audit ✓, status transition guards ✓, cosign details field ✓, module registered ✓.

## Diagnostics

- PT note index: `SELECT * FROM pt_note_index ORDER BY created_at DESC LIMIT 20;`
- Audit trail: `SELECT * FROM audit_logs WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC LIMIT 20;`
- Cosign rows: `SELECT details FROM audit_logs WHERE action = 'pt_note.cosign';` — shows `patient_id=X,encounter_id=Y` for S07
- Invalid status transitions surface as `AppError::Validation` with message: `"PT note {id} is locked and cannot be modified"` / `"cannot be co-signed: expected status 'draft', found '{actual}'"` / `"cannot be locked: expected status 'signed', found '{actual}'"`

## Deviations

None — implementation follows the plan exactly.

## Known Issues

None.

## Files Created/Modified

- `src-tauri/src/db/migrations.rs` — Migration 15 (pt_note_index) appended; MIGRATIONS made pub static
- `src-tauri/src/commands/pt_notes.rs` — new file: PT note types + 6 commands + 4 unit tests
- `src-tauri/src/commands/mod.rs` — `pub mod pt_notes;` added
- `src-tauri/src/lib.rs` — 6 new commands registered in invoke_handler
