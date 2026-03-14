---
estimated_steps: 6
estimated_files: 6
---

# T01: Backend — scoring module, Migration 16, and Tauri commands

**Slice:** S02 — Objective Measures & Outcome Scores
**Milestone:** M003

## Description

Create the entire Rust backend for S02: the `objective_measures` command module with pure scoring functions for all six outcome measures, FHIR resource builders, six Tauri commands, and Migration 16 (`outcome_score_index`). Append TypeScript types and wrappers so the frontend can compile. All scoring logic must be unit-tested before T02 begins.

This is the foundational task. T02 and T03 cannot proceed until these Tauri commands exist and `cargo test --lib` passes.

## Steps

1. **Create `src-tauri/src/commands/objective_measures.rs`** with the following sections:
   - *Imports*: mirror `pt_notes.rs` — `use crate::{audit::write_audit_entry, db::Database, error::AppError, rbac::{check_permission, Permission, Resource}}`, plus `serde::{Deserialize, Serialize}`, `uuid::Uuid`, `tauri::State`, and `std::sync::Mutex`.
   - *Input structs* (`#[derive(Debug, Deserialize)]`): `ObjectiveMeasuresInput` (fields: `patient_id: String`, `encounter_id: Option<String>`, `joints: serde_json::Value`, `mmt: serde_json::Value`, `ortho_tests: serde_json::Value`); `OutcomeScoreInput` (fields: `patient_id: String`, `encounter_id: Option<String>`, `measure_type: String`, `items: Vec<f64>`, `episode_phase: String`).
   - *Output structs* (`#[derive(Debug, Serialize)]`, `#[serde(rename_all = "camelCase")]`): `ObjectiveMeasuresRecord` (fields: `id: String`, `patient_id: String`, `encounter_id: Option<String>`, `recorded_at: String`); `OutcomeScoreRecord` (fields: `score_id: String`, `patient_id: String`, `encounter_id: Option<String>`, `measure_type: String`, `score: f64`, `score_secondary: Option<f64>`, `severity_class: String`, `recorded_at: String`, `episode_phase: String`); `OutcomeComparisonMeasure` (fields: `measure_type: String`, `display_name: String`, `unit: String`, `initial_score: f64`, `initial_severity: String`, `initial_date: String`, `discharge_score: f64`, `discharge_severity: String`, `discharge_date: String`, `change: f64`, `mcid: f64`, `achieved_mcid: bool`); `OutcomeComparison` (field: `measures: Vec<OutcomeComparisonMeasure>`).
   - *Pure scoring functions* (all `pub(crate)` for testability):
     - `fn score_lefs(items: &[f64]) -> Result<(f64, String), AppError>` — validates exactly 20 items each 0–4; sum = score; severity thresholds.
     - `fn score_dash(items: &[f64]) -> Result<(f64, String), AppError>` — validates items each 1–5; returns `Err(AppError::Validation(...))` if < 27 answered (check `items.len() < 27`); formula `((sum / n) - 1.0) * 25.0`; severity thresholds.
     - `fn score_ndi(items: &[f64]) -> Result<(f64, String), AppError>` — validates exactly 10 items each 0–5; score = `sum * 2.0` (percentage); severity thresholds in percentage points.
     - `fn score_oswestry(items: &[f64]) -> Result<(f64, String), AppError>` — validates exactly 10 items each 0–5; score = `(sum / 50.0) * 100.0`; severity thresholds.
     - `fn score_psfs(items: &[f64]) -> Result<(f64, String), AppError>` — validates 3–5 items each 0–10; returns `Err(AppError::Validation(...))` if < 3 items; score = average; severity thresholds.
     - `fn score_fabq(items: &[f64]) -> Result<(f64, f64, String), AppError>` — validates exactly 16 items each 0–6; PA subscale = sum of items at 1-indexed positions 2,3,4,5 (0-indexed: 1,2,3,4); work subscale = sum of items at 1-indexed positions 6,7,9,10,11,12,13,14,15 (0-indexed: 5,6,8,9,10,11,12,13,14); items at 1-indexed 1,8,16 (0-indexed 0,7,15) excluded; severity string "high_risk" if PA > 14 or work > 34 else "low_risk"; return `(work_score, pa_score, severity)`.
   - *FHIR builder functions* (pure, no I/O):
     - `fn build_objective_measures_fhir(id: &str, patient_id: &str, encounter_id: Option<&str>, joints_json: &serde_json::Value, mmt_json: &serde_json::Value, ortho_tests_json: &serde_json::Value, recorded_at: &str) -> serde_json::Value` — builds a JSON object with `resourceType: "PTObjectiveMeasures"`, code system `http://medarc.local/fhir/CodeSystem/pt-objective-measures`, and the three blobs as extensions.
     - `fn build_outcome_score_fhir(id: &str, patient_id: &str, score: f64, score_secondary: Option<f64>, loinc_code: &str, display_name: &str, items_json: &str, recorded_at: &str) -> serde_json::Value` — builds a FHIR Observation with `category: survey`, LOINC code, `valueQuantity`, and `extension` with item responses.
   - *Helper*: `fn loinc_for_measure(measure_type: &str) -> Option<(&'static str, &'static str)>` — returns `(loinc_code, display_name)` for the 6 measures; `None` for unknown.
   - *Tauri commands*:
     - `record_objective_measures(patient_id, encounter_id, joints, mmt, ortho_tests, db, session)` → `Result<ObjectiveMeasuresRecord, AppError>`: checks `ClinicalDocumentation::Create` permission; calls builder; inserts into `fhir_resources`; writes audit `"objective_measures.record"`.
     - `get_objective_measures(patient_id, encounter_id, db, session)` → `Result<Option<ObjectiveMeasuresRecord>, AppError>`: checks `ClinicalDocumentation::Read`; queries `fhir_resources` WHERE `resource_type = 'PTObjectiveMeasures'` and patient subject reference; writes audit `"objective_measures.get"`.
     - `record_outcome_score(patient_id, encounter_id, measure_type, items, episode_phase, db, session)` → `Result<OutcomeScoreRecord, AppError>`: checks `ClinicalDocumentation::Create`; calls appropriate scoring function; calls FHIR builder; inserts into both `fhir_resources` and `outcome_score_index`; writes audit `"outcome_score.record"`.
     - `list_outcome_scores(patient_id, measure_type_filter, db, session)` → `Result<Vec<OutcomeScoreRecord>, AppError>`: checks `ClinicalDocumentation::Read`; queries `outcome_score_index` ordered by `recorded_at DESC`; writes audit `"outcome_score.list"`.
     - `get_outcome_score(score_id, db, session)` → `Result<OutcomeScoreRecord, AppError>`: checks `ClinicalDocumentation::Read`; queries `outcome_score_index` by `score_id`; returns `AppError::NotFound` if absent; writes audit `"outcome_score.get"`.
     - `get_outcome_comparison(patient_id, db, session)` → `Result<OutcomeComparison, AppError>`: checks `ClinicalDocumentation::Read`; for each of the 6 measure types, finds the row with `episode_phase = 'initial'` (earliest `recorded_at`) and the row with `episode_phase = 'discharge'` (latest `recorded_at`); builds `OutcomeComparisonMeasure`; includes only measures that have both initial and discharge rows; writes audit `"outcome_comparison.get"`.
   - *Unit tests* (`#[cfg(test)]`): one test per scoring function (happy path) + DASH <27-item error + PSFS <3-item error + FABQ subscale totals verification (hard-coded reference case). At minimum 8 tests.

2. **Append Migration 16 to `src-tauri/src/db/migrations.rs`**: Add a new `M::up(...)` entry at the end of the `MIGRATIONS` vector (index 15, zero-based). SQL creates `outcome_score_index` with PK `score_id`, all columns defined in the research data shape, and three indexes (`idx_outcome_score_patient`, `idx_outcome_score_type`, `idx_outcome_score_date`). Enable `PRAGMA foreign_keys = ON;` in the migration preamble via the existing pattern.

3. **Register module in `src-tauri/src/commands/mod.rs`**: append `pub mod objective_measures;` after `pub mod pt_notes;`.

4. **Register commands in `src-tauri/src/lib.rs`**: append the 6 new commands to `invoke_handler!` under a `// M003/S02` comment block after the `// M003/S01` block.

5. **Append TypeScript types to `src/types/pt.ts`** under a `// M003/S02` comment:
   - `export type MeasureType = "lefs" | "dash" | "ndi" | "oswestry" | "psfs" | "fabq";`
   - `export interface OutcomeScoreInput` (all fields matching Rust input struct, camelCase)
   - `export interface OutcomeScoreRecord` (all fields matching Rust output struct)
   - `export interface ObjectiveMeasuresInput` (patient_id, encounter_id, joints, mmt, orthoTests as unknown for now)
   - `export interface ObjectiveMeasuresRecord` (id, patientId, encounterId, recordedAt)
   - `export interface OutcomeComparisonMeasure` (all fields matching Rust struct)
   - `export interface OutcomeComparison { measures: OutcomeComparisonMeasure[]; }`

6. **Append wrappers to `src/lib/tauri.ts`** under `// M003/S02` comment: `recordObjectiveMeasures`, `getObjectiveMeasures`, `recordOutcomeScore`, `listOutcomeScores`, `getOutcomeScore`, `getOutcomeComparison` — each using `invoke<ReturnType>(commandName, params)` pattern matching existing wrappers.

## Must-Haves

- [ ] All 6 scoring functions exist as pure Rust functions (no DB, no I/O)
- [ ] DASH returns `Err(AppError::Validation(...))` when fewer than 27 items provided
- [ ] PSFS returns `Err(AppError::Validation(...))` when fewer than 3 items provided
- [ ] NDI score is stored as percentage (multiply raw sum by 2, range 0–100)
- [ ] Oswestry score is stored as percentage (formula `(sum / 50.0) * 100.0`)
- [ ] FABQ stores work subscale in `score` column and PA subscale in `score_secondary` column
- [ ] `outcome_score_index` PK column is `score_id` (not `id`)
- [ ] Migration 16 is appended at index 15 (zero-based) in the `MIGRATIONS` vector — never modifying Migrations 1–15
- [ ] `episode_phase` CHECK constraint: `('initial','mid','discharge')`
- [ ] `measure_type` CHECK constraint: `('lefs','dash','ndi','oswestry','psfs','fabq')`
- [ ] All 6 Tauri commands write audit rows on both success and failure paths
- [ ] `get_outcome_comparison` only includes measures with BOTH initial and discharge recorded
- [ ] FABQ unit test uses hard-coded reference case verifying PA and work subscale totals independently
- [ ] `cargo test --lib` passes with ≥8 new tests in `commands::objective_measures::tests`
- [ ] `tsc --noEmit` exits 0 after types and wrappers are added

## Verification

- `cd src-tauri && cargo test --lib 2>&1 | tail -5` — must show `ok. NNN passed; 0 failed` with NNN ≥ 280
- `cd src-tauri && cargo test --lib -- commands::objective_measures` — must show all new tests passing
- `cd .. && npx tsc --noEmit` — must exit 0

## Observability Impact

- Signals added/changed: 6 new audit action strings (`objective_measures.record`, `objective_measures.get`, `outcome_score.record`, `outcome_score.list`, `outcome_score.get`, `outcome_comparison.get`) written to `audit_log` table on every ePHI operation
- How a future agent inspects this: `SELECT * FROM outcome_score_index WHERE patient_id = ?` for score history; `SELECT * FROM fhir_resources WHERE resource_type = 'PTObjectiveMeasures'` for ROM/MMT blob; `cargo test --lib -- commands::objective_measures` for scoring contract
- Failure state exposed: `AppError::Validation` returned (not panicked) for invalid item counts in DASH and PSFS; `AppError::NotFound` for missing score_id in `get_outcome_score`

## Inputs

- `src-tauri/src/commands/pt_notes.rs` — pattern to mirror exactly (audit write, FHIR builder, index insert, command signature shape)
- `src-tauri/src/db/migrations.rs` — Migration 15 as template for Migration 16; must append, never modify
- `src-tauri/src/commands/mod.rs` — add `pub mod objective_measures` after existing `pub mod pt_notes`
- `src-tauri/src/lib.rs` — `invoke_handler!` macro location; append after `// M003/S01` block
- `src/types/pt.ts` — existing `DischargeSummaryFields.outcomeComparisonPlaceholder` field (do not change); append new types at bottom
- `src/lib/tauri.ts` — existing wrapper style; append after existing PT note wrappers
- S02-RESEARCH.md — LOINC codes, scoring algorithms, data shapes, pitfall list

## Expected Output

- `src-tauri/src/commands/objective_measures.rs` — new ~450-line module with 6 scoring functions, 2 FHIR builders, 6 Tauri commands, ≥8 unit tests
- `src-tauri/src/db/migrations.rs` — Migration 16 appended (≤25 lines added)
- `src-tauri/src/commands/mod.rs` — one line added
- `src-tauri/src/lib.rs` — 6 lines added in `invoke_handler!`
- `src/types/pt.ts` — 7 new exported types/interfaces appended
- `src/lib/tauri.ts` — 6 new wrapper functions appended
