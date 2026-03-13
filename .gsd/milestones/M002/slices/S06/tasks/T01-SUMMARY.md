---
id: T01
parent: S06
milestone: M002
provides:
  - PhysicalExamTab component in EncounterWorkspace (fourth "Exam" tab)
  - physicalExamRecord state and savePhysicalExam action in useEncounter hook
  - extractPhysicalExamDisplay() pure extraction helper in fhirExtract.ts
key_files:
  - src/lib/fhirExtract.ts
  - src/hooks/useEncounter.ts
  - src/pages/EncounterWorkspace.tsx
key_decisions:
  - Used seededPhysicalExamId guard ("none" sentinel for null record) to avoid overwriting in-progress edits on reload — exact same pattern as soapSeededForId in useEncounter and seededVitalsId in VitalsTab
  - PhysicalExamTab uses disabled (not readOnly) on textareas to match Tailwind disabled:opacity-60 convention and prevent interaction more reliably
  - physicalExamRecordToForm reads finding[].itemCodeableConcept (code → field key, text → value) from FHIR ClinicalImpression, matching physical_exam.rs Rust structure
patterns_established:
  - getPhysicalExam(encounterId, patientId) added as fifth Promise.all entry in fetchAll — both params always passed, never omitted
  - seededPhysicalExamId guard in PhysicalExamTab useEffect with "none" sentinel for missing record
  - PhysicalExamInput mapping: all string fields coerced to null on empty string (|| null) before save
observability_surfaces:
  - console.error("[useEncounter] fetchAll failed …") already covers getPhysicalExam failures since it is in the same Promise.all
  - console.error("[EncounterWorkspace] savePhysicalExam failed: …") logged on save error
  - examError state rendered inline in PhysicalExamTab — visible without DevTools
  - seededPhysicalExamId inspectable in React DevTools on PhysicalExamTab component
duration: ~25 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T01: Add Physical Exam tab to EncounterWorkspace

**Added a four-system PhysicalExamTab with 13 body-system textareas + Additional Notes to EncounterWorkspace, wired to `getPhysicalExam`/`savePhysicalExam` commands via the `useEncounter` hook.**

## What Happened

Implemented all five steps from the task plan:

1. **`extractPhysicalExamDisplay()` added to `fhirExtract.ts`** — `ExtractedPhysicalExam` interface with 13 system fields + `additionalNotes` (all `string | null`). Reads `finding[].itemCodeableConcept` from a FHIR ClinicalImpression resource, keyed by system code. Returns all-null struct on null/undefined input. Pure, never throws.

2. **`useEncounter.ts` interface extended** — `physicalExamRecord: PhysicalExamRecord | null` and `savePhysicalExam: (input: PhysicalExamInput) => Promise<void>` added to `UseEncounterReturn`. Both types imported from `../types/documentation`.

3. **`useEncounter.ts` implementation extended** — `physicalExamRecord` state added, `getPhysicalExam(encounterId, patientId)` added as fifth entry in the `Promise.all` in `fetchAll`, `setPhysicalExamRecord(null)` added in the catch block, `savePhysicalExam` implemented with `useCallback` calling `commands.savePhysicalExam(input)` then `reload()`.

4. **`EncounterWorkspace.tsx` extended** — `ActiveTab` union extended to include `"exam"`, `{ id: "exam", label: "Exam" }` added to tab-bar array, `physicalExamRecord` and `savePhysicalExam` destructured from `useEncounter`. `PhysicalExamTab` sub-component added above the main component with seeded-ID guard, 13-system textarea grid + Additional Notes, save handler with inline error display, and read-only mode when `isReadOnly=true`. `PhysicalExamTab` rendered in tab body block with `isReadOnly={isFinalized}`.

5. **`npx tsc --noEmit` exits 0** — zero TypeScript errors.

## Verification

- `npx tsc --noEmit` → exits 0, no output (clean)
- Grep confirms `extractPhysicalExamDisplay` exported from `fhirExtract.ts`
- Grep confirms `physicalExamRecord`, `savePhysicalExam`, `getPhysicalExam` all present in `useEncounter.ts`
- Grep confirms `ActiveTab` includes `"exam"`, `PhysicalExamTab` component defined and rendered, tab entry `{ id: "exam", label: "Exam" }` in tab-bar array in `EncounterWorkspace.tsx`
- All 13 system fields + `additionalNotes` present in `PhysicalExamFormState` and `PHYSICAL_EXAM_SYSTEMS` array

## Diagnostics

- `console.error("[useEncounter] fetchAll failed …")` covers `getPhysicalExam` failures (same Promise.all)
- `console.error("[EncounterWorkspace] savePhysicalExam failed: …")` logged on save error
- `examError` inline error rendered below the textarea grid in PhysicalExamTab
- `seededPhysicalExamId` state visible in React DevTools on PhysicalExamTab
- TypeScript compiler (`tsc --noEmit`) is the primary static gate

## Deviations

none

## Known Issues

none

## Files Created/Modified

- `src/lib/fhirExtract.ts` — `ExtractedPhysicalExam` interface and `extractPhysicalExamDisplay()` function appended
- `src/hooks/useEncounter.ts` — `PhysicalExamRecord`, `PhysicalExamInput` imports; interface fields `physicalExamRecord` and `savePhysicalExam`; state, Promise.all entry, catch reset, and useCallback implementation added
- `src/pages/EncounterWorkspace.tsx` — `PhysicalExamInput`, `PhysicalExamRecord` imports; `ActiveTab` extended; `PhysicalExamFormState`, `PHYSICAL_EXAM_SYSTEMS`, `physicalExamRecordToForm`, `PhysicalExamTab` added; main component destructures and renders `PhysicalExamTab`
