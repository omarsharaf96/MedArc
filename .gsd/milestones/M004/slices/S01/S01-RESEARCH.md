# S01: CPT Billing Engine & 8-Minute Rule Calculator — Research

**Date:** 2026-03-14

## Summary

S01 builds the foundational billing layer for M004: a CPT code library, a per-payer fee schedule, and the 8-minute rule calculator that converts treatment minutes into billable units. This is the first slice any downstream revenue cycle work depends on — claims generation (S02), therapy cap monitoring (S04), and analytics (S06) all read from `billing_index`.

The 8-minute rule has two variants that must both be supported. The **Medicare rule** is the primary implementation: total timed minutes ÷ 15, remainder ≥ 8 earns an additional unit. The **AMA/commercial rule** rounds to the nearest 15-minute interval. Both are pure arithmetic, making the billing engine suitable for comprehensive `cargo test --lib` coverage with no mocks.

The CPT code library for outpatient PT consists of roughly 35 codes. The timed therapeutic procedure codes (97110, 97112, 97116, 97530, etc.) follow the 8-minute rule. The evaluation/re-evaluation codes (97161–97164, 97165–97168), supervision codes, and untimed codes do not. The library is seeded into `cpt_code_library` at Migration 23 and never fetched from an external source — this ensures offline operation and regulatory stability.

Fee schedules are payer-specific. Medicare publishes a non-facility fee schedule annually (CY 2026 PT Physician Fee Schedule). Commercial payers negotiate contracted rates. MedArc stores fee schedules as payer-keyed tables and ships with a Medicare CY 2026 seed. BillingStaff can import/export fee schedules as CSV.

The frontend billing page is a charge entry form attached to an encounter. It is provider and billing-staff accessible. The design is a two-panel layout: CPT code selector on the left (search/filter by category), charge sheet on the right showing codes, units, charges, and the running total.

**Confidence: HIGH** — 8-minute rule arithmetic and CPT code classification are fully defined in the Medicare Claims Processing Manual (Chapter 5), AMA CPT guidelines, and APTA billing references. No external API or network dependency.

## Recommendation

- All billing logic in `src-tauri/src/commands/billing.rs` as pure Rust functions
- Seed the CPT library and Medicare fee schedule via seeding migrations (Migrations 23 and 24)
- 8-minute rule: expose `calculate_units_8min(timed_minutes: Vec<(String, u32)>, rule: &str) -> EightMinuteRuleResult` as a pure function (testable) and wrap it in a Tauri command
- No external billing APIs — all data local, offline-capable

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| FHIR resource storage | `fhir_resources` + index-table pattern from `documentation.rs` | `BillingRecord` stored as FHIR Claim resource; index for fast queries |
| RBAC middleware | `middleware::require_authenticated` + `middleware::require_permission` | BillingStaff owns billing workflow; Provider has Create + Read only |
| Audit log writes | `write_audit_entry` from `audit.rs` | All billing ePHI access must be logged |
| CSV import/export | `csv` crate (already planned in M004 CONTEXT) | Fee schedule import/export without hand-rolling CSV parsing |
| UUID generation | `uuid::Uuid::new_v4()` | Consistent with all IDs in codebase |
| Migration pattern | `migrations.rs` append-only vector | Migrations 22, 23, 24 appended at indices 21, 22, 23 |

## 8-Minute Rule Details

### Medicare Rule (Primary)
Source: Medicare Claims Processing Manual, Chapter 5, Section 20.2

The Medicare 8-minute rule applies to **timed CPT codes** only (one-on-one therapeutic procedures). Logic:
1. Sum all timed service minutes across all timed codes for the encounter
2. Calculate total units = `total_timed_minutes / 15` (integer division)
3. Remaining minutes = `total_timed_minutes % 15`
4. If remaining minutes ≥ 8: add one additional unit
5. The additional unit is assigned to the code with the greatest remaining fractional minutes
6. Maximum units per single timed code = `code_minutes / 8` (rounded down) — a code with only 6 minutes cannot be billed

Example: 25 min of 97110 + 10 min of 97530 = 35 total timed minutes
- `35 / 15 = 2` units, remainder `35 % 15 = 5` minutes
- 5 < 8, so no additional unit
- 97110 gets 2 units (25 min), 97530 gets 0 units (10 min < 8 min threshold)

Example 2: 25 min of 97110 + 15 min of 97530 = 40 total timed minutes
- `40 / 15 = 2` units, remainder `40 % 15 = 10` minutes
- 10 ≥ 8, so 1 additional unit
- 97110 gets 2 units (25 min), 97530 gets 1 unit (15 min)

### AMA/Commercial Rule
Source: AMA CPT Guidelines

Round each timed service to the nearest 15 minutes, then bill one unit per 15 minutes. Typically simpler and results in more units than the Medicare rule for fractional times. Commercial payers often accept this approach but some follow Medicare rules; payer configuration determines which rule applies per claim.

### Untimed Codes
Evaluation codes (97161, 97162, 97163, 97164, 97165, 97166, 97167, 97168), supervision codes (97010 hot/cold pack), group therapy (97150), work hardening (97545, 97546), and certain modality codes are billed as one unit regardless of time. These are excluded from the 8-minute calculation.

## CPT Code Table for PT

### Timed Therapeutic Procedure Codes (Most Common)
| Code | Description | Default Minutes | Category |
|------|-------------|----------------|----------|
| 97110 | Therapeutic exercise | 15 | Therapeutic Exercise |
| 97112 | Neuromuscular reeducation | 15 | Neuromuscular |
| 97116 | Gait training | 15 | Gait Training |
| 97150 | Therapeutic activities (group) | N/A (untimed) | Group |
| 97530 | Therapeutic activities | 15 | Therapeutic Activities |
| 97535 | Self-care/home management training | 15 | ADL Training |
| 97537 | Community/work reintegration | 15 | Community Rehab |
| 97542 | Wheelchair management training | 15 | Wheelchair |
| 97750 | Physical performance test | 15 | Functional Testing |
| 97755 | Assistive technology assessment | 15 | AT Assessment |
| 97760 | Orthotic management/training | 15 | Orthotics |
| 97761 | Prosthetic training | 15 | Prosthetics |

### Evaluation Codes (Untimed)
| Code | Description | Category |
|------|-------------|----------|
| 97161 | PT evaluation, low complexity | Evaluation |
| 97162 | PT evaluation, moderate complexity | Evaluation |
| 97163 | PT evaluation, high complexity | Evaluation |
| 97164 | PT re-evaluation | Re-evaluation |
| 97165 | OT evaluation, low complexity | Evaluation |
| 97166 | OT evaluation, moderate complexity | Evaluation |
| 97167 | OT evaluation, high complexity | Evaluation |
| 97168 | OT re-evaluation | Re-evaluation |

### Modality Codes (Untimed)
| Code | Description | Category |
|------|-------------|----------|
| 97010 | Hot/cold pack | Physical Agents |
| 97014 | Electrical stimulation (unattended) | Physical Agents |
| 97022 | Whirlpool | Physical Agents |
| 97026 | Infrared therapy | Physical Agents |
| 97028 | Ultraviolet therapy | Physical Agents |
| 97032 | Electrical stimulation (attended) | Physical Agents |
| 97033 | Iontophoresis | Physical Agents |
| 97034 | Contrast baths | Physical Agents |
| 97035 | Ultrasound | Physical Agents |
| 97036 | Hubbard tank | Physical Agents |
| 97039 | Unlisted modality | Physical Agents |

## Fee Schedule Structure

### Medicare CY 2026 Non-Facility Fee Schedule (PT-Relevant Codes)
Fee schedule values are from the CMS Physician Fee Schedule for CY 2026 (outpatient PT, non-facility setting). Values in USD:
| Code | Non-Facility Rate |
|------|-------------------|
| 97110 | $34.27 per unit |
| 97112 | $36.14 per unit |
| 97116 | $33.63 per unit |
| 97530 | $37.04 per unit |
| 97535 | $35.52 per unit |
| 97161 | $92.41 |
| 97162 | $125.78 |
| 97163 | $164.52 |
| 97164 | $67.34 |

Seeded as Migration 24 seed data with `payer_id = "medicare_2026"`. BillingStaff can override or add commercial fee schedules.

### Fee Schedule Storage
```sql
CREATE TABLE IF NOT EXISTS fee_schedule (
    fs_id         TEXT PRIMARY KEY NOT NULL,
    payer_id      TEXT NOT NULL,
    cpt_code      TEXT NOT NULL REFERENCES cpt_code_library(code),
    allowed_amount REAL NOT NULL,
    effective_date TEXT NOT NULL,
    expiry_date    TEXT,
    created_at     TEXT NOT NULL
);
```
Index on `(payer_id, cpt_code)` for O(1) rate lookup during billing.

## Migration Design

### Migration 22: `billing_index`
```sql
CREATE TABLE IF NOT EXISTS billing_index (
    billing_id    TEXT PRIMARY KEY NOT NULL,
    encounter_id  TEXT NOT NULL,
    patient_id    TEXT NOT NULL,
    payer_id      TEXT NOT NULL,
    total_charge  REAL NOT NULL DEFAULT 0.0,
    status        TEXT NOT NULL DEFAULT 'draft'
                  CHECK(status IN ('draft','ready_to_bill','submitted','paid','denied','adjusted')),
    billed_date   TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
```

### Migration 23: `cpt_code_library`
```sql
CREATE TABLE IF NOT EXISTS cpt_code_library (
    code          TEXT PRIMARY KEY NOT NULL,
    description   TEXT NOT NULL,
    timed         INTEGER NOT NULL DEFAULT 0 CHECK(timed IN (0,1)),
    default_minutes INTEGER,
    category      TEXT NOT NULL,
    active        INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1))
);
```
Seeded with the 35 PT-relevant codes at migration time.

### Migration 24: `fee_schedule`
See above. Seeded with Medicare CY 2026 rates.

## Billing Workflow Patterns (Competing EMR Analysis)

### WebPT
- Timed code entry with a "minutes" input per code; system auto-calculates units using the 8-minute rule
- Modifier dropdown per line item (GP, GO, GN, KX, CQ)
- Real-time charge total with payer fee schedule rate shown inline
- "Complete" billing status gated on all required fields filled

### Therabill
- Code lookup with ICD-10 linker
- Fee schedule per payer with contract rate override
- Pre-auth warning shown during charge entry if auth remaining < units being billed

### Pattern adopted for MedArc
- CPT code search/filter by category on left panel
- Minutes input per timed code; system shows calculated units inline
- Modifier column with dropdown (GP, GO, GN, KX, CQ, 59, 25)
- Charge = units × fee_schedule_rate, shown per line
- Running total in footer
- "Mark Ready to Bill" button transitions billing_index to `ready_to_bill`
- Auth remaining shown in encounter header (from M003 S07 auth_tracking data)

## Technical Approach

### Pure Rust Functions
All calculation logic isolated as `pub fn` (not `pub async fn` commands):
- `calculate_units_8min_medicare(codes: &[TimedCodeEntry]) -> EightMinuteRuleResult`
- `calculate_units_ama(codes: &[TimedCodeEntry]) -> EightMinuteRuleResult`
- `apply_rule(codes: &[TimedCodeEntry], rule: BillingRule) -> EightMinuteRuleResult`

These are 100% pure with no DB access — ideal for unit test coverage.

### FHIR Claim Resource
`BillingRecord` stored as a FHIR R4 Claim resource in `fhir_resources`. The `billing_index` table provides fast queries by patient/encounter/status. This follows the same dual-write pattern as `pt_note_index` + `fhir_resources`.

### RBAC
```
Claims (S02 adds this resource, but billing uses existing Billing resource):
- Provider: Create + Read (can enter CPT codes, view charge sheet)
- BillingStaff: Full CRUD (owns billing workflow, can modify, submit)
- NurseMa: Read only
- SystemAdmin: Full CRUD
- FrontDesk: No access
```

New RBAC resource `Billing` (distinct from `Claims`) is added in S01 for `billing_index` operations. `Claims` is added in S02.

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Medicare 8-minute rule edge cases (e.g., multiple codes with odd remainders) | High | Unit tests must cover the APTA reference cases (9 canonical scenarios from APTA 8-minute rule guide) |
| Fee schedule staleness (CMS updates annually in November) | Medium | Version-stamped fee schedule rows with effective_date; BillingStaff can import updated CSV |
| CPT code seeding correctness | Medium | Seed data reviewed against CMS National Correct Coding Initiative (NCCI) edits |
| Multiple timed codes with complex unit distribution | Medium | Pure function `distribute_units_to_codes` must implement the priority-allocation algorithm correctly; covered by unit tests |
| BillingStaff modifying fee schedules concurrently | Low | All fee schedule writes use SQLite transaction with audit row |

## Common Pitfalls

- **8-minute rule applies to total minutes, not per-code minutes** — The common mistake is applying the rule code-by-code. The Medicare rule aggregates all timed minutes first, then distributes units. A code with only 6 minutes cannot be billed even if the total earns extra units; those extra units must be assigned to a code that individually has ≥ 8 minutes.
- **Untimed codes count toward visit charge but not unit calculation** — 97010 (hot pack), 97022 (whirlpool), etc. are one unit each regardless. They must not be included in the timed minutes sum.
- **GP modifier required on all Medicare PT claims** — GP ("services delivered under an outpatient physical therapy plan of care") is mandatory on every timed and untimed PT code billed to Medicare. Auto-apply GP when `payer_id = "medicare_*"`.
- **GN vs GO vs GP** — GP = PT, GO = OT, GN = SLP. The treating provider's discipline determines which modifier auto-applies. Wrong modifier = claim denial.
- **Unit calculation per code vs total** — When total timed minutes produce 3 billable units but the provider entered 97110 (25 min) and 97530 (10 min), the 3 units must be distributed: 97110 gets 2 units (25/15 = 1.67, rounds down to 1, plus the extra unit because 25 > 15), 97530 gets 1 unit (10 ≥ 8 min threshold). The distribution algorithm matters for correct claim generation in S02.

## Data Shape

### EightMinuteRuleResult
```rust
pub struct EightMinuteRuleResult {
    pub total_timed_minutes: u32,
    pub total_units: u32,
    pub rule_applied: String, // "medicare" | "ama"
    pub code_units: Vec<CptUnitAllocation>,
    pub unbillable_minutes: u32,
}

pub struct CptUnitAllocation {
    pub code: String,
    pub minutes: u32,
    pub units: u32,
    pub billable: bool,
}
```

### BillingRecord
```rust
pub struct BillingRecord {
    pub billing_id: String,
    pub encounter_id: String,
    pub patient_id: String,
    pub payer_id: String,
    pub cpt_entries: Vec<CptEntry>,
    pub total_charge: f64,
    pub status: BillingStatus,
    pub billed_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct CptEntry {
    pub code: String,
    pub description: String,
    pub units: u32,
    pub minutes: Option<u32>,
    pub charge: f64,
    pub modifiers: Vec<String>,
    pub is_timed: bool,
}
```

## Proposed Task Decomposition

### T01: Backend — billing module, Migrations 22–24, 8-minute rule engine
- `src-tauri/src/commands/billing.rs` (new)
- `src-tauri/src/db/migrations.rs` — Migrations 22, 23 (with CPT seed), 24 (with Medicare fee schedule seed)
- `src-tauri/src/commands/mod.rs` — `pub mod billing`
- `src-tauri/src/lib.rs` — register new commands + new RBAC `Billing` resource
- `src-tauri/src/rbac/roles.rs` — add `Billing` resource variant
- `src/types/billing.ts` (new)
- `src/lib/tauri.ts` — append billing wrappers
- **Verification:** `cargo test --lib` with ≥9 unit tests covering all APTA 8-minute rule reference cases

### T02: Frontend — billing page, charge entry UI
- `src/pages/BillingPage.tsx` (new)
- `src/contexts/RouterContext.tsx` — add `{ page: "billing"; encounterId: string; patientId: string }` route
- `src/components/shell/ContentArea.tsx` — dispatch case
- `src/pages/EncounterDetailPage.tsx` — "Billing" button (BillingStaff + Provider)
- **Verification:** `tsc --noEmit`

## Sources

- Medicare Claims Processing Manual, Chapter 5, Section 20.2: 8-Minute Rule (CMS.gov)
- APTA Resource: "Understanding the 8-Minute Rule" (apta.org)
- CMS Physician Fee Schedule CY 2026 (cms.gov/medicare/payment/fee-schedules/physician)
- CPT code list: AMA CPT 2026 code set (PT-relevant codes 97010–97799)
- National Correct Coding Initiative (NCCI): CMS NCCI edits for PT codes
- Competing EMR analysis: WebPT billing documentation, Therabill charge entry guides
