# S02: Patient Module

**Goal:** A practitioner can create a patient with demographics, insurance, and care team; search by name or MRN; view a patient detail page with all demographics; and edit existing records — all through the UI, backed by real Tauri commands.

**Demo:** Log in as Provider → navigate to Patients → see searchable patient roster → click "New Patient" → fill Basic Info + Insurance tabs → submit → land on PatientDetailPage showing full demographics/insurance/care team sections → click "Edit" → change a field → save → updated values appear on the detail page. TypeScript check exits 0.

## Must-Haves

- `PatientListPage` replaces the `PatientsPage` placeholder: shows a live searchable roster (name/MRN search, debounced) backed by `commands.searchPatients`; each row is clickable and navigates to `patient-detail`
- `PatientDetailPage` renders the full patient chart shell: demographics, insurance, employer, SDOH sections extracted from `PatientRecord.resource` (FHIR JSON) via a `extractPatientDisplay()` helper; care team panel via `getCareTeam`; related persons list via `listRelatedPersons`
- `PatientFormModal` handles create and edit flows: tabbed (Basic Info / Insurance + Other); calls `createPatient` or `updatePatient`; calls `upsertCareTeam` when care team fields are filled
- `usePatient(id)` hook encapsulates `getPatient` + `getCareTeam` + `listRelatedPersons` with `T | null` state pattern + loading/error
- `ContentArea.tsx` wired to render `PatientDetailPage` (not `PatientsPage` stub) for `patient-detail` route
- RBAC gate in list: "New Patient" button visible only to `Provider`, `NurseMa`, `FrontDesk`, `SystemAdmin`; not visible to `BillingStaff`
- RBAC gate in detail: `BillingStaff` sees only demographics/insurance; care team + SDOH sections hidden
- `extractPatientDisplay()` helper isolates all FHIR path navigation — no FHIR structure knowledge scattered in component bodies
- `tsc --noEmit` exits 0 after all three tasks

## Proof Level

- This slice proves: **integration** — real Tauri commands are called; real FHIR JSON is extracted and displayed
- Real runtime required: yes (Tauri app `npm run tauri dev` for final visual check)
- Human/UAT required: no (TypeScript check + runtime smoke test by the executing agent is sufficient)

## Verification

After all tasks complete, run in order:

```bash
# 1. TypeScript check (use cached binary — npx tsc hangs in this environment)
/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit
# Expected: exit 0, zero errors

# 2. Rust regression (no Rust changes in S02, but verify no regressions)
cd src-tauri && cargo test --lib 2>&1 | tail -5
# Expected: "test result: ok. 265 tests; 0 failed"

# 3. Structural file checks
ls src/pages/PatientDetailPage.tsx          # must exist
ls src/components/patient/PatientListPage.tsx   # must exist
ls src/components/patient/PatientFormModal.tsx  # must exist
ls src/hooks/usePatient.ts                  # must exist
ls src/lib/fhirExtract.ts                   # must exist

# 4. ContentArea wiring check
grep -n "PatientDetailPage" src/components/shell/ContentArea.tsx
# Expected: import line + render line (not PatientsPage stub)

# 5. RBAC gate check
grep -n "BillingStaff\|canCreate\|canEdit" src/components/patient/PatientListPage.tsx
# Expected: lines showing role-conditional "New Patient" button
```

## Observability / Diagnostics

- **Loading state**: all data-fetching components show a spinner/skeleton while loading and an inline error message on failure — a future agent can see the error text directly in the UI
- **FHIR extraction failures**: `extractPatientDisplay()` returns a plain object where missing fields are `null`; components render "—" for null fields rather than crashing — no FHIR extraction error is silent but also none is fatal
- **Unknown role in detail page**: if `role` is unrecognized, the detail page renders the public view (demographics only) rather than crashing — same safe-default pattern as `NAV_ITEMS_BY_ROLE`
- **Empty search results**: list page renders an explicit "No patients found" message when `searchPatients` returns `[]` — distinguishable from loading state
- **Form submit errors**: `PatientFormModal` surfaces the Tauri command error string inline above the submit button, not in a toast or console — visible in the app without DevTools

- Runtime signals: console.error calls on unexpected command failures (pattern from AuditLog.tsx)
- Inspection surfaces: React DevTools → `usePatient` hook state (`data`, `loading`, `error`); Tauri devtools for IPC call payloads
- Failure visibility: inline error strings in list page, detail page header, and form modal
- Redaction constraints: PatientRecord.resource is FHIR JSON — no raw resource blob logged to console; only extracted display fields are rendered

## Integration Closure

- Upstream surfaces consumed:
  - `AppShell` → `ContentArea` → `patient-detail` case (from S01)
  - `useNav()` hook and `Route` union type (`patient-detail` variant already defined) from `RouterContext.tsx`
  - `commands.createPatient`, `getPatient`, `updatePatient`, `searchPatients`, `upsertCareTeam`, `getCareTeam`, `addRelatedPerson`, `listRelatedPersons` from `src/lib/tauri.ts`
  - All `PatientInput`, `PatientRecord`, `PatientSummary`, `CareTeamMemberInput`, `CareTeamRecord`, `RelatedPersonRecord` types from `src/types/patient.ts`
- New wiring introduced in this slice:
  - `ContentArea.tsx` case `"patient-detail"` → `PatientDetailPage` (replacing the `PatientsPage` stub)
  - `PatientListPage` renders as the `patients` route target
  - `usePatient(id)` hook consumed by `PatientDetailPage` and (in edit mode) by `PatientFormModal`
- What remains before the milestone is truly usable end-to-end:
  - S03 must hang `EncounterWorkspace` off `PatientDetailPage` shell
  - S04 must hang `ClinicalSidebar` off `PatientDetailPage` shell
  - S05 must build the calendar UI

## Tasks

- [x] **T01: Build PatientListPage with live search and RBAC-gated New Patient button** `est:1h`
  - Why: Replaces the PatientsPage placeholder with a real data-backed roster; the slice cannot be demonstrated without a way to navigate to a patient
  - Files: `src/pages/PatientsPage.tsx` (replaced), `src/components/patient/PatientListPage.tsx` (new)
  - Do: Create `src/components/patient/` directory; build `PatientListPage` with debounced free-text search input calling `commands.searchPatients({ name: query || null, mrn: null, birthDate: null, limit: null })`; table rows show MRN, full name (`givenNames.join(" ") + " " + familyName`), DOB, gender, phone; clicking a row calls `navigate({ page: "patient-detail", patientId: row.id })`; "New Patient" button opens `PatientFormModal` — rendered only when `role` is in `["Provider", "NurseMa", "FrontDesk", "SystemAdmin"]`; loading spinner while fetching; "No patients found — try a different search" empty state; `BillingStaff` and unrecognized roles see list but no "New Patient" button; update `PatientsPage.tsx` to re-export from `PatientListPage` (pass `role` from `useAuth` via `AppShell` or embed auth in `PatientsPage`); use `useAuth` hook to obtain role
  - Verify: `tsc --noEmit` exits 0; `grep -n "searchPatients" src/components/patient/PatientListPage.tsx` returns hit; "New Patient" button conditionally rendered verified by `grep -n "canCreate\|Provider\|FrontDesk"` in the file
  - Done when: TypeScript check exits 0 AND `PatientListPage.tsx` exists AND `PatientsPage.tsx` renders it AND role-conditional button logic is present

- [x] **T02: Build extractPatientDisplay helper + usePatient hook + PatientDetailPage** `est:1.5h`
  - Why: Provides the patient chart shell that S03/S04 will extend; all FHIR path navigation is isolated in `extractPatientDisplay()`; `usePatient` encapsulates data-fetching
  - Files: `src/lib/fhirExtract.ts` (new), `src/hooks/usePatient.ts` (new), `src/pages/PatientDetailPage.tsx` (new)
  - Do: Write `src/lib/fhirExtract.ts` with `extractPatientDisplay(resource: Record<string, unknown>)` that navigates the FHIR blob safely with optional chaining — returns `PatientDisplay` plain object with: `familyName`, `givenNames`, `dob`, `gender`, `genderIdentity`, `phone`, `email`, `address`, `mrn`, `primaryProviderId`, `photoUrl`, `insurancePrimary` (sub-object with payerName/memberId/etc), `insuranceSecondary`, `insuranceTertiary`, `employer`, `sdoh` — all fields nullable strings/objects; guard against `resource == null`; use the exact FHIR extension URLs confirmed from `patient.rs`; write `src/hooks/usePatient.ts` implementing `usePatient(patientId: string)` that fetches `getPatient`, `getCareTeam`, `listRelatedPersons` in parallel (`Promise.all`), uses `mounted` boolean pattern from `useAuth.ts` to cancel stale state updates, returns `{ patient, careTeam, relatedPersons, loading, error, reload }`; write `src/pages/PatientDetailPage.tsx` with props `{ patientId: string; role: string }` that calls `usePatient`, renders a back-button header (calls `goBack()`), demographics section, insurance section, employer section, SDOH section (hidden when `role === "BillingStaff"`), care team panel (hidden when `role === "BillingStaff"`), related persons list; "Edit" button opens `PatientFormModal` pre-populated; loading/error states shown inline
  - Verify: `tsc --noEmit` exits 0; `grep -n "extractPatientDisplay\|http://medarc.local" src/lib/fhirExtract.ts` returns FHIR URL constants; `grep -n "BillingStaff" src/pages/PatientDetailPage.tsx` returns role-gate lines; file sizes reasonable (fhirExtract.ts < 150 lines, usePatient.ts < 80 lines)
  - Done when: TypeScript check exits 0 AND all three files exist AND FHIR URL constants match `patient.rs` exactly AND BillingStaff gate is present

- [x] **T03: Build PatientFormModal and wire ContentArea to render PatientDetailPage** `est:1.5h`
  - Why: Completes the create/edit patient flow and closes the last routing gap (`patient-detail` case in ContentArea still renders the PatientsPage stub)
  - Files: `src/components/patient/PatientFormModal.tsx` (new), `src/components/shell/ContentArea.tsx` (modified), `src/pages/PatientDetailPage.tsx` (minor — import check)
  - Do: Build `PatientFormModal` with two tabs — "Basic Info" (familyName, givenNames as a single "First Name" input that maps to `[firstName]` on submit, DOB, gender, phone, email, addressLine, city, state, postalCode) and "Insurance / Other" (insurancePrimary fields: payerName, memberId, planName, groupNumber; care team section: memberId, memberName, role, note — required fields validated before submit); controlled form state with `useState`; on submit: create path calls `commands.createPatient(input)`, then if care team fields are filled calls `commands.upsertCareTeam(...)`, then navigates to `patient-detail`; edit path calls `commands.updatePatient(patientId, input)`, then `upsertCareTeam` if care team changed, then closes modal; pre-populate from `PatientRecord.resource` via `extractPatientDisplay()` when `patientId` prop is provided; inline error display above submit button; `position: fixed inset-0 z-50 bg-black/40` backdrop matching `LockScreen.tsx` pattern; update `ContentArea.tsx`: import `PatientDetailPage`, change `patient-detail` case to render `<PatientDetailPage patientId={currentRoute.patientId} role={???} />` — need to thread `userRole` from AppShell props down to ContentArea; the cleanest approach is to read `useAuth` inside `ContentArea` (same as `PatientsPage` will do) to get `role`; update `PatientsPage.tsx` to pass role to `PatientListPage`
  - Verify: `tsc --noEmit` exits 0; `grep -n "PatientDetailPage" src/components/shell/ContentArea.tsx` shows import + case render (not PatientsPage stub); `grep -n "createPatient\|updatePatient" src/components/patient/PatientFormModal.tsx` returns hits; full smoke test: `npm run tauri dev` launches without console errors and patient list renders
  - Done when: TypeScript check exits 0 AND ContentArea no longer renders PatientsPage for patient-detail route AND PatientFormModal.tsx exists AND create + edit paths are wired to real commands

## Files Likely Touched

- `src/components/patient/PatientListPage.tsx` — new: searchable patient roster
- `src/components/patient/PatientFormModal.tsx` — new: create/edit patient form
- `src/pages/PatientsPage.tsx` — updated: re-export / wrap PatientListPage
- `src/pages/PatientDetailPage.tsx` — new: patient chart shell
- `src/hooks/usePatient.ts` — new: data-fetching hook
- `src/lib/fhirExtract.ts` — new: FHIR field extraction helpers
- `src/components/shell/ContentArea.tsx` — updated: wire patient-detail to PatientDetailPage
