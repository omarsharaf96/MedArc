# S03: Clinical Encounter Workspace — Research

**Date:** 2026-03-12
**Requirement:** UI-03 — Provider can write clinical encounter notes through a structured SOAP workspace

## Summary

S03 builds the clinical encounter workspace on top of the existing `PatientDetailPage` shell delivered in S02. The backend is fully implemented: `documentation.rs` exposes 16 Tauri commands covering encounters, vitals, ROS, physical exam, templates, co-sign, and drug-allergy CDS — all wired with correct TypeScript wrappers in `src/lib/tauri.ts` and typed in `src/types/documentation.ts`. The primary deliverable is an `EncounterWorkspace` component (tabbed editor: SOAP | Vitals | ROS) plus a `useEncounter` hook, both reachable from a new "Start Encounter" button on `PatientDetailPage`.

The highest-risk surface is the ROS form: 14 organ systems × 3 states (positive/negative/not_reviewed) × optional notes = 28+ interactive fields. This must be rendered without overwhelming the provider. The SOAP note editor decision was resolved in context: plain `<textarea>` per section (four tabs or four stacked sections) — no rich-text editor needed. BMI is auto-calculated on the backend (`record_vitals` returns it in `VitalsRecord.bmi`); the UI just needs to display it.

The router must gain a new `encounter-workspace` route variant carrying both `patientId` and `encounterId`. This is a one-line change to `RouterContext.tsx` and a `case` in `ContentArea.tsx` — the same pattern used for `patient-detail` in S01/S02.

## Recommendation

**Build `EncounterWorkspace` as a self-contained page component (not a modal) with three main tabs: SOAP | Vitals | ROS.** Physical exam is S06 scope — do not build it here. The workspace opens via a new route `{ page: "encounter-workspace"; patientId: string; encounterId: string }` navigated from `PatientDetailPage`. This matches the S03 boundary map (`EncounterWorkspace` → S06) and keeps the PatientDetailPage as the chart shell without bloating it.

Break the work into four tasks:
1. **T01** — Add `encounter-workspace` route, build `useEncounter` hook, create `EncounterWorkspace` shell (tabs only, no real content).
2. **T02** — Build the SOAP note tab with template picker and all four section textareas; wire `createEncounter` / `updateEncounter` / `listTemplates` / `getTemplate`.
3. **T03** — Build the Vitals tab with all 9 vital fields, BMI display, `recordVitals` wire.
4. **T04** — Build the ROS tab with 14-system toggle form; wire `saveRos` / `getRos`.

Each task leaves `tsc --noEmit` green.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Tabbed UI navigation within a workspace | `useState` + conditional rendering (see `PatientFormModal` two-tab pattern) | Already established — no need for a tab library |
| Modal overlay positioning | `position: fixed inset-0 z-50` (see `PatientFormModal`, `LockScreen`) | Consistent overlay pattern across app |
| Fetch-on-mount + reload pattern | `usePatient` hook (refreshCounter + mounted boolean) | Copy this exact structure for `useEncounter` |
| Inline error surfacing | Inline `<p className="text-red-600">` (see `PatientFormModal.submitError`) | Error always visible without DevTools |
| BMI calculation | Backend: `record_vitals` auto-calculates BMI, returns `VitalsRecord.bmi` | Do NOT recalculate in the frontend |
| Template content | `commands.getTemplate(templateId)` returns `TemplateRecord.defaultSoap` | Use to pre-populate SOAP textarea values |
| ROS status type | `RosStatus = "positive" \| "negative" \| "not_reviewed"` in `src/types/documentation.ts` | Already typed, use as-is |

## Existing Code and Patterns

- `src/contexts/RouterContext.tsx` — Add `| { page: "encounter-workspace"; patientId: string; encounterId: string }` to the `Route` union (one line). Follow the `patient-detail` variant exactly.
- `src/components/shell/ContentArea.tsx` — Add a `case "encounter-workspace"` that renders the new `EncounterWorkspace` component. Mirror the `patient-detail` case structure.
- `src/pages/PatientDetailPage.tsx` — Add "Start Encounter" and "View Encounters" buttons that call `commands.createEncounter` then navigate to `encounter-workspace`. RBAC gate: only Provider / NurseMa / SystemAdmin see these buttons (FrontDesk and BillingStaff do not).
- `src/hooks/usePatient.ts` — Reference for the `useEncounter` hook structure: `useState`, `useEffect` with mounted boolean, `refreshCounter`, `useCallback` reload. Copy this pattern, not the AuditLog pattern.
- `src/components/patient/PatientFormModal.tsx` — Reference for: tabbed form UX (two tabs, button row toggle), `INPUT_CLS` / `LABEL_CLS` Tailwind constants, `FormField` helper component, modal overlay z-50 pattern, `onSuccess` / `onClose` props.
- `src/lib/tauri.ts` — All 16 documentation commands are already wired. Key calls for S03:
  - `commands.createEncounter(input)` → `EncounterRecord`
  - `commands.updateEncounter(encounterId, input)` → `EncounterRecord`
  - `commands.listEncounters(patientId, null, null, null)` → `EncounterRecord[]`
  - `commands.recordVitals(input)` → `VitalsRecord` (BMI auto-populated)
  - `commands.listVitals(patientId, encounterId)` → `VitalsRecord[]`
  - `commands.listTemplates(null)` → `TemplateRecord[]`
  - `commands.getTemplate(templateId)` → `TemplateRecord`
  - `commands.saveRos(input)` → `RosRecord`
  - `commands.getRos(encounterId, patientId)` → `RosRecord | null` **(requires BOTH params)**
- `src/types/documentation.ts` — Fully typed: `EncounterInput`, `EncounterRecord`, `UpdateEncounterInput`, `SoapInput`, `VitalsInput`, `VitalsRecord`, `ReviewOfSystemsInput`, `RosRecord`, `TemplateRecord`, `RosStatus`.
- `src/lib/fhirExtract.ts` — Pattern for FHIR extraction helpers. If SOAP sections need to be re-read from `EncounterRecord.resource`, build an `extractSoapSections(resource)` helper here following the same pattern as `extractPatientDisplay`.

## Constraints

- **No new Rust changes** — all backend commands exist and are wired. S03 is purely frontend work.
- **`tsc --noEmit` must exit 0** after each task. Use `Option<T>` → `T | null` (never `T | undefined`) for all field types per the established convention.
- **Tailwind only** — no CSS modules, styled-components, or rich-text editor library.
- **Plain `<textarea>` for SOAP** — the decision was made in context (M002-CONTEXT.md open questions): `<textarea>` with section tabs is sufficient for MVP.
- **`getRos` requires BOTH `encounter_id` AND `patient_id`** — passing only `encounter_id` causes a silent null deserialization failure. The wrapper already enforces this: `commands.getRos(encounterId, patientId)`.
- **`listVitals` optional `encounter_id` filter** — pass `encounterId` (not null) to scope vitals to the current encounter: `commands.listVitals(patientId, encounterId)`.
- **RBAC for ClinicalDocumentation**: Provider + NurseMa + SystemAdmin can create/update; BillingStaff read-only; FrontDesk no access. Gate UI elements accordingly — do not hide the workspace from BillingStaff (they have Read), but disable editing controls.
- **NurseMa can create vitals** — the backend has a special carve-out allowing NurseMa to call `record_vitals` even though NurseMa cannot create encounters. The UI should show the Vitals tab to NurseMa even if the SOAP tab is read-only for them.
- **Encounter creation flow**: `createEncounter` is called when the provider clicks "Start Encounter" — not lazily inside the workspace. The workspace receives an `encounterId` from the route and calls `getEncounter` to hydrate itself. This avoids creating a blank encounter on every workspace mount.
- **`encounterDate` format**: The Rust field expects ISO 8601 without timezone suffix (e.g. `"2026-04-01T09:00:00"`) — use `new Date().toISOString().slice(0, 19)` for the current local time, NOT `.toISOString()` (which appends `Z`).
- **Template pre-population**: when a provider selects a template, `commands.getTemplate(templateId)` returns `TemplateRecord.defaultSoap` which has pre-filled text for all four SOAP sections. The UI should offer to apply the template (overwrite current) OR skip. Do not silently overwrite.

## Common Pitfalls

- **`getRos` silent null if `patient_id` omitted** — The invoke call must be `{ encounter_id: encounterId, patient_id: patientId }`. Passing only `encounter_id` returns null without an error (Tauri silently treats the missing param as None). The wrapper in `tauri.ts` is correct; do not call `invoke` directly.
- **Stale `encounterId` in hook** — `useEncounter` must take `{ patientId, encounterId }` as props, not derive the encounter ID from a global. The encounter ID is set at navigation time and must be passed through the route: `{ page: "encounter-workspace"; patientId; encounterId }`.
- **`mounted` boolean guard in async fetch** — All async operations in `useEncounter` must check `if (!mounted) return` after `await` calls, same as `usePatient`. Omitting this causes React state update on unmounted component warnings during navigation.
- **ROS state initialization** — `ReviewOfSystemsInput` has 28 fields (14 × status + 14 × notes). Initialize them all to `null` in the form state — not `undefined`. TypeScript will catch `undefined` in the invoke call since all ROS fields are `T | null`.
- **Vitals numeric input → `number | null`** — `VitalsInput` fields like `systolicBp` are `number | null`. HTML `<input type="number">` returns a string or empty string from `e.target.value`. Parse with `parseInt`/`parseFloat` and convert empty string to `null` explicitly: `value === "" ? null : parseFloat(value)`.
- **Route type exhaustiveness** — After adding `encounter-workspace` to the `Route` union in `RouterContext.tsx`, TypeScript will flag the `default` case in `ContentArea.tsx`'s switch as reachable until the matching `case` is added. This is correct behavior — fix both files together in T01.
- **`encounterType` must match Rust enum** — Valid values: `"office_visit"`, `"telehealth"`, `"urgent_care"`, `"follow_up"`, `"preventive"`, `"procedure"`. Do not send free-text here; use a `<select>` with these options.
- **`updateEncounter` for SOAP saves, not `createEncounter`** — After the initial encounter creation, all SOAP section saves must call `commands.updateEncounter(encounterId, { soap, status: null, chiefComplaint: null })`. Do not create a new encounter on every save.
- **`VitalsRecord.bmi` is server-computed** — Do not display or compute BMI from the form's weight/height fields in real time. Instead, display the `bmi` returned in `VitalsRecord` after `recordVitals` resolves. Showing an unconfirmed number before save would be misleading in a clinical context.

## Open Risks

- **Encounter list on PatientDetailPage** — The S03 boundary deliverable is `EncounterWorkspace`. But to reach it from the patient chart, `PatientDetailPage` needs both a "Start Encounter" button (creates + navigates) and a list of prior encounters. The encounter list requires `commands.listEncounters(patientId)` and a click-to-open flow. This is likely T01 scope to avoid a dead-end navigation. Confirm this is included in T01.
- **ROS form complexity budget** — 14 organ systems with 3-state toggles + optional free-text notes fields is the highest-density form in the app. A naive render (one row per system, all visible) creates a ~60-element form. Consider collapsing the form into a compact toggle-grid (system name + Positive/Negative/Not Reviewed radio buttons in a horizontal row) with the notes field revealed only when a system is marked Positive.
- **Encounter "draft vs finalized" UX** — The backend supports `status: "in-progress" | "finished"`. The workspace should show a "Finalize Encounter" action that calls `updateEncounter({ status: "finished", ... })`. After finalization, the workspace should switch to read-only mode. Without this, providers may leave encounters in `"in-progress"` indefinitely.
- **`useAuth` inside `EncounterWorkspace`** — The workspace needs `auth.user?.id` (the provider ID) for `createEncounter.providerId` and `recordVitals.patientId`. Pattern from `PatientDetailPage`: pass `role` and `userId` as props from the page wrapper, or call `useAuth()` once at the page level and pass values down. Avoid calling `useAuth()` in multiple sub-components.
- **Template picker interaction** — `listTemplates()` returns 12 built-in templates. A `<select>` dropdown is simplest. Applying a template to a non-empty SOAP note requires a confirmation step ("This will replace your current note. Continue?") — implement as a simple `window.confirm` or a tiny inline banner.

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React 18 / TypeScript | (core, no skill needed) | n/a |
| Tailwind CSS | (core, no skill needed) | n/a |
| Tauri 2.x | (established in codebase, patterns documented) | n/a |

No external skills required. The codebase already establishes all patterns needed (hook structure, modal overlay, Tailwind classes, form validation).

## Sources

- Backend API contract: `src-tauri/src/commands/documentation.rs` — all 16 Tauri commands, Rust struct field names, RBAC rules per command
- TypeScript contract: `src/types/documentation.ts` — fully typed `EncounterInput`, `VitalsInput`, `ReviewOfSystemsInput`, `TemplateRecord`, `RosStatus`
- Invoke wrappers: `src/lib/tauri.ts` — all wrappers already wired; `getRos` requires both `encounterId` + `patientId`
- Router pattern: `src/contexts/RouterContext.tsx` — discriminated union Route type; add `encounter-workspace` variant
- Dispatch pattern: `src/components/shell/ContentArea.tsx` — exhaustive switch for route rendering
- Hook pattern: `src/hooks/usePatient.ts` — mounted boolean, refreshCounter, parallel fetches
- Form pattern: `src/components/patient/PatientFormModal.tsx` — tabs, `INPUT_CLS` constants, `FormField` helper, inline error surfacing
- RBAC matrix: `src-tauri/src/rbac/roles.rs` lines 189–201 — ClinicalDocumentation rules per role
- Decisions: `.gsd/DECISIONS.md` — `getRos`/`getPhysicalExam` require both params; `T | null` not `T | undefined`; plain `<textarea>` for SOAP; `updateEncounter` for saves
