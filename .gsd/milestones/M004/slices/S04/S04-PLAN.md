# S04: Therapy Cap & KX Modifier Monitoring

**Goal:** Medicare cumulative PT charges are tracked per patient per calendar year. The KX modifier is automatically applied when cumulative charges reach $2,480; a Targeted Medical Review alert fires at $3,000; PTA encounters receive the CQ modifier; ABN workflow triggers when required. Proven by `cargo test --lib` asserting all four threshold/modifier rules and manual UI verification of banner alerts.

**Demo:** A test patient has accumulated $2,400 in PT charges for 2026. BillingStaff creates a billing record for an encounter with $150 of 97110. On save, the system automatically adds the KX modifier to 97110, transitions `therapy_cap_index.kx_applied = true`, and fires an amber banner: "KX modifier auto-applied — patient has crossed the 2026 Medicare therapy cap ($2,480)." After adding another $400 encounter, the TMR banner fires: "Patient has reached the Targeted Medical Review threshold ($3,000). Maintain complete documentation."

## Must-Haves

- Migrations 29 (`therapy_cap_index`) and 30 (`therapy_cap_alerts`) applied without errors
- `update_therapy_cap(patient_id, encounter_id, new_charge_amount, payer_id)` — pure logic function called within `create_encounter_billing` and `update_encounter_billing` when `payer_id` starts with `"medicare"`; updates cumulative total and fires alerts
- `get_therapy_cap_status(patient_id, calendar_year?)` returns `TherapyCapStatus` with cumulative charges, KX status, TMR alert status
- `list_therapy_cap_alerts()` returns unacknowledged alerts for all patients; used by BillingStaff dashboard
- `acknowledge_alert(alert_id)` marks an alert as acknowledged
- `generate_abn(patient_id, encounter_id)` generates a PDF pre-populated from encounter/patient data via `printpdf` pipeline; saves to document center under category "consent"
- `get_kx_status(patient_id, calendar_year?)` — returns bool indicating KX modifier required for patient
- `apply_cq_modifier(encounter_id)` — adds CQ modifier to all timed CPT entries when treating provider has `discipline = "PTA"`
- KX modifier auto-applies to all timed AND untimed CPT codes in the billing record when threshold is crossed
- `cargo test --lib` passes with ≥7 new therapy cap unit tests:
  - KX fires at exactly $2,480 (boundary condition)
  - KX does NOT fire below $2,480
  - TMR fires at exactly $3,000
  - Calendar year rollover: 2025 accumulation does not affect 2026
  - PT+SLP combined: both CPT categories contribute to same cap
  - CQ: `discipline = "PTA"` + timed code → CQ applied; non-timed code → no CQ
  - KX + CQ coexistence: modifier order is GP, KX, CQ
- Amber banner shown on encounter header and billing page when `kx_threshold` alert unacknowledged
- Red banner shown (and ABN button enabled) when `abn_required` alert unacknowledged
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — banners must appear correctly in the running Tauri app when threshold conditions are met
- Human/UAT required: yes — BillingStaff manually verifies KX modifier appears in charge sheet and banners fire at correct thresholds

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. Unit test spot-checks (embedded in cargo test --lib):
#    - Patient with $2479 cumulative + $50 new charge → KX fires (total $2529 > $2480)
#    - Patient with $2479 cumulative + $0.99 new charge → KX does NOT fire ($2479.99 < $2480)
#    - Patient with $2480 cumulative + $520 new → TMR fires (total $3000)
#    - Patient has 2025 accumulation of $3000; 2026 accumulation = $0 → no KX for 2026
#    - `should_apply_cq("PTA", true)` → true
#    - `should_apply_cq("PT", true)` → false
#    - `should_apply_cq("PTA", false)` → false (untimed/evaluation code)

# 4. Banner verification (manual, in running Tauri dev app):
#    - Set test patient to $2400 cumulative; add $100 billing record → amber KX banner
#    - Add $600 more → TMR amber banner
#    - Verify "Generate ABN" button appears when abn_required alert is active
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` for `therapy_cap.update`, `therapy_cap.kx_applied`, `therapy_cap.tmr_alert`, `therapy_cap.abn_generated`
- Inspection surfaces:
  - `therapy_cap_index WHERE patient_id = ?` — cumulative totals and modifier status
  - `therapy_cap_alerts WHERE acknowledged_at IS NULL` — active unacknowledged alerts
  - `fhir_resources WHERE resource_type = 'ABN'` — generated ABN documents
- Failure state: `generate_abn` returns `AppError::Io` if PDF generation fails (does not block billing); `update_therapy_cap` is called within `create_encounter_billing` transaction — if it fails, the whole billing create rolls back

## Integration Closure

- Upstream surfaces consumed:
  - `billing_index` (S01) — reads CPT charges for running total accumulation
  - `create_encounter_billing` (S01) — S04 hooks `update_therapy_cap` into this command as a post-save step
  - `printpdf` pipeline (M003/S05) — reused for ABN PDF generation
  - `document_center.rs` (M003/S04) — ABN saved to patient document center under "consent" category
  - Auth banner pattern (M003/S07) — same UX pattern reused for cap alerts
- New wiring introduced:
  - `commands/therapy_cap.rs` registered in `commands/mod.rs` and `lib.rs`
  - Six Tauri commands in `invoke_handler!`
  - `TherapyCapAlertBanner` component used by `EncounterDetailPage.tsx` and `BillingPage.tsx`
  - `create_encounter_billing` in `billing.rs` updated to call `update_therapy_cap` (read-only addition — no schema change to billing commands)
- What remains: S07 MIPS reads therapy cap data for completeness; ABN workflow lacks digital signature (deferred to future milestone)

## Tasks

- [ ] **T01: Backend — therapy cap module, Migrations 29–30, threshold engine** `est:3h`
  - Why: Pure Rust threshold logic is the primary `cargo test --lib` proof for this slice. All seven unit tests must pass before T02 begins.
  - Files: `src-tauri/src/commands/therapy_cap.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/billing.rs` (hook), `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `src/types/therapy_cap.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/therapy_cap.rs` with: (a) pure functions `should_apply_kx`, `should_apply_cq`, `should_fire_tmr`; (b) `update_therapy_cap` function (called within billing commands); (c) six Tauri commands: `get_therapy_cap_status`, `list_therapy_cap_alerts`, `acknowledge_alert`, `generate_abn`, `get_kx_status`, `apply_cq_modifier`; (d) `#[cfg(test)]` module with ≥7 unit tests per the verification section
    2. Add hook in `billing.rs` `create_encounter_billing` command: after successfully inserting `billing_index` row, call `update_therapy_cap(patient_id, encounter_id, total_charge, payer_id, &conn)` — wrapped in the same DB transaction
    3. Implement `generate_abn` using printpdf: CMS-R-131 form layout with pre-populated fields (provider name, address, patient name, Medicare ID from patient record, estimated cost from billing record)
    4. Append Migrations 29 and 30 to `MIGRATIONS`
    5. Create `src/types/therapy_cap.ts` with `TherapyCapStatus`, `TherapyCapAlert`, `AlertType`, `AbnRecord`
    6. Append therapy cap wrappers to `src/lib/tauri.ts` under `// M004/S04`
  - Verify: `cargo test --lib` passes with ≥7 new threshold tests; ABN PDF generates without error

- [ ] **T02: Frontend — therapy cap alert banners and ABN flow** `est:2h`
  - Why: The banners are the user-visible compliance mechanism. BillingStaff and Provider must see actionable alerts before submitting claims above the therapy cap.
  - Files: `src/components/billing/TherapyCapAlertBanner.tsx` (new), `src/pages/BillingPage.tsx`, `src/pages/EncounterDetailPage.tsx`
  - Do:
    1. Create `TherapyCapAlertBanner.tsx`: accepts `alerts: TherapyCapAlert[]` prop; renders amber banner for `kx_threshold` ("KX modifier auto-applied. Cumulative charges: $X / $2,480."), amber banner for `tmr_threshold` ("TMR threshold reached. Ensure full documentation."), red banner for `abn_required` with "Generate ABN" button that calls `generateAbn`
    2. Add `TherapyCapAlertBanner` to `BillingPage.tsx` header (load alerts on mount via `listTherapyCapAlerts`, filter by current patient)
    3. Add `TherapyCapAlertBanner` to `EncounterDetailPage.tsx` header
    4. Acknowledge button on each banner calls `acknowledgeAlert`; banner disappears on acknowledgement
  - Verify: `tsc --noEmit` exits 0; banners render correctly; ABN button generates PDF and shows confirmation

## Files Likely Touched

- `src-tauri/src/commands/therapy_cap.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod therapy_cap` (T01)
- `src-tauri/src/commands/billing.rs` — `update_therapy_cap` hook added (T01)
- `src-tauri/src/db/migrations.rs` — Migrations 29, 30 appended (T01)
- `src-tauri/src/lib.rs` — 6 commands registered (T01)
- `src/types/therapy_cap.ts` — new file (T01)
- `src/lib/tauri.ts` — therapy cap wrappers appended (T01)
- `src/components/billing/TherapyCapAlertBanner.tsx` — new component (T02)
- `src/pages/BillingPage.tsx` — banner added to header (T02)
- `src/pages/EncounterDetailPage.tsx` — banner added to header (T02)
