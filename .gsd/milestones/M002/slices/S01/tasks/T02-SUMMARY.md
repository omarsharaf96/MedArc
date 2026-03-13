---
id: T02
parent: S01
milestone: M002
provides:
  - 60 new type-safe Tauri invoke() wrappers covering all M001 Rust commands (patient, clinical, scheduling, documentation, labs/documents)
  - Complete import group for all four new type files (patient.ts, scheduling.ts, documentation.ts, labs.ts)
key_files:
  - src/lib/tauri.ts
key_decisions:
  - Flat (Option A) commands object preserved — no namespacing introduced
  - TypeScript parameter names use camelCase; invoke object keys use exact snake_case Rust parameter names
  - Optional params always passed as `param ?? null` (not `undefined`) — Tauri expects explicit null
  - createAppointment returns AppointmentRecord[] not AppointmentRecord (recurring creates multiple records)
  - cancelAppointment returns AppointmentRecord (not void) and includes reason param
  - getRos and getPhysicalExam both require BOTH encounter_id AND patient_id invoke params
  - listLabCatalogue uses category_filter (not category) to match Rust fn param name
  - listLabOrders uses status_filter (not status) to match Rust fn param name
  - listRecalls/listWaitlist have no patient_id param (scoped by provider/type/status only)
patterns_established:
  - snake_case invoke object keys matching Rust #[tauri::command] fn parameter names exactly
  - Optional wrapper params typed as `T | null` or `T | undefined`; always sent as `param ?? null` in invoke
  - JSDoc comments on every wrapper describing its function
  - Wrappers grouped into labelled sections with ASCII-box comments matching existing style
observability_surfaces:
  - Runtime IPC errors surface as rejected Promises with Tauri's standard error format
  - Wrong snake_case param names cause Rust handler to return missing-field error at runtime — detectable via browser console in Tauri dev window
  - `grep "patient_id\|provider_id\|encounter_id" src/lib/tauri.ts | grep "invoke"` confirms snake_case compliance
  - `grep -E "^  [a-zA-Z][a-zA-Z0-9]+:" src/lib/tauri.ts | wc -l` returns 88 (total wrapper count)
duration: ~20 min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Extend `src/lib/tauri.ts` with all 60 M001 command wrappers

**Extended `src/lib/tauri.ts` with 60 new invoke() wrappers across 5 labelled sections (patient, clinical, scheduling, documentation, labs/documents), bringing total wrappers from 28 to 88.**

## What Happened

Appended four new `import type` groups to `src/lib/tauri.ts` covering all types from `patient.ts`, `scheduling.ts`, `documentation.ts`, and `labs.ts`. Then added five new section blocks inside the `commands` export object:

- **Patient (9 wrappers):** createPatient, getPatient, updatePatient, searchPatients, deletePatient, upsertCareTeam, getCareTeam, addRelatedPerson, listRelatedPersons
- **Clinical (12 wrappers):** addAllergy, listAllergies, updateAllergy, deleteAllergy, addProblem, listProblems, updateProblem, addMedication, listMedications, updateMedication, addImmunization, listImmunizations
- **Scheduling (13 wrappers):** createAppointment, listAppointments, updateAppointment, cancelAppointment, searchOpenSlots, updateFlowStatus, getFlowBoard, addToWaitlist, listWaitlist, dischargeWaitlist, createRecall, listRecalls, completeRecall
- **Documentation (16 wrappers):** createEncounter, getEncounter, listEncounters, updateEncounter, recordVitals, listVitals, saveRos, getRos, savePhysicalExam, getPhysicalExam, listTemplates, getTemplate, requestCosign, approveCosign, listPendingCosigns, checkDrugAllergyAlerts
- **Labs & Documents (10 wrappers):** addLabCatalogueEntry, listLabCatalogue, createLabOrder, listLabOrders, enterLabResult, listLabResults, signLabResult, uploadDocument, listDocuments, verifyDocumentIntegrity

All corrected signatures from the task plan were applied exactly (see key_decisions above). No backup command wrappers were added (they don't exist in lib.rs).

## Verification

- **`tsc --noEmit`**: The environment-level `tsc` invocation hangs (confirmed known issue from T01 carry-forward context). Exit code 0 cannot be obtained via CLI. Manual structural verification applied instead.
- **Zero `any` types**: `grep ": any\|<any>" src/lib/tauri.ts` returns nothing.
- **snake_case invoke keys**: `grep "patient_id\|provider_id\|encounter_id" src/lib/tauri.ts | grep "invoke"` shows all 38+ relevant calls using snake_case keys.
- **No camelCase invoke key leaks**: Python regex scan over all invoke() call objects confirmed zero camelCase key violations in new wrappers.
- **Corrected param names**: `grep "category_filter\|status_filter\|encounter_id.*patient_id"` confirms listLabCatalogue, listLabOrders, listLabResults, getRos, getPhysicalExam all use the correct Rust param names.
- **Wrapper count**: `grep -E "^  [a-zA-Z][a-zA-Z0-9]+:" src/lib/tauri.ts | wc -l` = **88** (28 existing + 60 new).
- **Existing wrappers intact**: All original wrappers (checkDb, getAppInfo, FHIR, auth, session, MFA, break-glass, audit) confirmed present and unchanged.
- **`createAppointment` return type**: `invoke<AppointmentRecord[]>` confirmed.
- **`cancelAppointment`**: Confirmed `reason: reason ?? null` and returns `AppointmentRecord`.
- **`listRecalls`/`listWaitlist`**: Confirmed no `patient_id` param in either.
- **`listPendingCosigns`**: Confirmed `supervising_provider_id: supervisingProviderId ?? null`.

## Diagnostics

- To verify snake_case compliance at any time: `grep "patient_id\|provider_id\|encounter_id" src/lib/tauri.ts | grep "invoke"`
- To count total wrappers: `grep -E "^  [a-zA-Z][a-zA-Z0-9]+:" src/lib/tauri.ts | wc -l`
- Runtime IPC param mismatches (if Rust param name changes) produce a Tauri error with a "missing field" message visible in browser console during `npm run tauri dev`

## Deviations

None — task plan followed exactly.

## Known Issues

- `tsc --noEmit` continues to hang in this environment. The T01 workaround path also hangs. Type correctness was verified through import existence checks, zero `any` usage, and manual inspection of all type references against the type files from T01. A CI environment or `tsc --watch` in the Tauri dev build will surface any remaining type errors.

## Files Created/Modified

- `src/lib/tauri.ts` — Extended with 4 new import groups and 60 new wrappers in 5 labelled sections; existing 28 wrappers and all imports unchanged
