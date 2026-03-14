# S07: MIPS Quality Measure Capture — Research

**Date:** 2026-03-14

## Summary

S07 implements MIPS (Merit-based Incentive Payment System) quality measure capture for PT practices. MIPS is CMS's quality reporting programme for Medicare clinicians under the Quality Payment Programme (QPP). PTs who bill Medicare for covered services must either participate in MIPS (or an alternative payment model) or face a penalty on their Medicare payment rates.

MIPS is divided into 4 performance categories: Quality (50%), Promoting Interoperability (25%), Improvement Activities (15%), and Cost (10%). For PT, only the **Quality** category is actively captured in MedArc M004. The Quality category requires reporting at least 6 quality measures (one of which is an outcome measure).

The PT-specific measures that MedArc can auto-derive from existing data are: **#182** (Functional Outcome Assessment — must use a validated outcome tool at least once) and **#217–#222** (Functional status change measures derived from LEFS, DASH, NDI, and Oswestry scores). Measures #134 (PHQ-2/PHQ-9 depression screening), #155 (Falls risk screening), and #128 (BMI screening) require new data capture at the encounter level.

**Confidence: HIGH** for the measure derivation logic (rules are precisely defined in CMS eCQM specifications). **MEDIUM** for PHQ-2/PHQ-9 and falls risk capture (requires adding fields to the encounter/note workflow, which must be designed carefully to not disrupt existing PT note forms from M003).

## Recommendation

- Auto-derive measures #182 and #217–222 from existing `outcome_score_index` data (M003/S02)
- Capture #134, #155, and #128 via new encounter-level screening fields added in S07 (appended to encounter detail, not injected into PT note forms)
- Store MIPS measure status in `mips_measure_status` (Migration 33)
- Dashboard built on the S06 `KpiWidget` card pattern with recharts `BarChart` for performance rates
- Export as a CSV compatible with the CMS QPP portal import format

## MIPS 2026 Overview for PT

### Performance Threshold (CY 2026)
- **Final Score threshold for neutral payment adjustment:** CMS sets this annually; typically 75 points for 2026
- **Exception Reporting:** Solo practitioners with < $90,000 in Medicare-allowed charges AND < 200 Medicare patients covered by Part B are **excluded from MIPS** (low-volume threshold). MedArc should display a MIPS eligibility check at the start of the reporting year.
- **Payment adjustment:** Positive adjustment for exceptional performers (>= 89 points), negative penalty (up to -9%) for low performers (< 75 points)

### Quality Category Weight
50% of final MIPS score. Requires reporting at least 6 measures. One must be an outcome measure (#182 qualifies). Measures are scored based on a performance rate (numerator / denominator × 100%) compared to a benchmark.

## Quality Measures for PT

### Measure #182 — Functional Outcome Assessment
- **Title:** Functional Outcome Assessment
- **Denominator:** All patients aged ≥ 18 with at least 2 PT visits in the performance period
- **Numerator:** Patients in the denominator who had a functional outcome tool administered at baseline and at discharge (or end of episode)
- **Exclusions:** Patients with cognitive impairment (documented ICD-10 code)
- **MedArc derivation:** Patient has at least 2 entries in `outcome_score_index` for any measure type, one with `episode_phase = 'initial'` and one with `episode_phase = 'discharge'`

### Measures #217–222 — Functional Status Change Measures
These 6 measures are the eCQM functional status change measures. Each covers a body region:
| Measure | Instrument | Body Region |
|---------|------------|-------------|
| #217 | LEFS | Lower extremity (hip/knee/ankle/foot) |
| #218 | DASH | Upper extremity (shoulder/elbow/wrist/hand) |
| #219 | NDI | Neck (cervical) |
| #220 | Oswestry or PROMIS | Low back |
| #221 | LEFS/DASH/NDI | Multiple body regions |
| #222 | LEFS/DASH/NDI | Shoulder only |

**Denominator for #217–222:** Patients aged ≥ 18 who were treated for the relevant body region and had at least 2 visits
**Numerator:** Patients with a functional status score recorded at initial AND discharge episodes
**MedArc derivation:** For each measure, join `outcome_score_index` with the relevant `measure_type` values, find patients with both `initial` and `discharge` phase scores

### Measure #134 — Preventive Care and Screening: Screening for Depression
- **Denominator:** All patients aged ≥ 12 seen at least once during the performance period
- **Numerator:** Patients who were screened using PHQ-2 or PHQ-9 (or age-appropriate equivalent)
- **PHQ-2:** 2-item screen (scored 0–6; score ≥ 3 triggers PHQ-9 follow-up)
- **PHQ-9:** 9-item screen (scored 0–27; severity: minimal 0–4, mild 5–9, moderate 10–14, moderately severe 15–19, severe 20–27)
- **MedArc implementation:** New `phq_screen` field added at encounter level (separate from PT note); Provider selects PHQ-2 score at visit; if ≥ 3, PHQ-9 form appears

### Measure #155 — Falls Risk Screening
- **Denominator:** Patients aged ≥ 65 seen at least once during the performance period
- **Numerator:** Patients who were screened for falls risk using a standardised tool
- **Accepted tools:** Timed Up and Go (TUG), Berg Balance Scale, 4-Stage Balance Test, STEADI (Stopping Elderly Accidents, Deaths & Injuries) toolkit
- **MedArc implementation:** New `falls_screen` field at encounter level; Provider selects tool used and result (positive/negative/equivocal); linked to M003/S02 objective measures if TUG already recorded there

### Measure #128 — Body Mass Index (BMI) Screening
- **Denominator:** Patients aged ≥ 18 seen at least once during the performance period
- **Numerator:** Patients who had BMI calculated during the performance period
- **MedArc implementation:** BMI can be derived from existing vitals data if height/weight recorded; `bmi_screening` field at encounter level; if `height` and `weight` exist in vitals, BMI is auto-calculated

## PHQ-2 and PHQ-9 Scoring

### PHQ-2 (Items 1 and 2 of PHQ-9)
1. "Little interest or pleasure in doing things" (0–3)
2. "Feeling down, depressed, or hopeless" (0–3)

Score 0–6; score ≥ 3 indicates possible depression → administer PHQ-9

### PHQ-9 (9 items, each scored 0–3)
Items: depressed mood, anhedonia, sleep problems, low energy, appetite changes, feeling bad about self, concentration difficulty, psychomotor changes, suicidal ideation

Total score 0–27:
- 0–4: Minimal
- 5–9: Mild
- 10–14: Moderate
- 15–19: Moderately severe
- 20–27: Severe

### Suicide Safety Protocol Note
PHQ-9 item 9 asks about suicidal ideation. When item 9 score ≥ 1, the MedArc UI must display a safety protocol reminder: "Patient endorsed thoughts of self-harm. Follow your practice's safety protocol for assessment and documentation." This is not a clinical intervention — it is a reminder for the provider. No automated action is taken.

## Reporting Requirements

### CMS QPP Submission Methods for MIPS Quality
- **Electronic Clinical Quality Measures (eCQM):** Most common for EHRs; requires QRDA III XML export
- **Qualified Clinical Data Registry (QCDR):** PT-specific option via APTA's EDGE Registry or similar
- **Claims-based reporting:** Report via G-codes on claims (being phased out)
- **CSV upload:** CMS QPP portal accepts CSV for smaller practices

**MedArc approach for M004:** Generate a CSV export compatible with CMS QPP portal. Full eCQM QRDA III XML export is deferred to a future milestone. The CSV includes: measure ID, eligible patients, numerator, denominator, performance rate.

### Performance Calculation
```
Performance Rate = (Numerator / Denominator) × 100%
```

Exclusions reduce the denominator. Exceptions (patients who decline screening) are tracked separately via an `exclusion_reason` field.

### MIPS Exception: Low-Volume Threshold
Solo PT exempt if: Medicare-allowed charges < $90,000/year AND < 200 Medicare patients/year. MedArc checks this on MIPS dashboard load by querying `billing_index` and `claim_index` for the reporting year.

## Data Shapes

### Migration 33: `mips_measure_status`
```sql
CREATE TABLE IF NOT EXISTS mips_measure_status (
    status_id      TEXT PRIMARY KEY NOT NULL,
    measure_id     TEXT NOT NULL,        -- '182', '217', '218', '219', '220', '221', '222', '134', '155', '128'
    reporting_year INTEGER NOT NULL,
    provider_id    TEXT NOT NULL,
    eligible_patients INTEGER NOT NULL DEFAULT 0,
    numerator      INTEGER NOT NULL DEFAULT 0,
    denominator    INTEGER NOT NULL DEFAULT 0,
    exclusions     INTEGER NOT NULL DEFAULT 0,
    performance_rate REAL,
    last_refreshed TEXT NOT NULL,
    UNIQUE(measure_id, reporting_year, provider_id)
);
```

### Encounter-Level Screening Fields (PHQ-2/9, Falls, BMI)
New fields appended to `encounter_index` (or stored as a separate `encounter_screenings` table to avoid altering an existing migration — new Migration 21 reserved for this in M004 CONTEXT's migration gap):
```sql
-- OR stored in fhir_resources as separate Observation resources per screening type
-- Recommended: fhir_resources approach (same as vitals in M001)
-- resource_type = 'PHQScreen' | 'FallsScreen' | 'BMIScreen'
-- avoids altering any existing table
```

Decision: store all three screening types as FHIR Observation resources in `fhir_resources` (no new table). Use resource types `'MIPSPhqScreen'`, `'MIPSFallsScreen'`, `'MIPSBmiScreen'`. This avoids schema changes to existing tables and follows the established pattern.

## Common Pitfalls

- **MIPS eligibility check** — A solo PT below the low-volume threshold must not be shown MIPS as a required workflow. Always check eligibility first. Showing MIPS as required when exempt causes confusion.
- **Performance rate denominator exclusions** — Patients with a documented exclusion reason (cognitive impairment for #182; patient declined for #134) reduce the denominator, not the numerator. `performance_rate = numerator / (denominator - exclusions)`.
- **Measure #134 PHQ-9 item 9** — The suicidal ideation item requires a safety reminder in the UI. This is a legal and ethical requirement, not optional. A silent omission of this reminder could expose the practice to liability.
- **Measures #217–222 require both initial AND discharge** — A patient with only an initial score (common for patients who discharge prematurely) does not meet the numerator. Only completed episodes (initial + discharge) count.
- **Calendar year boundary for MIPS** — MIPS reporting is per calendar year (January 1 – December 31). The `reporting_year` column in `mips_measure_status` is critical. `refresh_mips_derivation(2026)` recalculates only 2026 data.
- **Low-volume threshold changes annually** — The $90,000 / 200-patient threshold is subject to CMS updates each November. Store as a constant with a comment noting the update cycle.

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| MIPS measure derivation produces incorrect performance rates | High | Unit test with a known CMS test case; compare derived performance rate against expected value |
| PHQ-9 item 9 safety protocol omission | High | Code review checklist item; must render safety reminder whenever item 9 >= 1 |
| Provider is below low-volume threshold but MIPS workflow is shown | Medium | Query `billing_index` for Medicare charges on dashboard load; if < $90,000 show "You may be exempt from MIPS" banner |
| CMS measure specification changes mid-year | Low | MIPS derivation logic is isolated in `mips.rs`; a code-only update (no schema change) handles annual measure spec updates |
| CSV export format compatibility with QPP portal | Medium | Validate export format against CMS QPP portal documentation; include a sample CSV in test fixtures |

## Sources

- CMS QPP Quality Payment Programme: qpp.cms.gov
- MIPS Quality Measure Specifications 2026: qpp.cms.gov/measures/quality
- Measure #182 eCQM specification: ecqi.healthit.gov (CMS0142v11 or current version)
- Measures #217–222 eCQM specification: ecqi.healthit.gov
- PHQ-2/PHQ-9 scoring: patient.info/health/PHQ; Kroenke K, Spitzer RL. The PHQ-9: A new depression diagnostic and severity measure. Psychiatr Ann. 2002
- Falls risk screening (STEADI): cdc.gov/steadi
- MIPS low-volume threshold: cms.gov/medicare/quality-initiatives-patient-assessment-instruments/value-based-programs/macra-mips-and-apms
- APTA MIPS resources: apta.org/your-practice/payment/medicare-fee-for-service/value-based-care/mips
