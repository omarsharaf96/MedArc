---
estimated_steps: 5
estimated_files: 3
---

# T01: Add Physical Exam tab to EncounterWorkspace

**Slice:** S06 — Labs, Documents & Physical Exam
**Milestone:** M002

## Description

Extend the `useEncounter` hook with physical exam data and the `savePhysicalExam` action, then add a fourth "Exam" tab to `EncounterWorkspace` with a 13-system free-text form. This closes CLIN-04 at the UI level. No new dependencies are required — all Tauri commands (`savePhysicalExam`, `getPhysicalExam`) are already registered and wrapped.

The implementation follows the exact same patterns established in S03/T04 for ROS:
- `physicalExamRecord: PhysicalExamRecord | null` state in the hook
- `getPhysicalExam(encounterId, patientId)` added to the `Promise.all` in `fetchAll`
- Seeded-ID guard (`seededPhysicalExamId`) to avoid overwriting in-progress edits on reload
- `savePhysicalExam(input)` calls the command then calls `reload()`
- `PhysicalExamTab` sub-component in `EncounterWorkspace` is read-only when `isFinalized === true`

## Steps

1. **Append `extractPhysicalExamDisplay()` to `src/lib/fhirExtract.ts`.**
   - Add an `ExtractedPhysicalExam` interface with all 13 system fields + `additionalNotes` (all `string | null`).
   - Implement `extractPhysicalExamDisplay(resource: Record<string, unknown> | null | undefined): ExtractedPhysicalExam` — reads `finding[].itemCodeableConcept` from a FHIR ClinicalImpression resource, keyed by system code (`general`, `heent`, `neck`, `cardiovascular`, `pulmonary`, `abdomen`, `extremities`, `neurological`, `skin`, `psychiatric`, `musculoskeletal`, `genitourinary`, `rectal`, `additionalNotes`). Returns all-null struct on null/undefined input. Never throws.

2. **Extend `UseEncounterReturn` in `src/hooks/useEncounter.ts`.**
   - Add `physicalExamRecord: PhysicalExamRecord | null` to the return interface.
   - Add `savePhysicalExam: (input: PhysicalExamInput) => Promise<void>` to the return interface.
   - Import `PhysicalExamRecord` and `PhysicalExamInput` from `../types/documentation`.

3. **Extend `useEncounter` implementation.**
   - Add `const [physicalExamRecord, setPhysicalExamRecord] = useState<PhysicalExamRecord | null>(null);`
   - Add `commands.getPhysicalExam(encounterId, patientId)` as the fifth entry in the `Promise.all` in `fetchAll`. Destructure the result and call `setPhysicalExamRecord(physicalExamResult)`.
   - Add `setPhysicalExamRecord(null)` in the catch block alongside the other null resets.
   - Implement `savePhysicalExam` with `useCallback`: call `await commands.savePhysicalExam(input)`, then `reload()`. Include `[encounterId, reload]` in the dependency array.
   - Add `physicalExamRecord` and `savePhysicalExam` to the hook's return object.

4. **Extend `EncounterWorkspace.tsx`.**
   - Change `type ActiveTab = "soap" | "vitals" | "ros"` to `type ActiveTab = "soap" | "vitals" | "ros" | "exam"`.
   - Add `{ id: "exam" as const, label: "Exam" }` to the tab-bar array after `"ros"`.
   - Destructure `physicalExamRecord` and `savePhysicalExam` from `useEncounter`.
   - Add a `PhysicalExamTab` inline sub-component (defined above `EncounterWorkspace` or as a named inner component):
     - Props: `physicalExamRecord: PhysicalExamRecord | null`, `isReadOnly: boolean`, `patientId: string`, `encounterId: string`, `onSave: (input: PhysicalExamInput) => Promise<void>`
     - State: `PhysicalExamFormState` — one `string` field per system (13 systems + `additionalNotes`), all initialized to `""`.
     - Seeding: use a `seededPhysicalExamId` guard (same pattern as `soapSeededForId` in `useEncounter`) — only re-seed when `physicalExamRecord?.id` changes from the last seeded ID to avoid overwriting in-progress edits.
     - Layout: 13-row textarea grid using Tailwind; each row has a label (`General`, `HEENT`, `Neck`, etc.) and a `<textarea className="...">` (disabled when `isReadOnly`). Additional Notes at the bottom.
     - Save button: calls `onSave({ patientId, encounterId, general: state.general || null, heent: state.heent || null, ... additionalNotes: state.additionalNotes || null })`. Hidden or disabled when `isReadOnly`.
   - Add `{activeTab === "exam" && <PhysicalExamTab ... />}` block after the ROS block. Pass `isReadOnly={isFinalized}`.

5. **Run `npx tsc --noEmit` and fix any type errors.**

## Must-Haves

- [ ] `extractPhysicalExamDisplay()` added to `fhirExtract.ts` — pure, never throws, returns all-null struct on null input
- [ ] `physicalExamRecord: PhysicalExamRecord | null` exposed from `useEncounter`
- [ ] `savePhysicalExam` in `useEncounter` calls `commands.savePhysicalExam(input)` then `reload()`
- [ ] `getPhysicalExam(encounterId, patientId)` included in `fetchAll`'s `Promise.all` — both params always passed
- [ ] `ActiveTab` union includes `"exam"`; "Exam" tab button visible in EncounterWorkspace tab bar
- [ ] `PhysicalExamTab` renders all 13 system textareas + Additional Notes
- [ ] `isReadOnly = isFinalized` guard: textareas disabled and save button hidden when encounter is finalized
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` exits 0
- Open EncounterWorkspace in dev app; confirm four tabs visible: SOAP, Vitals, ROS, Exam
- Switch to Exam tab; confirm 13 textareas and Additional Notes field visible; enter text and click Save; confirm no console error

## Observability Impact

- Signals added/changed: `console.error("[useEncounter] fetchAll failed …")` already emitted on fetch failure — now also covers `getPhysicalExam` failure since it is in the same `Promise.all`
- How a future agent inspects this: TS compiler gate (`tsc --noEmit`); React component renders physicalExamRecord fields — visible in React DevTools; `seededPhysicalExamId` state tracks which encounter's exam data was seeded
- Failure state exposed: `error` state in `useEncounter` (shown in EncounterWorkspace error banner) covers `getPhysicalExam` failures; `savePhysicalExam` throws to the caller — `PhysicalExamTab` should catch and show inline error

## Inputs

- `src/hooks/useEncounter.ts` — existing hook with `Promise.all` pattern for `getRos`; follow exactly for `getPhysicalExam`
- `src/pages/EncounterWorkspace.tsx` — existing three-tab shell; extend `ActiveTab` union and tab-bar array
- `src/lib/fhirExtract.ts` — existing extraction helpers; append new function at end of file
- `src/types/documentation.ts` — `PhysicalExamInput`, `PhysicalExamRecord` fully typed; all 13 system fields + `additionalNotes` as `string | null`
- `src/lib/tauri.ts` — `commands.savePhysicalExam`, `commands.getPhysicalExam` already wired; no changes needed

## Expected Output

- `src/lib/fhirExtract.ts` — `extractPhysicalExamDisplay()` added
- `src/hooks/useEncounter.ts` — `physicalExamRecord`, `savePhysicalExam` added to interface and implementation
- `src/pages/EncounterWorkspace.tsx` — `PhysicalExamTab` sub-component added; `ActiveTab` extended to `"exam"`; four tabs in tab bar
