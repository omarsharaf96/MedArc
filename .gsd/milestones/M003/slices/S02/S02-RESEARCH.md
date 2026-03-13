# S02: Objective Measures & Outcome Scores — Research

**Date:** 2026-03-13

## Summary

S02 adds three distinct clinical data surfaces onto the M003/S01 PT foundation: (1) ROM/MMT/ortho-test recording, (2) auto-scored standardised outcome measures (LEFS, DASH, NDI, Oswestry, PSFS, FABQ), and (3) a longitudinal trend graph plus the discharge-summary outcome comparison that S01 left as a placeholder. All are pure additions — no existing commands or types change.

The backend work is straightforward: one new module `commands/objective_measures.rs`, Migration 16 (`outcome_score_index`), and a handful of pure scoring functions that are the primary `cargo test --lib` target. The frontend is the larger challenge — body-diagram UI for ROM/MMT tap-to-enter, score entry forms, and an inline SVG trend chart — all without adding charting dependencies (no recharts/chart.js in the project; Tailwind-only convention must be maintained).

The `outcome_comparison_placeholder` field in `DischargeSummaryFields` already exists in both the Rust struct and `src/types/pt.ts`. S02 fills it with real data by fetching the latest initial-eval scores and latest discharge-eval scores and serialising a structured comparison blob into that field via `update_pt_note`.

## Recommendation

**All scoring logic lives in pure Rust functions; all state lives in `outcome_score_index` and FHIR Observation resources; body-diagram and outcome-score UI live in a new page `OutcomeMeasuresPage.tsx`; the trend chart is inline SVG (no new npm package).**

- Keep ROM/MMT/ortho-test recording as a single `record_objective_measures` command that stores a structured JSON blob as a FHIR Observation with a MedArc-local code (not LOINC, since ROM/MMT don't have a single LOINC panel code). Store in `fhir_resources` only (no secondary index needed — queries are patient-scoped reads, not multi-patient searches).
- Keep each outcome-measure session as a separate `record_outcome_score` command. One row per session per measure type in `outcome_score_index` (Migration 16) plus the full item responses in a FHIR Observation in `fhir_resources`. Scoring happens in Rust at record time — the score is denormalised into the index.
- Trend graph: inline SVG polyline. No npm install. Fixed-height SVG element with D3-free coordinate math (patient scores as Y, time as X). Tailwind colors. This matches the project's zero-dependency frontend ethos.
- Body diagram: inline SVG or simple tabular body-part selector. Since no body.svg asset exists in the project, use a **tabular layout with body-region buttons** that reveal per-joint ROM/MMT fields — significantly simpler than an interactive SVG and delivers the same clinical workflow. A real SVG body diagram can be added in a later slice.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Scoring formula for DASH | Pure arithmetic: `((sum_of_items / n) - 1) × 25` | No library needed — 3 lines of Rust |
| Trend chart rendering | Inline SVG polyline with coordinate math | No recharts — matches project's zero-dependency frontend |
| FHIR Observation for scores | Pattern from `build_vitals_fhir` in `documentation.rs` | Same structure: `resourceType: "Observation"`, LOINC code, `valueQuantity` |
| Outcome comparison in Discharge Summary | Existing `outcomeComparisonPlaceholder: Option<String>` field | S01 already scaffolded this; S02 populates it via `update_pt_note` |
| UUID generation | `uuid::Uuid::new_v4()` — already in Cargo.toml | Same as every other create command |
| Audit write | `write_audit_entry` from `crate::audit` | Exact same call pattern as `pt_notes.rs` |

## Existing Code and Patterns

- `src-tauri/src/commands/pt_notes.rs` — Primary pattern to mirror. One module per domain. Pure `build_*_fhir()` function, index insert + fhir_resources insert, audit write on success and failure paths. Tests at bottom. S02's `commands/objective_measures.rs` follows this exactly.
- `src-tauri/src/commands/documentation.rs` — `build_vitals_fhir()` shows how FHIR Observations are built with LOINC codes and `valueQuantity`. ROM/MMT uses a `valueString` or extension instead of `valueQuantity` (clinical measurements are not single scalars). Scoring Observations use `valueQuantity`.
- `src-tauri/src/db/migrations.rs` — Migration 15 (pt_note_index) is the template for Migration 16 (outcome_score_index). Pattern: `PRAGMA foreign_keys = ON;`, `CREATE TABLE IF NOT EXISTS`, `ON DELETE CASCADE` from fhir_resources, indexes on the query hot-path columns.
- `src-tauri/src/rbac/roles.rs` — `Resource::ClinicalDocumentation` already covers PT notes; **use the same resource for outcome measures** (they are PT clinical documentation). No new RBAC variant needed.
- `src-tauri/src/lib.rs` — invoke_handler macro at 272 tests passing baseline. New commands appended to the `// M003/S01` section.
- `src/lib/tauri.ts` — PT note wrappers appended after `listBackups`. New outcome-measure wrappers appended in the same `// M003/S02` section.
- `src/types/pt.ts` — New `OutcomeScoreRecord`, `OutcomeScoreInput`, `ObjectiveMeasuresRecord`, `ObjectiveMeasuresInput`, `MeasureType` types appended to this file (same domain).
- `src/contexts/RouterContext.tsx` — Two existing PT routes. New `{ page: "outcome-measures"; patientId: string }` variant added.
- `src/pages/PTNoteFormPage.tsx` — `DischargeSummaryForm` renders `outcomeComparisonPlaceholder` as amber info banner (lines 387-395). S02 replaces that banner with a real rendered comparison table **only when** `outcomeComparisonPlaceholder !== null`. The field name and type do not change.
- `src/pages/PatientDetailPage.tsx` — Has a "PT Notes" button. S02 adds a parallel "Outcome Measures" button for Provider/SystemAdmin.

## Constraints

- **No new npm packages.** Tailwind + inline SVG only. Recharts, d3, chart.js — all ruled out by existing project convention (zero runtime dependencies beyond React and Tauri API).
- **Migration 16 must be append-only.** The `outcome_score_index` table is a new row in the `MIGRATIONS` vector at index 15 (zero-based). Never modify Migrations 1–15.
- **`outcome_score_index` PK column is `score_id` (not `id`)** — mirrors `pt_note_index.pt_note_id` pattern to avoid shadowing `fhir_resources.id` in JOINs.
- **Scoring is pure Rust at record time.** Score stored in `outcome_score_index.score`. No client-side score calculation — the frontend only displays what the backend returns.
- **LOINC codes for the 6 measures:**
  - LEFS: `72748-4` (Lower Extremity Functional Scale [LEFS])
  - DASH: `44250-0` (DASH Disability/Symptom Score)
  - NDI: `89206-5` (Neck Disability Index)
  - Oswestry: `89209-9` (Oswestry Disability Index)
  - PSFS: `89210-7` (Patient-Specific Functional Scale)
  - FABQ: `89190-1` (Fear-Avoidance Beliefs Questionnaire)
- **ROM/MMT does NOT have a LOINC panel code.** Store as `resource_type = "PTObjectiveMeasures"` in `fhir_resources` with a MedArc-local code system (`http://medarc.local/fhir/CodeSystem/pt-objective-measures`). No index table needed (single patient-scoped read).
- **`cargo test --lib` must remain green.** Currently 272 tests, 0 failed. Each new scoring function needs ≥1 unit test (the scoring logic is the primary `cargo test --lib` target per the roadmap).
- **`tsc --noEmit` must exit 0** after each task.
- **No audio stored.** N/A for this slice, but re: general M003 constraint.
- **Audit rows required** on every ePHI-touching operation (`objective_measures.record`, `objective_measures.get`, `outcome_score.record`, `outcome_score.list`, `outcome_score.get`).

## Scoring Algorithms (Pure Rust Functions)

### LEFS (Lower Extremity Functional Scale)
- 20 items, each scored 0–4
- Total = sum of all items; range 0–80; higher = better function
- Severity: 0–25 = severe, 26–50 = moderate, 51–70 = mild, 71–80 = minimal/normal
- MCID: 9 points

### DASH (Disabilities of Arm, Shoulder, Hand)
- 30 items (optional items 31–38 ignored in standard scoring), each scored 1–5
- Formula: `((sum / n) - 1) × 25` where n = answered items (min 27 answered required)
- Range 0–100; higher = more disability
- Severity: 0–20 = mild, 21–40 = moderate, 41–60 = severe, 61–100 = very severe
- MCID: 10.8 points

### NDI (Neck Disability Index)
- 10 items, each scored 0–5
- Formula: `(sum / 50) × 100` (as percentage) OR just `sum × 2` for percent
- Range 0–100%; higher = more disability
- Severity: 0–8 = no disability, 9–28 = mild, 29–48 = moderate, 49–64 = severe, 65–100 = complete
- MCID: 7.5 points (percentage points)

### Oswestry Disability Index
- 10 items, each scored 0–5
- Formula: `(sum / 50) × 100` (as percentage)
- Range 0–100%; higher = more disability
- Severity: 0–20 = minimal, 21–40 = moderate, 41–60 = severe, 61–80 = crippling, 81–100 = bed-bound
- MCID: 10 percentage points

### PSFS (Patient-Specific Functional Scale)
- 3–5 patient-identified activities, each scored 0–10 (0 = unable, 10 = full ability)
- Score = average of item scores
- Range 0–10; higher = better function
- Severity: 0–3 = severe, 4–6 = moderate, 7–10 = mild/good
- MCID: 2.0 points (average)

### FABQ (Fear-Avoidance Beliefs Questionnaire)
- 16 items total, each scored 0–6
- Physical Activity subscale (FABQ-PA): items 2, 3, 4, 5 → max 24
- Work subscale (FABQ-W): items 6, 7, 9, 10, 11, 12, 13, 14, 15 → max 42
- Items 1, 8, 16 are NOT scored (excluded from subscales)
- Higher score = greater fear-avoidance beliefs (worse prognosis)
- Clinically significant thresholds: FABQ-PA > 14; FABQ-W > 34 (high risk of chronic disability)
- No single composite score — report subscales separately
- MCID: not well-established; typically 5–6 points per subscale used clinically

## Common Pitfalls

- **DASH: dividing by response count, not 30** — DASH requires ≥ 27 items answered. If < 27 answered, the score is invalid. The scoring function must return an error (not a score) when fewer than 27 items are present. Hard-coding `/ 30.0` instead of `/ n as f64` where n = answered items would give wrong scores when items are skipped.
- **FABQ item indexing** — FABQ items are 1-indexed in literature (items 1–16). Items 1, 8, and 16 are NOT scored. Mapping literature item numbers to a 0-indexed Rust Vec requires careful offset math. Unit tests must verify individual subscale totals, not just the aggregate.
- **NDI percentage vs raw** — NDI can be reported as raw score (0–50) or percentage (multiply by 2). The roadmap says "0–100%" — store as percentage in `outcome_score_index.score`. Unit tests must confirm the multiply-by-2 conversion.
- **PSFS with variable item count** — The input struct must accept 3–5 items. The scoring function must reject < 3 items. Use `Vec<u8>` with validation rather than a fixed-size array.
- **ROM values are bilateral** — Left and right ROM values must be stored separately. The FHIR Observation for ROM uses `component` array (same pattern as blood pressure in vitals) with laterality extension: `"extension": [{"url": "laterality", "valueString": "left"}]`.
- **Body diagram has no SVG asset** — Do NOT attempt to create or embed a complex interactive SVG body diagram from scratch. Use the tabular joint-selector approach (region buttons reveal per-joint input fields). This is still PT-workflow-correct and ships in one task.
- **Outcome comparison in Discharge Summary** — S02 fetches scores via `get_outcome_scores(patientId, measureType)` and serialises a structured JSON string into `outcomeComparisonPlaceholder`. This is passed to `update_pt_note` — the field type in both Rust (`Option<String>`) and TypeScript (`string | null`) does not change. The frontend renders this JSON string, so the comparison data format must be documented (see data shape below).
- **Trend graph SVG coordinate overflow** — The inline SVG chart must clamp Y values to the SVG viewBox. A score of 0 must not plot below the SVG bottom edge; a score equal to the max must not plot above the top. Always compute `y = height - (score / maxScore * height)` with explicit clamping.
- **`outcome_score_index` PK name** — Must be `score_id` (not `id`) per the established pattern from `pt_note_index.pt_note_id`. Mixing this up creates confusing JOIN aliases.

## Data Shapes

### Migration 16: `outcome_score_index`
```sql
CREATE TABLE IF NOT EXISTS outcome_score_index (
    score_id      TEXT PRIMARY KEY NOT NULL,   -- FK to fhir_resources(id)
    patient_id    TEXT NOT NULL,
    encounter_id  TEXT,                         -- nullable: links to encounter if recorded during visit
    measure_type  TEXT NOT NULL
                  CHECK(measure_type IN ('lefs','dash','ndi','oswestry','psfs','fabq')),
    score         REAL NOT NULL,               -- computed score (FABQ: work subscale)
    score_secondary REAL,                      -- FABQ PA subscale; NULL for all other measures
    severity_class TEXT NOT NULL,              -- e.g. 'mild', 'moderate', 'severe'
    recorded_at   TEXT NOT NULL,
    episode_phase TEXT NOT NULL DEFAULT 'mid'  -- 'initial' | 'mid' | 'discharge'
                  CHECK(episode_phase IN ('initial','mid','discharge'))
);
```
Index on: `patient_id`, `measure_type`, `recorded_at`.

### FHIR Observation for outcome scores
```json
{
  "resourceType": "Observation",
  "id": "<score_id>",
  "status": "final",
  "category": [{"coding": [{"system": "http://terminology.hl7.org/CodeSystem/observation-category", "code": "survey"}]}],
  "code": {"coding": [{"system": "http://loinc.org", "code": "<loinc_code>", "display": "<measure_name>"}]},
  "subject": {"reference": "Patient/<patient_id>"},
  "effectiveDateTime": "<recorded_at>",
  "valueQuantity": {"value": <score>, "unit": "score"},
  "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/pt-outcome-items", "valueString": "<json_of_item_responses>"}]
}
```

### Outcome comparison blob format (stored in `outcomeComparisonPlaceholder`)
```json
{
  "measures": [
    {
      "measureType": "lefs",
      "displayName": "LEFS",
      "unit": "points",
      "initialScore": 32,
      "initialSeverity": "moderate",
      "initialDate": "2026-01-15",
      "dischargeScore": 58,
      "dischargeSeverity": "mild",
      "dischargeDate": "2026-03-10",
      "change": 26,
      "mcid": 9,
      "achievedMcid": true
    }
  ]
}
```
Frontend deserialises this JSON string and renders a table. TypeScript interface `OutcomeComparison` and `OutcomeComparisonMeasure` defined in `src/types/pt.ts`.

### ROM/MMT data shape (FHIR extension blob)
ROM stored as a JSON extension on the Observation resource. Example shape:
```json
{
  "joints": [
    {
      "region": "shoulder",
      "joint": "shoulder_flexion",
      "leftActive": 150, "rightActive": 170,
      "leftPassive": 160, "rightPassive": 175,
      "endFeel": "firm",
      "painWithMotion": true, "painNrs": "4"
    }
  ],
  "mmt": [
    {
      "group": "shoulder_flexors",
      "leftGrade": "4", "rightGrade": "5"
    }
  ],
  "orthoTests": [
    {
      "testName": "Hawkins-Kennedy",
      "bodyRegion": "shoulder",
      "result": "positive",
      "note": "pain arc at 90 deg"
    }
  ]
}
```

## Proposed Task Decomposition

### T01: Backend — scoring module + Migration 16
- `src-tauri/src/commands/objective_measures.rs` — all scoring functions (pure), FHIR builders, 5 Tauri commands
- `src-tauri/src/db/migrations.rs` — Migration 16: `outcome_score_index`
- `src-tauri/src/commands/mod.rs` — `pub mod objective_measures`
- `src-tauri/src/lib.rs` — register new commands
- `src/types/pt.ts` — append new types
- `src/lib/tauri.ts` — append new wrappers
- **Verification:** `cargo test --lib` (scoring unit tests); verify Migration 16 passes `MIGRATIONS.validate()`

### T02: ROM/MMT/ortho-test UI
- `src/pages/ObjectiveMeasuresPage.tsx` — tabular body-region selector, ROM/MMT fields, ortho-test panel, save/load
- `src/contexts/RouterContext.tsx` — add `{ page: "outcome-measures"; patientId: string }` route
- `src/components/shell/ContentArea.tsx` — add route dispatch
- `src/pages/PatientDetailPage.tsx` — "Objective Measures" button (Provider/SystemAdmin)
- **Verification:** `tsc --noEmit`

### T03: Outcome score entry forms + trend graph
- `src/pages/ObjectiveMeasuresPage.tsx` — outcome score tab with per-measure entry forms, auto-displayed score + severity, inline SVG trend chart
- `src/pages/PTNoteFormPage.tsx` — replace amber placeholder in `DischargeSummaryForm` with real comparison table (fetches scores on load)
- **Verification:** `tsc --noEmit`

## Open Risks

- **DASH valid-response threshold** — DASH specifies ≥ 27 of 30 items must be answered for a valid score. The scoring function must return `Err(AppError::Validation(...))` when fewer than 27 items are present, not a mathematically computed but clinically invalid score. This is a correctness constraint, not a UI issue.
- **FABQ item-number mapping bug risk** — Literature uses 1-indexed items; Rust uses 0-indexed Vec. FABQ excludes items 1, 8, and 16 from subscale scoring. The work subscale uses items 6–7, 9–15 (1-indexed). A systematic off-by-one in the index mapping would silently produce wrong subscale scores. Unit tests must hard-code expected subscale totals from a reference case.
- **Trend graph with single data point** — When a patient has only one recorded session for a measure (common at initial eval), the polyline has only one point and renders nothing. The chart must degrade gracefully: show a single dot with label, not a blank SVG or a crash.
- **`outcomeComparisonPlaceholder` JSON format coupling** — The blob is serialised in Rust (when? During a separate `get_outcome_comparison` command or as a post-processing step on the frontend?) and deserialised in TypeScript. The format must be agreed before T01 and T03 start. Decision: backend provides `get_outcome_comparison(patientId)` → `OutcomeComparison` JSON; frontend serialises to string before calling `update_pt_note`. This keeps the Rust struct ownership clean.
- **ROM/MMT entry volume** — A full bilateral ROM battery (cervical, thoracic, lumbar, shoulder ×2, elbow ×2, wrist ×2, hip ×2, knee ×2, ankle ×2) generates 20+ joint rows. A naive rendered form is very long. The tabular body-region approach (collapse regions, expand on click) keeps the page navigable. Don't flatten everything into a single scrolling form.
- **No charting library: SVG y-axis math** — Score ranges differ by measure (LEFS 0–80, DASH 0–100, PSFS 0–10). The SVG chart must normalise all Y values to the SVG viewBox height with per-measure max. A single generic `ScoreChart` component that accepts `maxScore` as a prop handles this without code duplication.

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| Rust (pure functions + unit tests) | (built-in project patterns) | N/A — follow pt_notes.rs |
| React (form + SVG chart) | frontend-design | installed |
| Tauri 2 commands | (built-in project patterns) | N/A — follow pt_notes.rs |

No new skills to install. The `frontend-design` skill is available and should be consulted for the body-diagram tabular UI and SVG chart design in T02/T03.

## Sources

- Scoring algorithms: published clinical references (LEFS, DASH, NDI, Oswestry, PSFS, FABQ specification documents) — no URL needed; algorithms are established clinical standards
- LOINC codes confirmed: LOINC.org search for "LEFS", "DASH", "Neck Disability Index", "Oswestry", "PSFS", "FABQ"
- FHIR Observation pattern: `src-tauri/src/commands/documentation.rs` `build_vitals_fhir()` (internal)
- Migration pattern: `src-tauri/src/db/migrations.rs` Migration 15 (internal)
- Outcome comparison field: `src/pages/PTNoteFormPage.tsx` lines 387-395 (internal)
- Current test baseline: 272 tests, 0 failures — confirmed by `cargo test --lib` run (internal)
