# S05: Scheduling & Flow Board

**Goal:** Replace the `SchedulePage` stub with a fully functional scheduling UI — a day/week calendar grid, an appointment create/cancel/edit form with recurring-series support, open-slot search, and the real-time Patient Flow Board — plus Waitlist and Recall Board panels. All wired to the 13 scheduling Tauri commands already in `commands`. RBAC enforced: BillingStaff gets read-only; all other clinical roles get full create/cancel/update.

**Demo:** A Provider or FrontDesk user navigates to Schedule, sees a week-view calendar grid populated with today's appointments (fetched from the backend), clicks "New Appointment" to create an appointment (verifying it appears on the grid), views the Patient Flow Board below and transitions a patient from "scheduled" to "checked_in", and opens the Waitlist and Recall panels to confirm their lists load. `tsc --noEmit` exits 0.

## Must-Haves

- `extractAppointmentDisplay` appended to `src/lib/fhirExtract.ts` with all FHIR paths verified from `scheduling.rs`
- `useSchedule` hook with `mounted` guard, `refreshCounter`, per-domain error isolation, and all mutation callbacks (`createAppointment`, `cancelAppointment`, `updateFlowStatus`, `addToWaitlist`, `dischargeWaitlist`, `createRecall`, `completeRecall`)
- `CalendarPage` renders day/week grid (CSS Grid, no external lib); week starts Sunday; appointments render as `AppointmentCard` components positioned by time of day
- `FlowBoardPage` shows today's flow board; each card has status-transition buttons; clicking a button calls `updateFlowStatus` and re-fetches
- `AppointmentFormModal` handles create (with recurrence validation) and cancel; `recurrenceEndDate` required when recurrence is non-null; `cancelAppointment` always passes `reason ?? null`
- `WaitlistPanel` lists waitlist entries and supports add + discharge
- `RecallPanel` lists recalls and supports create + complete
- `SchedulePage` fully replaces the stub; obtains `role` and `userId` from `useAuth()` inside the component; RBAC `canWrite(role)` gates all write-capable UI
- `tsc --noEmit` exits 0 after each task; no `as any`, no `@ts-ignore`
- Calendar date arithmetic uses vanilla JS `Date` objects and string splitting — no `date-fns`, `dayjs`, or `luxon`
- Appointment cards show `apptType` + time only (no patient name) on the grid; patient chart link available in an expanded popover

## Proof Level

- This slice proves: integration
- Real runtime required: yes (appointment create/flow-status transitions exercised against the running Tauri app)
- Human/UAT required: no (agent verifies via `tsc --noEmit` + running dev app observation)

## Verification

- `cd /Users/omarsharaf96/Documents/GitHub/MedArc && npx tsc --noEmit` exits 0 after each task
- No `as any` or `@ts-ignore` in any new or modified file — confirmed by `grep -r 'as any\|@ts-ignore' src/hooks/useSchedule.ts src/lib/fhirExtract.ts src/components/scheduling/ src/pages/SchedulePage.tsx`
- `src/pages/SchedulePage.tsx` no longer contains the stub body ("coming in S03") — `grep "coming in S03" src/pages/SchedulePage.tsx` returns no matches
- `CalendarPage`, `FlowBoardPage`, `AppointmentFormModal`, `WaitlistPanel`, `RecallPanel` all exist under `src/components/scheduling/`
- `useSchedule` hook exported from `src/hooks/useSchedule.ts`
- `extractAppointmentDisplay` exported from `src/lib/fhirExtract.ts`

## Observability / Diagnostics

- Runtime signals: `console.error("[useSchedule] listAppointments failed:", msg)` / `[useSchedule] getFlowBoard failed:` / `[useSchedule] listWaitlist failed:` / `[useSchedule] listRecalls failed:` — one tag per domain, structured for grep
- Inspection surfaces: React DevTools → `useSchedule` state shows `appointments[]`, `flowBoard[]`, `waitlist[]`, `recalls[]`, and four independent `error*` fields; per-panel inline error banners visible without DevTools
- Failure state exposed: per-domain `errorAppointments | errorFlowBoard | errorWaitlist | errorRecalls` each independently surfaced; `submitError` rendered inline above submit in all modals
- Redaction constraints: no PHI in console.error tags — appointment IDs (UUIDs) only; patient names never logged

## Integration Closure

- Upstream surfaces consumed:
  - `src/contexts/RouterContext.tsx` — `{ page: "schedule" }` route already wired in `ContentArea`
  - `src/lib/tauri.ts` — all 13 scheduling commands already wired and typed
  - `src/types/scheduling.ts` — complete type layer; no new types needed
  - `src/hooks/useAuth.ts` — `role` and `user.id` obtained inside `SchedulePage`
  - `src/hooks/useClinicalData.ts` (pattern reference) — `useSchedule` mirrors this hook exactly
  - `src/lib/fhirExtract.ts` — `extractAppointmentDisplay` appended here
- New wiring introduced in this slice:
  - `src/hooks/useSchedule.ts` → `src/pages/SchedulePage.tsx` (hook called at SchedulePage level)
  - `src/components/scheduling/*` → `src/pages/SchedulePage.tsx` (all scheduling sub-components composed here)
  - `ContentArea.tsx` passes `role` and `userId` via `useAuth()` to `SchedulePage` (currently passes none — `SchedulePage` calls `useAuth()` internally per research constraint)
- What remains before the milestone is truly usable end-to-end: S06 (labs, documents, physical exam), S07 (settings, cleanup, final E2E verification)

## Tasks

- [x] **T01: Data layer — `extractAppointmentDisplay` + `useSchedule` hook** `est:45m`
  - Why: All three UI tasks depend on this hook and the FHIR extractor. No component can be built until the data types, extraction paths, and mutation callbacks are defined and TypeScript-clean.
  - Files: `src/lib/fhirExtract.ts`, `src/hooks/useSchedule.ts`
  - Do: Append `AppointmentDisplay` interface and `extractAppointmentDisplay()` to `fhirExtract.ts` (verified paths from `build_appointment_fhir` in `scheduling.rs`). Write `useSchedule(dateRange, providerId?)` hook following `useClinicalData` pattern exactly: `mounted` guard, `refreshCounter`, per-domain error isolation for 4 domains (appointments, flowBoard, waitlist, recalls). Expose all mutation callbacks memoized with `useCallback`. Export `UseScheduleReturn` interface. Also add a local `extractOpenSlot` helper (NOT in `fhirExtract.ts`) for `searchOpenSlots` result objects.
  - Verify: `npx tsc --noEmit` exits 0; `grep 'as any\|@ts-ignore' src/lib/fhirExtract.ts src/hooks/useSchedule.ts` returns nothing
  - Done when: `useSchedule` and `extractAppointmentDisplay` are fully typed with no TS errors; all 7 mutation callbacks present in `UseScheduleReturn`

- [x] **T02: Read-only calendar + flow board UI** `est:1h`
  - Why: Establishes the visual shell of the scheduling page — the day/week grid and flow board — in read-only form. T03 adds the write path on top of this foundation.
  - Files: `src/pages/SchedulePage.tsx`, `src/components/scheduling/CalendarPage.tsx`, `src/components/scheduling/FlowBoardPage.tsx`
  - Do: Replace `SchedulePage` stub entirely; call `useSchedule` at this level; pass data + callbacks as props. Build `CalendarPage` with CSS Grid day/week toggle, CalendarHeader (prev/next nav), TimeGutter, and `AppointmentCard` sub-components. Cards show `apptType` + formatted time only; clicking opens a read-only info popover with `patientId` and a "Go to chart" link (navigate to `patient-detail`). Build `FlowBoardPage` with cards per `FlowBoardEntry` showing `flowStatus`, `startTime`, `apptType`, `room`; include stub "Update Status" area (no-op button). `canWrite(role)` gates write buttons — all hidden in T02 (full rendering deferred to T03).
  - Verify: `npx tsc --noEmit` exits 0; `grep "coming in S03" src/pages/SchedulePage.tsx` returns no output
  - Done when: SchedulePage renders CalendarPage and FlowBoardPage without TS errors; grid renders correctly; stub body is gone

- [x] **T03: Write path — appointment form, flow status transitions, waitlist & recall panels** `est:1h`
  - Why: Closes the slice by wiring all user-facing write operations. After this task the full demo is achievable: create an appointment, see it on the calendar, transition flow status, manage waitlist and recalls.
  - Files: `src/components/scheduling/AppointmentFormModal.tsx`, `src/components/scheduling/WaitlistPanel.tsx`, `src/components/scheduling/RecallPanel.tsx`, `src/components/scheduling/CalendarPage.tsx`, `src/components/scheduling/FlowBoardPage.tsx`, `src/pages/SchedulePage.tsx`
  - Do: Build `AppointmentFormModal` (create + cancel modes; recurrence validation: `recurrenceEndDate` required when `recurrence` is non-null; `cancelAppointment` always passes `reason ?? null`; color as fixed palette swatches; `apptType` as bounded `<select>`). Build `WaitlistPanel` (list + add via `WaitlistInput` form + discharge). Build `RecallPanel` (list + create via `RecallInput` form + complete). Wire status-transition buttons in `FlowBoardPage` — each valid next-status rendered as a button; clicking calls `updateFlowStatus` then `reloadFlowBoard()`. Wire "New Appointment" + appointment-card click → cancel modal into `CalendarPage`. Mount all panels in `SchedulePage` using `canWrite(role)` to hide write-capable buttons for BillingStaff.
  - Verify: `npx tsc --noEmit` exits 0; `grep 'as any\|@ts-ignore' src/components/scheduling/ src/pages/SchedulePage.tsx -r` returns nothing
  - Done when: All 7 new files exist; `tsc --noEmit` exits 0; the full demo sequence is executable in the running Tauri app

## Files Likely Touched

- `src/lib/fhirExtract.ts` — append `AppointmentDisplay` + `extractAppointmentDisplay`
- `src/hooks/useSchedule.ts` — new file
- `src/pages/SchedulePage.tsx` — full replacement
- `src/components/scheduling/CalendarPage.tsx` — new file
- `src/components/scheduling/FlowBoardPage.tsx` — new file
- `src/components/scheduling/AppointmentFormModal.tsx` — new file
- `src/components/scheduling/WaitlistPanel.tsx` — new file
- `src/components/scheduling/RecallPanel.tsx` — new file
