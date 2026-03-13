---
estimated_steps: 6
estimated_files: 4
---

# T03: Add Migration 15, PT note Rust types, and pt_notes.rs commands

**Slice:** S01 — Touch ID Fix + PT Note Templates
**Milestone:** M003

## Description

This task builds the PT note data model: the `pt_note_index` migration, the Rust input/output types for all three note shapes (Initial Evaluation, Daily Progress Note, Discharge Summary), and the 6 Tauri commands (`create_pt_note`, `get_pt_note`, `list_pt_notes`, `update_pt_note`, `cosign_pt_note`, `lock_pt_note`).

All commands follow the established pattern from `documentation.rs`: FHIR Composition stored in `fhir_resources`, index table for fast queries, `require_authenticated` + `require_permission(ClinicalDocumentation, ...)`, `write_audit_entry` on every path. The addendum linkage field (`addendum_of`) ships in the schema now so S07 doesn't require a breaking migration later.

This task does NOT touch the frontend. T04 adds TypeScript types; T05 adds UI.

## Steps

1. **Append Migration 15 to `src-tauri/src/db/migrations.rs`**:
   ```sql
   CREATE TABLE IF NOT EXISTS pt_note_index (
       pt_note_id  TEXT PRIMARY KEY NOT NULL,
       patient_id  TEXT NOT NULL,
       encounter_id TEXT,
       note_type   TEXT NOT NULL
                   CHECK(note_type IN ('initial_eval','progress_note','discharge_summary')),
       status      TEXT NOT NULL DEFAULT 'draft'
                   CHECK(status IN ('draft','signed','locked')),
       provider_id TEXT NOT NULL,
       addendum_of TEXT REFERENCES pt_note_index(pt_note_id),
       created_at  TEXT NOT NULL,
       updated_at  TEXT NOT NULL
   );
   CREATE INDEX IF NOT EXISTS idx_pt_note_patient ON pt_note_index(patient_id);
   CREATE INDEX IF NOT EXISTS idx_pt_note_type    ON pt_note_index(note_type);
   CREATE INDEX IF NOT EXISTS idx_pt_note_status  ON pt_note_index(status);
   ```
   This is Migration index 14 (0-indexed, as the 15th `M::up` entry). Never modify migrations 0–13.

2. **Create `src-tauri/src/commands/pt_notes.rs`** — header comment explaining PT-DOC-01 through PT-DOC-04. Imports: `serde`, `tauri::State`, `uuid`, `chrono`, `crate::audit::*`, `crate::auth::session::SessionManager`, `crate::db::connection::Database`, `crate::device_id::DeviceId`, `crate::error::AppError`, `crate::rbac::middleware`, `crate::rbac::roles::{Action, Resource}`.

3. **Define Rust types** (all `#[derive(Debug, Clone, Serialize, Deserialize)]`, `#[serde(rename_all = "camelCase")]`):
   - `PtNoteType` enum with `#[serde(rename_all = "snake_case")]`: `InitialEval`, `ProgressNote`, `DischargeSummary`. Implement `Display` returning `"initial_eval"` etc. for the DB column.
   - `InitialEvalFields`: `chief_complaint`, `mechanism_of_injury`, `prior_level_of_function`, `pain_nrs`, `functional_limitations`, `icd10_codes`, `physical_exam_findings`, `short_term_goals`, `long_term_goals`, `plan_of_care`, `frequency_duration`, `cpt_codes`, `referring_physician`, `referral_document_id` — all `Option<String>`.
   - `ProgressNoteFields`: `subjective`, `patient_report_pain_nrs`, `hep_compliance`, `barriers`, `treatments`, `exercises`, `assessment`, `progress_toward_goals`, `plan`, `hep_updates`, `total_treatment_minutes` — all `Option<String>`.
   - `DischargeSummaryFields`: `total_visits_attended`, `total_visits_authorized`, `treatment_summary`, `goal_achievement`, `outcome_comparison_placeholder: Option<String>` (S02 fills this), `discharge_recommendations`, `hep_narrative`, `return_to_care` — all `Option<String>`.
   - `PtNoteFields` enum: `InitialEval(InitialEvalFields)`, `ProgressNote(ProgressNoteFields)`, `DischargeSummary(DischargeSummaryFields)` — with `#[serde(tag = "noteType", content = "fields", rename_all = "snake_case")]`.
   - `PtNoteInput`: `patient_id: String`, `encounter_id: Option<String>`, `note_type: PtNoteType`, `fields: Option<PtNoteFields>`, `addendum_of: Option<String>`.
   - `PtNoteRecord`: `id: String`, `patient_id: String`, `encounter_id: Option<String>`, `note_type: String`, `status: String`, `provider_id: String`, `resource: serde_json::Value`, `created_at: String`, `updated_at: String`, `addendum_of: Option<String>`.

4. **Implement the 6 commands** following `documentation.rs` create_encounter/get_encounter pattern:
   - `create_pt_note(input, db, session, device_id)`: require `ClinicalDocumentation::Create`. Generate UUID, build FHIR Composition JSON (`resourceType: "Composition"`, `type.coding[0].code` from note_type, `type.coding[0].system: "http://medarc.local/fhir/CodeSystem/pt-note-type"`, `subject.reference: "Patient/{patient_id}"`, `date: now`). INSERT into `fhir_resources` (resource_type = "PTNote") and `pt_note_index`. Write audit `pt_note.create`.
   - `get_pt_note(pt_note_id, db, session, device_id)`: require `ClinicalDocumentation::Read`. JOIN `fhir_resources` + `pt_note_index`. Write audit `pt_note.read`.
   - `list_pt_notes(patient_id, note_type, db, session, device_id)`: require `ClinicalDocumentation::Read`. Query `pt_note_index` by `patient_id` (+ optional `note_type` filter), JOIN `fhir_resources`. Write audit `pt_note.list`.
   - `update_pt_note(pt_note_id, input, db, session, device_id)`: require `ClinicalDocumentation::Update`. Check status != "locked" (locked notes are read-only → `Err(AppError::Validation(...))`). Update `fhir_resources.resource` and `pt_note_index.updated_at`. Write audit `pt_note.update`.
   - `cosign_pt_note(pt_note_id, db, session, device_id)`: require `ClinicalDocumentation::Update`. Check current status == "draft" → transition to "signed". UPDATE `pt_note_index.status = 'signed'`. Write audit `pt_note.cosign` with `details: Some(format!("patient_id={},encounter_id={}", ...))` (S07 hooks here).
   - `lock_pt_note(pt_note_id, db, session, device_id)`: require `ClinicalDocumentation::Update`. Check current status == "signed" → transition to "locked". UPDATE `pt_note_index.status = 'locked'`. Write audit `pt_note.lock`.

5. **Register the module**:
   - Add `pub mod pt_notes;` to `src-tauri/src/commands/mod.rs`.
   - Add all 6 commands to `lib.rs` invoke_handler after the existing backup commands.

6. **Add unit tests in `pt_notes.rs`** inside `#[cfg(test)]`:
   - `pt_note_type_serializes_correctly`: assert `serde_json::to_string(&PtNoteType::InitialEval).unwrap()` == `"\"initial_eval\""`.
   - `pt_note_type_all_variants_round_trip`: round-trip all 3 variants through JSON.
   - `migration_15_is_valid`: import `MIGRATIONS` from `db::migrations` and assert `MIGRATIONS.validate().is_ok()` (this verifies Migration 15 syntax is accepted by rusqlite_migration).
   - `initial_eval_fields_serialize_nulls`: create an `InitialEvalFields` with all None, serialize to JSON, assert all optional fields are `null` (not missing).

## Must-Haves

- [ ] Migration 15 is the 15th `M::up` in the vector (index 14) — migrations 0–13 are NOT modified
- [ ] `pt_note_index` has `CHECK(note_type IN ('initial_eval','progress_note','discharge_summary'))` and `CHECK(status IN ('draft','signed','locked'))`
- [ ] `pt_note_index` uses `pt_note_id` as PK (not `id`) — avoids shadowing `fhir_resources.id`
- [ ] `addendum_of` FK column is present in `pt_note_index` from day one
- [ ] `outcome_comparison_placeholder: Option<String>` present in `DischargeSummaryFields`
- [ ] All 6 commands use `require_authenticated` + `require_permission(ClinicalDocumentation, ...)`
- [ ] All 6 commands write audit rows — `let _ = write_audit_entry(...)` pattern
- [ ] `cosign_pt_note` audit row `details` includes `patient_id` and `encounter_id` for S07
- [ ] `update_pt_note` returns `AppError::Validation` if note status is "locked"
- [ ] `cosign_pt_note` returns `AppError::Validation` if status is not "draft"
- [ ] `lock_pt_note` returns `AppError::Validation` if status is not "signed"
- [ ] `pub mod pt_notes;` in `commands/mod.rs`
- [ ] All 6 commands registered in `lib.rs` invoke_handler
- [ ] `cargo test --lib` passes (265 existing + new PT model tests, 0 failures)

## Verification

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
```
Expected: shows all tests passing with `0 failed`. Count of tests increases by at least 4 (the new PT note tests).

```bash
cd src-tauri && cargo test --lib pt_note 2>&1
```
Expected: new PT note tests listed and passing.

## Observability Impact

- Signals added/changed: New audit action strings `pt_note.create`, `pt_note.read`, `pt_note.list`, `pt_note.update`, `pt_note.cosign`, `pt_note.lock`. Each row carries `patient_id` and `resource_id` (the PT note UUID). `cosign_pt_note` additionally puts `patient_id` and `encounter_id` in the `details` field so S07's visit counter can JOIN without schema changes.
- How a future agent inspects this: `SELECT * FROM pt_note_index ORDER BY created_at DESC LIMIT 20;` gives all PT notes with type/status. `SELECT * FROM audit_log WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC LIMIT 20;` gives all PT note activity.
- Failure state exposed: Invalid status transitions return `AppError::Validation` with a human-readable message surfaced to the frontend. Locked notes reject updates with a clear error.

## Inputs

- `src-tauri/src/db/migrations.rs` — append Migration 15 here
- `src-tauri/src/commands/documentation.rs` — reference for FHIR builder pattern, audit pattern, RBAC middleware calls, index insert pattern
- `src-tauri/src/audit/entry.rs` — `AuditEntryInput` struct fields
- `src-tauri/src/rbac/roles.rs` — `Resource::ClinicalDocumentation`, `Action` enum

## Expected Output

- `src-tauri/src/db/migrations.rs` — Migration 15 appended (vector length grows to 15)
- `src-tauri/src/commands/pt_notes.rs` (new) — full PT note types + 6 commands + unit tests
- `src-tauri/src/commands/mod.rs` — `pub mod pt_notes;` added
- `src-tauri/src/lib.rs` — 6 new commands in invoke_handler
