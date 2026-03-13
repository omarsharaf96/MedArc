---
id: T04
parent: S03
milestone: M002
provides:
  - RosTab component in EncounterWorkspace.tsx — 14-system toggle grid with Positive/Negative/Not Reviewed radio buttons, conditional notes inputs, save with spinner/error, RBAC/finalized-lock enforcement
  - useEncounter extended with rosRecord (RosRecord | null) and saveRos; getRos called in Promise.all with both encounterId and patientId
  - emptyRosState() — all 28 fields initialized to null (never undefined)
  - initRosFromRecord() — pure function parsing FHIR QuestionnaireResponse to restore toggle states; warns on unrecognized linkId for schema drift detection
  - ReviewOfSystemsInputSystems helper type + notesKey/statusKey helpers for type-safe dynamic field access
key_files:
  - src/hooks/useEncounter.ts
  - src/pages/EncounterWorkspace.tsx
key_decisions:
  - getRos requires both encounterId AND patientId — wired as commands.getRos(encounterId, patientId) in Promise.all alongside encounter/vitals/templates
  - seededRosId guard uses a "none" sentinel string for the no-record-yet case, mirroring the seededVitalsId/soapSeededForId pattern from T02/T03
  - rosState NOT reset on saveRos failure — user edits preserved for retry; rosError shown inline
  - NurseMa has full ROS edit access (isReadOnly = isFinalized only), consistent with CRU on ClinicalDocumentation per slice spec
  - (base as unknown as Record<string, unknown>) double-cast used in initRosFromRecord — needed because TypeScript does not allow direct cast from concrete interface to Record<string, unknown>; using unknown as intermediate satisfies the compiler without changing runtime behaviour
patterns_established:
  - seeded-ID guard for ROS form: track seededRosId (record.id or "none" sentinel) to prevent overwrite of in-progress edits on unrelated reloads — mirrors soapSeededForId (T02) and seededVitalsId (T03)
  - statusKey/notesKey helper functions provide type-safe dynamic property access on ReviewOfSystemsInput without index signature
  - console.warn in initRosFromRecord on unrecognized QuestionnaireResponse linkId — schema drift early-warning for future agents
observability_surfaces:
  - rosError inline red <p> — visible without DevTools on any saveRos failure
  - savingRos spinner — "Saving…" text on Save ROS button during async call
  - console.log("[useEncounter] rosRecord:", rosResult) on every fetch — confirms getRos was called with correct params
  - console.warn("[initRosFromRecord] Unrecognized linkId ...") on schema drift
  - React DevTools — RosTab component → rosState (all 28 fields), seededRosId, reviewedCount computed from state
  - Tauri stdout — console.error("[EncounterWorkspace] saveRos failed:") on saveRos errors
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T04: Build ROS tab with 14-system toggle grid and `saveRos`/`getRos` wiring

**Replaced the T01 ROS placeholder with a fully functional 14-system toggle grid, wired `getRos(encounterId, patientId)` in the parallel fetch, and built save/restore round-trip via `saveRos` + `initRosFromRecord`.**

## What Happened

Extended `useEncounter` with a fourth parallel fetch: `commands.getRos(encounterId, patientId)` — both params are required and wired explicitly. The result is stored as `rosRecord: RosRecord | null` and exposed in the hook's return type. A `saveRos` callback calls `commands.saveRos(input)` then `reload()`.

In `EncounterWorkspace.tsx`, replaced the T01 placeholder `<div>` with `RosTab`. The ROS tab implementation includes:

- `emptyRosState()` — initializes all 28 fields to `null` (14 system status fields + 14 notes fields)
- `initRosFromRecord(record)` — pure function that parses a FHIR QuestionnaireResponse to restore toggle states and notes; uses `item[].linkId` for system keys, `item[].answer[0].valueCoding.code` for status, and nested `item[].answer[0].valueString` for notes. Warns on unrecognized linkIds for schema drift detection.
- `ROS_SYSTEMS` typed const — 14 entries with `key: keyof ReviewOfSystemsInputSystems` and `label: string`
- `ReviewOfSystemsInputSystems` helper pick type — avoids duplicating the 14-key subset of `ReviewOfSystemsInput`
- `statusKey` / `notesKey` helper functions — type-safe dynamic key derivation (avoids index signature hack on the concrete interface)
- seeded-ID guard (`seededRosId`) — mirrors T02/T03 pattern; "none" sentinel handles the first-ever ROS (no record) case without re-seeding on every reload
- 14-system toggle grid: one div per system, label (w-48) + three `<button>` radio buttons (Positive/Negative/Not Reviewed) with active/inactive color styles (red/green/gray)
- Conditional notes `<input type="text">` — appears only when status is "positive"; hidden and nulled when status changes away from positive
- `reviewedCount` computed as systems with status "positive" or "negative"
- Save button with `savingRos` spinner and `rosError` inline banner; button hidden when finalized
- `isReadOnly = isFinalized` — NurseMa has full edit access; all buttons `disabled` when finalized; "ROS locked" notice shown

## Verification

- `npx tsc --noEmit` — exits 0, zero errors
- TypeScript compiler accepts all 30-field `ReviewOfSystemsInput` assembly, `RosRecord | null` return type, and all dynamic property accesses
- All 14 `ROS_SYSTEMS` entries verified against `ReviewOfSystemsInput` field names
- All 28 `emptyRosState()` fields confirmed null (14 status + 14 notes)
- Tauri dev server not launched (environment Node v23.7.0 has a vite chunk incompatibility unrelated to this task — same issue pre-existed T03)

## Diagnostics

- **rosError**: inline `<p className="text-red-600">` below Save ROS button — visible without DevTools on any `saveRos` failure; rosState preserved so user can retry
- **savingRos spinner**: "Saving…" on Save ROS button during async call
- **rosRecord log**: `console.log("[useEncounter] rosRecord:", rosResult)` on every fetch — confirms getRos called with correct params; check for non-null after second open
- **schema drift**: `console.warn("[initRosFromRecord] Unrecognized linkId ...")` if QuestionnaireResponse item has unexpected linkId
- **React DevTools**: `RosTab` component → `rosState` (all 28 fields), `seededRosId` (which record last populated form), `reviewedCount` computed via filter
- **Tauri stdout**: `console.error("[EncounterWorkspace] saveRos failed:", msg)` on any saveRos failure
- **getRos failure**: `[useEncounter] fetchAll failed for <encounterId>:` in Tauri stdout if Promise.all rejects

## Deviations

- `seededRosId` uses a `"none"` sentinel string instead of `null` for the no-record case (null would conflict with the initial state and prevent seeding when the first rosRecord arrives as null). This is a minor implementation detail not in the plan but follows the spirit of the seeded-ID guard pattern.
- `(base as unknown as Record<string, unknown>)` double-cast in `initRosFromRecord` — TypeScript requires the intermediate `unknown` cast when converting a concrete interface to `Record<string, unknown>`. The plan said `as Record<string, unknown>` which triggers TS2352; the double-cast is the correct fix.

## Known Issues

None.

## Files Created/Modified

- `src/hooks/useEncounter.ts` — added `rosRecord` state, `getRos(encounterId, patientId)` in Promise.all, `saveRos` callback; `ReviewOfSystemsInput` and `RosRecord` imported
- `src/pages/EncounterWorkspace.tsx` — added `ReviewOfSystemsInputSystems` type, `ROS_SYSTEMS` const, `statusKey`/`notesKey` helpers, `emptyRosState()`, `initRosFromRecord()`, full `RosTab` component; replaced T01 placeholder; destructured `rosRecord`/`saveRos` from hook; imported `ReviewOfSystemsInput`, `RosRecord`, `RosStatus`
