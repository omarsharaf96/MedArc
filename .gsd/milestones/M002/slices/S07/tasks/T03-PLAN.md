---
estimated_steps: 5
estimated_files: 1
---

# T03: End-to-end verification in live Tauri app and write UAT

**Slice:** S07 — Settings, Cleanup & End-to-End Verification
**Milestone:** M002

## Description

Launch the Tauri dev app, exercise the full patient visit workflow with a Provider account, then verify RBAC with a FrontDesk account. Document every step result in `S07-UAT.md`. This is the M002 milestone completion gate — the milestone is not done until this UAT passes.

Before launching, confirm `tsc --noEmit` is still clean and `cargo test --lib` still passes 265+ tests (regression gate — no Rust changes are expected, but confirm).

**Important operational note about restore:** Running `restore_backup` in a dev session replaces the live database file on disk. Do not run a restore during UAT unless testing on a throwaway session. The UAT should verify that the Restore button is visible only for SystemAdmin, and optionally test the UI flow (select path, confirm button) but stop before submitting.

## Steps

1. **Pre-flight checks** — Run `npx tsc --noEmit` (must exit 0) and `cargo test --lib` (must pass 265+ tests). Confirm `CalendarPage 2.tsx` is gone: `ls "src/components/scheduling/CalendarPage 2.tsx" 2>&1`.

2. **Launch and Provider workflow** — Run `npm run tauri dev`. Wait for the app window. Log in as Provider. Execute the following workflow in order, noting pass/fail:
   1. Log in as Provider
   2. Navigate to Patients → search for an existing patient (name or MRN)
   3. Open patient detail → confirm: demographics visible, ClinicalSidebar shows tabs (Problems/Medications/Allergies/Immunizations), LabResults panel visible, Documents panel visible
   4. Click "Start Encounter" → EncounterWorkspace opens with SOAP editor
   5. Write Subjective text + Plan text → click Save → confirm save succeeds (no error)
   6. Navigate to Vitals tab → enter BP systolic/diastolic + HR + Temp → Save → confirm BMI or vitals saved (no error)
   7. Navigate back to patient detail → ClinicalSidebar → Medications tab → "Add Medication" → fill in name/RxNorm/status → save → medication appears in list
   8. Navigate to Schedule → create a follow-up appointment for the patient (set date + duration + type) → confirm appointment appears in calendar
   9. Navigate to Settings → Backup tab → confirm backup history table renders (may be empty or have entries) → click "Choose Folder" → select any writable directory → click "Create Backup" → confirm success banner appears with file path
   10. Log out

3. **FrontDesk RBAC verification** — Log in as FrontDesk. Verify:
    - Sidebar shows only "Schedule" — no Patients link, no Settings link
    - Navigate to Schedule → confirm day/week calendar grid renders with appointment cards (or empty state)
    - Confirm Patient Flow Board is accessible
    - Log out

4. **Note deviations** — Record any steps that fail or behave unexpectedly. Distinguish between: (a) real bugs that block the step, (b) known limitations (e.g. first-run with no data), (c) environmental issues (dev build, no real Apple ID signing).

5. **Write S07-UAT.md** — Create `.gsd/milestones/M002/slices/S07/S07-UAT.md` using the format below. Every step must have a status (PASS / FAIL / SKIP) and a note.

## UAT Document Format

```markdown
# S07 UAT Results

**Date:** YYYY-MM-DD
**Tester:** Agent (automated walkthrough)
**Build:** npm run tauri dev
**tsc --noEmit:** PASS / FAIL
**cargo test --lib:** PASS (NNN tests) / FAIL

## Provider Workflow

| # | Step | Status | Notes |
|---|------|--------|-------|
| 1 | Log in as Provider | PASS/FAIL | |
| 2 | Search for patient | PASS/FAIL | |
| 3 | Patient detail — sidebar, labs, docs visible | PASS/FAIL | |
| 4 | Start Encounter → EncounterWorkspace opens | PASS/FAIL | |
| 5 | Write SOAP note → save | PASS/FAIL | |
| 6 | Record vitals → save | PASS/FAIL | |
| 7 | Add medication in ClinicalSidebar | PASS/FAIL | |
| 8 | Create follow-up appointment | PASS/FAIL | |
| 9 | Settings → list backups → create backup | PASS/FAIL | |
| 10 | Log out | PASS/FAIL | |

## FrontDesk RBAC

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 1 | Log in as FrontDesk | PASS/FAIL | |
| 2 | Sidebar shows only Schedule | PASS/FAIL | |
| 3 | Calendar renders | PASS/FAIL | |
| 4 | Flow Board accessible | PASS/FAIL | |
| 5 | Log out | PASS/FAIL | |

## Overall Verdict

**PASS** / **FAIL**

## Known Issues / Deviations

- (list any)
```

## Must-Haves

- [ ] `npx tsc --noEmit` passes before launching the app
- [ ] `cargo test --lib` passes 265+ tests before launching the app
- [ ] `CalendarPage 2.tsx` confirmed deleted
- [ ] Provider workflow steps 1–10 all attempted; steps 1, 2, 3, 4, 5, 9 must be PASS for overall PASS
- [ ] FrontDesk RBAC verified: step 2 (sidebar shows only Schedule) must be PASS
- [ ] `S07-UAT.md` written with a table for every step and an overall PASS/FAIL verdict
- [ ] Any FAIL steps documented with specific error observed

## Verification

- `cat .gsd/milestones/M002/slices/S07/S07-UAT.md` — must exist, contain a Results table, and show overall verdict
- `grep "Overall Verdict" .gsd/milestones/M002/slices/S07/S07-UAT.md` — must be present
- Provider workflow and FrontDesk RBAC tables both present in the UAT doc

## Observability Impact

- Signals added/changed: `S07-UAT.md` provides a permanent, human-readable verification record for M002 milestone completion; any future agent auditing the milestone can read it to understand which steps were verified and any known deviations
- How a future agent inspects this: `cat .gsd/milestones/M002/slices/S07/S07-UAT.md`
- Failure state exposed: FAIL rows in the table name the specific step and error; deviations section explains environmental or known limitations

## Inputs

- T01 + T02 complete — `tsc --noEmit` clean, SettingsPage fully built
- Running Tauri dev app — `npm run tauri dev` succeeds
- At least one Provider account and one FrontDesk account registered in the dev database
- An existing patient record (created via S02 UI) to use for the workflow

## Expected Output

- `.gsd/milestones/M002/slices/S07/S07-UAT.md` — UAT results document with pass/fail per step, overall verdict, and any known deviations
- M002 milestone definition of done confirmed: all 7 slices complete, tsc clean, cargo tests pass, full workflow verified, RBAC verified
