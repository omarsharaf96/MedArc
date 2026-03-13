---
estimated_steps: 5
estimated_files: 7
---

# T05: Add PT Notes page, route wiring, and note form shells

**Slice:** S01 — Touch ID Fix + PT Note Templates
**Milestone:** M003

## Description

T04 produced the TypeScript contract layer. This task builds the visible product: a PT Notes list page, a note form page for all three note types, route wiring, and a "PT Notes" link in the existing patient detail view. After T05, a provider can navigate to a patient's PT notes, create notes of all three types, co-sign them, and lock them — proving the full draft → signed → locked lifecycle works end-to-end in the app.

The UI deliberately follows existing patterns: state hooks with try/catch, status badges from Tailwind, same role-guard pattern used in `EncounterWorkspace`. No new libraries. No component library added.

## Steps

1. **Extend `RouterContext.tsx` with two new route variants**:
   Add to the `Route` union type:
   ```ts
   | { page: "pt-notes"; patientId: string }
   | { page: "pt-note-detail"; patientId: string; noteType: PtNoteType; ptNoteId: string }
   ```
   Import `PtNoteType` from `../types/pt`. The `ptNoteId` is either a real UUID (existing note) or the sentinel `"new"` (new note, create on first save).

2. **Create `src/pages/PTNotesPage.tsx`** — Provider-only list page:
   - Props: `patientId: string; role: string`.
   - If `role !== "Provider" && role !== "SystemAdmin"`: render a plain "Access denied" message (no access to PT notes for other roles).
   - On mount: call `commands.listPtNotes(patientId)` and store in local state; show a loading spinner while fetching; show an error banner on failure.
   - Render three collapsible sections (Initial Evaluations, Progress Notes, Discharge Summaries), each listing notes with: created date, status badge (draft = gray, signed = blue, locked = green), and a row click to navigate to `pt-note-detail`.
   - Header row has three "New" buttons: "New IE", "New Progress Note", "New Discharge Summary" — each navigates to `{ page: "pt-note-detail", patientId, noteType, ptNoteId: "new" }`.
   - "← Back to Patient" button calls `goBack()`.

3. **Create `src/pages/PTNoteFormPage.tsx`** — note editing/viewing form:
   - Props: `patientId: string; noteType: PtNoteType; ptNoteId: string; role: string`.
   - If `ptNoteId !== "new"`: on mount, call `commands.getPtNote(ptNoteId)` to load existing note.
   - Render correct field set based on `noteType`:
     - `"initial_eval"`: render a `<form>` with labeled `<textarea>` inputs for each `InitialEvalFields` key (chiefComplaint, mechanismOfInjury, priorLevelOfFunction, painNrs, functionalLimitations, icd10Codes, physicalExamFindings, shortTermGoals, longTermGoals, planOfCare, frequencyDuration, cptCodes, referringPhysician, referralDocumentId).
     - `"progress_note"`: render labeled inputs for each `ProgressNoteFields` key. `hepCompliance` uses a `<select>` with options `yes`, `no`, `partial`.
     - `"discharge_summary"`: render labeled inputs for each `DischargeSummaryFields` key. `outcomeComparisonPlaceholder` renders as a read-only informational section ("Outcome comparison will be available after S02").
   - If note status is `"locked"`: all inputs are `readOnly`; no Save/Co-sign/Lock buttons shown. A banner reads "This note is locked and cannot be edited."
   - If status is `"draft"` or is a new note: "Save Draft" button calls `createPtNote` (new) or `updatePtNote` (existing), then refreshes the note record. Show inline success/error feedback.
   - If status is `"draft"` and `ptNoteId !== "new"`: "Co-sign Note" button calls `cosignPtNote`, refreshes, updates local status to `"signed"`.
   - If status is `"signed"`: "Lock Note" button calls `lockPtNote`, refreshes, updates local status to `"locked"`.
   - "← Back" calls `goBack()`.

4. **Wire routes in `src/components/shell/ContentArea.tsx`**:
   Import `PTNotesPage` and `PTNoteFormPage`. Add cases to the switch:
   ```tsx
   case "pt-notes":
     return <PTNotesPage patientId={currentRoute.patientId} role={user?.role ?? ""} />;
   case "pt-note-detail":
     return (
       <PTNoteFormPage
         patientId={currentRoute.patientId}
         noteType={currentRoute.noteType}
         ptNoteId={currentRoute.ptNoteId}
         role={user?.role ?? ""}
       />
     );
   ```
   TypeScript exhaustiveness check must still compile — the `default: never` guard now covers the new routes.

5. **Add "PT Notes" link in `src/pages/PatientDetailPage.tsx`**:
   Add a button or tab in the existing patient detail header / action area that navigates to `{ page: "pt-notes", patientId }`. Follow the existing "Start Encounter" button pattern. Show only when `role === "Provider" || role === "SystemAdmin"`.

## Must-Haves

- [ ] Two new `Route` variants added to `RouterContext.tsx`; `tsc --noEmit` still passes with the `default: never` exhaustiveness guard
- [ ] `PTNotesPage.tsx` renders "Access denied" for non-Provider/SystemAdmin roles
- [ ] `PTNotesPage.tsx` lists notes grouped by type with status badges; loading and error states present
- [ ] "New IE", "New Progress Note", "New Discharge Summary" buttons navigate to correct route with `ptNoteId: "new"`
- [ ] `PTNoteFormPage.tsx` renders the correct field set for each `noteType`
- [ ] `PTNoteFormPage.tsx` shows read-only view with locked banner when status is `"locked"`
- [ ] "Save Draft" calls `createPtNote` (new) or `updatePtNote` (existing) and refreshes
- [ ] "Co-sign Note" button only visible when status is `"draft"` and `ptNoteId !== "new"`; calls `cosignPtNote`
- [ ] "Lock Note" button only visible when status is `"signed"`; calls `lockPtNote`
- [ ] `ContentArea.tsx` has cases for both new routes
- [ ] `PatientDetailPage.tsx` has "PT Notes" link that navigates to `pt-notes` route
- [ ] No `any` types in new files
- [ ] `tsc --noEmit` exits 0

## Verification

```bash
npx tsc --noEmit 2>&1 | tail -5
```
Expected: exits 0.

Runtime verification in Tauri dev app (manual, required for S01 demo):
1. Navigate to Patients → select a patient → click "PT Notes" → `PTNotesPage` renders (empty list for new patient).
2. Click "New IE" → `PTNoteFormPage` renders with IE fields.
3. Fill in "Chief Complaint" and click "Save Draft" → note appears in list with status "draft".
4. Click on note → "Co-sign Note" button visible → click → status becomes "signed".
5. "Lock Note" button now visible → click → status becomes "locked" → all fields read-only, locked banner shown.
6. Repeat steps 2–4 for "New Progress Note" and "New Discharge Summary".
7. Confirm non-Provider role (e.g. FrontDesk) sees "Access denied" on PTNotesPage.

## Observability Impact

- Signals added/changed: Every Save/Co-sign/Lock action in the UI triggers the corresponding backend audit row (from T03). The PT Notes list shows current status for every note — a provider can see at a glance which notes are draft/signed/locked without inspecting the DB.
- How a future agent inspects this: Navigate to a patient's PT Notes page in the running app. Alternatively, query `pt_note_index` directly or check the Audit Log page for `pt_note.*` entries.
- Failure state exposed: Loading/error states in `PTNotesPage` surface backend failures. Save/co-sign/lock buttons show inline error messages on failure (e.g. "Cannot co-sign: note is already signed"). Locked note's read-only state is an explicit UI signal.

## Inputs

- `src/types/pt.ts` — PT note types from T04
- `src/lib/tauri.ts` — 6 command wrappers from T04
- `src/contexts/RouterContext.tsx` — existing Route union to extend
- `src/components/shell/ContentArea.tsx` — existing switch to extend
- `src/pages/PatientDetailPage.tsx` — existing patient detail to add PT Notes link
- `src/pages/EncounterWorkspace.tsx` — reference for role guard and form patterns
- `src/hooks/useAuth.ts` — reference for `user.role` access pattern

## Expected Output

- `src/contexts/RouterContext.tsx` — two new route variants added
- `src/pages/PTNotesPage.tsx` (new) — list page with type grouping, status badges, and new-note buttons
- `src/pages/PTNoteFormPage.tsx` (new) — form page for all three note types with draft/sign/lock workflow
- `src/components/shell/ContentArea.tsx` — two new route cases
- `src/pages/PatientDetailPage.tsx` — "PT Notes" navigation link added
