# S07: MIPS Quality Measure Capture

**Goal:** MIPS measures #182, #217–222 are auto-derived from existing `outcome_score_index` data; PHQ-2/PHQ-9 (#134), falls risk screening (#155), and BMI screening (#128) are captured at the encounter level; performance rates are calculated and visible in a MIPS reporting dashboard; data is export-ready as CSV for CMS QPP portal submission. Proven by `cargo test --lib` asserting correct numerator/denominator derivation against known CMS test cases and `tsc --noEmit` for the reporting dashboard UI.

**Demo:** `cargo test --lib` exits 0 with new MIPS derivation tests. Provider opens the MIPS dashboard for reporting year 2026, clicks "Refresh Derivation", and sees: Measure #182 — 8/10 patients (80%), Measure #217 — 5/6 (83%), Measure #134 — 9/9 (100%), Measure #155 — 4/4 (100%). A "Download CSV" button exports the performance rate table in CMS QPP portal format.

## Must-Haves

- Migration 33 (`mips_measure_status`) applied without errors
- `refresh_mips_derivation(reporting_year)` recalculates all measure numerators/denominators by querying `outcome_score_index`, `pt_note_index`, `claim_index`, and MIPS screening FHIR resources
- Measures #182 and #217–222 derived from `outcome_score_index` (no new data capture required)
- PHQ-2/PHQ-9 (#134) capture: new encounter-level screening form stored as `MIPSPhqScreen` FHIR Observation; PHQ-9 item 9 safety reminder displayed when item 9 ≥ 1
- Falls risk screening (#155) capture: `MIPSFallsScreen` FHIR Observation with tool name and result
- BMI screening (#128) capture: `MIPSBmiScreen` FHIR Observation; auto-populated when height/weight in vitals
- MIPS eligibility check: if Medicare charges < $90,000 OR < 200 Medicare patients in reporting year → show "You may be exempt from MIPS" banner (does not hide the dashboard)
- `get_mips_performance(reporting_year)`, `list_mips_measures()`, `get_measure_status(measure_id, reporting_year)`, `refresh_mips_derivation(reporting_year)`, `export_mips_report(reporting_year)` Tauri commands registered
- `export_mips_report` returns CSV string with columns: Measure ID, Measure Title, Eligible Patients, Numerator, Denominator, Exclusions, Performance Rate
- All MIPS commands use `Analytics` RBAC resource (SystemAdmin + Provider read; BillingStaff no access)
- `cargo test --lib` passes with ≥5 new MIPS tests:
  - Measure #182 derivation: 3 patients with initial+discharge scores → numerator=3; 2 with initial only → not in numerator; denominator=5
  - PHQ-2 score ≥ 3 → triggers PHQ-9 form in UI (frontend test not applicable; logic tested in backend via data structure)
  - Performance rate calculation: numerator=8, denominator=10, exclusions=0 → 80.0%
  - Exclusion reduces denominator: denominator=10, exclusions=2 → performance rate = 8/8 = 100%
  - Low-volume check: Medicare charges < $90,000 → `is_mips_eligible = false` in `MipsEligibility`
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — MIPS dashboard must render with real derived data from the test database
- Human/UAT required: yes — Provider verifies performance rates match manually calculated values from known test data; CSV export opens correctly in Excel/Numbers

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. MIPS derivation unit tests (embedded in cargo test --lib):
#    Seed an in-memory DB with:
#    - 5 patients with 2+ encounters; 3 have initial+discharge LEFS scores → #182 numerator=3/5
#    - Patient aged 72 → eligible for #155 (falls risk)
#    - Patient with PHQ-2 score = 4 → PHQ-9 required
#    Assert MipsMeasureStatus fields match expected values

# 4. CSV export (manual):
#    Call export_mips_report(2026) → CSV string
#    Open in Excel → verify 9 rows (one per measure), columns correct, rates match dashboard

# 5. Performance rate verification:
#    Manually calculate expected rates from test dataset
#    Compare to refresh_mips_derivation output
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` for `mips.refresh`, `mips.export`, `mips.screen_capture`
- Inspection surfaces:
  - `mips_measure_status WHERE reporting_year = 2026` — per-measure performance rates
  - `fhir_resources WHERE resource_type LIKE 'MIPS%'` — screening observations
  - `tracing::info!` log on each `refresh_mips_derivation` call with per-measure counts
- Failure state: `refresh_mips_derivation` logs per-measure errors but does not fail the entire refresh if one measure derivation fails; each measure is calculated independently; partial results stored

## Integration Closure

- Upstream surfaces consumed:
  - `outcome_score_index` (M003/S02) — primary data source for #182 and #217–222
  - `pt_note_index` (M003/S01) — denominators (patients with 2+ encounters)
  - `claim_index` (S02) — Medicare billing for low-volume eligibility check
  - `billing_index` (S01) — Medicare charge total for eligibility check
  - `AnalyticsDashboard` layout and `KpiWidget` component (S06) — MIPS dashboard reuses these
  - `Analytics` RBAC resource (S06) — MIPS uses same resource for access control
- New wiring introduced:
  - `commands/mips.rs` registered in `commands/mod.rs` and `lib.rs`
  - Five Tauri commands in `invoke_handler!`
  - `MipsDashboard.tsx` as new route target
  - PHQ/falls/BMI screening forms added to `EncounterDetailPage.tsx` (collapsible section, not disruptive to existing PT note workflow)
  - Sidebar navigation item "MIPS" added for SystemAdmin + Provider

## Tasks

- [ ] **T01: Backend — MIPS module, Migration 33, measure derivation** `est:4h`
  - Why: MIPS derivation accuracy is the highest-risk item per the ROADMAP. Unit tests against known CMS test case values are the primary proof. The screening FHIR resource approach avoids schema changes to existing tables.
  - Files: `src-tauri/src/commands/mips.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `src/types/mips.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/mips.rs` with: (a) per-measure derivation functions (pure, testable): `derive_measure_182`, `derive_measure_217_to_222`, `derive_measure_134`, `derive_measure_155`, `derive_measure_128`; (b) `check_mips_eligibility(provider_id, reporting_year, conn)` — queries Medicare charges and patient count; (c) five Tauri commands; (d) CSV serialisation for `export_mips_report`; (e) `#[cfg(test)]` module with ≥5 unit tests using seeded in-memory DB
    2. Append Migration 33 (`mips_measure_status`) to `MIGRATIONS`
    3. Add `pub mod mips;` to `commands/mod.rs`; register commands in `lib.rs`
    4. Create `src/types/mips.ts` with `MipsMeasure`, `MipsMeasureStatus`, `MipsReport`, `MipsEligibility`, `PhqScreenInput`, `FallsScreenInput`
    5. Append mips wrappers to `src/lib/tauri.ts` under `// M004/S07`
  - Verify: `cargo test --lib` passes with ≥5 new MIPS tests; derivation matches expected CMS test case values

- [ ] **T02: Encounter-level screening capture (PHQ-2/9, Falls, BMI)** `est:2h`
  - Why: Measures #134, #155, and #128 require data that doesn't exist in the app yet. Screening forms must be non-disruptive to existing PT note workflow.
  - Files: `src/components/encounter/MipsScreeningPanel.tsx` (new), `src/pages/EncounterDetailPage.tsx`
  - Do:
    1. Create `MipsScreeningPanel.tsx` — collapsible accordion section at the bottom of `EncounterDetailPage`:
       - **PHQ-2 section** (all patients ≥ 12): 2-item score inputs (0–3 each); total auto-calculated; if total ≥ 3 → PHQ-9 section expands; PHQ-9 has 9 items (0–3 each); if item 9 ≥ 1 → safety reminder banner: "Patient endorsed thoughts of self-harm. Follow your practice's safety protocol." "Save PHQ" button stores as `MIPSPhqScreen` FHIR Observation
       - **Falls Risk section** (patients ≥ 65): tool selector dropdown (TUG, Berg Balance, 4-Stage Balance Test, STEADI); result field (Positive/Negative/Equivocal); "Save Falls Screen" button stores as `MIPSFallsScreen` FHIR Observation
       - **BMI section** (all patients ≥ 18): height (cm or ft/in), weight (kg or lbs), auto-calculated BMI with severity class; "Save BMI" button stores as `MIPSBmiScreen` FHIR Observation; if vitals already have height/weight, pre-populate
    2. Add `MipsScreeningPanel` to `EncounterDetailPage.tsx` (below clinical content, above save buttons)
    3. MIPS screening commands added to `mips.rs` in T01: `record_phq_screen(encounter_id, phq_input)`, `record_falls_screen(encounter_id, falls_input)`, `record_bmi_screen(encounter_id, bmi_input)`; wrappers in `tauri.ts`
  - Verify: `tsc --noEmit` exits 0; PHQ-9 expands when PHQ-2 total ≥ 3; safety reminder renders when item 9 ≥ 1; BMI auto-calculates from height/weight inputs

- [ ] **T03: Frontend — MipsDashboard** `est:2h`
  - Why: Makes MIPS performance rates visible to Provider and SystemAdmin. Completes the MIPS workflow end-to-end.
  - Files: `src/pages/MipsDashboard.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/components/shell/Sidebar.tsx`
  - Do:
    1. Create `MipsDashboard.tsx`:
       - Reporting year selector at top (default: current calendar year)
       - MIPS eligibility banner: if `isEligible = false`, show amber banner "Based on current data, you may be below the MIPS low-volume threshold. Participation is optional."
       - Performance rate table (recharts `BarChart`): one bar per measure (#182, #217–222, #134, #155, #128); Y-axis 0–100%; benchmark line at 75%
       - Per-measure detail table below chart: Measure | Title | Eligible Patients | Numerator | Denominator | Exclusions | Performance Rate %
       - "Refresh Derivation" button calls `refreshMipsDerivation(year)` with loading indicator
       - "Download CSV" button calls `exportMipsReport(year)` and triggers file save via Tauri `save` dialog
    2. Add `{ page: "mips" }` route to `RouterContext.tsx`
    3. Add ContentArea dispatch
    4. Add "MIPS" to sidebar navigation (SystemAdmin + Provider)
  - Verify: `tsc --noEmit` exits 0; dashboard renders with derived data; CSV download creates correct file; eligibility banner logic works

## Files Likely Touched

- `src-tauri/src/commands/mips.rs` — new module (T01, T02)
- `src-tauri/src/commands/mod.rs` — `pub mod mips` (T01)
- `src-tauri/src/db/migrations.rs` — Migration 33 appended (T01)
- `src-tauri/src/lib.rs` — 8 commands registered (T01, T02)
- `src/types/mips.ts` — new file (T01)
- `src/lib/tauri.ts` — MIPS wrappers appended (T01)
- `src/components/encounter/MipsScreeningPanel.tsx` — new component (T02)
- `src/pages/EncounterDetailPage.tsx` — screening panel added (T02)
- `src/pages/MipsDashboard.tsx` — new page (T03)
- `src/contexts/RouterContext.tsx` — new route (T03)
- `src/components/shell/ContentArea.tsx` — dispatch case (T03)
- `src/components/shell/Sidebar.tsx` — MIPS navigation item (T03)
