# S04: Clinical Data Sidebar — Research

**Date:** 2026-03-12

## Summary

S04 builds the `ClinicalSidebar` — a tabbed panel (Problems | Medications | Allergies | Immunizations) mounted within `PatientDetailPage` — plus a `DrugAllergyAlertBanner` surfacing passive CDS alerts. The backend is fully wired: all 10 Tauri commands (add/list/update/delete allergy, add/list/update problem, add/list medication, add/list immunization, check_drug_allergy_alerts) exist in `clinical.rs`, have complete TypeScript wrappers in `commands`, and are typed in `src/types/patient.ts` and `src/types/documentation.ts`. Zero Rust changes are needed.

The primary work is building the React component layer: 4 tab panels, a `useClinicalData(patientId)` hook, modal forms for add/edit flows, status-change actions, and the alert banner. The pattern playbook is already established by `usePatient`, `useEncounter`, `PatientDetailPage`, and `EncounterWorkspace` — S04 follows identical conventions and can be executed with near-zero architectural ambiguity.

The highest-complexity surface is the allergy form (category selector, severity, reaction, optional RxNorm code for drug allergies) and the medication form (status lifecycle with 8 valid states, optional prescriber linkage, effective period). The drug-allergy alert banner is simple: one `commands.checkDrugAllergyAlerts(patientId)` call, a null-safe render loop. The key risk is fitting the sidebar into the existing `PatientDetailPage` layout without breaking the existing demographics/insurance/care-team sections.

## Recommendation

Mount `ClinicalSidebar` as a new `<SectionCard>` block near the top of `PatientDetailPage` (above Demographics, below Encounters), RBAC-gated to Provider / NurseMa / SystemAdmin. This avoids restructuring the page layout, matches the established SectionCard pattern, and keeps the existing scroll-based layout intact. Do not add a fixed sidebar column or split layout — S06 does not depend on a specific layout shape; only on the data surface being present.

Build `useClinicalData(patientId)` following the `usePatient` / `useEncounter` mounted-guard + refreshCounter + reload pattern. The hook loads all four lists in `Promise.all` and exposes typed add/update/delete callbacks that call the relevant command then `reload()`.

For forms, reuse the `PatientFormModal` pattern: a `fixed inset-0` overlay with a centered modal panel, controlled form state, and inline error display.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Mounting guard for async fetch | `mounted` boolean + `refreshCounter` pattern from `usePatient.ts` / `useEncounter.ts` | Prevents stale state after unmount; already proven |
| Modal overlay | `PatientFormModal.tsx` — `fixed inset-0 bg-black/40` + centered panel | Established pattern; consistent UX |
| FHIR field extraction | `fhirExtract.ts` — add helpers like `extractAllergyDisplay()` | FHIR JSON path knowledge isolated from components |
| Section layout in PatientDetailPage | `SectionCard` + `InfoRow` sub-components (already in file) | Consistent card styling without new CSS |
| Tailwind input/label classes | `INPUT_CLS` / `LABEL_CLS` constants from `EncounterWorkspace.tsx` | Re-use the exact same string constants |
| Tab chrome | Pattern from `EncounterWorkspace.tsx` — button row with `active` highlight style | Re-use tab rendering; no new UI primitives needed |

## Existing Code and Patterns

- `src/hooks/usePatient.ts` — `mounted` boolean guard, `refreshCounter`, `reload` callback, `Promise.all` parallel fetch. **Copy this pattern verbatim for `useClinicalData`.**
- `src/hooks/useEncounter.ts` — extends the hook return type with additional callbacks (`saveSoap`, `saveVitals`, `saveRos`). `useClinicalData` will expose `addAllergy`, `updateAllergy`, `deleteAllergy`, `addProblem`, `updateProblem`, `addMedication`, `updateMedication`, `addImmunization`.
- `src/pages/PatientDetailPage.tsx` — chart shell; S04 inserts `ClinicalSidebar` here. `SectionCard`, `InfoRow` components are already defined in-file. The `isBillingStaff` flag controls section visibility — use it to gate the clinical sidebar too.
- `src/components/patient/PatientFormModal.tsx` — the modal pattern to copy for add/edit forms.
- `src/pages/EncounterWorkspace.tsx` — tab chrome (active/inactive button classes), `INPUT_CLS`/`LABEL_CLS` constants, inline spinner + error banner pattern (`savingX` / `xError` state).
- `src/lib/tauri.ts` → `commands.addAllergy`, `commands.listAllergies`, `commands.updateAllergy`, `commands.deleteAllergy`, `commands.addProblem`, `commands.listProblems`, `commands.updateProblem`, `commands.addMedication`, `commands.listMedications`, `commands.updateMedication`, `commands.addImmunization`, `commands.listImmunizations`, `commands.checkDrugAllergyAlerts` — **all already wired**. No new wrappers needed.
- `src/types/patient.ts` — `AllergyInput`, `AllergyRecord`, `ProblemInput`, `ProblemRecord`, `MedicationInput`, `MedicationRecord`, `ImmunizationInput`, `ImmunizationRecord` — **all typed**. No new types needed.
- `src/types/documentation.ts` — `DrugAllergyAlert` — typed, imported from `commands`.
- `src/lib/fhirExtract.ts` — add `extractAllergyDisplay()`, `extractProblemDisplay()`, `extractMedicationDisplay()`, `extractImmunizationDisplay()` helpers here. Follow `extractPatientDisplay()` structure: take `Record<string, unknown>`, return a typed flat struct.

## Constraints

- **No Rust changes** — all backend commands exist. Zero new Tauri commands or DB schema changes.
- **`tsc --noEmit` must exit 0** — TypeScript strict mode, no `any`. All optional fields are `T | null` (never `T | undefined`).
- **No CSS modules** — Tailwind only.
- **`listProblems` and `listMedications` accept an optional `statusFilter`** — pass `null` to get all statuses on initial load. Active-only filtered views are a nice-to-have, not required for S04.
- **`deleteAllergy` is the only physical delete** — medications and problems use status transitions only (`update_medication` to "stopped", `update_problem` to "resolved"). No delete commands for medications/problems exist on the backend.
- **`listImmunizations` has no status filter** — immunizations are immutable records. No `update_immunization` command exists. The UI shows all immunizations; the only correction pattern is adding a new record with `status: "entered-in-error"`.
- **RBAC: BillingStaff and FrontDesk get Read only** — the `isBillingStaff` gate in `PatientDetailPage` already gates sections. Extend this: FrontDesk has no access to clinical data at all (they only see Schedule in the nav). For Provider/NurseMa/SystemAdmin: show full sidebar with add/edit actions. For BillingStaff: either hide completely or show read-only (no add/edit buttons). The S04 boundary map says "Providers can view and manage" — implement as Provider/NurseMa/SystemAdmin get full CRUD; BillingStaff see read-only.
- **`checkDrugAllergyAlerts(patientId)` param is `patient_id`** — wired correctly in `tauri.ts` already.
- **Alert banner fires only for drug/biologic category allergies** — the Rust backend already filters; the UI just renders what comes back from `checkDrugAllergyAlerts`.
- **`useClinicalData` should not be called inside `PatientDetailPage`'s top-level hook area** — it may cause the page to re-render when adding items. Pattern: call it in `ClinicalSidebar` directly (component-local hook). `ClinicalSidebar` receives only `patientId` and `role` as props.

## Common Pitfalls

- **`T | undefined` instead of `T | null`** — all Rust `Option<T>` fields deserialize as `null` in JSON. Using `undefined` causes TypeScript errors when assigning backend responses. Match the pattern in `src/types/patient.ts` exactly.
- **Forgetting `patient_id` in allergy/problem/medication/immunization inputs** — all four input types require `patientId` as a top-level field. The commands will return a validation error if it's empty.
- **Putting `useClinicalData` in `PatientDetailPage` before knowing the patientId is valid** — call it inside the `ClinicalSidebar` component, which only renders after patient is loaded. This avoids fetching clinical data before the patient record is confirmed.
- **RxNorm code field in allergy form** — only meaningful for `category === "drug"`. Show/hide the substance code + system fields based on selected category. The backend accepts them as null for non-drug allergies.
- **`deleteAllergy` requires both `allergyId` AND `patientId`** — wired in `tauri.ts` as `invoke("delete_allergy", { allergy_id: allergyId, patient_id: patientId })`. Both params required; missing patient_id causes a silent Tauri deserialization failure.
- **Medication status has 8 valid values** — `"active" | "completed" | "entered-in-error" | "intended" | "stopped" | "on-hold" | "unknown" | "not-taken"`. Use a `<select>` with these exact strings, defaulting to `"active"`.
- **`checkDrugAllergyAlerts` does two-pass matching** — RxNorm exact + name fuzzy. If a medication has no `rxnormCode`, it still matches by name. This is correct backend behavior; the UI doesn't need to filter.
- **Tab state resets on parent re-render** — `ClinicalSidebar` manages its own `activeTab` state. A parent `reload()` from PatientDetailPage will unmount/remount `ClinicalSidebar` if it's gated by loading state — ensure it's rendered persistently (not inside the `if (loading)` block in PatientDetailPage).
- **Alert banner should refresh when medications or allergies change** — the `checkDrugAllergyAlerts` call in `useClinicalData` should be re-triggered after any `addMedication`, `updateMedication`, `addAllergy`, `updateAllergy`, or `deleteAllergy` completes (i.e., on `reload()`).

## Open Risks

- **PatientDetailPage layout width** — the existing page is a single-column stack of SectionCards. Adding a large ClinicalSidebar section could make the page very long. Consider a 2-column grid for the clinical section (Problems | Medications on left; Allergies | Immunizations on right) to reduce vertical scroll.
- **FHIR resource display extraction** — the `AllergyRecord.resource`, `ProblemRecord.resource`, etc. are raw FHIR JSON blobs. Adding `extractAllergyDisplay()` etc. to `fhirExtract.ts` is the right approach, but these functions must be written carefully: the FHIR paths for AllergyIntolerance, Condition, MedicationStatement, and Immunization differ significantly from Patient. Reading `clinical.rs` `build_*_fhir()` functions directly is the authoritative source of truth for FHIR field paths.
- **Alert banner positioning** — placing the `DrugAllergyAlertBanner` above the clinical tabs gives it high visibility. Placing it inline within the Allergies or Medications tab reduces visibility. The M002-ROADMAP says "passive drug-allergy alerts surfaced when medications and allergies overlap" — above the tabs (or at page top near the header) is the safest choice.
- **`useClinicalData` load time** — 4 parallel requests on first mount is fine. If any one of them fails, the hook should degrade gracefully (show error for that tab only, not crash all tabs).

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React 18 / TypeScript | (no specialized skill needed — standard Tailwind + hooks) | none found |
| Tauri 2 invoke | (established pattern in codebase) | none found |

## Sources

- `src-tauri/src/commands/clinical.rs` — authoritative source for all FHIR field paths (`build_allergy_fhir`, `build_problem_fhir`, `build_medication_fhir`, `build_immunization_fhir`); command signatures and required params
- `src/lib/tauri.ts` — all clinical command wrappers confirmed present and correctly parameterized
- `src/types/patient.ts` — all clinical input/record types confirmed present; field names match Rust `#[serde(rename_all = "camelCase")]`
- `src/pages/PatientDetailPage.tsx` — integration point; `SectionCard`, `InfoRow`, `isBillingStaff` gate, `usePatient` hook ownership
- `src/hooks/usePatient.ts` — hook pattern to replicate for `useClinicalData`
- `src/pages/EncounterWorkspace.tsx` — tab chrome pattern, INPUT_CLS/LABEL_CLS constants, form state conventions
- `src/lib/fhirExtract.ts` — where FHIR extraction helpers live; `extractAllergyDisplay` etc. go here
- `.gsd/DECISIONS.md` — confirmed: `T | null` for all optional fields (never `T | undefined`); flat `commands` object; `ClinicalData` RBAC resource; NurseMa gets CRU not Delete on clinical data
