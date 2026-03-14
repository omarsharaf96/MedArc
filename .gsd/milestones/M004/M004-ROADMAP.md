# M004: Claims, Billing & Practice Management

**Vision:** Transform MedArc into a financially self-sufficient PT practice platform. Build on the complete M001/M002/M003 foundation to deliver a full revenue cycle: CPT coding with 8-minute rule automation, 837P electronic claims to Office Ally, ERA/835 remittance auto-posting, therapy cap monitoring with KX/CQ modifier automation, a Home Exercise Program builder with PDF output, an analytics dashboard with operational and financial KPIs, and MIPS quality measure capture — giving a solo PT everything needed to bill Medicare and commercial payers accurately from inside the app, with zero external spreadsheets.

## Success Criteria

- Provider selects CPT codes on an encounter; units are calculated automatically via the 8-minute rule and a billing summary shows the expected charge
- An 837P EDI file can be generated for any completed encounter, validated, and submitted to Office Ally; the file parses without errors against ANSI X12N 5010A1 spec
- An 835 ERA file can be imported; payments auto-post to the patient account and denials surface in a review queue within 5 seconds
- Medicare cumulative charges per patient per calendar year are tracked; KX modifier is automatically applied at the $2,480 threshold; a Targeted Medical Review alert fires at $3,000
- Provider can build a HEP with exercises from the bundled library, assign sets/reps/frequency, and export a patient-facing PDF with images in under 2 minutes
- Analytics dashboard renders all operational and financial KPIs from real encounter and billing data (no mock fixtures)
- MIPS measures #182, #217-222 are auto-populated from existing outcome scores; performance rate is visible in the reporting dashboard
- All new Tauri commands covered by `cargo test --lib`; `tsc --noEmit` exits 0 after all slices
- All ePHI access in new commands audit-logged via existing hash-chain system
- New RBAC resources (Claims, Analytics) correctly gate all new commands per the permission matrix

## Key Risks / Unknowns

- **837P segment generation correctness** — X12 5010A1 has strict loop/segment ordering; generated files must validate against official WEDI schematron — retire in S02 T01 by validating a generated file with the WEDI validator tool
- **Office Ally SFTP connectivity** — SFTP credential exchange, known_hosts fingerprint, and file naming conventions must match Office Ally spec — retire in S02 T02 by sending a real test claim file and receiving an acknowledgement
- **835 EDI parser completeness** — ERA files from different payers have subtle non-standard variations; parser must handle CAS/CLP/SVC loops without panicking on unknown segments — retire in S03 T01 by parsing real ERA samples from at least two payers
- **Rust `edi` crate maturity** — `edi` 0.4 covers X12 parsing but generated 837P must be hand-assembled via string building with correct ISA/GS envelope — retire in S02 T01 by end-to-end roundtrip test
- **Free Exercise DB image bundling** — 800+ exercise images must ship inside the app bundle without inflating binary; evaluate asset embedding vs. side-loaded JSON bundle at first build — retire in S05 T01 by verifying bundle loads under 500 ms
- **recharts SSR safety** — recharts must be imported only in browser context; Tauri webview has no SSR issue but dynamic imports should be verified — retire in S06 T01 by rendering real chart data
- **MIPS measure derivation accuracy** — CMS measure specifications are legally precise; derived performance rates must exactly match CMS eCQM logic — retire in S07 T01 with side-by-side comparison against CMS measure test deck

## Proof Strategy

- 837P correctness → retire in S02 T01 by running the WEDI free online validator against a generated file for a synthetic patient
- Office Ally SFTP → retire in S02 T02 by transmitting a real test claim and confirming receipt in the Office Ally portal
- 835 parser → retire in S03 T01 by parsing at least two real ERA samples (one Medicare, one commercial) with zero panics and correct payment amounts
- Exercise DB bundle → retire in S05 T01 by measuring load time of the bundled JSON on the target Apple Silicon machine
- recharts rendering → retire in S06 T01 by mounting the dashboard with live DB data and confirming charts render without console errors
- MIPS derivation → retire in S07 T01 by running the derivation logic against a known CMS test case and asserting expected numerator/denominator

## Verification Classes

- Contract verification: `cargo test --lib` for all new Tauri command data models, 8-minute rule calculator, therapy cap logic, and MIPS derivation; `tsc --noEmit` for all new React components and pages
- Integration verification: 837P file validates with WEDI validator; Office Ally SFTP round-trip completes; 835 ERA parses and posts payments correctly; HEP PDF opens in Preview with exercise images
- Operational verification: Therapy cap background check fires on encounter co-sign; KX modifier auto-applies when cumulative charges cross $2,480; MIPS dashboard refreshes on demand
- UAT / human verification: Billing staff completes full billing cycle (encounter → CPT entry → 837P submission → ERA posting → patient balance updated); provider builds and exports HEP; analytics dashboard shows correct KPIs for a month of real encounter data

## Milestone Definition of Done

This milestone is complete only when all are true:

- All 7 slices are marked `[x]` with verified summaries
- 8-minute rule calculator produces correct units for all standard PT timed code combinations verified against APTA reference cases
- A complete 837P file is generated, validated against X12N 5010A1, and accepted by Office Ally without errors
- An 835 ERA file is parsed and payments auto-posted to patient accounts; denials appear in review queue
- KX modifier fires automatically at the $2,480 Medicare threshold for a test patient with cumulative charges
- HEP PDF exports with exercise images and opens correctly in Preview
- Analytics dashboard renders all 6 KPI widgets from real encounter data
- MIPS performance rates for measures #182 and #134 match expected values from CMS test cases
- `cargo test --lib` passes (all existing M001-M003 tests + new M004 tests)
- `tsc --noEmit` exits 0
- `.gsd/REQUIREMENTS.md` all BILL-*, CLAIM-*, ERA-*, TCAP-*, HEP-*, ANLT-*, MIPS-* requirements marked validated

## Requirement Coverage

- Covers: BILL-01, BILL-02, BILL-03, BILL-04, BILL-05, CLAIM-01, CLAIM-02, CLAIM-03, CLAIM-04, CLAIM-05, ERA-01, ERA-02, ERA-03, ERA-04, TCAP-01, TCAP-02, TCAP-03, TCAP-04, HEP-01, HEP-02, HEP-03, HEP-04, HEP-05, ANLT-01, ANLT-02, ANLT-03, ANLT-04, ANLT-05, ANLT-06, MIPS-01, MIPS-02, MIPS-03, MIPS-04, MIPS-05
- Partially covers: none
- Leaves for later: ELIG-01 (eligibility verification via Change Healthcare), STMT-01 (patient statement generation), COLL-01 (collections workflow)
- Orphan risks: none

## Slices

- [ ] **S01: CPT Billing Engine & 8-Minute Rule Calculator** `risk:high` `depends:[]`
  > After this: Provider can attach CPT codes to any encounter from the bundled PT code library; units are auto-calculated using the correct Medicare 8-minute rule or AMA/commercial rule; a per-payer fee schedule drives the billing summary; BillingStaff and Provider can view the complete charge sheet — proven by cargo test --lib covering all timed/untimed unit calculation cases and tsc --noEmit for the CPT entry UI.

- [ ] **S02: Electronic Claims Submission (837P)** `risk:high` `depends:[S01]`
  > After this: Any encounter with a completed billing summary can be wrapped in a standards-compliant 837P EDI file, validated locally, and transmitted to Office Ally via SFTP; the claim lifecycle (draft → submitted → accepted → paid → denied) is tracked in the DB and visible to BillingStaff — proven by WEDI validator accepting the generated file and Office Ally confirming receipt of a test claim.

- [ ] **S03: ERA/835 Remittance Processing** `risk:high` `depends:[S02]`
  > After this: BillingStaff can import an 835 ERA file; payments auto-post to patient accounts; adjustments and denials are flagged for manual review; the A/R aging table reflects the new balances in real time — proven by parsing two real ERA samples (Medicare and commercial) with correct payment amounts and zero panics.

- [ ] **S04: Therapy Cap & KX Modifier Monitoring** `risk:medium` `depends:[S01]`
  > After this: Medicare cumulative PT charges are tracked per patient per calendar year; the KX modifier is auto-applied when the running total crosses $2,480; a Targeted Medical Review alert fires at $3,000; PTA encounters automatically receive the CQ modifier; ABN workflow is triggered when required — proven by cargo test --lib asserting all four threshold/modifier rules and manual UI verification of banner alerts.

- [ ] **S05: Home Exercise Program (HEP) Builder** `risk:medium` `depends:[]`
  > After this: Provider can search the bundled 800+ exercise library, drag exercises into a HEP card with sets/reps/frequency, save/load named HEP templates, link the HEP to an encounter note, and export a patient-facing PDF with exercise images — proven by a generated HEP PDF opening correctly in Preview with at least one exercise image rendered and correct programme details.

- [ ] **S06: Analytics & Outcomes Dashboard** `risk:medium` `depends:[S01,S03]`
  > After this: The analytics dashboard renders 6 configurable KPI widget cards (visits/day, cancellation rate, units/visit, revenue/visit, net collection rate, days in A/R) plus a clinical outcomes panel (MCID achievement rate by measure, payer mix) using recharts; all values are derived from live DB data with a date-range filter — proven by rendering the dashboard against at least one month of real encounter and billing data with zero console errors.

- [ ] **S07: MIPS Quality Measure Capture** `risk:medium` `depends:[S02,S06]`
  > After this: MIPS measures #182, #217-222 are auto-extracted from existing outcome_score_index data; PHQ-2/PHQ-9 (Measure #134), falls risk screening (Measure #155), and BMI screening (Measure #128) are captured at the encounter level; performance rates are calculated and visible in the MIPS reporting dashboard; data is export-ready for CMS submission — proven by cargo test --lib asserting correct numerator/denominator derivation against known CMS test cases and tsc --noEmit for the reporting dashboard UI.

---

## Boundary Map

### S01 → S02
Produces:
- `commands/billing.rs` → `create_encounter_billing(encounter_id, cpt_entries)`, `get_encounter_billing(encounter_id)`, `update_encounter_billing(id, ...)`, `list_encounter_billings(patient_id, date_range?)`, `calculate_units_8min(timed_minutes, untimed_codes)`, `get_cpt_code(code)`, `list_cpt_codes(category?)`, `get_fee_schedule(payer_id)`, `set_fee_schedule(payer_id, entries)`
- `BillingRecord` type with encounter_id, cpt_entries (Vec<CptEntry>), total_charge, payer_id, status
- `CptEntry` type with code, description, units, charge, modifier, is_timed
- `CptCode` library struct with code, description, timed flag, default_minutes, category
- `EightMinuteRuleResult` with recommended_units, rule_applied ("medicare" | "ama"), minutes_remaining
- Migration 22: `billing_index` (encounter_id, patient_id, payer_id, total_charge, status, billed_date)
- Migration 23: `cpt_code_library` (code TEXT PK, description, timed, default_minutes, category, active)
- Migration 24: `fee_schedule` (payer_id, cpt_code, allowed_amount, effective_date)
- `src/types/billing.ts` — TypeScript types for all billing shapes
- `src/lib/tauri.ts` additions — all billing command wrappers
- New RBAC resources: `Claims`, `Analytics` added to `Resource` enum in `rbac/roles.rs`

Consumes: nothing (S01 is an independent slice; reads existing encounter data via `encounter_id` FK)

### S01 → S04
Produces:
- `BillingRecord.cpt_entries` — therapy cap tracker reads CPT codes and charges from this to accumulate Medicare totals
- `get_encounter_billing(encounter_id)` — therapy cap reads charge amounts on co-sign

Consumes: nothing (first billing slice)

### S02 → S03
Produces:
- `commands/claims.rs` → `generate_837p(billing_id)`, `validate_claim(claim_id)`, `submit_claim_sftp(claim_id)`, `list_claims(patient_id?, status?, date_range?)`, `get_claim(claim_id)`, `update_claim_status(claim_id, status)`, `get_payer_config(payer_id)`, `set_payer_config(payer_id, config)`
- `ClaimRecord` type with claim_id, billing_id, patient_id, payer_id, status, edi_content, submitted_at, accepted_at
- `PayerConfig` type with payer_id, payer_name, edi_id, clearinghouse, sftp_host, sftp_path, billing_npi
- `ClaimStatus` enum: Draft | Validated | Submitted | Accepted | Paid | Denied | Appealed
- Migration 25: `claim_index` (claim_id, billing_id, patient_id, payer_id, status, submitted_at, total_charge)
- Migration 26: `payer_config` (payer_id TEXT PK, payer_name, edi_id, sftp_host, sftp_credential_key, billing_npi, active)
- RBAC: `Claims` resource — BillingStaff: full CRUD; Provider: Create + Read; NurseMa: Read; FrontDesk: none

Consumes from S01:
- `BillingRecord` — 837P generation reads CPT codes, charges, diagnoses, and payer from this
- `get_encounter_billing(encounter_id)` — wraps the billing record for the claim loop

### S03 → S06
Produces:
- `commands/era.rs` → `import_era_835(file_bytes)`, `list_era_batches(date_range?)`, `get_era_batch(batch_id)`, `list_payment_postings(patient_id?, date_range?)`, `list_pending_denials()`, `resolve_denial(posting_id, resolution_note)`, `get_ar_aging(as_of_date?)`
- `EraRecord` type with batch_id, payer_id, check_number, payment_date, total_payment, file_hash
- `PaymentPosting` type with posting_id, claim_id, patient_id, amount_paid, adjustment, denial_code, status ("posted" | "pending_review")
- `ArAgingBucket` type with bucket ("0-30" | "31-60" | "61-90" | "91-120" | "120+"), total_balance, claim_count
- Migration 27: `era_batch_index` (batch_id, payer_id, check_number, payment_date, total_payment, imported_at)
- Migration 28: `payment_posting_index` (posting_id, batch_id, claim_id, patient_id, amount_paid, adjustment, denial_code, status)

Consumes from S02:
- `ClaimRecord.claim_id` — ERA posting matches remittance to existing claims via claim number
- `update_claim_status(claim_id, status)` — ERA processor calls this to advance claim lifecycle

### S04 → S07
Produces:
- `commands/therapy_cap.rs` → `get_therapy_cap_status(patient_id, calendar_year?)`, `list_therapy_cap_alerts()`, `acknowledge_alert(alert_id)`, `generate_abn(patient_id, encounter_id)`, `get_kx_status(patient_id)`, `apply_cq_modifier(encounter_id)`
- `TherapyCapStatus` type with patient_id, calendar_year, cumulative_pt_charges, cumulative_ot_charges, kx_applied, tmr_alert_fired
- `TherapyCapAlert` type with alert_id, patient_id, alert_type ("kx_threshold" | "tmr_threshold" | "abn_required"), fired_at, acknowledged_at
- Migration 29: `therapy_cap_index` (patient_id, calendar_year, cumulative_pt_charges, cumulative_ot_charges, kx_applied, last_updated)
- Migration 30: `therapy_cap_alerts` (alert_id, patient_id, alert_type, encounter_id, fired_at, acknowledged_at)

Consumes from S01:
- `get_encounter_billing(encounter_id)` — reads CPT charges to update running cumulative total on each billing record save

### S05 → (standalone, integrates with S01)
Produces:
- `commands/hep.rs` → `list_exercises(search?, category?, body_part?)`, `create_hep(patient_id, encounter_id?, exercises)`, `get_hep(hep_id)`, `update_hep(hep_id, ...)`, `list_heps(patient_id)`, `save_hep_template(name, exercises)`, `list_hep_templates()`, `load_hep_template(template_id)`, `export_hep_pdf(hep_id)`
- `ExerciseRecord` type with exercise_id, name, description, body_part, category, image_path, default_sets, default_reps
- `HepRecord` type with hep_id, patient_id, encounter_id, exercises (Vec<HepExercise>), name, created_at
- `HepExercise` type with exercise_id, sets, reps, frequency, hold_seconds, notes
- `HepTemplate` type with template_id, name, exercises, created_by, created_at
- Migration 31: `hep_index` (hep_id, patient_id, encounter_id, name, created_at, updated_at)
- Migration 32: `hep_template_index` (template_id, name, created_by, created_at)
- Free Exercise DB JSON bundle shipped as a compiled-in static asset; `exercise_library` populated at first launch via seeding migration
- `src/types/hep.ts` — TypeScript types for all HEP shapes

Consumes from S01 (optional):
- `encounter_id` FK — HEP can be linked to an encounter but this is optional

### S06 → S07
Produces:
- `commands/analytics.rs` → `get_operational_kpis(date_range)`, `get_financial_kpis(date_range)`, `get_clinical_outcomes(date_range, provider_id?)`, `get_payer_mix(date_range)`, `get_ar_snapshot()`, `get_cancellation_rate(date_range)`
- `OperationalKpis` type with visits_per_day, cancellation_rate, units_per_visit, avg_visit_duration
- `FinancialKpis` type with revenue_per_visit, net_collection_rate, days_in_ar, gross_charges, payments_posted
- `ClinicalOutcomes` type with mcid_rate_by_measure (Map<MeasureType, f64>), improvement_rate, provider_breakdown
- `PayerMix` type with payer_id, payer_name, visit_count, revenue, percentage
- RBAC: `Analytics` resource — SystemAdmin + Provider: full Read; BillingStaff: Read financial KPIs only; NurseMa + FrontDesk: none
- `src/pages/AnalyticsDashboard.tsx` — configurable widget card layout
- recharts added to `package.json` for chart rendering

Consumes from S01:
- `billing_index` — reads charges for financial KPIs (revenue/visit, gross charges)

Consumes from S03:
- `payment_posting_index` — reads posted payments for net collection rate
- `get_ar_aging()` — days in A/R derived from A/R aging data

### S07 → (terminal slice)
Produces:
- `commands/mips.rs` → `get_mips_performance(reporting_year)`, `list_mips_measures()`, `get_measure_status(measure_id, reporting_year)`, `refresh_mips_derivation(reporting_year)`, `export_mips_report(reporting_year)`
- `MipsMeasure` type with measure_id, title, description, denominator_criteria, numerator_criteria, exclusion_criteria
- `MipsMeasureStatus` type with measure_id, reporting_year, eligible_patients, numerator, denominator, exclusions, performance_rate
- `MipsReport` type with reporting_year, provider_npi, measures (Vec<MipsMeasureStatus>), generated_at
- Migration 33: `mips_measure_status` (measure_id, reporting_year, provider_id, eligible_patients, numerator, denominator, performance_rate, last_refreshed)
- Auto-derivation reads `outcome_score_index` (Migration 16, M003/S02) for measures #182, #217-222
- PHQ-2/PHQ-9, falls risk, BMI captured via new encounter-level fields added in this slice
- `src/pages/MipsDashboard.tsx` — performance rate table with CMS submission export

Consumes from S02:
- `claim_index` — denominator for several measures requires a billed encounter within the reporting period

Consumes from S06:
- `AnalyticsDashboard` layout pattern — MIPS dashboard reuses the same widget card component system

Consumes from S01 (M003):
- `outcome_score_index` — primary data source for LEFS/DASH/NDI/Oswestry scores feeding measures #182, #217-222
