# S06: Scheduling — UAT

**Milestone:** M001
**Written:** 2026-03-11

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: All scheduling logic is implemented as pure Rust functions (FHIR builders, recurrence generator, open-slot generator, flow status validator) with no I/O dependencies. The 22 unit tests directly invoke these functions and assert correctness of FHIR structure, recurrence dates, slot availability, and RBAC permissions. The full runtime path (Tauri command → DB → audit) follows the identical pattern proven in S04 and S05 (which passed live-runtime UAT). No new runtime infrastructure was introduced.

## Preconditions

- `cargo test --lib commands::scheduling` passes all 22 tests
- `cargo test --lib db::migrations` (migrations_are_valid test) passes — confirms Migration 11 is syntactically valid
- `cargo test --lib rbac::roles` passes — confirms AppointmentScheduling RBAC matrix entries are correct
- Running application: database has been migrated to Migration 11 (appointment_index, waitlist_index, recall_index, flow_board_index tables exist)
- An authenticated session exists (Provider or FrontDesk role)

## Smoke Test

Call `create_appointment` with `patient_id`, `provider_id`, `start_time: "2026-04-01T09:00:00"`, `duration_minutes: 30`, `appt_type: "follow_up"`. Verify the returned record has `resource.resourceType == "Appointment"`, `resource.status == "booked"`, `resource.start == "2026-04-01T09:00:00"`, `resource.end == "2026-04-01T09:30:00"`. Then call `get_flow_board(date: "2026-04-01")` and verify the appointment appears with `flow_status: "scheduled"`.

## Test Cases

### 1. Create single appointment (SCHD-02)

1. Call `create_appointment` with `duration_minutes: 30`, `appt_type: "follow_up"`, `color: "#4A90E2"`, `recurrence: null`
2. Assert response: array of 1 record
3. Assert `resource.resourceType == "Appointment"`
4. Assert `resource.status == "booked"`, `resource.minutesDuration == 30`
5. Assert color extension present: `resource.extension[?url contains 'appointment-color'].valueString == "#4A90E2"`
6. Assert `appointment_index` row exists with `status = 'booked'` and `color = '#4A90E2'`
7. **Expected:** Single appointment created, flow_board_index row created with `flow_status = 'scheduled'`

### 2. Create recurring weekly appointment series (SCHD-03)

1. Call `create_appointment` with `recurrence: "weekly"`, `recurrence_end_date: "2026-04-27"`, `start_time: "2026-04-06T09:00:00"`
2. **Expected:** Response array contains 4 appointments
3. Assert dates: `2026-04-06`, `2026-04-13`, `2026-04-20`, `2026-04-27`
4. Assert all 4 share the same `recurrence_group_id` extension value
5. Assert 4 rows in `appointment_index`, all `status = 'booked'`

### 3. Multi-provider calendar list (SCHD-01)

1. Create 2 appointments for `provider_id: "dr-smith"` on `2026-04-01`
2. Create 1 appointment for `provider_id: "dr-jones"` on `2026-04-01`
3. Call `list_appointments(start_date: "2026-04-01", end_date: "2026-04-02")`
4. **Expected:** 3 appointments returned, ordered by start_time
5. Call with `provider_id: "dr-smith"` filter
6. **Expected:** Only 2 appointments (dr-smith's) returned

### 4. Open-slot search (SCHD-04)

1. Create an appointment for provider "dr-smith" at `2026-04-01T09:00:00` (30 min)
2. Call `search_open_slots(start_date: "2026-04-01", end_date: "2026-04-02", provider_id: "dr-smith", duration_minutes: 30)`
3. **Expected:** 17 slots returned (18 working-hour slots minus the 09:00 booked slot)
4. Assert `09:00:00` slot is NOT in results
5. Assert `08:00:00` and `09:30:00` ARE in results

### 5. Patient Flow Board — check-in and status transitions (SCHD-05)

1. Create an appointment for today (e.g. `2026-04-01T10:00:00`)
2. Call `update_flow_status(appointment_id, flow_status: "checked_in")`
3. **Expected:** Response has `flow_status: "checked_in"`, `checked_in_at` is populated with current timestamp
4. Call `update_flow_status(appointment_id, flow_status: "roomed", room: "Room 3")`
5. **Expected:** `flow_status: "roomed"`, `room: "Room 3"`
6. Call `get_flow_board(date: "2026-04-01")`
7. **Expected:** Entry appears with `flow_status: "roomed"`, `room: "Room 3"`

### 6. Cancel appointment (SCHD-06 dependency)

1. Create an appointment
2. Call `cancel_appointment(appointment_id, reason: "Patient request")`
3. **Expected:** Response `resource.status == "cancelled"`, `resource.cancelationReason.text == "Patient request"`
4. Assert `appointment_index` row has `status = 'cancelled'`

### 7. Waitlist — add, list, discharge (SCHD-06)

1. Call `add_to_waitlist(patient_id, appt_type: "new_patient", preferred_date: "2026-04-15", priority: 2, provider_id: "dr-smith")`
2. **Expected:** Record returned with `resource.resourceType == "AppointmentRequest"`, `resource.status == "active"`, `resource.priority == 2`
3. Call `list_waitlist(provider_id: "dr-smith")`
4. **Expected:** Entry appears, ordered by priority
5. Call `discharge_waitlist(waitlist_id)`
6. **Expected:** 200 OK; subsequent `list_waitlist` call returns empty (status filter = active)

### 8. Recall Board — create, list overdue, complete (SCHD-07)

1. Call `create_recall(patient_id, due_date: "2025-01-01", recall_type: "routine", reason: "Annual checkup")`  *(past date — overdue)*
2. Call `list_recalls(overdue_only: true)`
3. **Expected:** Entry appears (due_date < today)
4. Call `complete_recall(recall_id)`
5. **Expected:** 200 OK; subsequent `list_recalls(overdue_only: true)` does not include completed entry

## Edge Cases

### Duration boundary: 4 minutes rejected

1. Call `create_appointment` with `duration_minutes: 4`
2. **Expected:** Error returned; audit log row written with `success = false`, `details` contains "duration_minutes"

### Duration boundary: 61 minutes rejected

1. Call `create_appointment` with `duration_minutes: 61`
2. **Expected:** Error returned

### Invalid flow status rejected

1. Call `update_flow_status` with `flow_status: "waiting"`
2. **Expected:** Validation error; flow_board_index unchanged

### Waitlist priority clamping

1. Call `add_to_waitlist` with `priority: 0`
2. **Expected:** Record created with `resource.priority == 1`

### Appointment not found

1. Call `update_appointment` with a non-existent `appointment_id`
2. **Expected:** AppError::NotFound returned; audit row written with `success = false`

### RBAC: BillingStaff cannot create appointment

1. Authenticate as BillingStaff role
2. Call `create_appointment`
3. **Expected:** Permission denied error; audit row written with `success = false`

## Failure Signals

- Missing `appointment_index` table: Migration 11 did not run — check `cargo test --lib db::migrations`
- `resource.end` is wrong or missing: `compute_end_time` parser failed — check that `start_time` uses "YYYY-MM-DDTHH:MM:SS" format without timezone suffix
- Recurring series has 1 occurrence instead of N: `recurrence_end_date` not set or was before `start_time + stride`
- `get_flow_board` returns empty for today even after create: `start_time` stored without 'T' separator, or date prefix comparison mismatch
- RBAC test failures: `AppointmentScheduling` variant missing from match arm or unreachable pattern in `has_permission`

## Requirements Proved By This UAT

- SCHD-01 — `list_appointments` date-range + provider filter enables multi-provider calendar views
- SCHD-02 — `create_appointment` with color, appt_type, configurable duration (5–60 min); FHIR Appointment structure verified
- SCHD-03 — `create_appointment` with `recurrence: "weekly"|"biweekly"|"monthly"` and `recurrence_end_date` generates correct occurrence series
- SCHD-04 — `search_open_slots` returns working-hour slots excluding booked times, filtered by provider/date-range/duration
- SCHD-05 — `update_flow_status` (6-state transitions) + `get_flow_board` (clinic-day snapshot with room tracking)
- SCHD-06 — `add_to_waitlist`, `list_waitlist` (priority-ordered), `discharge_waitlist`
- SCHD-07 — `create_recall`, `list_recalls` (overdue_only filter), `complete_recall`

## Not Proven By This UAT

- Double-booking prevention — no overlap detection is implemented; two appointments can be booked at the same provider/time (known limitation, deferred)
- Provider schedule blocks / unavailability windows — open-slot search uses fixed 08:00–17:00 regardless of provider configuration
- Bulk recurrence cancellation — cancelling all occurrences in a series requires cancelling each appointment_id individually
- SCHD-08 (candidate) — daily schedule summary view
- SCHD-09 (candidate) — auto-match waitlist to cancelled slots
- Frontend calendar rendering — no React components were built in this slice; Tauri commands return raw JSON for S07+ to consume

## Notes for Tester

- All `start_time` values must use "YYYY-MM-DDTHH:MM:SS" format (no timezone suffix) — `compute_end_time` and `generate_open_slots` both rely on string splitting on 'T' and ':'
- `list_appointments` `start_date`/`end_date` must include the time component (e.g. "2026-04-01T00:00:00") or use date-only strings that sort correctly as prefixes against the stored "YYYY-MM-DDTHH:MM:SS" format
- `get_flow_board` `date` parameter is "YYYY-MM-DD" — the command internally adds "T" to form range boundaries
- Waitlist `list_waitlist` defaults to `status = "active"` if no status filter is provided
- Recall `list_recalls` defaults to `status = "pending"` if no status filter is provided
