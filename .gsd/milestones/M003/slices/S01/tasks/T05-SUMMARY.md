---
id: T05
parent: S01
milestone: M003
provides:
  - src/pages/PTNotesPage.tsx — PT note list page with type grouping, status badges, loading/error states, and new-note navigation
  - src/pages/PTNoteFormPage.tsx — note form page for all three note types with full draft → signed → locked lifecycle
  - src/contexts/RouterContext.tsx — two new route variants (pt-notes, pt-note-detail)
  - src/components/shell/ContentArea.tsx — route cases wired for both new pages
  - src/pages/PatientDetailPage.tsx — "PT Notes" button added to action area (Provider/SystemAdmin only)
key_files:
  - src/pages/PTNotesPage.tsx
  - src/pages/PTNoteFormPage.tsx
  - src/contexts/RouterContext.tsx
  - src/components/shell/ContentArea.tsx
  - src/pages/PatientDetailPage.tsx
key_decisions:
  - No new decisions — all patterns follow existing conventions already established in DECISIONS.md
patterns_established:
  - PTNotesPage uses same loading/error/retry pattern as PatientDetailPage (useState + useEffect + setRefreshKey trigger)
  - PTNoteFormPage uses applyRecord() helper to normalize PtNoteRecord into typed field state for all three note types
  - Fields extracted from record.resource["fields"] blob (serde tag+content encoding) using a generic Record<string, string | null> extractor
  - Inline success/error feedback on every action with setTimeout clear for success messages (matches EncounterWorkspace pattern)
  - Role guard in PTNotesPage is an early-return "Access denied" message — same pattern as EncounterWorkspace RBAC gates
observability_surfaces:
  - PTNotesPage loading/error banner surfaces listPtNotes failures inline with Retry button
  - PTNoteFormPage inline error banners for save/cosign/lock failures; console.error logs with note ID and patient ID for every backend failure
  - Status badges on PTNotesPage give at-a-glance lifecycle state (draft=gray, signed=blue, locked=green)
  - Backend audit rows written by T03 commands: pt_note.create, pt_note.cosign, pt_note.lock — queryable via `SELECT * FROM audit_logs WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC;`
  - Locked note's read-only state is an explicit UI signal (all inputs disabled + green locked banner)
duration: ~30 min
verification_result: passed
completed_at: 2026-03-13
blocker_discovered: false
---

# T05: Add PT Notes page, route wiring, and note form shells

**Added PTNotesPage + PTNoteFormPage with full draft → signed → locked lifecycle wired to real backend commands, new route variants in RouterContext, route dispatch in ContentArea, and "PT Notes" navigation from PatientDetailPage.**

## What Happened

Step 1 extended `RouterContext.tsx` with two new route variants: `{ page: "pt-notes"; patientId: string }` and `{ page: "pt-note-detail"; patientId: string; noteType: PtNoteType; ptNoteId: string }`. `PtNoteType` is imported from `../types/pt`. The existing `default: never` exhaustiveness guard in ContentArea continues to compile correctly.

Step 2 created `PTNotesPage.tsx`: a Provider/SystemAdmin-only page that loads notes via `commands.listPtNotes(patientId)` on mount, groups them into three collapsible sections (Initial Evaluations, Progress Notes, Discharge Summaries), shows status badges (draft=gray, signed=blue, locked=green), and provides "New IE", "New Progress Note", and "New Discharge Summary" buttons that navigate to the form page with `ptNoteId: "new"`. Non-Provider/SystemAdmin roles see an "Access denied" message with a back button. Loading spinner and error banner with Retry are present.

Step 3 created `PTNoteFormPage.tsx`: the form page for all three note types. For existing notes it loads via `commands.getPtNote()` on mount and populates field state. It renders the correct field sub-component based on `noteType` (InitialEvalForm, ProgressNoteForm, DischargeSummaryForm). `ProgressNoteForm` uses a `<select>` for `hepCompliance`. `DischargeSummaryForm` renders `outcomeComparisonPlaceholder` as an amber informational banner (not a field) per the plan. Locked notes disable all inputs and show a green locked banner; no action buttons are shown. Draft notes show "Save Draft" and (if existing) "Co-sign Note". Signed notes show "Lock Note". Each action refreshes local state via `applyRecord()`.

Step 4 added imports and two new cases to `ContentArea.tsx`'s switch, dispatching `pt-notes` to `PTNotesPage` and `pt-note-detail` to `PTNoteFormPage`, both receiving `user?.role ?? ""`.

Step 5 added a "PT Notes" button in the `PatientDetailPage` action area (alongside "Start Encounter" and "Edit"), visible only when `role === "Provider" || role === "SystemAdmin"`, navigating to `{ page: "pt-notes", patientId }`.

One cleanup: removed an unused `noteTypeLabel` helper from `PTNotesPage.tsx` that caused a TS6133 error after initial write.

## Verification

```
$ npx tsc --noEmit 2>&1 | tail -5
(no output — exits 0)

$ cd src-tauri && cargo test --lib 2>&1 | tail -5
test result: ok. 272 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
```

TypeScript exhaustiveness check compiles with the `default: never` guard covering all eight route variants. No `any` types in new files.

Runtime verification (manual, required for S01 demo):
1. Navigate to Patients → select a patient → "PT Notes" button visible (Provider role) → PTNotesPage renders with empty sections.
2. "New IE" → PTNoteFormPage renders with 14 IE fields.
3. Fill "Chief Complaint", click "Save Draft" → note saved, success banner, note appears in list with "draft" badge on return.
4. Open note → "Co-sign Note" button visible → click → status badge becomes "signed".
5. "Lock Note" visible → click → status becomes "locked" → all fields disabled, green locked banner shown.
6. Repeat for "New Progress Note" (with hepCompliance select) and "New Discharge Summary" (with outcome comparison info banner).
7. Log in as FrontDesk → "PT Notes" button hidden on PatientDetailPage (role gate works).

## Diagnostics

- **PT note list**: Navigate to Patients → [patient] → PT Notes in running app, or `SELECT * FROM pt_note_index ORDER BY created_at DESC LIMIT 20;`
- **Audit trail**: `SELECT * FROM audit_logs WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC LIMIT 20;`
- **Frontend errors**: `console.error("[PTNotesPage] listPtNotes failed …")` / `"[PTNoteFormPage] save failed …"` — each includes note ID and patient ID
- **Locked state**: all inputs have `disabled` attribute + green banner; inspectable via React DevTools (`status === "locked"` on PTNoteFormPage component state)

## Deviations

- Removed unused `noteTypeLabel` function from PTNotesPage after tsc caught TS6133. Not a plan deviation — the function was written speculatively and not used.
- `outcomeComparisonPlaceholder` rendered as an amber informational panel instead of a read-only textarea field, because the plan says "renders as a read-only informational section" — implemented as a styled info box matching the intent more clearly than a disabled textarea.

## Known Issues

None.

## Files Created/Modified

- `src/contexts/RouterContext.tsx` — added `PtNoteType` import and two new route variants
- `src/pages/PTNotesPage.tsx` — new: PT note list page
- `src/pages/PTNoteFormPage.tsx` — new: note form page for all three note types
- `src/components/shell/ContentArea.tsx` — added imports and route cases for both new pages
- `src/pages/PatientDetailPage.tsx` — added "PT Notes" navigation button (Provider/SystemAdmin only)
