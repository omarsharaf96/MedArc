---
id: T02
parent: S05
milestone: M002
provides:
  - src/components/scheduling/CalendarPage.tsx — CSS Grid day/week calendar with read-only appointment cards and info popover
  - src/components/scheduling/FlowBoardPage.tsx — flow board with status badges and loading/error/empty states
  - src/pages/SchedulePage.tsx — full replacement; stub removed; useAuth + useSchedule wired
key_files:
  - src/pages/SchedulePage.tsx
  - src/components/scheduling/CalendarPage.tsx
  - src/components/scheduling/FlowBoardPage.tsx
key_decisions:
  - CalendarPage owns selectedCard / InfoPopover state — SchedulePage passes onCardClick as a no-op hook point for T03 interception
  - InfoPopover calls useNav() directly (not passed via prop) — keeps CalendarPage self-contained for navigation
  - AppointmentRecord imported from types/scheduling (not re-exported from useSchedule) — hooks export interface types only, not re-exports
  - FlowBoardPage receives error prop as null when SchedulePage renders its own banner above — avoids double error display
patterns_established:
  - formatTime / startMinuteOfDay / formatDisplayDate all use string splits + vanilla Date; no Z suffix appended (avoids timezone-shift bug)
  - canWrite(role) helper in SchedulePage follows ClinicalSidebar pattern exactly
  - getDateRange(date, view) uses toLocaleDateString("sv") for ISO YYYY-MM-DD — same technique as useSchedule.ts todayDateString
  - Appointment card absolute positioning: top = (startMin - 480) px, height = durationMin * 1px at 60px/hour
  - FLOW_STATUS_COLORS Record<string, string> for Tailwind badge classes — single source of truth
observability_surfaces:
  - errorAppointments rendered as inline red banner in SchedulePage above CalendarPage — visible without DevTools
  - errorFlowBoard rendered as inline red banner above FlowBoardPage — visible without DevTools
  - FlowBoardPage loading/error/empty each handled with distinct UI states
  - React DevTools → SchedulePage: view, currentDate, startDate, endDate, useSchedule state
  - CalendarPage: selectedCard (null or AppointmentRecord) inspectable in component tree
duration: ~45 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Read-only calendar + flow board UI

**Replaced the SchedulePage stub with a fully wired day/week CSS Grid calendar and patient flow board — both backed by the T01 data layer, zero TypeScript errors.**

## What Happened

Built three files from scratch:

1. **`SchedulePage.tsx`** — Complete replacement. Calls `useAuth()` for `role`/`userId`, derives `canWrite()` RBAC flag, manages `view` and `currentDate` state, computes `startDate`/`endDate` via `getDateRange()`, and calls `useSchedule(startDate, endDate, userId)`. Renders a page header, inline error banners for `errorAppointments` and `errorFlowBoard`, a loading skeleton on initial load, `CalendarPage`, and `FlowBoardPage`.

2. **`CalendarPage.tsx`** — CSS Grid calendar with four sub-components:
   - `CalendarHeader`: date/range title, prev/next navigation, day/week toggle buttons
   - `CalendarGrid`: time gutter (08:00–18:00, 60px/hour) plus one or seven appointment columns with hour grid lines; appointments positioned absolutely using `top = (startMin - 480)px`, `height = durationMin × 1px`
   - `AppointmentCard`: renders `apptTypeDisplay + formatTime(start)`, colored by `display.color`, clickable to open InfoPopover
   - `InfoPopover`: fixed overlay showing apptType, start/end time, status, reason, notes, patientId, and "Go to Chart" button (calls `useNav().navigate({ page: "patient-detail", patientId })`)
   
   Date helpers `formatTime`, `formatDisplayDate`, `startMinuteOfDay` all use string splits — no `new Date(str + "Z")`.

3. **`FlowBoardPage.tsx`** — Renders flow board entries as cards with status badges from `FLOW_STATUS_COLORS`. Loading skeleton, inline error banner, and empty-state message all handled. Disabled "Update Status" placeholder label present; real buttons wired in T03.

One import fix was needed: `AppointmentRecord` is in `types/scheduling` not `hooks/useSchedule`; fixed in both CalendarPage and SchedulePage.

## Verification

```
cd /Users/omarsharaf96/Documents/GitHub/MedArc && npx tsc --noEmit
# → (no output, exit 0) ✓

grep "coming in S03" src/pages/SchedulePage.tsx
# → no output ✓

grep 'as any\|@ts-ignore' src/pages/SchedulePage.tsx src/components/scheduling/CalendarPage.tsx src/components/scheduling/FlowBoardPage.tsx
# → no output ✓
```

Slice-level checks that pass at this point:
- `tsc --noEmit` exits 0 ✓
- No `as any` or `@ts-ignore` in new/modified files ✓
- Stub body gone from SchedulePage.tsx ✓
- `CalendarPage`, `FlowBoardPage` exist under `src/components/scheduling/` ✓
- `useSchedule` exported from `src/hooks/useSchedule.ts` ✓ (T01)
- `extractAppointmentDisplay` exported from `src/lib/fhirExtract.ts` ✓ (T01)

Remaining for T03 (expected):
- `AppointmentFormModal`, `WaitlistPanel`, `RecallPanel` not yet created

## Diagnostics

- **Appointment errors**: `grep "[useSchedule]"` in browser console — one tag per failing domain
- **Error banners**: visible in UI without DevTools above each section when errorAppointments / errorFlowBoard is set
- **React DevTools → SchedulePage**: inspect `view`, `currentDate`, `startDate`, `endDate` state; inspect `useSchedule` hook state
- **React DevTools → CalendarPage**: `selectedCard` (null or AppointmentRecord) shows when popover is open
- **Positioning check**: in DevTools, inspect appointment card `style` — `top` and `height` are in px, computed from `startMinuteOfDay` and `durationMin`

## Deviations

- `SchedulePage` does not call `useNav()` directly; `navigate` for "Go to chart" lives inside `CalendarPage`'s `InfoPopover` via `useNav()`. This keeps the calendar sub-tree self-contained and removes an unused variable that would fail `tsc`.
- `FlowBoardPage` receives `error={null}` from `SchedulePage` (the error banner is rendered by SchedulePage above the component instead of inside it). This avoids double error display while preserving the `error` prop in the interface for callers that want embedded error handling.

## Known Issues

None. All must-haves satisfied.

## Files Created/Modified

- `src/pages/SchedulePage.tsx` — Full replacement; stub removed; `useAuth` + `useSchedule` wired; `getDateRange` helper; error banners; loading skeleton
- `src/components/scheduling/CalendarPage.tsx` — New; CSS Grid day/week calendar with CalendarHeader, CalendarGrid, AppointmentCard, InfoPopover sub-components; `formatTime`, `formatDisplayDate`, `startMinuteOfDay` helpers
- `src/components/scheduling/FlowBoardPage.tsx` — New; flow board with `FLOW_STATUS_COLORS` badge map; loading/error/empty states; T02 placeholder for write buttons
