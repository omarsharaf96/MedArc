---
id: T01
parent: S04
milestone: M002
provides:
  - extractAllergyDisplay, extractProblemDisplay, extractMedicationDisplay, extractImmunizationDisplay helpers in fhirExtract.ts
  - AllergyDisplay, ProblemDisplay, MedicationDisplay, ImmunizationDisplay exported interfaces
  - useClinicalData(patientId) hook with per-domain error isolation and all mutation callbacks
  - UseClinicalDataReturn exported interface
key_files:
  - src/lib/fhirExtract.ts
  - src/hooks/useClinicalData.ts
key_decisions:
  - Four FHIR extract functions added to existing fhirExtract.ts (not split into a new file) to stay consistent with the established module pattern.
  - Per-domain loading flags (loadingAllergies, loadingProblems, etc.) added alongside the top-level loading flag so the skeleton spinner and per-tab spinners can work independently.
  - deleteAllergy callback closes over patientId from hook params (not the record) since commands.deleteAllergy requires both allergyId and patientId.
patterns_established:
  - Clinical FHIR extract functions follow exact same null-guard → optional-chaining → typed-struct pattern as extractPatientDisplay.
  - Per-domain error isolation via five independent async IIFEs inside Promise.all — one failure sets only that domain's error state.
  - Mutation callbacks are useCallback-memoized, call the command, then call reload() on success; errors propagate to callers.
observability_surfaces:
  - console.error("[useClinicalData] listAllergies failed for <id>:", msg)
  - console.error("[useClinicalData] listProblems failed for <id>:", msg)
  - console.error("[useClinicalData] listMedications failed for <id>:", msg)
  - console.error("[useClinicalData] listImmunizations failed for <id>:", msg)
  - console.error("[useClinicalData] checkDrugAllergyAlerts failed for <id>:", msg)
  - React DevTools — useClinicalData state shows allergies[], problems[], medications[], immunizations[], alerts[], errorAllergies|errorProblems|errorMedications|errorImmunizations|errorAlerts
duration: ~20 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T01: FHIR extraction helpers + `useClinicalData` hook

**Added four FHIR extract functions + four display structs to `fhirExtract.ts` and wrote the `useClinicalData` hook with per-domain error isolation and all eight mutation callbacks.**

## What Happened

Read `clinical.rs` `build_*_fhir` functions to confirm exact FHIR JSON paths, then:

1. Appended `AllergyDisplay`, `ProblemDisplay`, `MedicationDisplay`, `ImmunizationDisplay` interfaces and their four `extract*Display` functions to `src/lib/fhirExtract.ts`. All paths verified against the Rust builders. All functions follow the null-guard → optional-chaining → no-throw pattern of `extractPatientDisplay`.

2. Created `src/hooks/useClinicalData.ts` following `usePatient.ts` verbatim for the skeleton: `mounted` boolean, `refreshCounter`, `useCallback`-stable `reload`. The five domains (allergies, problems, medications, immunizations, alerts) are fetched in parallel via `Promise.all` with five independent async IIFEs — each has its own try/catch, its own `setError*`, and its own `setLoading*`. A single top-level `setLoading(true/false)` drives the skeleton spinner. Eight mutation callbacks (`addAllergy`, `updateAllergy`, `deleteAllergy`, `addProblem`, `updateProblem`, `addMedication`, `updateMedication`, `addImmunization`) are `useCallback`-memoized, async, call the relevant `commands.*`, then call `reload()` on success; errors propagate to callers for modal display.

## Verification

- `npx tsc --noEmit` exited 0 with no output.
- `grep ' as any\|: any\b' src/lib/fhirExtract.ts src/hooks/useClinicalData.ts` returned nothing — confirmed no `any`.

## Diagnostics

- React DevTools → `useClinicalData` state: `allergies`, `problems`, `medications`, `immunizations`, `alerts` arrays; `loading`, `loadingAllergies`, `loadingProblems`, `loadingMedications`, `loadingImmunizations`, `loadingAlerts` booleans; `errorAllergies`, `errorProblems`, `errorMedications`, `errorImmunizations`, `errorAlerts` strings.
- Per-tab error state is independently surfaced — one tab can fail without crashing the rest.
- Console errors follow `[useClinicalData] <command> failed for <patientId>:` tag format consistent with `usePatient`.

## Deviations

None. Implemented exactly as specified.

## Known Issues

None.

## Files Created/Modified

- `src/lib/fhirExtract.ts` — appended AllergyDisplay, ProblemDisplay, MedicationDisplay, ImmunizationDisplay interfaces and extractAllergyDisplay, extractProblemDisplay, extractMedicationDisplay, extractImmunizationDisplay functions
- `src/hooks/useClinicalData.ts` — new file; the data backbone for S04
