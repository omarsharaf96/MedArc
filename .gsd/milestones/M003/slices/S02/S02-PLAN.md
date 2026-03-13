# S02: Objective Measures & Outcome Scores

**Goal:** Provider can record ROM/MMT/ortho-test data via a tabular body-region UI and administer all six standardised outcome measures (LEFS, DASH, NDI, Oswestry, PSFS, FABQ) with auto-scoring, severity classification, and longitudinal trending. The Discharge Summary outcome-comparison placeholder is replaced with a live comparison table pulled from real scored data.

**Demo:** Provider opens Objective Measures for a patient, selects the Shoulder region, enters bilateral ROM and MMT grades, records a positive Hawkins-Kennedy test, then switches to the Outcome Scores tab, enters all 20 LEFS items, and sees the auto-calculated score (e.g. 45 / moderate) appear immediately. A second LEFS session is recorded; the inline SVG trend chart shows two points. The Discharge Summary form replaces the amber placeholder with a rendered comparison table showing initial vs discharge LEFS with MCID met/not-met status.

## Must-Haves

- Migration 16 (`outcome_score_index`) applied without errors; `cargo test --lib` remains green with ≥6 new scoring unit tests (one per measure)
- `record_objective_measures`, `get_objective_measures`, `record_outcome_score`, `list_outcome_scores`, `get_outcome_score`, and `get_outcome_comparison` Tauri commands registered and returning correct types
- All six scoring functions are pure Rust: LEFS, DASH (≥27-item guard), NDI (percentage output), Oswestry (percentage output), PSFS (3–5 item guard), FABQ (PA + work subscales separately)
- Body-region tabular UI: shoulder, cervical, thoracic, lumbar, hip, knee, ankle, elbow, wrist — each region expandable, reveals bilateral ROM (active/passive, degrees, end-feel, pain) and MMT grade fields; ortho-test panel filterable by region
- Six outcome-measure entry forms with per-item inputs, auto-displayed score + severity, and MCID callout on each form
- Inline SVG trend chart that correctly handles 1-point (single dot) and multi-point (polyline) data; normalises Y-axis to per-measure max; does not overflow viewBox
- Discharge Summary `outcomeComparisonPlaceholder` amber banner replaced with a rendered comparison table when data exists; falls back to informational message when no scores recorded
- `tsc --noEmit` exits 0 after every task
- Audit rows written for every ePHI-touching command (record, get, list)
- `OutcomeScoreRecord`, `OutcomeScoreInput`, `ObjectiveMeasuresRecord`, `ObjectiveMeasuresInput`, `MeasureType`, `OutcomeComparison`, `OutcomeComparisonMeasure` types added to `src/types/pt.ts`
- New Tauri wrappers appended to `src/lib/tauri.ts` under `// M003/S02` comment

## Proof Level

- This slice proves: contract + integration
- Real runtime required: no (TypeScript checked via `tsc --noEmit`; Rust checked via `cargo test --lib`)
- Human/UAT required: no (runtime verification deferred to milestone end-to-end UAT in S07)

## Verification

- `cd src-tauri && cargo test --lib` — must pass with 272 + ≥6 new tests, 0 failures; new tests cover each scoring function with at least one happy-path case and the DASH ≥27-item guard and PSFS item-count guard
- `cd .. && npx tsc --noEmit` — must exit 0 after T02 and again after T03
- Scoring correctness spot-check (embedded in unit tests):
  - LEFS: 20 items all scored 4 → score = 80, severity = "minimal"
  - DASH: 30 items all scored 1 → score = 0, severity = "mild"; 26 items answered → `Err(Validation(...))`
  - NDI: 10 items all scored 0 → score = 0 (%), severity = "no_disability"
  - Oswestry: 10 items all scored 5 → score = 100 (%), severity = "bed_bound"
  - PSFS: 3 items [10, 10, 10] → score = 10, severity = "mild"; 2 items → `Err(Validation(...))`
  - FABQ: PA subscale (items 2–5, 1-indexed) = 6+6+6+6 = 24; work subscale (items 6–7, 9–15, 1-indexed) = 7×6 = 42

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` rows written on every `record_objective_measures`, `get_objective_measures`, `record_outcome_score`, `list_outcome_scores`, `get_outcome_score`, `get_outcome_comparison` call; audit action strings are `"objective_measures.record"`, `"objective_measures.get"`, `"outcome_score.record"`, `"outcome_score.list"`, `"outcome_score.get"`, `"outcome_comparison.get"`
- Inspection surfaces:
  - `outcome_score_index` table: query `SELECT * FROM outcome_score_index WHERE patient_id = ?` to inspect all recorded scores, severity classes, episode phases, and secondary scores (FABQ PA)
  - `fhir_resources` WHERE `resource_type = 'Observation'` for full item-response JSON blob
  - `fhir_resources` WHERE `resource_type = 'PTObjectiveMeasures'` for ROM/MMT blob
  - `cargo test --lib -- commands::objective_measures` for scoring contract
- Failure state exposed: Tauri commands return `AppError::Validation` with descriptive message for DASH <27 items, PSFS <3 items; `AppError::NotFound` for missing score/measures; all errors propagate through existing frontend error banner
- Redaction constraints: `valueString` extension blob on FHIR Observation may contain clinical responses — do not log raw blob content; audit `details` field contains only resource_id + measure_type, not item responses

## Integration Closure

- Upstream surfaces consumed:
  - `src-tauri/src/commands/pt_notes.rs` — `write_audit_entry`, `AppError`, `Database` patterns copied
  - `src-tauri/src/db/migrations.rs` — Migration 16 appended after Migration 15
  - `src/types/pt.ts` — `DischargeSummaryFields.outcomeComparisonPlaceholder: string | null` already defined (S01 placeholder)
  - `src/pages/PTNoteFormPage.tsx` lines 387-395 — amber outcome-comparison banner replaced in T03
  - `src/pages/PatientDetailPage.tsx` — "Objective Measures" button added alongside existing "PT Notes" button
  - `src/contexts/RouterContext.tsx` — `{ page: "outcome-measures"; patientId: string }` route variant added
  - `src/components/shell/ContentArea.tsx` — dispatch case for `"outcome-measures"` added
- New wiring introduced in this slice:
  - `commands/objective_measures.rs` module registered in `commands/mod.rs` and `lib.rs`
  - Six Tauri commands wired into `invoke_handler!` macro
  - `OutcomeMeasuresPage.tsx` as a new route target
  - `get_outcome_comparison` command called by `PTNoteFormPage.tsx` `DischargeSummaryForm` on mount
- What remains before the milestone is truly usable end-to-end: S03 (AI voice), S04 (Document Centre), S05 (PDF export, which reads outcome scores for reports), S06 (fax), S07 (auth tracking)

## Tasks

- [ ] **T01: Backend — scoring module, Migration 16, and Tauri commands** `est:2h`
  - Why: Establishes the entire data layer for S02. All frontend tasks depend on these commands existing and returning correct types. Scoring unit tests provide the primary `cargo test --lib` proof for this slice.
  - Files: `src-tauri/src/commands/objective_measures.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `src/types/pt.ts`, `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/objective_measures.rs` with: (a) pure scoring functions for all 6 measures with correct algorithms and validation guards; (b) `build_objective_measures_fhir()` pure function producing a `PTObjectiveMeasures` FHIR-like resource; (c) `build_outcome_score_fhir()` pure function producing a FHIR Observation with LOINC code; (d) five Tauri commands: `record_objective_measures`, `get_objective_measures`, `record_outcome_score`, `list_outcome_scores`, `get_outcome_score`; (e) one computed command: `get_outcome_comparison` that fetches the earliest-phase and latest-phase score per measure type and returns the `OutcomeComparison` struct; (f) `#[cfg(test)]` module with ≥1 unit test per scoring function plus DASH <27-item guard and PSFS <3-item guard tests
    2. Append Migration 16 (`outcome_score_index`) to `MIGRATIONS` vector in `migrations.rs` at index 15 (zero-based, after Migration 15). PK column is `score_id`. Include `score_secondary REAL` for FABQ PA subscale. Include `episode_phase` with CHECK constraint. Add indexes on `patient_id`, `measure_type`, `recorded_at`.
    3. Add `pub mod objective_measures;` to `commands/mod.rs`
    4. Append the 6 new commands to `invoke_handler!` in `lib.rs` under a `// M003/S02` comment
    5. Append new TypeScript types to `src/types/pt.ts`: `MeasureType`, `OutcomeScoreInput`, `OutcomeScoreRecord`, `ObjectiveMeasuresInput`, `ObjectiveMeasuresRecord`, `OutcomeComparisonMeasure`, `OutcomeComparison`
    6. Append new wrappers to `src/lib/tauri.ts` under `// M003/S02`: `recordObjectiveMeasures`, `getObjectiveMeasures`, `recordOutcomeScore`, `listOutcomeScores`, `getOutcomeScore`, `getOutcomeComparison`
  - Verify: `cd src-tauri && cargo test --lib` passes with ≥6 new tests, 0 failures; total ≥278 tests
  - Done when: `cargo test --lib` exits 0 with all new scoring tests included; `tsc --noEmit` exits 0 (types + wrappers compile)

- [ ] **T02: ROM/MMT/ortho-test UI (ObjectiveMeasuresPage — objective tab)** `est:2h`
  - Why: Delivers PT-OBJ-01 (ROM), PT-OBJ-02 (MMT), PT-OBJ-03 (ortho tests). Wires the page into the router and the PatientDetailPage navigation button.
  - Files: `src/pages/ObjectiveMeasuresPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/PatientDetailPage.tsx`
  - Do:
    1. Create `src/pages/ObjectiveMeasuresPage.tsx` with two tabs: "Objective Measures" and "Outcome Scores". In T02 implement only the Objective Measures tab:
       - Nine collapsible body-region sections (Cervical, Thoracic, Lumbar, Shoulder, Elbow, Wrist, Hip, Knee, Ankle). Each collapsed by default; clicking the region header expands it.
       - Each expanded region shows: bilateral ROM fields (active degrees, passive degrees, end-feel dropdown [firm/soft/hard/springy/empty/none], pain with motion checkbox + NRS 0–10 input) per joint motion (e.g. Shoulder: Flexion, Extension, Abduction, IR, ER). Normal reference value shown inline as grey text.
       - MMT section below ROM in each region: bilateral MMT grade dropdowns using Kendall scale (0, 1, 2, 3, 3+, 4-, 4, 4+, 5-, 5) per muscle group.
       - Ortho Tests panel at the bottom: body-region filter buttons, searchable test library (20+ pre-loaded tests covering shoulder, cervical, lumbar, knee, hip), each row shows test name, result dropdown (Positive/Negative/Equivocal), clinical note textarea.
       - "Save Objective Measures" button calls `recordObjectiveMeasures` with assembled `ObjectiveMeasuresInput`. On mount: calls `getObjectiveMeasures` and pre-fills form with existing data (last saved session).
       - The Outcome Scores tab content is a placeholder `<div>` in T02 (implemented in T03).
    2. Add `{ page: "outcome-measures"; patientId: string }` to the `Route` union in `RouterContext.tsx`
    3. Add `case "outcome-measures":` dispatch in `ContentArea.tsx` rendering `<ObjectiveMeasuresPage patientId={route.patientId} role={role} userId={userId} />`
    4. Add "Objective Measures" button to `PatientDetailPage.tsx` adjacent to the existing "PT Notes" button, visible to Provider and SystemAdmin only
  - Verify: `npx tsc --noEmit` exits 0; page renders without runtime errors in dev
  - Done when: `tsc --noEmit` exits 0; all route wiring compiles; ROM/MMT/ortho-test UI renders all body regions with correct fields

- [ ] **T03: Outcome score entry forms, trend chart, and Discharge Summary integration** `est:2h`
  - Why: Delivers PT-OBJ-04 (auto-scored outcome measures with trending) and completes the Discharge Summary outcome-comparison integration (PT-DOC-03 supporting requirement). This is the final task that makes S02 demonstrable end-to-end.
  - Files: `src/pages/ObjectiveMeasuresPage.tsx`, `src/pages/PTNoteFormPage.tsx`
  - Do:
    1. Implement the Outcome Scores tab in `ObjectiveMeasuresPage.tsx`:
       - Measure type selector (LEFS, DASH, NDI, Oswestry, PSFS, FABQ) as segmented control or tabs
       - Per-measure entry form: each item rendered as a labelled input (number or select as appropriate). Item count per measure: LEFS 20, DASH 30, NDI 10, Oswestry 10, PSFS 3–5 configurable, FABQ 16. Item labels drawn from published questionnaire item text (hard-coded constants in the component).
       - "Episode phase" radio: Initial / Mid / Discharge
       - "Score & Save" button: calls `recordOutcomeScore`. On success, the returned `OutcomeScoreRecord` is displayed inline: score value, severity class, MCID note (e.g. "MCID is 9 pts — change of 13 pts exceeds threshold"). For FABQ, display both PA and work subscale scores.
       - Score history section: calls `listOutcomeScores` on mount filtered by currently selected measure type; displays a table of past sessions (date, phase, score, severity).
       - Inline SVG trend chart (`ScoreChart` component): accepts `scores: {date: string; score: number}[]` and `maxScore: number` props. Renders a polyline for ≥2 points; renders a single labelled dot for 1 point; renders empty-state text for 0 points. Y-axis normalised to viewBox height using `y = height - (score / maxScore) * height` with clamping. X-axis evenly spaced by index. No npm packages.
    2. Replace the amber outcome-comparison banner in `DischargeSummaryForm` (`PTNoteFormPage.tsx` lines ~387-395) with logic that:
       - Calls `getOutcomeComparison(patientId)` on `DischargeSummaryForm` mount (pass `patientId` as a new prop)
       - When the result has `measures.length > 0`: renders a table with columns Measure / Initial / Discharge / Change / MCID Met
       - When no data: renders the existing amber informational message (no data recorded yet)
       - The `outcomeComparisonPlaceholder` field in `dsFields` is populated by serialising the `OutcomeComparison` JSON via `JSON.stringify` before calling `updatePtNote`, so the data is persisted with the note
  - Verify: `npx tsc --noEmit` exits 0; SVG chart renders without overflow (clamp logic correct); Discharge Summary comparison table renders when comparison data present; amber fallback renders when no data
  - Done when: `tsc --noEmit` exits 0; all six measure forms compile with correct item counts; `ScoreChart` handles 0, 1, and 2+ data points without TypeScript errors; `DischargeSummaryForm` correctly branches on comparison data

## Files Likely Touched

- `src-tauri/src/commands/objective_measures.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod objective_measures` (T01)
- `src-tauri/src/db/migrations.rs` — Migration 16 appended (T01)
- `src-tauri/src/lib.rs` — 6 commands registered (T01)
- `src/types/pt.ts` — 7 new types appended (T01)
- `src/lib/tauri.ts` — 6 new wrappers appended (T01)
- `src/pages/ObjectiveMeasuresPage.tsx` — new page (T02 + T03)
- `src/contexts/RouterContext.tsx` — new route variant (T02)
- `src/components/shell/ContentArea.tsx` — new dispatch case (T02)
- `src/pages/PatientDetailPage.tsx` — Objective Measures button (T02)
- `src/pages/PTNoteFormPage.tsx` — Discharge Summary comparison table (T03)
