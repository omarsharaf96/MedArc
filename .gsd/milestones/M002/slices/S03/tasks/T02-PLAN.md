---
estimated_steps: 7
estimated_files: 3
---

# T02: Build SOAP note tab with template picker, four-section editor, and save/finalize flow

**Slice:** S03 — Clinical Encounter Workspace
**Milestone:** M002

## Description

This task builds the clinical core of the encounter workspace: the SOAP note editing surface. It adds an `extractSoapSections` FHIR helper to re-hydrate a saved note from `EncounterRecord.resource`, extends `useEncounter` with SOAP save and finalize actions, and builds the full SOAP tab UI: a template `<select>` with confirmation step, four `<textarea>` sections, a "Save Note" button wired to `updateEncounter`, and a "Finalize Encounter" action that transitions status to `"finished"` and locks the workspace.

This task retires the highest-risk UX surface identified in the M002 proof strategy.

## Steps

1. **Add `extractSoapSections` to `src/lib/fhirExtract.ts`**: Function signature `extractSoapSections(resource: Record<string, unknown>): SoapInput`. Parse `resource["note"]` as `Array<Record<string, unknown>>`. For each annotation item, check its `extension` array for `url` values matching the MedArc section URLs (e.g. `"http://medarc.local/fhir/ext/soap-section"` with valueCode of `subjective`/`objective`/`assessment`/`plan`). Extract the annotation `text` field. Return `{ subjective, objective, assessment, plan }` with each section as `string | null`. If `resource["note"]` is absent or empty, return all-null `SoapInput`.

2. **Extend `useEncounter`**: Add to the hook's return type:
   - `soapState: SoapInput` — initialized from `extractSoapSections(encounter.resource)` when encounter first loads; kept in separate `useState` to allow uncommitted edits
   - `setSoapState: (s: SoapInput) => void`
   - `saveSoap: (soap: SoapInput) => Promise<void>` — calls `commands.updateEncounter(encounterId, { soap, status: null, chiefComplaint: null })` then `reload()`
   - `finalizeEncounter: (soap: SoapInput) => Promise<void>` — calls `commands.updateEncounter(encounterId, { soap, status: "finished", chiefComplaint: null })` then sets `isFinalized: true`
   - `isFinalized: boolean` — derived from `encounter?.resource["status"] === "finished"` on load, plus local state flag that gets set immediately on finalize (optimistic)
   - When `reload()` re-fetches the encounter, re-initialize `soapState` from the fresh `encounter.resource` via `extractSoapSections`. Use a `useEffect` that watches `encounter` and sets `soapState` — guarded so it only runs when encounter ID matches (prevent overwriting in-progress edits on unrelated reloads).

3. **Build SOAP tab in `EncounterWorkspace.tsx`**: Replace the placeholder "SOAP note editor — T02" with real content:
   - Template picker: `<select>` with options from `templates` array. Show "— Select template —" as default (empty value). Wire `onChange` to set `pendingTemplateId` state.
   - Template confirmation banner: when `pendingTemplateId !== null` and any `soapState` section is non-empty, render an inline banner: "Apply '{templateName}'? This will replace your current note." with "Apply" and "Cancel" buttons. On Apply: call `commands.getTemplate(pendingTemplateId)` → set `soapState` from `TemplateRecord.defaultSoap` → clear `pendingTemplateId`. On Cancel: clear `pendingTemplateId` only. If all SOAP sections are null/empty, apply immediately without confirmation.

4. **Four section `<textarea>` fields**: Render in order — Subjective, Objective, Assessment, Plan. Each: `<textarea>` with `value={soapState.section ?? ""}`, `onChange` updating `soapState` via `setSoapState`. Use `INPUT_CLS` from PatientFormModal pattern. Minimum height: `rows={5}`. Section label uses `LABEL_CLS` styling.

5. **Save Note button**: Disabled while `savingSoap` is true. On click: set `savingSoap: true`, call `saveSoap(soapState)`, catch error → set `soapSaveError: string | null`, finally set `savingSoap: false`. Render inline `<p className="text-red-600">` for `soapSaveError`. Show "Saving…" text on button while in progress.

6. **Finalize Encounter button**: Separate from Save — labeled "Finalize Encounter" with a destructive-action visual style (amber/orange border). On click: call `finalizeEncounter(soapState)`. On success: workspace UI transitions — all textareas get `readOnly={true}`, "Save Note" and "Finalize" buttons hidden, a green "✓ Finalized" badge appears in the header. `isFinalized` state drives this.

7. **RBAC enforcement in SOAP tab**:
   - `role === "NurseMa"`: all textareas `readOnly={true}`, "Save Note" and "Finalize" buttons not rendered, template picker disabled. Show a subtle "Read-only for your role" label.
   - `role === "BillingStaff"`: EncounterWorkspace is not routable to BillingStaff (guarded in ContentArea), but add defensive read-only render as fallback.
   - When `isFinalized`: all textareas `readOnly={true}`, "Save Note" and "Finalize" buttons not rendered.

## Must-Haves

- [ ] `extractSoapSections` parses `Encounter.resource.note` correctly and returns `SoapInput` with null for missing sections
- [ ] `soapState` initializes from `encounter.resource` on load (round-trips saved content)
- [ ] Template picker shows all templates from `listTemplates`; applying a template pre-populates all four SOAP sections
- [ ] Applying a template to a non-empty note shows confirmation banner first (no silent overwrite)
- [ ] "Save Note" calls `updateEncounter` (not `createEncounter`); error shown inline; save spinner shown
- [ ] "Finalize Encounter" calls `updateEncounter({ status: "finished", … })`; workspace becomes read-only after finalize
- [ ] NurseMa sees SOAP tab in read-only mode; Provider and SystemAdmin can edit and save
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` — must exit 0 with zero errors
- In running Tauri app:
  1. Open encounter workspace as Provider → SOAP tab shows four empty textareas
  2. Select "General Office Visit" template from dropdown → if SOAP is empty, note pre-populates immediately; if non-empty, confirmation banner appears first
  3. Edit Subjective text → click "Save Note" → button shows "Saving…" → resolves → navigate back to patient chart → re-open same encounter → Subjective text persists
  4. Click "Finalize Encounter" → all textareas become `readOnly`, buttons disappear, "✓ Finalized" badge appears
  5. Log in as NurseMa → open same encounter → SOAP tab textareas are `readOnly`; no Save/Finalize buttons visible

## Observability Impact

- Signals added/changed: `soapSaveError` inline banner surfaces Rust errors (e.g. permission denied, DB error) directly in the UI; `savingSoap` spinner provides visual feedback for async latency
- How a future agent inspects this: React DevTools → `EncounterWorkspace` component → `soapState`, `savingSoap`, `soapSaveError`, `isFinalized` all visible as component state; `encounter.resource.note` inspectable via FHIR explorer tab in DatabaseStatus
- Failure state exposed: if `updateEncounter` fails, error message from Rust AppError serializes to the inline banner; `isFinalized` only sets true on resolved promise (not optimistic for save failures)

## Inputs

- `src/hooks/useEncounter.ts` (from T01) — hook to extend with `soapState`, `saveSoap`, `finalizeEncounter`, `isFinalized`
- `src/pages/EncounterWorkspace.tsx` (from T01) — shell to fill with SOAP tab content
- `src/lib/fhirExtract.ts` — existing FHIR helper file; add `extractSoapSections`
- `src/lib/tauri.ts` — `commands.updateEncounter`, `commands.getTemplate` already wired
- `src/types/documentation.ts` — `UpdateEncounterInput`, `SoapInput`, `TemplateRecord` already typed
- `src/components/patient/PatientFormModal.tsx` — reference for `INPUT_CLS`, `LABEL_CLS`, `FormField` pattern

## Expected Output

- `src/lib/fhirExtract.ts` — `extractSoapSections(resource)` helper added
- `src/hooks/useEncounter.ts` — extended with `soapState`, `setSoapState`, `saveSoap`, `finalizeEncounter`, `isFinalized`
- `src/pages/EncounterWorkspace.tsx` — SOAP tab fully functional: template picker, four textareas, save, finalize, RBAC read-only mode
