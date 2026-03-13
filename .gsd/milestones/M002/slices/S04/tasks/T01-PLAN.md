---
estimated_steps: 5
estimated_files: 2
---

# T01: FHIR extraction helpers + `useClinicalData` hook

**Slice:** S04 — Clinical Data Sidebar
**Milestone:** M002

## Description

Establishes the data layer for the entire slice. Before any UI can be built, the FHIR extraction helpers and the `useClinicalData` hook must exist with fully-typed return values. All three UI tasks depend on the hook's return type and on the `extract*Display` helpers to render list rows.

This task adds four pure FHIR extraction helpers to `src/lib/fhirExtract.ts` and writes the `src/hooks/useClinicalData.ts` hook. No UI changes. No Rust changes. The only deliverable is a green `tsc --noEmit`.

## Steps

1. **Read FHIR field paths from `clinical.rs`** — Confirm the exact JSON paths produced by `build_allergy_fhir`, `build_problem_fhir`, `build_medication_fhir`, `build_immunization_fhir`. These are the authoritative source; never guess FHIR paths.
   - AllergyIntolerance: `code.text` (substance name), `category[0]` (category), `clinicalStatus.coding[0].code`, `type` (allergyType), `reaction[0].severity`, `reaction[0].manifestation[0].text`, `onsetDateTime`, `code.coding[0].code` (substanceCode), `code.coding[0].system` (substanceSystem)
   - Condition: `code.coding[0].code` (icd10Code), `code.text` (display), `clinicalStatus.coding[0].code`, `onsetDateTime`, `abatementDateTime`
   - MedicationStatement: `medication.concept.text` (display), `status`, `medication.concept.coding[0].code` (rxnormCode), `dosage[0].text`, `effectivePeriod.start`, `effectivePeriod.end`
   - Immunization: `vaccineCode.coding[0].code` (cvxCode), `vaccineCode.text` (display), `occurrenceDateTime`, `lotNumber`, `status`

2. **Add typed display structs and four extract functions to `fhirExtract.ts`**:
   - `AllergyDisplay`: `substance: string | null; category: string | null; clinicalStatus: string | null; allergyType: string | null; severity: string | null; reaction: string | null; onsetDate: string | null; substanceCode: string | null; substanceSystem: string | null`
   - `ProblemDisplay`: `icd10Code: string | null; display: string | null; clinicalStatus: string | null; onsetDate: string | null; abatementDate: string | null`
   - `MedicationDisplay`: `drugName: string | null; status: string | null; rxnormCode: string | null; dosage: string | null; effectiveStart: string | null; effectiveEnd: string | null`
   - `ImmunizationDisplay`: `vaccineName: string | null; cvxCode: string | null; occurrenceDate: string | null; lotNumber: string | null; status: string | null`
   - Each `extract*Display(resource: Record<string, unknown> | null | undefined): *Display` function follows `extractPatientDisplay` pattern: null guard → return all-null empty struct; use optional chaining; no `as any`; never throw.

3. **Write `src/hooks/useClinicalData.ts`** following `usePatient.ts` verbatim:
   - `mounted` boolean guard in `useEffect`
   - `refreshCounter` state incremented by `reload` callback
   - Load with per-item try/catch inside the Promise.all — load all 5 in parallel: `listAllergies`, `listProblems`, `listMedications`, `listImmunizations`, `checkDrugAllergyAlerts`. If one fails, set that tab's `error` state but do not block the others. Implement as five separate try/catches inside `fetchAll`.
   - State: `allergies: AllergyRecord[]`, `problems: ProblemRecord[]`, `medications: MedicationRecord[]`, `immunizations: ImmunizationRecord[]`, `alerts: DrugAllergyAlert[]`, plus five `loadingX: boolean` and five `errorX: string | null` vars (one per domain).
   - Use a single top-level `setLoading(true)` and `finally { setLoading(false) }` for the overall skeleton spinner, plus per-domain `error` state.
   - Expose mutation callbacks: `addAllergy(input: AllergyInput)`, `updateAllergy(id: string, input: AllergyInput)`, `deleteAllergy(id: string)`, `addProblem(input: ProblemInput)`, `updateProblem(id: string, input: ProblemInput)`, `addMedication(input: MedicationInput)`, `updateMedication(id: string, input: MedicationInput)`, `addImmunization(input: ImmunizationInput)`. Each is `async`, calls the relevant command, then calls `reload()`. Errors propagate (callers catch and show in modal).
   - Export `UseClinicalDataReturn` interface.

4. **Import types at top of `useClinicalData.ts`**: `AllergyRecord`, `AllergyInput`, `ProblemRecord`, `ProblemInput`, `MedicationRecord`, `MedicationInput`, `ImmunizationRecord`, `ImmunizationInput` from `../types/patient`; `DrugAllergyAlert` from `../types/documentation`; `commands` from `../lib/tauri`.

5. **Run `tsc --noEmit`** and fix any TypeScript errors before marking done.

## Must-Haves

- [ ] `extractAllergyDisplay`, `extractProblemDisplay`, `extractMedicationDisplay`, `extractImmunizationDisplay` added to `fhirExtract.ts` with correct FHIR paths matching `clinical.rs`
- [ ] All four display structs exported with only `string | null` fields (no `undefined`)
- [ ] `useClinicalData(patientId: string): UseClinicalDataReturn` exported from `src/hooks/useClinicalData.ts`
- [ ] Per-domain error isolation: one failing list call does not set all tabs to error
- [ ] All mutation callbacks are async and call `reload()` on success
- [ ] `tsc --noEmit` exits 0

## Verification

- `cd /Users/omarsharaf96/Documents/GitHub/MedArc && npx tsc --noEmit` exits 0
- No `any` in the two new files — confirmed by inspection

## Observability Impact

- Signals added/changed: `console.error("[useClinicalData] listAllergies failed:", msg)` etc. per domain — structured tag format consistent with `usePatient`
- How a future agent inspects this: React DevTools → `ClinicalSidebar` → `useClinicalData` state shows `allergies[]`, `problems[]`, `medications[]`, `immunizations[]`, `alerts[]`, and all `error*` fields
- Failure state exposed: per-tab `errorAllergies | errorProblems | errorMedications | errorImmunizations | errorAlerts` each independently surfaced

## Inputs

- `src/hooks/usePatient.ts` — exact hook pattern to replicate
- `src-tauri/src/commands/clinical.rs` — authoritative FHIR field paths in `build_*_fhir` functions
- `src/lib/fhirExtract.ts` — existing file to extend; follow `extractPatientDisplay` structure exactly
- `src/types/patient.ts` — `AllergyRecord`, `ProblemRecord`, `MedicationRecord`, `ImmunizationRecord` and their `*Input` counterparts
- `src/types/documentation.ts` — `DrugAllergyAlert` type
- `src/lib/tauri.ts` — confirms `listAllergies`, `listProblems`, `listMedications`, `listImmunizations`, `checkDrugAllergyAlerts` are all wired

## Expected Output

- `src/lib/fhirExtract.ts` — four new display structs + four extract functions appended
- `src/hooks/useClinicalData.ts` — new file; the data backbone for S04
