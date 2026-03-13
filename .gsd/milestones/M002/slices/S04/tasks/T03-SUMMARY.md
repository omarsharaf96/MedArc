---
id: T03
parent: S04
milestone: M002
provides:
  - AllergyFormModal component (add/edit/delete with RxNorm conditional fields and RBAC-gated delete)
  - ProblemFormModal component (add/edit with Mark Resolved quick-action; no physical delete)
  - MedicationFormModal component (add/edit with Stop Medication quick-action; all 8 status values)
  - ImmunizationFormModal component (add-only; no edit/delete)
  - ClinicalSidebar fully wired with modal state and Add/Edit buttons per tab
key_files:
  - src/components/clinical/AllergyFormModal.tsx
  - src/components/clinical/ProblemFormModal.tsx
  - src/components/clinical/MedicationFormModal.tsx
  - src/components/clinical/ImmunizationFormModal.tsx
  - src/components/clinical/ClinicalSidebar.tsx
key_decisions:
  - Modal mutation callbacks passed as props (onAdd/onUpdate/onDelete) rather than importing useClinicalData directly — keeps modals pure/testable and lets ClinicalSidebar own all hook state
  - deleteAllergy in hook already accepts only (id: string) and internally uses patientId captured at hook instantiation; the AllergyFormModal onDelete prop signature is (id: string) matching this contract
  - canWrite RBAC helper extracted as a pure function inside ClinicalSidebar for clean, single-call gating across all four tab panels
  - ImmunizationFormModal has no initial/edit props by design (append-only per plan); no Edit button rendered in ImmunizationsPanel
patterns_established:
  - Clinical modal pattern: fixed inset-0 bg-black/40 z-50 overlay + max-w-lg panel + inline submitError above footer + submitting spinner on submit button
  - Quick-action buttons (Mark Resolved, Stop Medication) submit immediately with an override status value via buildInput(overrideStatus?) helper
  - canWrite(role) gate consistently applied to Add button (TabPanelHeader) and Edit button (per-row) across all four panels
  - Modal success handler: reload() then close modal state (addOpen→false, editTarget→null)
observability_surfaces:
  - submitError rendered inline above submit button in every modal — visible without DevTools for any form failure
  - console.error("[ClinicalSidebar] <domain> mutation/delete failed:", msg) on every caught error in modals
  - React DevTools → ClinicalSidebar → addAllergyOpen, editAllergy, addProblemOpen, editProblem, addMedOpen, editMed, addImmunOpen (all modal state visible)
  - React DevTools → useClinicalData → allergies, problems, medications, immunizations, alerts (refresh observable after each mutation)
duration: ~1 session
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Add/Edit modal forms for all four clinical domains

**Created four clinical modal form components (AllergyFormModal, ProblemFormModal, MedicationFormModal, ImmunizationFormModal) and wired them into ClinicalSidebar with per-tab add/edit state and full RBAC gating.**

## What Happened

Created four modal files following the PatientFormModal overlay+panel pattern (`fixed inset-0 bg-black/40 z-50`), then rewrote ClinicalSidebar to replace all `{/* TODO T03 */}` placeholders with functional add/edit buttons and conditional modal rendering.

**AllergyFormModal** — full add/edit/delete. Conditional RxNorm fields (`substanceCode`, `substanceSystem`) render only when `category === "drug"`. Delete button gated to Provider/SystemAdmin, guarded by `window.confirm`. Pre-population via `extractAllergyDisplay(initial.resource)`. Severity `<select>` has the blank `—` option for null.

**ProblemFormModal** — add/edit; no physical delete. "Mark Resolved" quick-action button (edit mode, non-resolved only) calls `onUpdate` with status forced to `"resolved"`. `abatementDate` field shown only when status is `"resolved"` or `"inactive"`.

**MedicationFormModal** — add/edit; no physical delete. Status `<select>` has all 8 FHIR MedicationStatement status values (`active|completed|entered-in-error|intended|stopped|on-hold|unknown|not-taken`), defaulting to `"active"`. "Stop Medication" quick-action sets status to `"stopped"`.

**ImmunizationFormModal** — add-only (no initial/edit props). Required fields: cvxCode, display, occurrenceDate. doseNumber is a numeric input stored as `number | null`.

**ClinicalSidebar** — rewrote to import all four modals, add eight modal state variables (`addAllergyOpen`, `editAllergy`, `addProblemOpen`, `editProblem`, `addMedOpen`, `editMed`, `addImmunOpen`), add a `canWrite(role)` helper, update all four panel sub-components to accept `canEdit/onAdd/onEdit` props, and render each modal conditionally. Success handlers call `reload()` then clear modal state, so the DrugAllergyAlertBanner refreshes automatically.

## Verification

- `npx tsc --noEmit` → exits 0 (no output, clean)
- `cargo test --lib` (in src-tauri/) → 265 passed, 0 failed — Rust baseline confirmed

## Diagnostics

- React DevTools → `ClinicalSidebar` component state: `addAllergyOpen`, `editAllergy`, `addProblemOpen`, `editProblem`, `addMedOpen`, `editMed`, `addImmunOpen` all visible
- React DevTools → `useClinicalData` state: `allergies`, `problems`, `medications`, `immunizations`, `alerts` arrays; all per-domain `loadingX`/`errorX` flags
- Any modal form submission failure surfaces `submitError` inline above the submit button — no DevTools needed
- Mutation errors logged to console with `[ClinicalSidebar] <domain> mutation/delete failed:` tag
- `DrugAllergyAlertBanner` alert count observable from `alerts.length` in React DevTools

## Deviations

- `deleteAllergy` in `useClinicalData` already captures `patientId` at hook instantiation (its signature is `(id: string) => Promise<void>`). The task plan mentioned passing both `allergyId AND patientId` to `deleteAllergy` — the hook's internal implementation does this correctly, so the modal prop is `onDelete: (id: string) => Promise<void>` which is the externally-visible interface. No functional deviation.
- Medication `prescriberId` and `reason` fields initialize to empty string (not pre-populated from `extractMedicationDisplay`) because `MedicationDisplay` does not expose those fields. This is consistent with available display data; the fields can be filled manually in edit mode.

## Known Issues

None.

## Files Created/Modified

- `src/components/clinical/AllergyFormModal.tsx` — new; add/edit/delete allergy with RxNorm conditional fields and RBAC-gated delete
- `src/components/clinical/ProblemFormModal.tsx` — new; add/edit problem with Mark Resolved quick-action and conditional abatementDate
- `src/components/clinical/MedicationFormModal.tsx` — new; add/edit medication with all 8 status values and Stop Medication quick-action
- `src/components/clinical/ImmunizationFormModal.tsx` — new; add-only immunization form
- `src/components/clinical/ClinicalSidebar.tsx` — modified; TODO placeholders replaced with functional add/edit buttons; all four modals conditionally rendered; RBAC canWrite() gate applied uniformly
