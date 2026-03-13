---
estimated_steps: 7
estimated_files: 2
---

# T04: Build ROS tab with 14-system toggle grid and `saveRos`/`getRos` wiring

**Slice:** S03 — Clinical Encounter Workspace
**Milestone:** M002

## Description

This task builds the 14-system Review of Systems form — the highest-density interactive surface in the app. It extends `useEncounter` to fetch `getRos` (requiring BOTH `encounterId` and `patientId` — the most critical constraint in this task), replaces the T01 placeholder with a compact toggle grid (one row per system: label + three inline radio buttons + optional notes input), wires `saveRos`, and enforces finalization read-only.

The ROS form must initialize all 28 state fields (`null`, never `undefined`) and parse the saved `RosRecord.resource` QuestionnaireResponse on reload to restore toggle states.

## Steps

1. **Extend `useEncounter` with ROS fetch**: Add `getRos(encounterId, patientId)` to the `Promise.all` fetch in `fetchAll`. This command requires BOTH params passed as `{ encounter_id: encounterId, patient_id: patientId }`. The wrapper in `tauri.ts` already enforces this — always use `commands.getRos(encounterId, patientId)`, never raw `invoke`. Expose `rosRecord: RosRecord | null` in the hook's return type. Add `saveRos: (input: ReviewOfSystemsInput) => Promise<void>` — calls `commands.saveRos(input)` then `reload()`.

2. **Define the 14 system definitions array** as a typed const in `EncounterWorkspace.tsx`:
   ```typescript
   const ROS_SYSTEMS: { key: keyof ReviewOfSystemsInputSystems; label: string }[] = [
     { key: "constitutional", label: "Constitutional" },
     { key: "eyes", label: "Eyes" },
     { key: "ent", label: "ENT / Head" },
     { key: "cardiovascular", label: "Cardiovascular" },
     { key: "respiratory", label: "Respiratory" },
     { key: "gastrointestinal", label: "Gastrointestinal" },
     { key: "genitourinary", label: "Genitourinary" },
     { key: "musculoskeletal", label: "Musculoskeletal" },
     { key: "integumentary", label: "Integumentary / Skin" },
     { key: "neurological", label: "Neurological" },
     { key: "psychiatric", label: "Psychiatric" },
     { key: "endocrine", label: "Endocrine" },
     { key: "hematologic", label: "Hematologic / Lymphatic" },
     { key: "allergicImmunologic", label: "Allergic / Immunologic" },
   ];
   ```
   Note: define a helper type `ReviewOfSystemsInputSystems` (pick of the 14 status keys from `ReviewOfSystemsInput`) to make the key type safe without fully duplicating the interface.

3. **Initialize `rosState`**: Use `useState<ReviewOfSystemsInput>` initialized with all 28 fields set to `null` (both status fields and notes fields). Define an `initRosFromRecord(record: RosRecord | null): ReviewOfSystemsInput` pure function that parses `record.resource` (a FHIR QuestionnaireResponse) to extract status and notes for each system. The QuestionnaireResponse `item` array uses `linkId` values matching system keys. If record is null or a system's item is missing, that field stays `null`. Add a `useEffect` watching `rosRecord` that calls `setRosState(initRosFromRecord(rosRecord))` when `rosRecord` changes — guarded by `mounted` check via the hook's reload cycle.

4. **Render the ROS toggle grid**: Replace "Review of Systems — T04" placeholder with:
   - A heading "Review of Systems" + system count summary (e.g. "3 systems reviewed")
   - For each of the 14 systems, render a horizontal row: system label (w-48) + three radio-button-style buttons for "Positive" / "Negative" / "Not Reviewed". Each button is styled as a small `<button>` with active/inactive styles: active = colored background (Positive = red-50 border-red-400 text-red-700; Negative = green-50 border-green-400 text-green-700; Not Reviewed = gray-50 border-gray-300 text-gray-500). Click: set `rosState[key + "Status"]` to the corresponding `RosStatus`.
   - Below each row: when `rosState[key]` is `"positive"`, render a full-width `<input type="text" placeholder="Notes…">` for `rosState[key + "Notes"]`. When status changes away from "positive", set notes to `null` and hide the input.

5. **Save ROS button**: Assemble `ReviewOfSystemsInput` with `patientId`, `encounterId`, and all 28 fields from `rosState`. Call `saveRos(assembled)` → `savingRos: boolean` → inline `rosError: string | null`. The hook calls `commands.saveRos(input)` then `reload()`. After reload, `useEffect` re-initializes `rosState` from the fresh `rosRecord`, restoring saved toggle states.

6. **System count summary**: Compute `reviewedCount` as the number of systems where status is `"positive"` or `"negative"` (not null, not `"not_reviewed"`). Display as "X of 14 systems reviewed" near the Save button.

7. **RBAC and finalization**: When `isFinalized`: all radio buttons have `disabled={true}`, notes inputs have `readOnly={true}`, "Save ROS" button not rendered. Show "ROS locked — encounter finalized" notice. NurseMa: for consistency (ROS is clinical documentation), treat NurseMa same as Provider for ROS editing (NurseMa can create vitals and assist with ROS). This matches the RBAC research note that NurseMa has CRU on ClinicalDocumentation.

## Must-Haves

- [ ] `useEncounter` calls `commands.getRos(encounterId, patientId)` with BOTH params; `rosRecord` exposed in return
- [ ] All 28 `rosState` fields initialized to `null` (not `undefined`)
- [ ] 14-system toggle grid renders all systems with 3-state radio buttons
- [ ] Notes input appears only when system is marked Positive; hides and nulls when status changes
- [ ] `saveRos` assembles full `ReviewOfSystemsInput` with all 30 fields; calls `commands.saveRos`; errors shown inline
- [ ] After save + reload, toggle states restore from persisted `rosRecord`
- [ ] Finalized encounters: all toggles disabled, Save hidden
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` — must exit 0 with zero errors
- In running Tauri app:
  1. Open encounter workspace → ROS tab → all 14 systems show "Not Reviewed" (no selection highlighted)
  2. Click "Positive" for Constitutional → notes input appears below → type "Fatigue x3 days" → click "Negative" for Respiratory → no notes input
  3. Click "Save ROS" → no error; spinner shows briefly
  4. Navigate away → re-open same encounter → ROS tab → Constitutional shows "Positive" with "Fatigue x3 days" in notes; Respiratory shows "Negative"
  5. System count summary shows "2 of 14 systems reviewed"
  6. Open a finalized encounter → ROS tab → all buttons `disabled`; Save hidden
  7. Verify in console: first load logs `[useEncounter] rosRecord:` — check that it's non-null after the second open (confirming `getRos` was called with both params correctly)

## Observability Impact

- Signals added/changed: `rosError` inline banner surfaces Rust errors from `saveRos`; `savingRos` spinner; `initRosFromRecord` uses console.warn if a QuestionnaireResponse item has an unrecognized `linkId` — helps detect schema drift
- How a future agent inspects this: React DevTools → `rosState` shows all 28 field values; `rosRecord` shows the raw FHIR QuestionnaireResponse from the backend; system count computed value visible in UI
- Failure state exposed: if `saveRos` fails, `rosState` is NOT reset (user edits preserved); `rosError` shows the error; user can retry without re-entering data. If `getRos` returns null (first-ever ROS for this encounter), all fields initialize to null — correct behavior, not an error.

## Inputs

- `src/hooks/useEncounter.ts` (from T01/T02/T03) — extend with `rosRecord`, `saveRos`
- `src/pages/EncounterWorkspace.tsx` (from T01/T02/T03) — replace ROS tab placeholder
- `src/lib/tauri.ts` — `commands.saveRos`, `commands.getRos` already wired; `getRos` requires both `encounterId` and `patientId`
- `src/types/documentation.ts` — `ReviewOfSystemsInput`, `RosRecord`, `RosStatus` already typed

## Expected Output

- `src/hooks/useEncounter.ts` — extended with `rosRecord: RosRecord | null` and `saveRos`; `getRos(encounterId, patientId)` called in initial fetch
- `src/pages/EncounterWorkspace.tsx` — ROS tab fully functional: 14-system toggle grid, conditional notes inputs, save with spinner/error, RBAC and finalized-lock enforcement
