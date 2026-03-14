# S01: CPT Billing Engine & 8-Minute Rule Calculator

**Goal:** Provider and BillingStaff can attach CPT codes to any encounter from the bundled PT code library; units are auto-calculated via the 8-minute rule (Medicare and AMA variants); a per-payer fee schedule drives the billing summary. The billing engine is proven by `cargo test --lib` covering all standard unit-calculation scenarios; the charge entry UI is proven by `tsc --noEmit`.

**Demo:** `cargo test --lib` exits 0 with ≥9 new 8-minute rule unit tests. `tsc --noEmit` exits 0. BillingStaff navigates to an encounter, opens the Billing tab, searches for "97110", enters 25 minutes, sees "2 units @ $34.27 = $68.54" with GP modifier auto-applied, adds 97530 at 10 minutes and sees the total update; clicking "Calculate Units" shows the Medicare rule explanation. "Mark Ready to Bill" transitions the record to `ready_to_bill`.

## Must-Haves

- Migrations 22 (`billing_index`), 23 (`cpt_code_library` with 35 PT-relevant codes seeded), and 24 (`fee_schedule` with Medicare CY 2026 rates seeded) applied without errors
- `calculate_units_8min_medicare` and `calculate_units_ama` are pure Rust functions with zero side effects; exposed via `calculate_units_8min(encounter_id, rule)` Tauri command
- Unit distribution algorithm correctly assigns units to codes with the most minutes (not uniformly)
- GP modifier auto-applied on all Medicare claims; GO/GN auto-applied for OT/SLP disciplines
- `create_encounter_billing`, `get_encounter_billing`, `update_encounter_billing`, `list_encounter_billings`, `calculate_units_8min`, `get_cpt_code`, `list_cpt_codes`, `get_fee_schedule`, `set_fee_schedule` Tauri commands registered and returning correct types
- `billing_index` status constrained to `draft | ready_to_bill | submitted | paid | denied | adjusted`
- New RBAC `Billing` resource added to `roles.rs`; BillingStaff has full CRUD; Provider has Create + Read; NurseMa has Read; FrontDesk has none
- All billing commands write audit rows; all ePHI-touching billing operations logged
- `src/types/billing.ts` — TypeScript types for all billing shapes with `T | null` for optionals
- Billing wrappers appended to `src/lib/tauri.ts` under `// M004/S01` comment
- `BillingPage.tsx` renders CPT code search, charge entry with timed/untimed distinction, modifier column, per-line charge, running total footer, and "Mark Ready to Bill" button
- `cargo test --lib` passes with ≥9 new billing tests (all APTA 8-minute rule reference cases), 0 failures
- `tsc --noEmit` exits 0 after T02

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — billing page must render with real CPT codes from the seeded library and compute correct units when minutes are entered
- Human/UAT required: yes — BillingStaff manually enters CPT codes for an encounter and verifies unit calculation matches expected APTA reference values

## Verification

```bash
# 1. Contract — all existing tests + new billing tests pass
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. Migration smoke test
cd src-tauri && cargo test --lib -- db::migrations 2>&1

# 4. 8-minute rule reference cases (embedded in unit tests):
#    Case 1: 25 min 97110 + 10 min 97530 = 35 total → 2 units, no extra (5 < 8)
#    Case 2: 25 min 97110 + 15 min 97530 = 40 total → 2 units + 1 extra = 3 units
#    Case 3: 8 min 97110 only = 8 total → 1 unit
#    Case 4: 7 min 97110 only = 7 total → 0 units (< 8 min threshold)
#    Case 5: 15 min 97110 + 15 min 97112 = 30 total → 2 units (no extra)
#    Case 6: 16 min 97110 + 16 min 97112 = 32 total → 2 units + 1 extra (2 > 8)
#    Case 7: 97010 (untimed) + 25 min 97110 = 25 timed → 1 unit + extra (25=1*15+10 ≥ 8)
#    Case 8: 0 timed minutes, 97010 only → 0 units from timed rule
#    Case 9: AMA rule: 22 min 97110 → round to 30 min → 2 units
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` rows written on every `billing.create`, `billing.update`, `billing.get`, `billing.list` call; audit action strings are `"billing.create"`, `"billing.update"`, `"billing.get"`, `"billing.list"`, `"billing.ready_to_bill"`
- Inspection surfaces:
  - `billing_index` table: `SELECT * FROM billing_index WHERE patient_id = ?` shows all billing records per patient
  - `fhir_resources` WHERE `resource_type = 'Claim'` for full CPT entry JSON blob
  - `cargo test --lib -- commands::billing` for 8-minute rule contract
- Failure state exposed: `AppError::Validation` returned when encounter has no CPT codes at `ready_to_bill` transition; `AppError::NotFound` for missing billing record or CPT code; all propagate through existing frontend error banner
- Redaction constraints: Audit `details` contains only `billing_id`, `encounter_id`, `patient_id` — not CPT codes or charge amounts (to avoid logging financial PHI in audit details)

## Integration Closure

- Upstream surfaces consumed:
  - `src-tauri/src/commands/documentation.rs` — `encounter_index.encounter_id` FK for linking billing to encounters
  - `src-tauri/src/rbac/roles.rs` — existing `Resource` enum extended with `Billing`
  - `src-tauri/src/db/migrations.rs` — Migrations 22, 23, 24 appended after Migration 21
  - `src/lib/tauri.ts` — billing wrappers appended under `// M004/S01`
- New wiring introduced in this slice:
  - `commands/billing.rs` module registered in `commands/mod.rs` and `lib.rs`
  - `Billing` resource added to RBAC matrix in `roles.rs`
  - Nine Tauri commands wired into `invoke_handler!` macro
  - `BillingPage.tsx` as a new route target (`{ page: "billing"; encounterId: string; patientId: string }`)
  - "Billing" button in `EncounterDetailPage.tsx`
- What remains before the milestone is truly usable end-to-end: S02 (837P claim generation reads from `billing_index`), S03 (ERA posting updates billing status), S04 (therapy cap reads CPT charges), S06 (analytics reads financial KPIs from billing data)

## Tasks

- [ ] **T01: Backend — billing module, Migrations 22–24, 8-minute rule engine** `est:3h`
  - Why: Establishes the entire billing data layer. All downstream slices (S02, S04, S06) depend on `billing_index` and the `calculate_units_8min` logic. Unit tests provide the primary `cargo test --lib` proof for this slice.
  - Files: `src-tauri/src/commands/billing.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/rbac/roles.rs`, `src-tauri/src/lib.rs`, `src/types/billing.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/billing.rs` with: (a) pure functions `calculate_units_8min_medicare`, `calculate_units_ama`, `distribute_units_to_codes`; (b) FHIR Claim builder `build_billing_fhir()`; (c) nine Tauri commands: `create_encounter_billing`, `get_encounter_billing`, `update_encounter_billing`, `list_encounter_billings`, `calculate_units_8min`, `get_cpt_code`, `list_cpt_codes`, `get_fee_schedule`, `set_fee_schedule`; (d) `#[cfg(test)]` module with ≥9 unit tests covering all APTA reference cases and the untimed code exclusion
    2. Append Migrations 22, 23 (with CPT seed INSERT statements for all 35 codes), and 24 (with Medicare CY 2026 fee schedule seed) to `MIGRATIONS` vector in `migrations.rs` at indices 21, 22, 23
    3. Add `Billing` resource variant to `Resource` enum in `rbac/roles.rs`; assign permissions: SystemAdmin = CRUD, Provider = CR, NurseMa = R, BillingStaff = CRUD, FrontDesk = none
    4. Add `pub mod billing;` to `commands/mod.rs`
    5. Append nine commands to `invoke_handler!` in `lib.rs` under `// M004/S01` comment
    6. Create `src/types/billing.ts` with: `BillingStatus`, `CptEntry`, `BillingRecord`, `BillingInput`, `CptCode`, `FeeScheduleEntry`, `EightMinuteRuleResult`, `CptUnitAllocation` — all with `T | null` optionals, no `T | undefined`
    7. Append new wrappers to `src/lib/tauri.ts` under `// M004/S01`: `createEncounterBilling`, `getEncounterBilling`, `updateEncounterBilling`, `listEncounterBillings`, `calculateUnits8min`, `getCptCode`, `listCptCodes`, `getFeeSchedule`, `setFeeSchedule`
  - Verify: `cd src-tauri && cargo test --lib` passes with ≥9 new billing tests, 0 failures; `npx tsc --noEmit` exits 0 (types + wrappers compile)
  - Done when: `cargo test --lib` exits 0 with all new billing tests included; Migrations 22–24 validate; `tsc --noEmit` exits 0

- [ ] **T02: Frontend — BillingPage charge entry UI** `est:2h`
  - Why: Delivers BILL-01 through BILL-04 (CPT entry, unit calculation, fee schedule display, modifier support). Makes the billing engine visible and interactive for BillingStaff and Provider.
  - Files: `src/pages/BillingPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/EncounterDetailPage.tsx`
  - Do:
    1. Create `src/pages/BillingPage.tsx` with two-panel layout:
       - Left panel: CPT code search input + category filter buttons (Evaluation, Therapeutic Exercise, Neuromuscular, Gait Training, Physical Agents, etc.). Code list shows code, description, timed/untimed badge. Clicking a code adds it to the right panel.
       - Right panel: charge sheet table with columns: Code | Description | Type | Minutes (input, timed only) | Units (calculated, read-only) | Modifiers (multi-select dropdown: GP, GO, GN, KX, CQ, 59, 25) | Rate | Charge. Footer row: Total Charge.
       - "Calculate Units (8-min rule)" button calls `calculateUnits8min`. Result panel below shows rule explanation text and per-code unit allocation.
       - "Save Draft" button calls `createEncounterBilling` or `updateEncounterBilling`.
       - "Mark Ready to Bill" button (BillingStaff only) transitions status. Disabled if no codes or untimed codes are missing required modifiers.
       - Encounter header shows auth remaining (read from M003 auth_tracking if available; gracefully hidden if not).
    2. Add `{ page: "billing"; encounterId: string; patientId: string }` to the `Route` union in `RouterContext.tsx`
    3. Add `case "billing":` dispatch in `ContentArea.tsx`
    4. Add "Billing" button to `EncounterDetailPage.tsx` visible to BillingStaff and Provider
  - Verify: `npx tsc --noEmit` exits 0; page renders without runtime errors; CPT code lookup returns real codes from the seeded library; unit calculation updates inline when minutes change
  - Done when: `tsc --noEmit` exits 0; charge sheet adds/removes codes; 8-minute rule calculation fires and displays correctly; status transition to `ready_to_bill` persists

## Files Likely Touched

- `src-tauri/src/commands/billing.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod billing` (T01)
- `src-tauri/src/db/migrations.rs` — Migrations 22, 23, 24 appended (T01)
- `src-tauri/src/rbac/roles.rs` — `Billing` resource added (T01)
- `src-tauri/src/lib.rs` — 9 commands registered (T01)
- `src/types/billing.ts` — new file (T01)
- `src/lib/tauri.ts` — 9 new wrappers appended (T01)
- `src/pages/BillingPage.tsx` — new page (T02)
- `src/contexts/RouterContext.tsx` — new route variant (T02)
- `src/components/shell/ContentArea.tsx` — new dispatch case (T02)
- `src/pages/EncounterDetailPage.tsx` — Billing button (T02)
