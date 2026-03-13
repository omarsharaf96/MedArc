# S03: Clinical Encounter Workspace

**Goal:** A provider can open a patient's chart, start a clinical encounter, write a structured SOAP note using a built-in template, record vitals (BP, HR, Temp, SpO2, Weight, Height, BMI auto-displayed, pain score), complete a 14-system ROS form, and save the encounter — all wired to the real Tauri commands.

**Demo:** Log in as Provider → open any patient chart → click "Start Encounter" → encounter workspace opens → select a template → SOAP sections pre-populate → edit the note → save → switch to Vitals tab → enter BP/HR/Temp/weight/height → save vitals → BMI displays from server response → switch to ROS tab → mark 3–4 systems → save ROS → finalize encounter → workspace becomes read-only → navigate back to patient chart → prior encounter appears in the encounter list on the chart.

## Must-Haves

- `encounter-workspace` route variant in `RouterContext.tsx` carrying `patientId` and `encounterId`
- `EncounterWorkspace` page component (tabbed: SOAP | Vitals | ROS) registered in `ContentArea.tsx`
- `useEncounter` hook that loads an existing encounter and saves updates — mirrors `usePatient` structure
- "Start Encounter" button on `PatientDetailPage` (RBAC-gated to Provider/NurseMa/SystemAdmin) that calls `createEncounter` then navigates to `encounter-workspace`
- Encounter list on `PatientDetailPage` showing prior encounters with click-to-open navigation
- SOAP tab with four `<textarea>` sections (Subjective, Objective, Assessment, Plan), template picker `<select>`, and a "Apply Template" confirmation step
- Vitals tab with 9 numeric input fields (systolicBp, diastolicBp, heartRate, respiratoryRate, temperatureCelsius, spo2Percent, weightKg, heightCm, painScore), BMI displayed after save from server response
- ROS tab with 14-system toggle grid (Positive / Negative / Not Reviewed per system), optional per-system notes revealed when marked Positive
- Finalize Encounter action that calls `updateEncounter({ status: "finished", ... })` and switches workspace to read-only
- RBAC enforcement: NurseMa sees Vitals tab (full edit); NurseMa sees SOAP tab in read-only; BillingStaff no access to encounter workspace navigation
- All four `VitalsInput` numeric fields parsed from HTML string with empty → null conversion
- `tsc --noEmit` exits 0 after every task

## Proof Level

- This slice proves: **integration** — real Tauri commands invoked, real FHIR data persisted and round-tripped; verified in the running Tauri app
- Real runtime required: yes (Tauri app must be launched; `tsc --noEmit` alone is not sufficient)
- Human/UAT required: no (provider workflow exercised by the developer acting as Provider role)

## Verification

- `npx tsc --noEmit` exits 0 after each task (zero TypeScript errors)
- In the running Tauri app (`npm run tauri dev`):
  1. Log in as Provider → open any patient → "Start Encounter" button is visible
  2. Click "Start Encounter" → workspace opens with correct patientId in the page header
  3. Select "General Office Visit" template → SOAP sections populate → confirm modal triggers
  4. Edit Subjective text → click "Save Note" → no error; navigate back and re-open same encounter → text persists
  5. Switch to Vitals tab → enter BP 120/80, HR 72, Temp 37.0, SpO2 98, Weight 75, Height 175 → save → BMI displayed (≈24.5)
  6. Switch to ROS tab → mark "Constitutional" Positive → notes field appears → save → no error
  7. Click "Finalize Encounter" → SOAP/Vitals/ROS editing disabled; finalized badge shows
  8. Navigate back to patient chart → encounter appears in the prior encounters list
  9. Log in as FrontDesk → open patient chart → "Start Encounter" button NOT visible; `encounter-workspace` route not reachable via sidebar
  10. Log in as NurseMa → Vitals tab fully editable; SOAP tab shows read-only mode

## Observability / Diagnostics

- Runtime signals: all Tauri invoke calls wrapped in try/catch; errors surfaced inline as `<p className="text-red-600">` banners inside each tab — visible without DevTools
- Inspection surfaces:
  - React DevTools: `useEncounter` hook state (`encounter`, `vitals`, `ros`, `loading`, `error`) inspectable as component state
  - Tauri stdout: all command errors logged via `console.error("[useEncounter] …")` with context (command name + encounterID)
  - `encounter_index` SQLite table: verifiable via `DatabaseStatus` FHIR explorer for persisted records
- Failure visibility: per-tab `saveError` string state → inline red banner; `loading` boolean → spinner overlay; `mounted` guard prevents stale update warnings in console
- Redaction constraints: SOAP note free-text may contain PHI — log only IDs and error codes, never SOAP section content

## Integration Closure

- Upstream surfaces consumed:
  - `src/contexts/RouterContext.tsx` — Route union (extended)
  - `src/components/shell/ContentArea.tsx` — route dispatcher (extended)
  - `src/pages/PatientDetailPage.tsx` — chart shell (extended with encounter list + Start Encounter button)
  - `src/lib/tauri.ts` — `createEncounter`, `getEncounter`, `updateEncounter`, `listEncounters`, `recordVitals`, `listVitals`, `listTemplates`, `getTemplate`, `saveRos`, `getRos` (all already wired; consumed here for the first time in UI)
  - `src/types/documentation.ts` — `EncounterInput`, `EncounterRecord`, `UpdateEncounterInput`, `VitalsInput`, `VitalsRecord`, `ReviewOfSystemsInput`, `RosRecord`, `TemplateRecord`, `RosStatus` (all already typed; consumed here)
- New wiring introduced in this slice:
  - `encounter-workspace` route variant → `EncounterWorkspace` component (first route consuming documentation commands)
  - `useEncounter` hook → the documentation command layer (first React data-fetching hook for encounters)
  - "Start Encounter" button on `PatientDetailPage` → `createEncounter` invoke → `navigate({ page: "encounter-workspace", … })`
- What remains before the milestone is truly usable end-to-end:
  - S04: Clinical sidebar (problems, medications, allergies, immunizations) displayed alongside the patient chart
  - S05: Calendar and Flow Board (scheduling UI)
  - S06: Lab orders/results panel, document upload, physical exam form within `EncounterWorkspace`
  - S07: Settings panel, backup UI, end-to-end cleanup

## Tasks

- [x] **T01: Add `encounter-workspace` route, `useEncounter` hook, workspace shell, and encounter list on patient chart** `est:2h`
  - Why: Establishes the navigation contract (route type → component wiring), the data-fetching hook, and the entry point from the patient chart. Nothing else in S03 is reachable until this task is done.
  - Files: `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/hooks/useEncounter.ts`, `src/pages/PatientDetailPage.tsx`, `src/pages/EncounterWorkspace.tsx`
  - Do:
    1. Add `| { page: "encounter-workspace"; patientId: string; encounterId: string }` to the `Route` union in `RouterContext.tsx`. Follow the `patient-detail` variant.
    2. Add `case "encounter-workspace"` in `ContentArea.tsx` that renders `<EncounterWorkspace patientId={…} encounterId={…} role={user?.role ?? ""} userId={user?.id ?? ""} />`. Pass role and userId from `useAuth()` at the ContentArea level (same pattern as PatientDetailPage).
    3. Create `src/hooks/useEncounter.ts`. Copy `usePatient` structure exactly: `useState`, `useEffect` with mounted boolean, `refreshCounter`, `useCallback` reload. The hook takes `{ patientId: string; encounterId: string }`. It fetches `getEncounter(encounterId)`, `listVitals(patientId, encounterId)`, and `listTemplates(null)` in parallel via `Promise.all`. Returns `{ encounter, vitals, templates, loading, error, reload }`.
    4. Create `src/pages/EncounterWorkspace.tsx` — shell only: three tab buttons (SOAP | Vitals | ROS), `activeTab` state, a `<LoadingSkeleton>` while loading, an inline error banner, and a "← Back" button calling `goBack()`. Render placeholder `<div>` content per tab. No functional forms yet.
    5. In `PatientDetailPage.tsx`: add `useState` for `encounters: EncounterRecord[]` and `encountersLoading`. Add a `useEffect` that calls `commands.listEncounters(patientId, null, null, null)` on mount (mounted guard). Render an "Encounters" `<SectionCard>` listing each encounter (date, type, status) with a clickable row that calls `navigate({ page: "encounter-workspace", patientId, encounterId: enc.id })`. RBAC gate: hide the "Start Encounter" button for FrontDesk and BillingStaff. Add the "Start Encounter" button that calls `commands.createEncounter(input)` with `encounterDate: new Date().toISOString().slice(0, 19)`, `encounterType: "office_visit"`, `patientId`, `providerId: userId`, `chiefComplaint: null`, `templateId: null`, `soap: null` — then navigates to encounter-workspace. Show inline error if createEncounter fails.
  - Verify: `npx tsc --noEmit` exits 0; run `npm run tauri dev` → open a patient → "Start Encounter" button visible for Provider → clicking it opens EncounterWorkspace shell with the correct patient header and three tabs
  - Done when: `tsc --noEmit` exits 0; EncounterWorkspace shell renders with tabs for a real encounterId; prior encounters list on PatientDetailPage is populated; FrontDesk and BillingStaff cannot see "Start Encounter"

- [x] **T02: Build SOAP note tab with template picker, four-section editor, and save/finalize flow** `est:2h`
  - Why: SOAP note editing is the highest-risk UX surface (M002 proof strategy item). This task closes that risk by wiring real createEncounter/updateEncounter/listTemplates/getTemplate commands and verifying round-trip persistence.
  - Files: `src/pages/EncounterWorkspace.tsx`, `src/hooks/useEncounter.ts`
  - Do:
    1. Extend `useEncounter` to also expose: `soapState: SoapInput` (initialized from `encounter.resource` via `extractSoapSections(encounter.resource)` helper, or all-null if no note yet), `setSoapState`, `saveSoap(soap: SoapInput): Promise<void>`, `finalizeEncounter(): Promise<void>`, `isFinalized: boolean`.
    2. Add `extractSoapSections(resource: Record<string, unknown>): SoapInput` helper to `src/lib/fhirExtract.ts`. Parse `Encounter.note` annotation array: find annotations with extension URL matching each section (`subjective`/`objective`/`assessment`/`plan`) and return their text, null for missing.
    3. In `EncounterWorkspace.tsx` SOAP tab: render a `<select>` dropdown populated from `templates`. When a template is selected and the SOAP state has any non-null section, show an inline confirmation banner: "Apply template? This will replace your current note." with "Apply" and "Cancel" buttons. On Apply: call `commands.getTemplate(templateId)` → set `soapState` from `TemplateRecord.defaultSoap`. No window.confirm — use a React state flag `pendingTemplateId`.
    4. Render four labeled `<textarea>` fields for Subjective, Objective, Assessment, Plan using `INPUT_CLS` pattern. Each `onChange` updates `soapState` in local state.
    5. Render a "Save Note" button that calls `saveSoap(soapState)` → calls `commands.updateEncounter(encounterId, { soap: soapState, status: null, chiefComplaint: null })` → reload. Show save spinner (disable button during save) and inline `saveError`.
    6. Render a "Finalize Encounter" button that calls `finalizeEncounter()` → calls `commands.updateEncounter(encounterId, { soap: soapState, status: "finished", chiefComplaint: null })` → sets `isFinalized: true`. When finalized: all textareas become `readOnly`, buttons disabled, show a "Finalized" badge.
    7. RBAC: if `role === "NurseMa"` — SOAP tab textareas are `readOnly`; "Save Note" and "Finalize" buttons hidden. If `role === "BillingStaff"` — EncounterWorkspace should not be reachable (guarded by route in ContentArea), but add a defensive read-only render.
  - Verify: `npx tsc --noEmit` exits 0; in running app: select template → confirmation fires → apply → SOAP sections populate → edit text → save → navigate away → re-open → text persists; finalize → textareas go read-only
  - Done when: `tsc --noEmit` exits 0; SOAP round-trip (create → update → reload shows saved text) verified in running app; template pre-population confirmed; finalization confirmed

- [x] **T03: Build Vitals tab with 9 numeric fields, BMI display after save, and `recordVitals` wiring** `est:1.5h`
  - Why: Vitals recording is CLIN-02 scope and uses a backend-computed BMI that must not be calculated client-side. This task wires `recordVitals` and verifies the server-returned BMI display.
  - Files: `src/pages/EncounterWorkspace.tsx`, `src/hooks/useEncounter.ts`
  - Do:
    1. Extend `useEncounter` to expose: `latestVitals: VitalsRecord | null` (first item from `vitals` list, or null), `saveVitals(input: VitalsInput): Promise<void>`.
    2. In `EncounterWorkspace.tsx` Vitals tab: render a two-column grid of `<input type="number">` fields for all 9 vital signs — systolicBp, diastolicBp, heartRate, respiratoryRate, temperatureCelsius, spo2Percent, weightKg, heightCm, painScore — plus a `<textarea>` for vitalsNotes. Include unit labels (mmHg, bpm, breaths/min, °C, %, kg, cm, 0–10 NRS).
    3. Initialize form state from `latestVitals` if present (convert VitalsRecord resource fields back to input values), otherwise all empty strings.
    4. Parse all numeric inputs strictly: `value === "" ? null : parseFloat(value)`. Never assign a string to a `number | null` field. Pain score: clamp to 0–10 via `Math.min(10, Math.max(0, parsed))` on save — the backend also clamps, but clamp in UI too.
    5. "Save Vitals" button: assemble `VitalsInput` with `patientId`, `encounterId`, `recordedAt: new Date().toISOString().slice(0, 19)`, and all parsed fields. Call `saveVitals(input)` → `commands.recordVitals(input)` → `reload()`. Show save spinner and inline `vitalsError`.
    6. After save/reload, display BMI from `latestVitals?.bmi` as a read-only labeled value: "BMI: {bmi?.toFixed(1)} kg/m²" (or "—" if null). Do NOT compute BMI from the form fields.
    7. RBAC: NurseMa sees full Vitals tab (all fields editable, Save Vitals available). If `isFinalized`, all fields `readOnly` and Save hidden.
  - Verify: `npx tsc --noEmit` exits 0; in running app: enter weight 75 kg, height 175 cm → save → BMI displays ≈24.5 (from server); vitals record persists across workspace reload
  - Done when: `tsc --noEmit` exits 0; BMI displays server-returned value (not client-computed) after save; all 9 fields save and reload correctly in running app

- [x] **T04: Build ROS tab with 14-system toggle grid and `saveRos`/`getRos` wiring** `est:1.5h`
  - Why: 14-system ROS is the highest-density form (CLIN-03 scope). This task proves the toggle-grid UX is usable and wires `saveRos`/`getRos` with both required params.
  - Files: `src/pages/EncounterWorkspace.tsx`, `src/hooks/useEncounter.ts`
  - Do:
    1. Extend `useEncounter` to fetch `getRos(encounterId, patientId)` in the `Promise.all` block (add to the initial fetch). Expose: `rosRecord: RosRecord | null`, `saveRos(input: ReviewOfSystemsInput): Promise<void>`. **Always pass both `encounterId` and `patientId` to `getRos`** — passing only one causes silent null return.
    2. Define the 14 system definitions as a `const` array in `EncounterWorkspace.tsx`: `[{ key: "constitutional", label: "Constitutional" }, { key: "eyes", label: "Eyes" }, … { key: "allergicImmunologic", label: "Allergic / Immunologic" }]` for all 14 systems.
    3. Initialize `rosState` from `rosRecord?.resource` if present (parse QuestionnaireResponse items), otherwise all fields `null`. All 28 fields (14 × status + 14 × notes) initialized to `null` — never `undefined`.
    4. Render the 14 systems as a compact toggle grid. Each row: system label + three inline radio buttons (Positive / Negative / Not Reviewed). When status is set to "positive", reveal a one-line `<input type="text">` for the system's notes field. When status is null or "not_reviewed", hide the notes input and set that system's notes to null.
    5. "Save ROS" button: assemble `ReviewOfSystemsInput` with all 30 fields (`patientId`, `encounterId`, plus all 28 status/notes fields). Call `saveRos(input)` → `commands.saveRos(input)` → reload. Show save spinner and inline `rosError`.
    6. After reload, if `rosRecord` is present, re-initialize `rosState` from saved data so the toggle states reflect what was persisted.
    7. If `isFinalized`: all radio buttons `disabled`, notes inputs `readOnly`, Save hidden.
  - Verify: `npx tsc --noEmit` exits 0; in running app: mark Constitutional as Positive → notes field appears → enter text → save → reload workspace → Constitutional shows Positive with saved notes; mark Respiratory as Negative → save → persists; finalize → all toggles disabled
  - Done when: `tsc --noEmit` exits 0; full ROS round-trip (save → reload → values restored from backend) verified in running app; `getRos` called with both `encounterId` and `patientId` (confirmed via console log on first load showing non-null rosRecord after second save)

## Files Likely Touched

- `src/contexts/RouterContext.tsx` — add `encounter-workspace` route variant
- `src/components/shell/ContentArea.tsx` — add `case "encounter-workspace"` + userId prop pass
- `src/pages/EncounterWorkspace.tsx` — new file: tabbed workspace (SOAP | Vitals | ROS)
- `src/hooks/useEncounter.ts` — new file: data-fetching hook for encounter + vitals + ROS + templates
- `src/pages/PatientDetailPage.tsx` — add encounter list, Start Encounter button, RBAC gate
- `src/lib/fhirExtract.ts` — add `extractSoapSections` helper
- `src/types/documentation.ts` — read-only (all types already defined; no changes expected)
- `src/lib/tauri.ts` — read-only (all wrappers already defined; no changes)
