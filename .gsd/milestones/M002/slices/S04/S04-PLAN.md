# S04: Clinical Data Sidebar

**Goal:** Mount a `ClinicalSidebar` component (Problems | Medications | Allergies | Immunizations tabs) inside `PatientDetailPage`, backed by a `useClinicalData(patientId)` hook wired to all existing Tauri commands. Add FHIR extraction helpers for all four resource types. Surface passive drug-allergy CDS alerts via a `DrugAllergyAlertBanner`. Add/edit modal forms for all four data domains with full RBAC gating.

**Demo:** A Provider opens a patient chart and sees the Clinical Data section immediately below Encounters. The Problems tab lists ICD-10-coded diagnoses with status badges; clicking "Add Problem" opens a modal that saves successfully. The Medications tab lists active medications; a "Stop" status change flows through `updateMedication`. The Allergies tab allows adding a drug allergy with RxNorm coding; after saving, the Drug-Allergy Alert Banner appears if an active medication matches. The Immunizations tab lists administered vaccines. BillingStaff and FrontDesk see no clinical data. `tsc --noEmit` exits 0.

## Must-Haves

- `useClinicalData(patientId)` hook — `mounted`-guard, `refreshCounter`/`reload`, `Promise.all` for all 4 lists plus alert check; per-tab error isolation
- FHIR extraction helpers in `fhirExtract.ts`: `extractAllergyDisplay`, `extractProblemDisplay`, `extractMedicationDisplay`, `extractImmunizationDisplay`
- `ClinicalSidebar` component with four tabs (Problems | Medications | Allergies | Immunizations) mounted in `PatientDetailPage` above Demographics, below Encounters; RBAC-gated (Provider/NurseMa/SystemAdmin = full; BillingStaff/FrontDesk = hidden)
- `DrugAllergyAlertBanner` rendered above the tabs; re-fires after any allergy or medication mutation
- Add modal forms for all four domains (allergy, problem, medication, immunization) following `PatientFormModal` pattern
- Edit/update modal for allergies (status change + field edit), problems (status change), and medications (status change via `updateMedication`)
- `deleteAllergy` with confirmation; medications/problems use status transitions (no physical delete)
- Allergy form: show RxNorm `substanceCode`/`substanceSystem` fields only when `category === "drug"`
- Medication status `<select>` with all 8 valid values defaulting to `"active"`
- `tsc --noEmit` exits 0 throughout; no `any`, all optional fields `T | null`

## Proof Level

- This slice proves: integration
- Real runtime required: yes (Tauri dev app)
- Human/UAT required: no (agent-verified in browser)

## Verification

- `tsc --noEmit` exits 0 after each task
- `ClinicalSidebar` renders under PatientDetailPage in the running Tauri app (Provider role)
- All four tabs cycle correctly; Problems, Medications, Allergies, Immunizations tabs each render their respective list
- "Add Problem" modal round-trip: save → list refreshes → new problem visible
- Drug-allergy alert banner appears after adding a drug allergy that matches an active medication
- BillingStaff role: `ClinicalSidebar` is not rendered in the page
- `cargo test --lib` continues to pass (no Rust changes; baseline confirmation only)

## Observability / Diagnostics

- Runtime signals: `console.error("[useClinicalData] …")` per-tab on fetch failure; `console.error("[ClinicalSidebar] …")` on mutation failure; inline error banners visible without DevTools
- Inspection surfaces: React DevTools — `useClinicalData` state (`allergies`, `problems`, `medications`, `immunizations`, `alerts`, `loadingX`, `errorX`); `ClinicalSidebar` state (`activeTab`, `addOpen`, `editTarget`)
- Failure visibility: per-tab error state (one tab can fail independently without crashing the rest); modal `submitError` inline above submit button
- Redaction constraints: no PHI in `console.error` messages — log IDs and error strings only

## Integration Closure

- Upstream surfaces consumed: `PatientDetailPage` (chart shell, `SectionCard`, `InfoRow`, `isBillingStaff`), `usePatient` hook pattern, `PatientFormModal` modal pattern, `EncounterWorkspace` tab chrome + `INPUT_CLS`/`LABEL_CLS`, all `commands.add*/list*/update*/delete*/checkDrug*` wrappers, all `*Record`/`*Input` types from `src/types/patient.ts` + `DrugAllergyAlert` from `src/types/documentation.ts`
- New wiring introduced in this slice: `ClinicalSidebar` import + JSX mounted in `PatientDetailPage`; `useClinicalData` hook exported from `src/hooks/`; four FHIR display helpers exported from `fhirExtract.ts`
- What remains before the milestone is truly usable end-to-end: S05 (scheduling/flow board), S06 (labs/documents/physical exam), S07 (settings + final cleanup)

## Tasks

- [x] **T01: FHIR extraction helpers + `useClinicalData` hook** `est:45m`
  - Why: Establishes the data layer for the entire slice. All components in T02/T03 depend on the hook return type and FHIR helpers. Build these first so TypeScript contracts are locked before UI work.
  - Files: `src/lib/fhirExtract.ts`, `src/hooks/useClinicalData.ts`
  - Do: Add `extractAllergyDisplay`, `extractProblemDisplay`, `extractMedicationDisplay`, `extractImmunizationDisplay` to `fhirExtract.ts` following the `extractPatientDisplay` pattern — each takes `Record<string, unknown> | null | undefined`, reads from the FHIR paths confirmed in `clinical.rs` (`build_allergy_fhir` etc.), and returns a typed flat struct. Write `useClinicalData(patientId)` following `usePatient.ts` verbatim: `mounted` boolean, `refreshCounter`/`reload`, `Promise.all` loading all 4 lists (`listAllergies`, `listProblems`, `listMedications`, `listImmunizations`) plus `checkDrugAllergyAlerts` in a single `Promise.all([...])`. Per-tab error isolation: catch each list call independently so one failure doesn't blank all tabs. Expose typed add/update/delete callbacks that call the relevant command then `reload()`. Return type: `UseClinicalDataReturn` with `allergies`, `problems`, `medications`, `immunizations`, `alerts`, per-tab `loading`/`error`, and all mutation callbacks.
  - Verify: `tsc --noEmit` exits 0
  - Done when: `useClinicalData.ts` and four new FHIR helpers in `fhirExtract.ts` compile cleanly; return type fully typed with no `any`

- [x] **T02: `ClinicalSidebar` shell + tab chrome + list views** `est:60m`
  - Why: Builds the visible component and all four read-only list panels. Wires the hook. Mounts the component in `PatientDetailPage`. After this task the full sidebar is visible and functional for the read path.
  - Files: `src/components/clinical/ClinicalSidebar.tsx`, `src/pages/PatientDetailPage.tsx`
  - Do: Create `src/components/clinical/ClinicalSidebar.tsx`. Props: `patientId: string; role: string`. Call `useClinicalData(patientId)` at the top level of `ClinicalSidebar` (not in `PatientDetailPage`). Tab chrome from `EncounterWorkspace.tsx` pattern — `activeTab` state with button row, `active` highlight class. Four tabs: Problems | Medications | Allergies | Immunizations. Each tab renders its list using `extract*Display` helpers; per-tab loading spinner + per-tab error banner with Retry. Empty-state messages when list is empty. `DrugAllergyAlertBanner` above the tabs: renders alert cards with severity badge (`warning` = yellow, `contraindicated` = red) and the `message` field; hidden when `alerts.length === 0`. RBAC: hide add/edit/delete action buttons for roles without write access (show only for Provider/NurseMa/SystemAdmin — NurseMa has no delete). In `PatientDetailPage.tsx`: import `ClinicalSidebar` and mount it inside a `SectionCard` titled "Clinical Data" above the Demographics section, gated by `!isBillingStaff && role !== "FrontDesk"`. Render `ClinicalSidebar` outside any `loading` conditional block so `activeTab` state persists across parent refreshes.
  - Verify: `tsc --noEmit` exits 0; app renders Clinical Data section under Provider role; all four tabs visible; BillingStaff/FrontDesk: section absent
  - Done when: Four tabs render correctly with list data from backend; alert banner appears for matching drug allergies; RBAC gate confirmed

- [x] **T03: Add/Edit modal forms for all four clinical domains** `est:75m`
  - Why: Completes the write path — providers can add and update clinical data. Without these modals the sidebar is read-only and the slice demo cannot be proven.
  - Files: `src/components/clinical/AllergyFormModal.tsx`, `src/components/clinical/ProblemFormModal.tsx`, `src/components/clinical/MedicationFormModal.tsx`, `src/components/clinical/ImmunizationFormModal.tsx`, `src/components/clinical/ClinicalSidebar.tsx`
  - Do: Each modal follows `PatientFormModal.tsx` pattern exactly: `fixed inset-0 bg-black/40 z-50` overlay, centered panel, controlled form state, inline `submitError`, `submitting` spinner on submit button. Use `INPUT_CLS`/`LABEL_CLS` constants from `EncounterWorkspace.tsx`. **AllergyFormModal**: fields — substance (required), category `<select>` (drug/food/environment/biologic), clinicalStatus `<select>`, allergyType `<select>`, severity `<select>`, reaction, onsetDate, notes. Show `substanceCode` + `substanceSystem` fields only when `category === "drug"`. Props: `patientId`, `initial: AllergyRecord | null` (null = add mode), `onSuccess`, `onClose`. On save, calls `addAllergy` or `updateAllergy` then `onSuccess`. Delete button (Provider/SystemAdmin only) calls `deleteAllergy(allergyId, patientId)` with inline confirmation (`window.confirm` is OK here — Tauri WKWebView blocks on it but it's a safety-critical delete confirmation that should be explicit). **ProblemFormModal**: fields — icd10Code (required), display (required), clinicalStatus `<select>` (active/inactive/resolved), onsetDate, abatementDate, notes. Props same pattern. **MedicationFormModal**: fields — display (required), rxnormCode, status `<select>` with all 8 values defaulting to "active", dosage, effectiveStart, effectiveEnd, prescriberId, reason, notes. **ImmunizationFormModal**: fields — cvxCode (required), display (required), occurrenceDate (required), lotNumber, expirationDate, site, route, doseNumber, status `<select>` (completed/entered-in-error/not-done). Wire all four modal components into `ClinicalSidebar.tsx`: add `addOpen`/`editTarget` state per tab, "Add" button per tab, row-level "Edit" button on each list item. After modal `onSuccess`, call `reload()` from `useClinicalData`.
  - Verify: `tsc --noEmit` exits 0; Add Problem flow: fill form → save → list refreshes; medication status change: select "stopped" → save → row updates; allergy delete: confirm dialog → row removed; alert banner refreshes after allergy add
  - Done when: All four add flows and allergy/problem/medication edit flows work end-to-end in the running app; `tsc --noEmit` exits 0

## Files Likely Touched

- `src/lib/fhirExtract.ts`
- `src/hooks/useClinicalData.ts` (new)
- `src/components/clinical/ClinicalSidebar.tsx` (new)
- `src/components/clinical/AllergyFormModal.tsx` (new)
- `src/components/clinical/ProblemFormModal.tsx` (new)
- `src/components/clinical/MedicationFormModal.tsx` (new)
- `src/components/clinical/ImmunizationFormModal.tsx` (new)
- `src/pages/PatientDetailPage.tsx`
