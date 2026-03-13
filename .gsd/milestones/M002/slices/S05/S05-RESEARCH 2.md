# S05 — Scheduling & Flow Board Research

**Date:** 2026-03-12  
**Milestone:** M002  
**Requirements covered:** UI-02 (calendar + flow board + waitlist + recall board)

---

## Summary

S05 builds the Scheduling page from the placeholder `SchedulePage` stub into a fully functional scheduling UI: a day/week calendar grid, an appointment create/cancel form (with recurring series), open-slot search, and the real-time Patient Flow Board. It also exposes Waitlist and Recall Board management surfaces.

All 13 scheduling commands are already wired in `commands` (`createAppointment`, `listAppointments`, `updateAppointment`, `cancelAppointment`, `searchOpenSlots`, `updateFlowStatus`, `getFlowBoard`, `addToWaitlist`, `listWaitlist`, `dischargeWaitlist`, `createRecall`, `listRecalls`, `completeRecall`). The full TypeScript type layer in `src/types/scheduling.ts` is complete. There are **no new Tauri commands to write** — this is pure UI work.

The primary complexity is the calendar grid itself (date arithmetic, day/week view layout, appointment positioning by time-of-day), and the Flow Board's status-transition UX. Every other surface (waitlist, recall, appointment form) is a modal/list pattern that mirrors S04's ClinicalSidebar work.

No new dependencies are allowed. Date arithmetic must be done with vanilla JS (same philosophy as the Rust JDN calendar engine). This is viable — the calendar only needs: day names, formatted dates, ISO date string generation, and time slot positioning. All achievable with `Date` objects and string manipulation.

---

## Recommendation

**3-task decomposition (T01 → T02 → T03)** following the S04 pattern:

- **T01** — Calendar data layer: `useSchedule` hook + `extractAppointmentDisplay` FHIR helper (reads `start`, `end`, `minutesDuration`, `reason`, `status`, color extension, `appt_type` from the Appointment FHIR JSON). The hook loads `listAppointments` for the displayed date range and `getFlowBoard` for today. Exported as the backbone for T02/T03.
- **T02** — Read-only calendar + flow board UI: `CalendarPage` with day/week view tabs, appointment cards, and `FlowBoardPage` as a standalone page section beneath the calendar. The calendar grid renders without edit capability; clicking a card shows a read-only popover.
- **T03** — Write path: `AppointmentFormModal` (create + cancel + recurrence options), open-slot search panel, flow status transition buttons on flow board cards, and waitlist/recall management modals. Wire cancel/create/update into `CalendarPage` and `FlowBoardPage`.

`tsc --noEmit` is the verification gate after each task (consistent with S03/S04 precedent).

---

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Date label formatting | Inline `Date` methods + `.toLocaleDateString('en-US', {…})` | No lib needed; already used in `PatientListPage.tsx` `formatDate()` |
| Day/week grid layout | CSS Grid with fixed time column + auto rows | Native CSS; avoids a calendar library whose API surface is large |
| Modal overlay | `fixed inset-0 bg-black/40 z-50` established pattern | Matches `PatientFormModal`, `AllergyFormModal`, `LockScreen` — do not invent a new overlay pattern |
| Error boundary for fetch failures | Per-domain try/catch in hook, inline error banners in components | Same pattern as `useClinicalData` — do not use React ErrorBoundary here |

---

## Existing Code and Patterns

- `src/pages/SchedulePage.tsx` — current placeholder; **replace the whole body** with the real implementation (do not layer on top of the stub).
- `src/types/scheduling.ts` — **complete** — `AppointmentInput`, `AppointmentRecord`, `UpdateAppointmentInput`, `WaitlistInput`, `WaitlistRecord`, `RecallInput`, `RecallRecord`, `UpdateFlowStatusInput`, `FlowBoardEntry` all defined. No new types needed in `scheduling.ts`.
- `src/lib/tauri.ts` — **all 13 scheduling commands already wired** and typed. Read each signature carefully before calling: several have non-obvious param names documented in `DECISIONS.md`.
- `src/contexts/RouterContext.tsx` — `{ page: "schedule" }` route already exists. The `SchedulePage` receives no props — role and userId must be obtained inside it via `useAuth()` (same pattern as `PatientsPage`).
- `src/hooks/useClinicalData.ts` — **follow this hook pattern exactly** for `useSchedule`: mounted guard, `refreshCounter`, per-domain try/catch, `useCallback`-memoized reload and mutations.
- `src/components/patient/PatientFormModal.tsx` — authoritative modal pattern: `fixed inset-0 bg-black/40 z-50 flex items-center justify-center`, `max-w-lg w-full` inner panel, inline `submitError` above submit button.
- `src/components/clinical/ClinicalSidebar.tsx` / `AllergyFormModal.tsx` — authoritative tab-panel + form-modal pattern. The `canWrite(role)` helper should be replicated for scheduling RBAC gates.
- `src/lib/fhirExtract.ts` — append `extractAppointmentDisplay` here (same as T01 added `extractAllergyDisplay`). Never navigate `AppointmentRecord.resource` inline in components.
- `src/components/patient/PatientListPage.tsx` — `formatDate()` helper (local date split, not `new Date(iso)`) avoids timezone shift on YYYY-MM-DD strings. Use the same technique for displaying appointment dates.

---

## Constraints

- **No new dependencies.** No `react-big-calendar`, `fullcalendar`, `date-fns`, `dayjs`, `luxon`. The calendar grid must be built with CSS Grid and vanilla JS `Date` arithmetic.
- **RBAC second layer in `SchedulePage`.** The sidebar already hides the Schedule nav item for some roles, but `SchedulePage` must independently check the user's role before rendering write-capable UI. `BillingStaff` gets read-only. `FrontDesk` gets full CRUD (including cancel). `NurseMa`/`Provider` get Create + Update + Cancel (no hard delete). `SystemAdmin` gets full CRUD.
- **`AppointmentRecord` does not carry patient name.** The backend returns `patientId` only. Displaying patient names on calendar cards requires either: (a) a secondary `searchPatients` call to resolve IDs to names, or (b) showing only the appointment type and time on the card (patient name shown in a popover/detail after click). Option (b) is correct for MVP — it avoids N+1 queries and the UI remains functional without patient name resolution.
- **`searchOpenSlots` returns `Record<string, unknown>[]`, not a typed struct.** The `commands.searchOpenSlots` wrapper returns `Record<string, unknown>[]` because the backend returns a `Vec<serde_json::Value>`. A local extraction helper (not in `fhirExtract.ts` — these aren't FHIR resources) should read `start_time`, `end_time`, `duration_minutes`, `available` from each object.
- **`createAppointment` returns `AppointmentRecord[]` (array, not single).** Recurring series returns multiple records. Callers must handle the array; single appointments are a length-1 array.
- **Datetime strings: no timezone suffix.** The Rust backend stores datetimes as `"2026-04-01T09:00:00"` without `Z` or offset. JavaScript `new Date("2026-04-01T09:00:00")` parses this as local time. Do NOT append `Z` — that shifts the time by the UTC offset. Use the local-parse behavior intentionally.
- **`cancelAppointment` requires `reason: string | null` as second param.** Always pass `reason ?? null`, never omit the argument.
- **`listRecalls` has NO `patient_id` param.** Recalls are provider-scoped, not patient-scoped (per `DECISIONS.md`). Do not try to pass a patient ID to this command.
- **`listWaitlist` has NO `patient_id` param.** Same constraint — waitlist is a provider-level view.
- **`getFlowBoard` takes `date: string` as `"YYYY-MM-DD"`, NOT a full datetime.** The backend queries `start_time >= '2026-04-01T'`. Pass the date portion only.
- **Color field.** `AppointmentInput.color` is `string | null` (hex, e.g. `"#4A90E2"`). The FHIR resource stores it as an extension. `extractAppointmentDisplay` must read it from `resource.extension[]` where `url === "http://medarc.local/fhir/StructureDefinition/appointment-color"`.
- **`tsc --noEmit` exits 0 after each task.** No `as any`, no `@ts-ignore`.

---

## Common Pitfalls

- **`new Date("2026-04-01T09:00:00Z")` shifts the time.** The backend datetimes have no `Z`. If any display code appends `Z` before constructing a `Date`, all appointment times will be offset by the local timezone. Always split on `T` and parse hours/minutes manually or use the no-Z string as-is.
- **Calendar week boundary at Sunday vs. Monday.** Decide at T01 planning time: week starts on Sunday (US clinical standard) or Monday. Implement consistently in both the week grid header and the date-range query sent to `listAppointments`. Recommend: Sunday start for US clinical workflow.
- **`AppointmentRecord.resource` structure.** `start` and `end` live at the top level of the FHIR Appointment resource (`resource.start`, `resource.end`). `appt_type` is at `resource.serviceType[0].coding[0].code`. Color is in `resource.extension[]` filtered by URL. Do not guess paths — read `build_appointment_fhir` in `scheduling.rs` as the ground truth.
- **Flow Board refresh.** `getFlowBoard` is a one-shot query per date. After `updateFlowStatus`, the component must call `reload()` to re-fetch the board — there is no WebSocket push. The hook must expose a `reloadFlowBoard()` callback that re-runs `getFlowBoard`.
- **Recurrence end date validation.** When the user selects a recurrence pattern, `recurrenceEndDate` becomes required. The form must validate that `recurrenceEndDate` is set and is after `startTime` before calling `createAppointment`. Missing `recurrenceEndDate` with a non-null `recurrence` will create infinite occurrences until the backend's 52-occurrence cap.
- **`completeRecall` returns `void`.** The TypeScript wrapper is `invoke<void>`. Do not attempt to read a return value from it.
- **Modal state management.** The calendar page will need multiple modal states: `createOpen`, `editTarget: AppointmentRecord | null`, `cancelTarget: AppointmentRecord | null`. Keep them as independent `useState` booleans/values in `SchedulePage` (not in a reducer) — consistent with S04's `ClinicalSidebar` modal state pattern.
- **Day view vs. week view date range.** For day view, `listAppointments(startDate, endDate)` where `endDate` is the next day's date. For week view, `startDate` is the Sunday of the week, `endDate` is the following Sunday. A single `useSchedule` hook parameter controls which range is active.
- **Open-slot search returns slot objects, not appointment records.** `searchOpenSlots` returns `Record<string, unknown>[]` — each object has `start_time`, `end_time`, `duration_minutes`, `available`, `appt_type`. These are not `AppointmentRecord`s and cannot be passed to `extractAppointmentDisplay`.

---

## Open Risks

- **Patient name resolution on calendar cards.** The appointment backend returns `patientId`, not patient name. If users need to see patient names on calendar appointment cards, a secondary lookup is required (N+1 via `searchPatients` or `getPatient` per appointment). For MVP: show `apptType + time` only; reveal `patientId` in an expanded popover where clicking navigates to the patient chart. If the product decides patient names are required on cards, a new `getPatient` per appointment will be needed, or a batch endpoint would be required (not available in Phase 1).
- **Flow Board auto-refresh.** Clinic flow boards in production need periodic auto-refresh (polling) since multiple staff update statuses simultaneously. The S05 UI is single-user per session, so manual refresh is acceptable for MVP. Tauri does not have a WebSocket channel to push updates. If real-time multi-user updates are a requirement, a polling interval (e.g. 30s `setInterval`) must be added to `useSchedule`. Flag this as a follow-up risk if not resolved before execution.
- **`apptType` values are free-text.** The backend accepts any string for `appt_type`. The form must provide a bounded `<select>` of meaningful options (e.g. `new_patient`, `follow_up`, `procedure`, `telehealth`, `annual_wellness`, `urgent`). Hard-code this list in the form constant; it can be made configurable in a later milestone.
- **Color picker UX.** The `AppointmentInput.color` field is a hex string. A native `<input type="color">` renders a system color picker in WKWebView but its visual styling is platform-controlled. For MVP, provide a fixed palette of 6–8 named colors as selectable swatches instead — avoids the `<input type="color">` cross-platform issues and makes appointment types visually consistent.

---

## Planned Component Tree

```
SchedulePage (src/pages/SchedulePage.tsx)        ← replaces the stub; owns useSchedule
  CalendarPage (src/components/scheduling/CalendarPage.tsx)
    CalendarHeader (day/week toggle, prev/next nav)
    CalendarGrid (day view: single column; week view: 7 columns)
      TimeGutter (00:00–23:00 row labels)
      AppointmentCard[]
    AppointmentFormModal (src/components/scheduling/AppointmentFormModal.tsx)
  FlowBoardPage (src/components/scheduling/FlowBoardPage.tsx)
    FlowBoardEntry[] (patient cards with status transition buttons)
  WaitlistPanel (src/components/scheduling/WaitlistPanel.tsx)
  RecallPanel (src/components/scheduling/RecallPanel.tsx)
```

New files to create:
- `src/hooks/useSchedule.ts` — data backbone; exports appointments, flowBoard, waitlist, recalls, all mutations
- `src/lib/fhirExtract.ts` — append `extractAppointmentDisplay` (do not create a new file)
- `src/components/scheduling/CalendarPage.tsx`
- `src/components/scheduling/FlowBoardPage.tsx`
- `src/components/scheduling/AppointmentFormModal.tsx`
- `src/components/scheduling/WaitlistPanel.tsx`
- `src/components/scheduling/RecallPanel.tsx`

Files to modify:
- `src/pages/SchedulePage.tsx` — full replacement of the stub

---

## RBAC Summary for Scheduling

| Role | Create Appt | Cancel Appt | Update Flow Status | Waitlist CRUD | Recall CRUD |
|---|---|---|---|---|---|
| FrontDesk | ✅ | ✅ | ✅ | ✅ | ✅ |
| NurseMa | ✅ | ✅ | ✅ | ✅ | ✅ |
| Provider | ✅ | ✅ | ✅ | ✅ | ✅ |
| BillingStaff | ❌ (read-only) | ❌ | ❌ | ❌ (read-only) | ❌ (read-only) |
| SystemAdmin | ✅ | ✅ | ✅ | ✅ | ✅ |

Enforce via a `canWrite(role)` helper in `SchedulePage` (same pattern as `ClinicalSidebar`). BillingStaff sees the calendar and flow board in read-only mode; create/edit buttons are hidden.

---

## FHIR Extraction — AppointmentDisplay

`extractAppointmentDisplay` should read from the FHIR Appointment `resource` blob:
```
start:        resource.start                                  → string | null
end:          resource.end                                    → string | null
durationMin:  resource.minutesDuration                        → number | null
status:       resource.status                                 → string | null ("booked"|"cancelled"|"noshow"|…)
apptType:     resource.serviceType[0].coding[0].code          → string | null
apptTypeDisplay: resource.serviceType[0].coding[0].display    → string | null
reason:       resource.reason[0].text                         → string | null
color:        resource.extension[].url === "…/appointment-color" → valueString → string | null
recurrence:   resource.extension[].url === "…/appointment-recurrence" → valueString → string | null
recurrenceGroup: resource.extension[].url === "…/appointment-recurrence-group" → valueId → string | null
notes:        resource.extension[].url === "…/appointment-notes" → valueString → string | null
```

Source of truth: `build_appointment_fhir()` in `src-tauri/src/commands/scheduling.rs` (lines ~200-260 in the file).

---

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React / TypeScript | (core stack — no external skill needed) | n/a |
| Tauri 2.x IPC | (established patterns in codebase) | n/a |
| CSS Grid calendar | (no specialized skill available) | none found |

---

## Sources

- Rust scheduling command source (`src-tauri/src/commands/scheduling.rs`) — authoritative for FHIR structure, param names, return types, validation rules (5–60 min duration, flow status state machine, slot search working hours 08:00–17:00, 52-occurrence recurrence cap).
- `src/types/scheduling.ts` — complete TypeScript type layer; no new types needed.
- `src/lib/tauri.ts` — all 13 scheduling command wrappers with exact invoke key names.
- `DECISIONS.md` (M002/S01 section) — critical gotchas: `createAppointment` returns `AppointmentRecord[]`, `cancelAppointment` needs `reason ?? null`, `listRecalls` has no `patient_id`, `listWaitlist` has no `patient_id`, `completeRecall` returns `void`.
- S04 task summaries (T01, T02, T03) — established patterns for hook structure, FHIR extract helpers, modal overlay, per-domain error isolation, `canWrite()` RBAC helper, `tsc --noEmit` as verification gate.
