# M004: User Acceptance Test Criteria

**Date:** 2026-03-14
**Milestone:** M004 — Claims, Billing & Practice Management

This document defines the user acceptance test scenarios for M004. Each scenario must be executed manually in the running Tauri app on macOS using a test dataset with at least 3 months of encounter data and at least 2 Medicare patients.

---

## UAT Prerequisites

- M003 milestone is complete (PT notes, objective measures, auth tracking, fax, PDF export all operational)
- Test dataset loaded: ≥ 20 patients, ≥ 3 months of encounter data, ≥ 5 Medicare patients, ≥ 10 encounters with signed PT notes
- Office Ally sandbox credentials configured in Settings (for S02 SFTP UAT)
- At least one real ERA 835 file available for S03 testing (Medicare or commercial)
- `cargo test --lib` passes before UAT begins

---

## S01: CPT Billing Engine

### UAT-S01-01: Basic CPT Entry and 8-Minute Rule (Medicare)
**Actor:** BillingStaff
**Scenario:** Create a billing record for an encounter with two timed CPT codes.
**Steps:**
1. Navigate to Patients > [patient] > [encounter] > Billing tab
2. Search for "97110" (Therapeutic exercise), click Add
3. Enter 25 minutes for 97110
4. Add 97530 (Therapeutic activities), enter 10 minutes
5. Click "Calculate Units"

**Expected result:**
- 97110 gets 2 units (25 min ÷ 15 = 1.67, rounds down to 1 + 1 extra because 25 > 15 min individual threshold)
- 97530 gets 0 units (10 min, individual threshold not met but remainder counted in total)
- Total timed minutes: 35; total units: 2 (35 ÷ 15 = 2 rem 5; 5 < 8, no extra unit)
- Charge = 97110 rate × 2 + 97530 × 0 = $68.54 (2 × $34.27)
- GP modifier auto-applied to both codes (Medicare payer)

### UAT-S01-02: Untimed Code Does Not Count Toward 8-Minute Calculation
**Actor:** BillingStaff
**Scenario:** Add an untimed modality code alongside timed codes.
**Steps:**
1. Same encounter as UAT-S01-01
2. Add 97010 (Hot/cold pack)
3. No minutes entry appears for 97010

**Expected result:**
- 97010 shows "1 unit" (untimed) with no minutes input field
- 97010 charge = $1 (or fee schedule value); added to total charge
- 97010 excluded from 8-minute rule calculation for timed codes

### UAT-S01-03: KX Modifier Auto-Apply (Pre-S04 Threshold Test)
**Actor:** BillingStaff
**Scenario:** Verify that when a patient crosses the $2,480 Medicare therapy cap, KX is auto-applied.
**Steps:**
1. Use a test patient with $2,400 in 2026 Medicare PT charges
2. Create a new billing record for $150 of 97110 (2 units)
3. Click Save

**Expected result:**
- KX modifier appears in 97110's modifier column alongside GP
- Amber banner: "KX modifier auto-applied — patient has crossed the 2026 Medicare therapy cap ($2,480)"
- `therapy_cap_index.kx_applied` transitions to true

### UAT-S01-04: Mark Ready to Bill
**Actor:** BillingStaff
**Steps:**
1. Open billing record with CPT codes entered and saved
2. Click "Mark Ready to Bill"

**Expected result:**
- Status badge transitions from "Draft" to "Ready to Bill"
- "Generate Claim" button becomes available (S02)

### UAT-S01-05: Fee Schedule Rate Display
**Actor:** BillingStaff
**Steps:**
1. Add 97161 (PT evaluation, low complexity) to a billing record
2. Payer is Medicare

**Expected result:**
- Rate shown: $92.41 (Medicare CY 2026 non-facility)
- 97161 shown as untimed (1 unit)

---

## S02: Electronic Claims Submission (837P)

### UAT-S02-01: Generate and Validate an 837P Claim
**Actor:** BillingStaff
**Scenario:** Generate a 837P claim for a completed billing record and validate it.
**Steps:**
1. Navigate to a billing record with `ready_to_bill` status
2. Click "Generate Claim"
3. Claim summary page appears with patient, payer, CPT lines, total charge
4. Click "Validate"

**Expected result:**
- Status transitions to "Validated" with green badge
- No validation errors shown
- GP modifier present on all timed codes in the claim preview
- PRV segment shows PT taxonomy 225100000X in EDI preview

### UAT-S02-02: Validate 837P Against WEDI Validator
**Actor:** Developer / QA
**Steps:**
1. Generate a claim for a synthetic test patient
2. Click "Validate" → download the generated 837P file
3. Upload to wedi.org/resources validator tool

**Expected result:**
- WEDI validator returns: "Transaction accepted" with 0 fatal errors
- (Warnings acceptable if they are informational only)

### UAT-S02-03: Submit Claim to Office Ally Sandbox
**Actor:** BillingStaff
**Steps:**
1. Configure Office Ally sandbox SFTP credentials in Settings > Claims
2. Open a validated claim
3. Click "Submit to Office Ally"
4. Confirm the submission dialog

**Expected result:**
- Status transitions to "Submitted" with timestamp
- File appears in Office Ally portal under the sandbox test account
- Audit log shows `claim.submit` entry with claim_id and submitted_at

### UAT-S02-04: 999 Acknowledgement Auto-Update
**Actor:** System (background task)
**Steps:**
1. Submit a claim to Office Ally sandbox (UAT-S02-03)
2. Wait for acknowledgement polling (30 minutes, or trigger manually via a debug command)

**Expected result:**
- Claim status updates to "Accepted" automatically
- `claim_index.accepted_at` timestamp populated
- Notification or status change visible in Claims page without manual refresh

### UAT-S02-05: Claim State Machine Enforcement
**Actor:** BillingStaff
**Steps:**
1. Try to call "Submit" on a "Draft" claim (skip Validate step)

**Expected result:**
- Error banner: "Claim must be validated before submission"
- Status does not change

---

## S03: ERA/835 Remittance Processing

### UAT-S03-01: Import an ERA File (Medicare)
**Actor:** BillingStaff
**Steps:**
1. Navigate to ERA page
2. Click "Import ERA", select a real Medicare 835 file
3. Confirm import

**Expected result:**
- Import completes in < 5 seconds
- Summary shows: total claims processed, posted count, denial count, unmatched count
- All claims with CLP02=1 (paid) show status "posted" in payment_posting_index
- Claim status in claim_index updated to "paid" for matched claims

### UAT-S03-02: Denial Queue Shows CARC/RARC Codes
**Actor:** BillingStaff
**Steps:**
1. After ERA import with at least one denial (CO-97 or CO-119)
2. Navigate to ERA > Pending Denials tab

**Expected result:**
- Denied line items show: CARC code, RARC code (if present), human-readable description, CPT code, denied amount
- "Resolve Denial" button available for each row

### UAT-S03-03: Duplicate ERA Detection
**Actor:** BillingStaff
**Steps:**
1. Import the same ERA file twice in a row

**Expected result:**
- Second import shows error: "Duplicate ERA file — this file has already been imported"
- No duplicate payment postings created

### UAT-S03-04: A/R Aging Table Accuracy
**Actor:** BillingStaff
**Steps:**
1. After importing ERA (UAT-S03-01)
2. Navigate to ERA > A/R Aging tab

**Expected result:**
- A/R aging table shows correct bucket totals (0-30, 31-60, 61-90, 91-120, 120+)
- Claims paid via ERA do not appear in A/R totals
- A/R balance for a specific test claim matches manual calculation (claim total charge minus payments posted)

### UAT-S03-05: Unmatched Claim Handling
**Actor:** BillingStaff
**Steps:**
1. Import an ERA file with a CLP01 claim number that does not match any `claim_index` record

**Expected result:**
- Import succeeds (no crash)
- Summary shows unmatched_count = 1
- Unmatched posting appears in payment_posting_index with status = 'unmatched'

---

## S04: Therapy Cap & KX Modifier Monitoring

### UAT-S04-01: KX Modifier Threshold (Boundary Condition)
**Actor:** BillingStaff
**Steps:**
1. Set up test patient with $2,479.00 in 2026 Medicare PT charges
2. Add a $1.01 billing record (any timed code)
3. Save

**Expected result:**
- KX modifier auto-applied to all timed codes in the billing record
- Amber banner fires: "KX modifier auto-applied..."
- `therapy_cap_index.kx_applied = true`

### UAT-S04-02: Below-Threshold: No KX
**Actor:** BillingStaff
**Steps:**
1. Same patient, set cumulative to $2,478.00
2. Add a $1.00 billing record
3. Save

**Expected result:**
- No KX modifier applied
- No threshold banner
- `therapy_cap_index.kx_applied` remains false

### UAT-S04-03: TMR Alert at $3,000
**Actor:** BillingStaff
**Steps:**
1. Patient has $2,900 in 2026 Medicare PT charges (KX already applied)
2. Add a $150 billing record

**Expected result:**
- Amber TMR banner: "Patient has reached the Targeted Medical Review threshold ($3,000). Ensure clinical documentation fully supports medical necessity."
- KX modifier still present (already applied at $2,480)
- `therapy_cap_index.tmr_alert_fired = true`

### UAT-S04-04: ABN Generation
**Actor:** Provider
**Steps:**
1. Patient has active `abn_required` alert
2. Click "Generate ABN" in the red banner

**Expected result:**
- PDF opens in Preview with CMS-R-131 layout
- Provider name, address, patient name, Medicare ID pre-populated
- Estimated cost from billing record populated
- ABN saved to patient document center under "consent" category
- Red banner dismissed after ABN generated

### UAT-S04-05: CQ Modifier for PTA
**Actor:** BillingStaff
**Steps:**
1. Change a test provider's discipline to "PTA" in Settings
2. Create a billing record for that provider with timed code 97110
3. Save

**Expected result:**
- CQ modifier added to 97110 alongside GP
- Modifier order: GP, CQ (or GP, KX, CQ if above threshold)
- 97161 (evaluation code, untimed) does NOT get CQ modifier

---

## S05: HEP Builder

### UAT-S05-01: Search and Add Exercises
**Actor:** Provider
**Steps:**
1. Navigate to Patients > [patient] > HEP
2. Search "shoulder" in the exercise library
3. Add "Shoulder External Rotation" to the HEP

**Expected result:**
- Exercise appears in the HEP card list
- Sets/reps/frequency/resistance fields appear in the exercise card
- Library search returns relevant results from the seeded exercise database

### UAT-S05-02: Drag-and-Drop Reordering
**Actor:** Provider
**Steps:**
1. Add 3 exercises to a HEP
2. Drag the third exercise to the top position

**Expected result:**
- Exercise moves to the top of the list
- `sort_order` updates persist after page reload
- Other exercise order preserved

### UAT-S05-03: Export HEP to PDF
**Actor:** Provider
**Steps:**
1. Build a HEP with 3 exercises with prescription details filled in
2. Click "Export PDF"

**Expected result:**
- PDF opens in Preview within 5 seconds
- PDF contains: practice letterhead, patient name, date, "Home Exercise Program" title
- Each exercise shows: name, image (or placeholder), sets/reps/frequency, provider notes
- No blank pages or layout overflow

### UAT-S05-04: Save and Load HEP Template
**Actor:** Provider
**Steps:**
1. Build a HEP with 3 exercises
2. Click "Save as Template", name it "Shoulder Protocol"
3. Create a new HEP for a different patient
4. Click "Load Template", select "Shoulder Protocol"

**Expected result:**
- Template exercises appear in the new patient's HEP
- Modifying the loaded HEP does not change the original template
- Template appears in the template list with correct name

### UAT-S05-05: Exercise Library Load Time
**Actor:** Developer
**Steps:**
1. Clear exercise library (or use a fresh DB)
2. First HEP page open triggers seeding
3. Measure time from page open to exercise library populated

**Expected result:**
- Exercise library seeds in < 500 ms (Apple Silicon target)
- All 869 exercises available in search after seeding

---

## S06: Analytics Dashboard

### UAT-S06-01: KPI Cards Render with Live Data
**Actor:** Provider
**Steps:**
1. Navigate to Analytics
2. Select "Last 90 days" date range

**Expected result:**
- All 6 KPI cards show values (or "No data" if truly empty)
- No browser console errors
- Values are plausible for the test dataset (e.g., visits/day is not 0 if encounters exist)

### UAT-S06-02: Date Range Filter Updates All Cards
**Actor:** Provider
**Steps:**
1. Note values for "Last 90 days"
2. Switch to "Last 30 days"

**Expected result:**
- All KPI values update
- Narrower date range may show lower totals (visits count drops if recent data is sparser)
- Charts re-render without errors

### UAT-S06-03: Payer Mix Pie Chart
**Actor:** Provider / SystemAdmin
**Steps:**
1. Ensure at least 2 different payer claims exist in the test dataset
2. Navigate to Analytics > "Payer Mix" section

**Expected result:**
- Pie chart renders with each payer as a segment
- Segment labels show payer name and percentage
- Hovering a segment shows tooltip with claim count and revenue

### UAT-S06-04: MCID Achievement Rate by Measure
**Actor:** Provider
**Steps:**
1. Ensure test dataset has patients with initial AND discharge LEFS scores
2. Navigate to Analytics > "Clinical Outcomes"

**Expected result:**
- Bar chart shows MCID achievement rate for LEFS (and any other measure with complete episodes)
- Percentage calculated correctly: patients meeting MCID / total completed episodes
- Measure types with no data show 0% or "No data"

### UAT-S06-05: BillingStaff Can Only See Financial KPIs
**Actor:** BillingStaff (not SystemAdmin or Provider)
**Steps:**
1. Log in as BillingStaff
2. Navigate to Analytics

**Expected result:**
- Financial KPIs (revenue/visit, net collection rate, days in A/R, gross charges, payments posted) are visible
- Operational KPIs (visits/day, cancellation rate, units/visit) may be visible or hidden based on implementation
- Clinical outcomes panel is NOT accessible to BillingStaff (RBAC enforcement)

---

## S07: MIPS Quality Measure Capture

### UAT-S07-01: MIPS Eligibility Check
**Actor:** Provider
**Steps:**
1. Navigate to MIPS dashboard
2. Test dataset has Medicare charges < $90,000 for the year

**Expected result:**
- Amber banner: "Based on current data, you may be below the MIPS low-volume threshold. Participation is optional."
- Dashboard still visible and usable; eligibility is informational, not a blocker

### UAT-S07-02: Measure #182 Auto-Derivation
**Actor:** Provider
**Steps:**
1. Navigate to MIPS > Reporting Year 2026
2. Click "Refresh Derivation"
3. Review Measure #182 (Functional Outcome Assessment)

**Expected result:**
- Eligible patients count matches number of patients with ≥ 2 encounters in 2026
- Numerator matches patients with both an initial AND discharge outcome score
- Performance rate = numerator / denominator × 100%

### UAT-S07-03: PHQ-2/PHQ-9 Capture and Safety Protocol
**Actor:** Provider
**Steps:**
1. Navigate to Patients > [patient] > [encounter] > MIPS Screening section
2. Enter PHQ-2 score: Item 1 = 2, Item 2 = 2 (total = 4, ≥ 3)

**Expected result:**
- PHQ-9 section expands automatically
- Enter PHQ-9 item 9 = 1 (thoughts of self-harm)
- Safety reminder banner appears: "Patient endorsed thoughts of self-harm. Follow your practice's safety protocol."
- Save button saves the PHQ screen as `MIPSPhqScreen` FHIR Observation

### UAT-S07-04: Falls Risk Screening Capture
**Actor:** Provider
**Steps:**
1. Navigate to encounter for a patient aged ≥ 65
2. Open MIPS Screening section
3. Select "Timed Up and Go (TUG)" as the screening tool
4. Select result "Positive (≥ 12 seconds)"
5. Click Save

**Expected result:**
- Falls screen saved as `MIPSFallsScreen` FHIR Observation
- Measure #155 numerator increments on next `refreshMipsDerivation` call

### UAT-S07-05: MIPS CSV Export
**Actor:** Provider
**Steps:**
1. After refreshing derivation for 2026
2. Click "Download CSV"

**Expected result:**
- File save dialog appears
- Downloaded CSV has columns: Measure ID, Measure Title, Eligible Patients, Numerator, Denominator, Exclusions, Performance Rate
- Values match the dashboard display
- CSV opens correctly in Excel/Numbers with correct column formatting

---

## End-to-End Billing Cycle Workflow

### UAT-E2E-01: Complete Revenue Cycle for One Encounter
**Actor:** BillingStaff + Provider
**Scenario:** Full cycle from encounter to payment posting.

**Steps:**
1. Provider creates and co-signs a PT Progress Note for an encounter
2. BillingStaff opens the encounter Billing tab
3. BillingStaff adds 97110 (25 min), 97530 (10 min), 97010 (untimed) with Medicare payer
4. Units calculated via 8-minute rule; GP modifier auto-applied
5. BillingStaff clicks "Mark Ready to Bill" → status: `ready_to_bill`
6. BillingStaff clicks "Generate Claim" → 837P generated
7. BillingStaff clicks "Validate" → status: `validated`
8. BillingStaff clicks "Submit to Office Ally" → status: `submitted`
9. 999 acknowledgement received → status: `accepted`
10. BillingStaff imports ERA file with payment for this claim
11. Payment auto-posted → claim status: `paid`
12. BillingStaff checks A/R aging — claim no longer in outstanding A/R

**Expected result:**
- Each step completes without errors
- Claim lifecycle progresses correctly through all states
- A/R aging reflects the payment after ERA import
- Audit log shows all steps with correct timestamps

---

## Performance Criteria

| Scenario | Target | Acceptable |
|----------|--------|------------|
| 8-minute rule calculation | < 100 ms | < 500 ms |
| 837P generation | < 2 s | < 5 s |
| ERA import (100-claim file) | < 5 s | < 10 s |
| Analytics dashboard load | < 2 s | < 5 s |
| MIPS derivation refresh | < 10 s | < 30 s |
| HEP PDF export | < 5 s | < 10 s |
| Exercise library seeding | < 500 ms | < 1 s |

---

## Edge Cases to Test

### Billing Edge Cases
- Encounter with no CPT codes → "Mark Ready to Bill" blocked with validation error
- Encounter billed to non-Medicare payer → no GP modifier auto-applied
- Encounter with 97161 (evaluation, untimed) + 97110 (timed) — evaluation code does not participate in 8-minute rule
- BillingStaff cannot access Patient clinical notes (RBAC boundary)

### Claims Edge Cases
- Generate 837P for a patient with no insurance payer set → validation error: "Payer configuration required"
- Submit claim when SFTP credentials not configured → error: "Office Ally SFTP credentials not configured. Set up in Settings > Claims."
- Claim with > 4 modifiers on a single CPT code → validation error (SV1 supports max 4 modifiers)

### ERA Edge Cases
- ERA file with zero claims (empty GE/IEA only) → import succeeds with counts all = 0
- ERA with a payer not in `payer_config` → creates unmatched posting; does not crash
- ERA import during concurrent SFTP poll → only one process should run (advisory lock)

### Therapy Cap Edge Cases
- Patient with $2,480.00 exactly in charges → KX fires on the NEXT charge (not retroactively)
- Calendar year rollover: December 31 encounter + January 1 encounter for same patient → separate `therapy_cap_index` rows for 2025 and 2026
- CQ modifier with non-timed code (97161) → CQ NOT applied even for PTA discipline

### MIPS Edge Cases
- Patient with PHQ-2 score = 2 (< 3) → PHQ-9 section does not expand
- Provider who is not a Medicare clinician → MIPS dashboard shows "No Medicare encounters in this period"
- MIPS derivation with no outcome scores in `outcome_score_index` → Measure #182 shows 0/0 (not a divide-by-zero error)
