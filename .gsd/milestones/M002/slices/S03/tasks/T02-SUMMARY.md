---
id: T02
parent: S03
milestone: M002
provides:
  - extractSoapSections FHIR helper in fhirExtract.ts (parses Encounter.resource.note annotations into SoapSections)
  - useEncounter extended with soapState, setSoapState, saveSoap, finalizeEncounter, isFinalized
  - SoapTab component in EncounterWorkspace with template picker, four textareas, save/finalize flow, RBAC read-only
key_files:
  - src/lib/fhirExtract.ts
  - src/hooks/useEncounter.ts
  - src/pages/EncounterWorkspace.tsx
key_decisions:
  - soapSeededForId guards soapState re-seeding: only seeds on encounter ID change (initial load or new encounter), not on every reload/save, preventing overwrite of in-progress edits
  - isFinalized set optimistically on finalizeEncounter success (before reload) so UI transitions immediately without async lag
  - Template apply-to-empty is immediate; apply-to-non-empty shows inline amber confirmation banner (no modal)
  - Template picker select resets to "" after onChange so same template can be re-selected if needed
  - SoapTab extracted as a named inner component (not inline JSX in the tab body) for readability and to isolate all SOAP-specific state
  - void operator used on async handler calls in JSX onClick to satisfy noUnusedLocals/TypeScript strict mode without wrapping in arrow functions
patterns_established:
  - FHIR annotation SOAP sections stored as note[] items with extension[{url: SOAP_SECTION_URL, valueCode: section}] — extractSoapSections is the canonical reader
  - useEncounter hook returns soapState + setSoapState as controlled form bindings; actions (saveSoap, finalizeEncounter) are stable callbacks via useCallback
  - Per-tab save state pattern: savingSoap (bool), soapSaveError (string|null) as local tab state, not in the hook
observability_surfaces:
  - React DevTools → EncounterWorkspace: soapState, savingSoap, soapSaveError, finalizing, finalizeError, isFinalized, pendingTemplateId all visible as component state
  - console.error("[EncounterWorkspace] saveSoap failed:") + "[EncounterWorkspace] finalizeEncounter failed:" with error message (no PHI — only error string from Rust)
  - Inline red <p className="text-red-600"> for soapSaveError and finalizeError visible without DevTools
  - "Saving…" / "Finalizing…" spinner text on buttons provides visual feedback during async operations
  - Green "✓ Finalized" badge in both header and tab body when isFinalized
duration: 1 session
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Build SOAP note tab with template picker, four-section editor, and save/finalize flow

**Added `extractSoapSections` FHIR helper, extended `useEncounter` with SOAP save/finalize actions, and built the full functional SOAP tab with template picker, four textareas, RBAC enforcement, and finalization flow.**

## What Happened

**Step 1 — `extractSoapSections` in `fhirExtract.ts`:**
Added `SoapSections` interface and `extractSoapSections(resource)` function. Parses `resource["note"]` as FHIR Annotation array; for each item looks for an extension with `url === "http://medarc.local/fhir/ext/soap-section"` and reads `valueCode` to identify the section (subjective/objective/assessment/plan), then maps the annotation `text` to the appropriate field. Returns all-null `SoapSections` on absent/malformed input. No throws.

**Step 2 — Extended `useEncounter`:**
Added to hook return type:
- `soapState: SoapInput` — initialized from `extractSoapSections(encounter.resource)` via a `useEffect` guarded by `soapSeededForId` (re-seeds only when encounter ID changes, not on every reload)
- `setSoapState` — direct setter for controlled textarea bindings
- `saveSoap` — `updateEncounter({ soap, status: null, chiefComplaint: null })` then `reload()`
- `finalizeEncounter` — `updateEncounter({ soap, status: "finished", chiefComplaint: null })` then optimistic `setIsFinalized(true)` then `reload()`
- `isFinalized` — derived from `encounter.resource["status"] === "finished"` on seed + optimistic local flag

**Step 3 — `SoapTab` component in `EncounterWorkspace.tsx`:**
Replaced the "SOAP note editor — T02" placeholder with a fully functional `SoapTab` named component:
- Template picker: `<select>` with all templates; empty SOAP → immediate apply; non-empty SOAP → amber confirmation banner with Apply/Cancel
- Four `<textarea>` fields (Subjective, Objective, Assessment, Plan), each `rows={5}`, using `INPUT_CLS`/`LABEL_CLS`, fully controlled via `soapState`
- "Save Note" button: shows "Saving…" spinner, catches error → `soapSaveError` inline red banner
- "Finalize Encounter" button: amber border destructive styling, shows "Finalizing…", catches error → `finalizeError` inline red banner
- RBAC: `isReadOnly = isFinalized || role === "NurseMa" || role === "BillingStaff"` drives `readOnly` on all textareas, hides both buttons, shows role notice for NurseMa/BillingStaff, green finalized badge when isFinalized
- Header gets "✓ Finalized" badge pill when finalized

## Verification

- `tsc --noEmit` → **exit 0, zero errors** (run via bg_shell with TSC_EXIT:0 confirmed)
- Must-haves checklist:
  - [x] `extractSoapSections` parses `Encounter.resource.note` and returns `SoapInput` with null for missing sections
  - [x] `soapState` initializes from `encounter.resource` on load (guarded by soapSeededForId)
  - [x] Template picker shows all templates; applying pre-populates all four SOAP sections
  - [x] Applying to non-empty note shows confirmation banner first
  - [x] "Save Note" calls `updateEncounter` (not createEncounter); error shown inline; spinner shown
  - [x] "Finalize Encounter" calls `updateEncounter({ status: "finished", … })`; workspace becomes read-only
  - [x] NurseMa sees SOAP tab in read-only mode; Provider/SystemAdmin can edit and save

## Diagnostics

- **soapSaveError**: inline red `<p>` below textarea block — visible without DevTools; console.error with context logged
- **soapState content**: React DevTools → `EncounterWorkspace` → component state → `soapState.subjective`, etc.
- **isFinalized**: inspectable in React DevTools; also visible in UI via green badge
- **Rust errors**: serialize to the inline banner via the Error.message chain in catch handlers
- **useEncounter failures**: `[useEncounter] fetchAll failed for <encounterId>:` in Tauri stdout (unchanged from T01)

## Deviations

- `SoapSections` (returned by `extractSoapSections`) is a distinct interface from `SoapInput` (used by `updateEncounter`) to maintain clean type boundaries — `SoapSections` is the FHIR extraction output type; `SoapInput` is the Tauri command input type. The hook converts between them when seeding `soapState`.
- `extractSoapSections` was made to return `SoapSections` (with `string | null` fields matching `SoapInput`) rather than `SoapInput` directly, to keep FHIR extraction concerns separate from command input types.

## Known Issues

None.

## Files Created/Modified

- `src/lib/fhirExtract.ts` — added `SoapSections` interface and `extractSoapSections(resource)` FHIR helper
- `src/hooks/useEncounter.ts` — extended with `soapState`, `setSoapState`, `saveSoap`, `finalizeEncounter`, `isFinalized` and associated seeding logic
- `src/pages/EncounterWorkspace.tsx` — SOAP tab fully functional: extracted `SoapTab` component with template picker, four textareas, save/finalize flow, RBAC read-only mode, finalized badge
