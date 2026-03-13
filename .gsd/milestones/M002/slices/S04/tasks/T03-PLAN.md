---
estimated_steps: 6
estimated_files: 6
---

# T03: Add/Edit modal forms for all four clinical domains

**Slice:** S04 — Clinical Data Sidebar
**Milestone:** M002

## Description

Completes the write path: providers can add and update all four clinical data types. Builds four modal form components (AllergyFormModal, ProblemFormModal, MedicationFormModal, ImmunizationFormModal) following the `PatientFormModal` pattern and wires them into `ClinicalSidebar` via `addOpen`/`editTarget` state. After this task the full slice demo is provable — the Drug-Allergy Alert Banner also refreshes automatically because `useClinicalData.reload()` re-fetches alerts on every mutation.

This task also confirms the complete slice by running `tsc --noEmit` and verifying the round-trip flows in the running Tauri app.

## Steps

1. **`AllergyFormModal.tsx`**:
   - Props: `patientId: string; initial: AllergyRecord | null; onSuccess: () => void; onClose: () => void`
   - Overlay: `fixed inset-0 bg-black/40 z-50 flex items-center justify-center` (matches `PatientFormModal`)
   - Panel: `bg-white rounded-lg shadow-xl w-full max-w-lg p-6`
   - Controlled form state; `INPUT_CLS`/`LABEL_CLS` from `EncounterWorkspace.tsx`
   - Fields: substance (text, required), category `<select>` (drug/food/environment/biologic), clinicalStatus `<select>` (active/inactive/resolved), allergyType `<select>` (allergy/intolerance), severity `<select>` (mild/moderate/severe/life-threatening/— for null), reaction (text), onsetDate (date input), notes (textarea)
   - **Conditional RxNorm fields**: `{formState.category === "drug" && (<>substanceCode input + substanceSystem input</>)}` — shown only for drug category
   - On submit: if `initial === null` → `useClinicalData.addAllergy({ patientId, ...formState })` (caller receives callback via prop); if `initial !== null` → `updateAllergy(initial.id, { patientId, ...formState })`
   - Inline `submitError` rendered above submit button; `submitting` spinner on button
   - Delete button (shown only when `initial !== null` and role is Provider/SystemAdmin): `window.confirm("Delete this allergy? This cannot be undone.")` → if true, call `deleteAllergy(initial.id, patientId)` → `onSuccess()`
   - Pre-populate from `extractAllergyDisplay(initial.resource)` when in edit mode

2. **`ProblemFormModal.tsx`**:
   - Props: `patientId: string; initial: ProblemRecord | null; onSuccess: () => void; onClose: () => void`
   - Fields: icd10Code (text, required), display (text, required), clinicalStatus `<select>` (active/inactive/resolved), onsetDate (date), abatementDate (date — show only when status is resolved/inactive), notes (textarea)
   - No physical delete — only status transitions. When `initial !== null`, show "Resolve" quick-action button that sets clinicalStatus to "resolved" and submits immediately.
   - Pre-populate from `extractProblemDisplay(initial.resource)` when in edit mode

3. **`MedicationFormModal.tsx`**:
   - Props: `patientId: string; initial: MedicationRecord | null; onSuccess: () => void; onClose: () => void`
   - Fields: display (text, required), rxnormCode (text, optional), status `<select>` with all 8 valid values (`active|completed|entered-in-error|intended|stopped|on-hold|unknown|not-taken`) defaulting to "active", dosage (text), effectiveStart (date), effectiveEnd (date), prescriberId (text), reason (text), notes (textarea)
   - No physical delete — "Stop Medication" quick-action button sets status to "stopped" and submits.
   - Pre-populate from `extractMedicationDisplay(initial.resource)` when in edit mode

4. **`ImmunizationFormModal.tsx`**:
   - Props: `patientId: string; onSuccess: () => void; onClose: () => void` (no `initial` — immunizations are append-only; no edit/delete)
   - Fields: cvxCode (text, required), display (text, required), occurrenceDate (date, required), lotNumber, expirationDate, site, route, doseNumber (number input), status `<select>` (completed/entered-in-error/not-done; default "completed"), notes
   - Only an "Add" form; no edit mode.

5. **Wire modals into `ClinicalSidebar.tsx`**:
   - Add per-tab state: `addAllergyOpen: boolean`, `editAllergy: AllergyRecord | null`, `addProblemOpen: boolean`, `editProblem: ProblemRecord | null`, `addMedOpen: boolean`, `editMed: MedicationRecord | null`, `addImmunOpen: boolean`
   - In each tab panel header, add "Add [X]" button gated by `role === "Provider" || role === "NurseMa" || role === "SystemAdmin"`, styled `bg-indigo-600 text-white` small button
   - On each list row, add "Edit" button gated by the same roles (NurseMa excluded from delete inside modals via `role !== "NurseMa"` flag passed to AllergyFormModal)
   - Replace the `{/* TODO T03 */}` placeholders from T02 with these buttons
   - Import and render all four modals conditionally: `{addAllergyOpen && <AllergyFormModal .../>}` etc. On `onSuccess`: call `reload()` then close modal state.
   - Pass `role` prop into `AllergyFormModal` so it can conditionally show the Delete button

6. **Final verification**:
   - `npx tsc --noEmit` exits 0
   - In Tauri dev app: Add Problem → form submits → list refreshes → new row visible
   - Add drug allergy (category=drug, substance="Penicillin") for patient with active Penicillin medication → alert banner appears
   - Stop a medication → status badge changes to "stopped"
   - Delete an allergy (Provider role) → row removed; alert banner refreshes
   - `cargo test --lib` passes (baseline, no Rust changes)

## Must-Haves

- [ ] All four modal files created; each follows `PatientFormModal` overlay+panel pattern exactly
- [ ] Allergy form: RxNorm fields only visible when `category === "drug"`
- [ ] Allergy delete: only visible for Provider/SystemAdmin; requires `window.confirm` before proceeding; passes both `allergyId` AND `patientId` to `deleteAllergy`
- [ ] Medication status `<select>` has all 8 valid values; defaults to "active"
- [ ] All modals call `reload()` (from `useClinicalData`) on success so alert banner refreshes
- [ ] Immunization modal is add-only (no edit/delete)
- [ ] `tsc --noEmit` exits 0; no `any`; all optional fields `T | null`
- [ ] Full add round-trip verified in running Tauri app for at least one domain

## Verification

- `npx tsc --noEmit` exits 0
- `cargo test --lib` passes with ≥265 tests (no Rust regressions)
- Provider role: Add Problem form submits → list refreshes with new row
- Drug-allergy alert banner: add drug allergy matching active medication → banner appears; delete allergy → banner clears
- BillingStaff role: no add/edit buttons visible; no Clinical Data section

## Observability Impact

- Signals added/changed: each modal renders inline `submitError` above submit button — visible without DevTools for any form submission failure; mutation errors logged to console with `[ClinicalSidebar]` prefix
- How a future agent inspects this: React DevTools → `ClinicalSidebar` → `addAllergyOpen`, `editAllergy`, `addProblemOpen`, etc. all visible as component state; `DrugAllergyAlertBanner` count observable from `alerts.length`
- Failure state exposed: `submitError` shown inline in modal; modal stays open on error so user can retry; underlying error message from Tauri backend surfaced directly

## Inputs

- `src/components/patient/PatientFormModal.tsx` — modal overlay+panel pattern to replicate exactly
- `src/pages/EncounterWorkspace.tsx` — `INPUT_CLS`/`LABEL_CLS` constants (copy verbatim)
- `src/hooks/useClinicalData.ts` — from T01; `addAllergy`, `updateAllergy`, `deleteAllergy`, `addProblem`, `updateProblem`, `addMedication`, `updateMedication`, `addImmunization`, `reload`
- `src/lib/fhirExtract.ts` — from T01; `extractAllergyDisplay`, `extractProblemDisplay`, `extractMedicationDisplay` for modal pre-population
- `src/types/patient.ts` — `AllergyRecord`, `AllergyInput`, `ProblemRecord`, `ProblemInput`, `MedicationRecord`, `MedicationInput`, `ImmunizationRecord`, `ImmunizationInput`
- `src/components/clinical/ClinicalSidebar.tsx` — from T02; the file that receives all four modals

## Expected Output

- `src/components/clinical/AllergyFormModal.tsx` — new; add/edit/delete for allergy records
- `src/components/clinical/ProblemFormModal.tsx` — new; add/status-change for problem records
- `src/components/clinical/MedicationFormModal.tsx` — new; add/status-change for medication records
- `src/components/clinical/ImmunizationFormModal.tsx` — new; add-only for immunization records
- `src/components/clinical/ClinicalSidebar.tsx` — modified; TODO placeholders replaced with modal wiring; add/edit buttons functional
