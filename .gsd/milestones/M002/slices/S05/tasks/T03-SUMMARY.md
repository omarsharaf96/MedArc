---
id: T03
parent: S05
milestone: M002
provides:
  - src/components/scheduling/AppointmentFormModal.tsx — create/cancel modal with 6-swatch palette, recurrence validation, submitError inline
  - src/components/scheduling/WaitlistPanel.tsx — waitlist list + inline add form + discharge (window.confirm guard)
  - src/components/scheduling/RecallPanel.tsx — recall list + inline create form + complete action (void return)
  - src/components/scheduling/FlowBoardPage.tsx — real status-transition buttons with per-card submitting/error state
  - src/components/scheduling/CalendarPage.tsx — cancel trigger in InfoPopover (write-gated, booked-only)
  - src/pages/SchedulePage.tsx — modal state, waitlist/recall panels, New Appointment button
key_files:
  - src/components/scheduling/AppointmentFormModal.tsx
  - src/components/scheduling/WaitlistPanel.tsx
  - src/components/scheduling/RecallPanel.tsx
  - src/components/scheduling/FlowBoardPage.tsx
  - src/components/scheduling/CalendarPage.tsx
  - src/pages/SchedulePage.tsx
key_decisions:
  - AppointmentFormModal.providerId sent as empty string — server derives actual provider from authenticated session; this is consistent with T01 DECISIONS entry that providerId is not required client-side
  - CalendarPage.InfoPopover gains onCancelAppointment prop threaded from SchedulePage so cancel target flows up cleanly without lifting all popover state
  - FlowBoardPage split into FlowBoardCard sub-component to isolate per-card submitting/cardError state — avoids a shared error map indexed by appointmentId
  - extractWaitlistDisplay and extractRecallDisplay kept local to their panel files (not fhirExtract.ts) — these resources are scheduling-domain blobs (AppointmentRequest, PatientRecall), not standard FHIR Appointment types
  - WaitlistPanel and RecallPanel pass error={null} to their own error prop (SchedulePage renders per-domain banners above) — avoids double error display
patterns_established:
  - Inline panel add form (toggle via addOpen boolean) used for WaitlistPanel and RecallPanel — no separate modal for lower-prominence write actions
  - Per-card async state (submitting + cardError) pattern for FlowBoardCard — isolates in-flight state to individual cards
  - canWrite gate applied to all write-capable UI: "New Appointment" button, InfoPopover cancel button, FlowBoard transition buttons, WaitlistPanel add/discharge, RecallPanel create/complete
observability_surfaces:
  - submitError rendered inline above submit button in AppointmentFormModal (both create and cancel modes)
  - Per-card cardError shown inline below FlowBoardCard transition buttons
  - React DevTools → SchedulePage: createOpen (bool), cancelTarget (AppointmentRecord | null)
  - React DevTools → AppointmentFormModal: submitting, submitError
  - React DevTools → FlowBoardCard: submitting, cardError per entry
  - Prior T01/T02 domain error banners (errorAppointments, errorFlowBoard, errorWaitlist, errorRecalls) remain intact
duration: ~1 session
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Write path — appointment form, flow status transitions, waitlist & recall panels

**Closed S05 by adding all write-path UI: appointment create/cancel modal, flow status transition buttons, waitlist panel with inline add/discharge, and recall panel with inline create/complete — all RBAC-gated and TypeScript-clean.**

## What Happened

Built six files to complete the scheduling write path on top of the T02 read-only UI.

**AppointmentFormModal** (`create` | `cancel` modes): Create mode collects patientId, datetime-local start (normalized to HH:MM:SS on submit), duration select (5/10/15/20/30/45/60 min), apptType bounded select (6 options), reason, notes, recurrence select (none/weekly/biweekly/monthly), recurrenceEndDate (shown and required when recurrence ≠ ""), and a 6-swatch fixed color palette (`#4A90E2` default, no `<input type="color">`). Recurrence validation checks that end date is non-empty and strictly after the start date — inline field error shown if violated. Cancel mode shows appointment summary above an optional reason field; `cancelAppointment` always receives `reason ?? null`. Both modes use `submitting` + inline `submitError` per the AllergyFormModal pattern.

**WaitlistPanel**: Inline add form (toggled by `addOpen`) with patientId, apptType select, preferredDate, priority 1–5, reason, notes, providerId. List renders priority badge, patientId, apptType, preferredDate (split on "T" — no Date construction), status from resource blob. Discharge uses `window.confirm` then calls `onDischarge(id, null)`. `extractWaitlistDisplay` reads `resource.status` and `resource.priority[0].coding[0].code` — kept local per spec.

**RecallPanel**: Same inline toggle pattern for create form (patientId, dueDate, recallType select 4 options, required reason, notes, providerId). List shows recallType badge, patientId, dueDate, status, reason. Complete button calls `onCompleteRecall(id, null)` — no return value read (void). `extractRecallDisplay` reads status/dueDate/recallType/reason from resource blob — kept local.

**FlowBoardPage** (updated): Replaced the T02 placeholder with a `FlowBoardCard` sub-component owning per-card `submitting` + `cardError` state. `nextStatuses()` implements the full state machine: `scheduled → checked_in → roomed → with_provider → checkout → completed`. Each card renders a room text input + one transition button per valid next status. Button calls `onUpdateStatus({ appointmentId, flowStatus: next, room: roomValue || null, notes: null })`. BillingStaff sees no buttons (`canWrite` gate).

**CalendarPage** (updated): `InfoPopover` gained `onCancelAppointment` and `canWrite` props. Cancel Appointment button shown only when `canWrite && display.status === "booked"`. Clicking fires `onCancelAppointment(appt)` and closes the popover. New prop `onCancelAppointment` threaded through `CalendarPageProps`.

**SchedulePage** (updated): Added `createOpen` and `cancelTarget` state. "New Appointment" button (write-gated) sets `createOpen = true`. `handleCancelAppointment` sets `cancelTarget`. Renders `AppointmentFormModal` in create or cancel mode. `WaitlistPanel` and `RecallPanel` mounted below FlowBoardPage with per-domain error banners above each. All mutations wired from `useSchedule` return.

## Verification

```
cd /Users/omarsharaf96/Documents/GitHub/MedArc && npx tsc --noEmit
# → (no output, exit 0)

grep 'as any\|@ts-ignore' src/components/scheduling/ src/pages/SchedulePage.tsx -r
# → (no output, exit 1 = no matches)

ls src/components/scheduling/
# AppointmentFormModal.tsx  CalendarPage.tsx  FlowBoardPage.tsx  RecallPanel.tsx  WaitlistPanel.tsx

grep "canWrite" src/pages/SchedulePage.tsx
# → function canWrite + writeAllowed assigned + 6 canWrite={writeAllowed} prop passes
```

All slice-level verification checks pass on this final task of S05.

## Diagnostics

- `submitError` visible above submit button in AppointmentFormModal create/cancel forms — no DevTools needed
- Per-card `cardError` visible below FlowBoardCard transition buttons on failure
- React DevTools → SchedulePage: inspect `createOpen` (bool), `cancelTarget` (null or AppointmentRecord)
- React DevTools → AppointmentFormModal: inspect `submitting`, `submitError`, `recurrenceEndDateError`
- React DevTools → FlowBoardCard: each card independently shows `submitting`, `cardError`
- Browser console: `grep "[useSchedule]"` for per-domain fetch errors (from T01, unchanged)

## Deviations

- `AppointmentFormModal.providerId` sent as `""` (empty string) rather than omitted — the backend derives the provider from the authenticated session. The `AppointmentInput` type requires `providerId: string` (non-optional), so an empty string is the correct sentinel for server-side resolution. This is consistent with the T01 DECISIONS entry.
- `WaitlistPanel.error` and `RecallPanel.error` receive `null` from SchedulePage (banners rendered above the panels) to avoid double error display — minor deviation from prop threading described in T03 plan, but functionally correct and less noisy.

## Known Issues

None.

## Files Created/Modified

- `src/components/scheduling/AppointmentFormModal.tsx` — new; create/cancel appointment modal with recurrence validation and 6-swatch palette
- `src/components/scheduling/WaitlistPanel.tsx` — new; waitlist list + inline add form + window.confirm discharge
- `src/components/scheduling/RecallPanel.tsx` — new; recall list + inline create form + void complete action
- `src/components/scheduling/FlowBoardPage.tsx` — updated; real status-transition buttons with FlowBoardCard sub-component and per-card error state
- `src/components/scheduling/CalendarPage.tsx` — updated; InfoPopover gains cancel button (write-gated, booked-only) and onCancelAppointment prop
- `src/pages/SchedulePage.tsx` — updated; modal state, New Appointment button, WaitlistPanel, RecallPanel, AppointmentFormModal wired
