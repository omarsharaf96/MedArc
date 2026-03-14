# S04: Therapy Cap & KX Modifier Monitoring — Research

**Date:** 2026-03-14

## Summary

S04 automates Medicare therapy cap compliance monitoring. This is not optional regulatory work — billing Medicare PT services above the threshold without the KX modifier results in automatic claim denial. The 2026 therapy cap is $2,480 for PT and SLP combined (separate cap for OT). Once a patient crosses this threshold, every subsequent claim line must include the KX modifier. At $3,000, the claim enters Targeted Medical Review (TMR) and requires additional documentation.

The monitoring logic is pure arithmetic: accumulate PT and SLP charges from `billing_index` for a patient within a calendar year, compare to thresholds, auto-apply modifiers, fire alerts. This is an ideal `cargo test --lib` target — no external deps.

CQ modifier is separate: it applies when a PTA performs more than 10% of a timed service. The treating provider's discipline (PT vs PTA) is stored in the user record and drives automatic CQ application.

ABN (Advance Beneficiary Notice, CMS Form CMS-R-131) must be generated and given to the patient before service when Medicare may deny a claim. MedArc generates a pre-populated ABN from encounter data using the existing `printpdf` pipeline from M003/S05.

**Confidence: HIGH** — Threshold values, modifier rules, and ABN requirements are defined in the Medicare Benefit Policy Manual (Chapter 15) and are not subject to interpretation. CY 2026 thresholds are confirmed.

## Recommendation

- Pure Rust threshold logic in `therapy_cap.rs` with zero external dependencies
- Hook `update_therapy_cap` into `cosign_pt_note` (M003/S01) and `create_encounter_billing` (M004/S01) via an event-style call (not a Tauri plugin — direct function call in the same command module)
- Auto-apply KX modifier in `create_encounter_billing` when `therapy_cap_index.cumulative_pt_charges >= 2480.0` and `payer_id` is Medicare
- Generate ABN via existing `printpdf` pipeline (same pattern as M003/S05 `generate_pdf`)
- Alerts surfaced as amber/red banners in the encounter header and on the billing page

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Threshold arithmetic | Pure Rust (`f64` comparison) | No library needed — 3 comparisons |
| ABN PDF generation | `printpdf` pipeline from M003/S05 | Reuses existing PDF generation infrastructure |
| Alert banner pattern | M003/S07 auth banner pattern (`auth_tracking.rs`) | Same amber/red threshold alert UX already in the app |
| RBAC | `Claims` resource (S02) | Therapy cap is a billing compliance function |
| Audit log | `write_audit_entry` | All cap monitoring writes are ePHI-adjacent |
| Migration pattern | Append-only `migrations.rs` | Migrations 29, 30 at indices 28, 29 |

## 2026 Therapy Cap Thresholds

### Physical Therapy + Speech-Language Pathology (Combined Cap)
- **$2,480** — Cap threshold. PT and SLP charges are combined. Charges above this amount require KX modifier.
- **$3,000** — Targeted Medical Review (TMR) threshold. Claims above $3,000 cumulative are subject to CMS medical necessity review. Additional documentation may be requested.
- Calculation basis: Calendar year (January 1 – December 31), per Medicare beneficiary, for outpatient PT and SLP services

### Occupational Therapy (Separate Cap)
- **$2,480** — Separate cap for OT services (tracked separately from PT+SLP cap)
- Not in scope for M004 (solo PT practice); included in `therapy_cap_index` schema for completeness

### Historical Context
The therapy cap was a statutory limit from 1997 that required Congressional waiver exemptions annually until 2018. The Bipartisan Budget Act of 2018 permanently eliminated the hard cap but retained KX modifier requirements above the threshold. The threshold is now indexed to inflation and updated by CMS annually each November for the following calendar year.

## KX Modifier Rules

### When to Apply
- Apply KX modifier to **every timed and untimed CPT code** (not just the first) on a claim when cumulative Medicare PT charges for the patient in the current calendar year equal or exceed $2,480
- KX attestation: "The services are medically necessary and that there is a reasonable expectation that the patient's condition will improve significantly in a reasonable (and generally predictable) period of time"
- KX modifier does NOT require separate documentation beyond existing progress notes; it is a modifier, not a prior auth

### Auto-Apply Logic
```rust
pub fn should_apply_kx(cumulative_charges: f64, new_charge: f64) -> bool {
    // Apply KX if cumulative INCLUDING the new charge will cross or already crosses 2480.0
    cumulative_charges >= 2480.0 || (cumulative_charges + new_charge) >= 2480.0
}
```

The modifier must appear in the SV1 segment of the 837P. In S01/S02, the `CptEntry.modifiers` vec is updated by S04's `get_kx_status` call during billing record creation.

### Interaction with GP Modifier
For Medicare PT: `GP + KX` are the two modifiers when above threshold. Order in SV1: GP first, KX second. The 837P SV1 modifier field supports 4 positions: `HC:<code>:GP:KX` for above-threshold timed codes.

## Targeted Medical Review (TMR) at $3,000

### What Triggers TMR
- When cumulative PT (+ SLP) charges for a Medicare patient in a calendar year reach **$3,000**, the claim enters the TMR programme
- CMS conducts prepayment reviews for claims above $3,000 (random selection + algorithm-flagged claims)
- The provider must maintain documentation that justifies medical necessity for all services above the cap

### MedArc Response
- Fire a `tmr_threshold` alert when cumulative charges reach $3,000
- Alert is informational (not a blocker) — provider must be aware documentation is subject to review
- Surfaced as an amber banner in the encounter header: "Patient has reached the Targeted Medical Review threshold. Ensure clinical documentation fully supports medical necessity for all services."

## Targeted Medical Review Alert Workflow

```
On encounter_billing save:
  1. Fetch therapy_cap_index for (patient_id, current_year)
  2. Add new charge amount to cumulative_pt_charges
  3. If cumulative_pt_charges >= 3000 AND tmr_alert_fired = false:
     a. Create therapy_cap_alerts row with alert_type = 'tmr_threshold'
     b. Set therapy_cap_index.tmr_alert_fired = true
  4. If cumulative_pt_charges >= 2480 AND kx_applied = false:
     a. Create therapy_cap_alerts row with alert_type = 'kx_threshold'
     b. Set therapy_cap_index.kx_applied = true
     c. Auto-apply KX modifier to all CPT entries in the current billing record
  5. If ABN required (kx_applied = true AND patient has not signed ABN this year):
     a. Create therapy_cap_alerts row with alert_type = 'abn_required'
```

## ABN Workflow (CMS-R-131)

### When Required
An ABN (Advance Beneficiary Notice of Noncoverage) is required when:
1. The provider believes Medicare may not pay for the service (e.g., cap exceeded, service not covered)
2. Given BEFORE the service is performed
3. Patient must sign the ABN and receive a copy

### CMS Form CMS-R-131 Fields
- Provider name, address, phone
- Patient name and Medicare ID
- Description of the service(s) Medicare may not cover
- Estimated cost of service
- Option selection: "Option 1: I want the service; I understand Medicare may not pay, and I may be responsible for the cost." etc.
- Patient signature and date

### MedArc ABN Generation
S04 generates the ABN pre-populated from encounter and patient data using `printpdf`. The ABN is:
1. Generated as a PDF by `generate_abn(patient_id, encounter_id)`
2. Saved to the patient's document center under category "consent"
3. Flagged as requiring patient signature (provider workflow: print or use digital signature — digital signature is outside M004 scope)

## CQ Modifier for PTA Services

### Rule
The CQ modifier is required on all **timed CPT codes** when:
- A Physical Therapist Assistant (PTA) performs more than 10% of the service duration, OR
- The supervising PT and PTA both provide services in the same session

The 10% threshold means: if a 30-minute session has 4 minutes or more performed by a PTA, CQ is required.

### Implementation
- Provider record has a `discipline` field: `"PT" | "PTA" | "SLPA" | "SLP" | "OT" | "OTA"`
- When `discipline = "PTA"` and the service is a timed code, `apply_cq_modifier` automatically adds CQ to the CPT entry's modifiers
- CQ modifier position: after GP (and after KX if applicable): `HC:97110:GP:KX:CQ` (Medicare timed, above cap, PTA treating)
- CQ modifier is NOT applied to evaluation codes (97161–97164) per CMS guidelines

### Auto-Apply Logic
```rust
pub fn should_apply_cq(provider_discipline: &str, cpt_is_timed: bool) -> bool {
    provider_discipline == "PTA" && cpt_is_timed
}
```

## Alert Banner UX Pattern

Following M003/S07 auth banner pattern:
- **Amber banner** (kx_threshold, tmr_threshold): appears in encounter header and billing page header; can be dismissed per session
- **Red banner** (abn_required): appears in encounter header; cannot be dismissed until ABN is generated and saved to document center
- `list_therapy_cap_alerts()` is called on encounter load; frontend filters by `acknowledged_at IS NULL`

## Common Pitfalls

- **Calendar year boundary** — Cumulative charges reset to $0 on January 1. `therapy_cap_index.calendar_year` must be the 4-digit year as a TEXT field. A patient seen in December 2025 and January 2026 has two separate rows in `therapy_cap_index`.
- **PT + SLP combined cap vs OT separate cap** — PT charges (CPT 97xxx with GP modifier) and SLP charges (CPT 92xxx or 97xxx with GN modifier) are summed together for the combined cap. OT charges (GP modifier on OT codes) count toward a separate OT cap. Wrong accumulation would over- or under-charge patients.
- **KX applies to the entire claim, not just the amount above the cap** — Once cumulative charges hit $2,480, the KX modifier goes on ALL timed codes in that encounter and all subsequent encounters in the year — not just the portion above the cap.
- **ABN is not a substitute for prior auth** — ABN is required for services Medicare might not cover (cap-related). Prior auth is for services requiring pre-approval. They are separate workflows. The `abn_required` alert must not suppress or replace the M003/S07 auth warning banners.
- **CQ and KX can coexist** — A PTA performing services above the therapy cap: `GP:KX:CQ` all three modifiers on a single SVC line. The order matters: GP, then KX, then CQ (Medicare modifier priority sequence).

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| KX auto-apply not triggered when billing record created | High | Unit test: create billing record with charges that cross $2,480 threshold; assert KX modifier in resulting CptEntry.modifiers |
| Calendar year rollover mid-January (patient seen Jan 1) | Medium | Unit test with `calendar_year = 2026` vs `2025` for same patient |
| ABN PDF generation failure | Medium | Use existing printpdf pipeline; if PDF fails, alert remains in queue; no crash |
| Therapy cap threshold changes in November | Low | Threshold value stored as a constant in `therapy_cap.rs` with a comment referencing the CMS update cycle; doc note for annual review |

## Sources

- Medicare Benefit Policy Manual, Chapter 15, Section 220 (cms.gov)
- CMS Therapy Cap information (cms.gov/medicare/coverage/therapyservices)
- CMS CY 2026 Physician Fee Schedule Final Rule — therapy cap threshold update (cms.gov)
- ABN Form CMS-R-131 Instructions (cms.gov/medicare/medicare-general-information/bni)
- CQ modifier guidance: CMS Medicare Claims Processing Manual, Chapter 5, Section 10.2
- KX modifier guidance: CMS Medicare Claims Processing Manual, Chapter 5, Section 220
- APTA: "Therapy Cap and KX Modifier Guide 2026" (apta.org)
