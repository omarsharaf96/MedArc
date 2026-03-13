---
estimated_steps: 5
estimated_files: 5
---

# T01: Add `encounter-workspace` route, `useEncounter` hook, workspace shell, and encounter list on patient chart

**Slice:** S03 — Clinical Encounter Workspace
**Milestone:** M002

## Description

This task establishes all the navigational and data-layer scaffolding that the rest of S03 depends on. It extends the Route union in RouterContext to carry `patientId` and `encounterId`, wires the new `EncounterWorkspace` page component in ContentArea, creates the `useEncounter` data-fetching hook (mirroring `usePatient`), adds the encounter list and "Start Encounter" button to `PatientDetailPage`, and creates the `EncounterWorkspace` shell with three tabs (no functional form content yet — just tab chrome and loading/error states).

By the end of this task, a Provider can click "Start Encounter" on any patient chart, have the encounter created in the backend, and be navigated to an `EncounterWorkspace` page that shows the patient name, three tab buttons, and a loading skeleton — all TypeScript-clean.

## Steps

1. **Extend the Route union** in `src/contexts/RouterContext.tsx`: add `| { page: "encounter-workspace"; patientId: string; encounterId: string }` to the `Route` type. No other changes needed in this file.

2. **Wire the new route in `ContentArea.tsx`**: import `EncounterWorkspace` from `../../pages/EncounterWorkspace`. Add `case "encounter-workspace"` that renders `<EncounterWorkspace patientId={currentRoute.patientId} encounterId={currentRoute.encounterId} role={user?.role ?? ""} userId={user?.id ?? ""} />`. `user?.id` is available from `useAuth()` which is already called in ContentArea. TypeScript exhaustiveness: the existing `default: never` case will now be satisfiable again once the case is added.

3. **Create `src/hooks/useEncounter.ts`**: Copy the `usePatient` hook structure exactly — `useState`, `useEffect` with `mounted` boolean, `refreshCounter`, `useCallback reload`. Props: `{ patientId: string; encounterId: string }`. The `fetchAll` function runs `Promise.all([commands.getEncounter(encounterId), commands.listVitals(patientId, encounterId), commands.listTemplates(null)])`. Return type: `{ encounter: EncounterRecord | null, vitals: VitalsRecord[], templates: TemplateRecord[], loading: boolean, error: string | null, reload: () => void }`. Guard all `setX` calls with `if (!mounted) return`. Log errors as `console.error("[useEncounter] fetchAll failed for ${encounterId}:", msg)`.

4. **Create `src/pages/EncounterWorkspace.tsx`** — shell only:
   - Props: `{ patientId: string; encounterId: string; role: string; userId: string }`
   - Call `useEncounter({ patientId, encounterId })` and `useNav()`.
   - Render a `<LoadingSkeleton>` while `loading`.
   - Render an inline error banner with "Retry" button if `error` is non-null.
   - When data loaded: render a page header with "← Back" button + encounter type + encounter date extracted from `encounter.resource` (or fallback "Encounter" if null).
   - Render three tab buttons: SOAP | Vitals | ROS using `useState<"soap" | "vitals" | "ros">` defaulting to `"soap"`.
   - Each tab body renders a `<div>` with placeholder text: e.g. "SOAP note editor — T02", "Vitals form — T03", "Review of Systems — T04".

5. **Extend `PatientDetailPage.tsx`** with encounter list and "Start Encounter":
   - Add `useState<EncounterRecord[]>([])` and `useState(true)` for encountersLoading / encountersError.
   - Add `useEffect` (with mounted guard) that calls `commands.listEncounters(patientId, null, null, null)` on mount and on `refreshCounter` increment. Set state on resolution.
   - Add a `SectionCard` titled "Encounters" that renders a table/list of prior encounters: each row shows encounter date (ISO slice to date portion), encounter type (format `office_visit` → "Office Visit"), and status badge. Each row is a `<button>` calling `navigate({ page: "encounter-workspace", patientId, encounterId: enc.id })`.
   - Add "Start Encounter" button in the header area, visible only when `role` is `"Provider"`, `"NurseMa"`, or `"SystemAdmin"`. On click: assemble `EncounterInput` with `patientId`, `providerId: userId` (pass `userId` as prop from ContentArea via `useAuth`), `encounterDate: new Date().toISOString().slice(0, 19)`, `encounterType: "office_visit"`, `chiefComplaint: null`, `templateId: null`, `soap: null`. Call `commands.createEncounter(input)` in a try/catch. On success: navigate to encounter-workspace. On error: set inline `startEncounterError` state and display inline below the button.
   - Import `EncounterRecord` from `../types/documentation`.
   - Pass `userId` prop to `PatientDetailPage` from `ContentArea.tsx` (add to props interface).

## Must-Haves

- [ ] `Route` union in `RouterContext.tsx` includes `encounter-workspace` variant with `patientId` and `encounterId`
- [ ] `ContentArea.tsx` case renders `EncounterWorkspace` with all four props; `tsc` exhaustiveness check passes (no `never` error on default case)
- [ ] `useEncounter` hook follows mounted-boolean / refreshCounter / reload pattern from `usePatient`; all three fetches in `Promise.all`; all errors logged with context
- [ ] `EncounterWorkspace` shell renders loading / error / tabs states; "← Back" works
- [ ] `PatientDetailPage` shows "Start Encounter" for Provider/NurseMa/SystemAdmin only; FrontDesk and BillingStaff do NOT see the button
- [ ] Prior encounters list on `PatientDetailPage` renders and each row navigates to encounter-workspace
- [ ] `encounterDate` assembled as `new Date().toISOString().slice(0, 19)` (no trailing `Z`)
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` — must exit 0 with zero errors
- In running Tauri app:
  - Log in as Provider → open any patient chart → "Start Encounter" button visible → click → workspace opens with three tab buttons (SOAP | Vitals | ROS)
  - Click "← Back" from workspace → returns to patient chart
  - Patient chart shows an "Encounters" section; newly created encounter appears in the list; click its row → workspace opens for that encounter
  - Log in as FrontDesk → open patient chart → "Start Encounter" button NOT present

## Observability Impact

- Signals added/changed: `console.error("[useEncounter] fetchAll failed …")` on hook fetch failure; `startEncounterError` inline banner on create failure
- How a future agent inspects this: React DevTools shows `useEncounter` state (`encounter`, `vitals`, `templates`, `loading`, `error`); ContentArea logs `user?.role` accessible in same component
- Failure state exposed: if `createEncounter` fails (e.g. wrong role), inline error appears below "Start Encounter" button with the Rust error message; tab shell still renders correctly (no blank screen)

## Inputs

- `src/contexts/RouterContext.tsx` — existing Route union to extend
- `src/components/shell/ContentArea.tsx` — existing switch to extend; `useAuth` already imported
- `src/hooks/usePatient.ts` — reference for hook structure (copy exactly)
- `src/pages/PatientDetailPage.tsx` — existing patient chart shell to extend
- `src/lib/tauri.ts` — `commands.createEncounter`, `commands.listEncounters`, `commands.getEncounter`, `commands.listVitals`, `commands.listTemplates` already wired
- `src/types/documentation.ts` — `EncounterRecord`, `EncounterInput`, `VitalsRecord`, `TemplateRecord` already typed

## Expected Output

- `src/contexts/RouterContext.tsx` — Route union extended with `encounter-workspace` variant
- `src/components/shell/ContentArea.tsx` — `case "encounter-workspace"` added; `userId` passed to PatientDetailPage and EncounterWorkspace
- `src/hooks/useEncounter.ts` — new file: data-fetching hook returning encounter + vitals + templates
- `src/pages/EncounterWorkspace.tsx` — new file: tab shell (SOAP | Vitals | ROS) with loading/error states
- `src/pages/PatientDetailPage.tsx` — extended: encounter list section + Start Encounter button + RBAC gate
