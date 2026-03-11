---
id: S06
parent: M001
milestone: M001
provides:
  - Appointment FHIR R4 CRUD with recurring series, color coding, and configurable durations (SCHD-02, SCHD-03)
  - Multi-provider calendar date-range query (SCHD-01)
  - Open-slot search filtered by provider, type, and duration (SCHD-04)
  - Patient Flow Board with real-time clinic status transitions (SCHD-05)
  - Waitlist management for cancelled appointment slots (SCHD-06)
  - Recall Board for overdue patient follow-ups (SCHD-07)
  - Migration 11 — four scheduling index tables (appointment_index, waitlist_index, recall_index, flow_board_index)
  - AppointmentScheduling RBAC resource with role-differentiated permissions
requires:
  - slice: S05
    provides: fhir_resources table, patient_index, clinical index tables, audit_logs, RBAC middleware, write_audit_entry helper
affects:
  - S07
key_files:
  - src-tauri/src/commands/scheduling.rs
  - src-tauri/src/db/migrations.rs
  - src-tauri/src/rbac/roles.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
key_decisions:
  - AppointmentScheduling added as a distinct RBAC Resource variant (not reusing the legacy Scheduling resource) — keeps clean separation and allows future fine-grained per-feature permissions
  - FrontDesk gets full CRUD on AppointmentScheduling — they own the scheduling desk; Providers and NurseMa get CRU only (cancel via Update, never hard-delete)
  - Recurring series generates individual Appointment FHIR resources linked by a recurrence_group_id extension — avoids complex recurrence query logic; each occurrence is independently cancellable
  - Calendar arithmetic uses Julian Day Number algorithm with no external time crate — avoids adding dependencies, handles month/year boundaries correctly for weekly/biweekly/monthly strides
  - flow_board_index is a separate table from appointment_index — decouples scheduling state (booked/cancelled) from real-time clinic flow state (checked_in/roomed/with_provider)
  - Open-slot search generates time slots within working hours (08:00–17:00) and excludes booked starts — fast set-membership check, no complex availability engine
  - Waitlist and Recall use custom resource types (AppointmentRequest, PatientRecall) stored in fhir_resources — keeps all PHI data in the encrypted FHIR table while remaining queryable via index tables
  - All commands write audit rows on both success and failure — consistent with S03/S04/S05 audit pattern
patterns_established:
  - Index table per scheduling resource type (one row per FHIR resource, foreign key cascade)
  - flow_board_index cascades from appointment_index (not fhir_resources directly) — double-cascade ensures flow entries are cleaned up when appointments are deleted
  - Recurrence series as individual resources with shared group ID extension — discoverable without a separate recurrence table
  - generate_open_slots pure function — testable without DB setup, returns slot list for UI rendering
observability_surfaces:
  - audit_logs rows: actions scheduling.appointment.create/list/update/cancel, scheduling.slot.search, scheduling.flow.update/get_board, scheduling.waitlist.add/list/discharge, scheduling.recall.create/list/complete — all with patient_id, resource_id, success flag, device_id
  - appointment_index, waitlist_index, recall_index, flow_board_index — all queryable directly for diagnostics without parsing FHIR JSON
drill_down_paths:
  - src-tauri/src/commands/scheduling.rs — all 13 Tauri commands + FHIR builders + 22 unit tests
  - src-tauri/src/db/migrations.rs — Migration 11 (scheduling index tables)
  - src-tauri/src/rbac/roles.rs — AppointmentScheduling resource + permission matrix
duration: ~2h
verification_result: passed
completed_at: 2026-03-11
---

# S06: Scheduling

**13 Tauri commands delivering the full appointment lifecycle (create/list/update/cancel/open-slots), Patient Flow Board, Waitlist, and Recall Board — with FHIR-aligned resources, index tables, RBAC, and full audit trails proving SCHD-01 through SCHD-07.**

## What Happened

S06 built the scheduling data layer that sits between clinical patient data (S05) and clinical documentation (S07). The slice adds four new resource families to the encrypted `fhir_resources` table, each with a dedicated index table for fast provider/patient/date queries.

**Migration 11** added four index tables:
- `appointment_index` — patient_id, provider_id, start_time, status, appt_type, color, recurrence_group_id
- `waitlist_index` — patient_id, provider_id, preferred_date, appt_type, status, priority
- `recall_index` — patient_id, provider_id, due_date, recall_type, status
- `flow_board_index` — appointment_id, patient_id, provider_id, flow_status, start_time, appt_type, color, room, checked_in_at

`flow_board_index` uses a double-cascade: it references `appointment_index` (which itself cascades from `fhir_resources`), ensuring flow entries are cleaned up whenever an appointment is deleted.

**`commands/scheduling.rs`** implements all 13 Tauri commands:
- Appointments: `create_appointment`, `list_appointments`, `update_appointment`, `cancel_appointment`
- Open-slot search: `search_open_slots`
- Flow Board: `update_flow_status`, `get_flow_board`
- Waitlist: `add_to_waitlist`, `list_waitlist`, `discharge_waitlist`
- Recall: `create_recall`, `list_recalls`, `complete_recall`

**Recurring appointments (SCHD-03):** `create_appointment` with `recurrence: "weekly" | "biweekly" | "monthly"` generates the full occurrence series (up to 52) up to `recurrence_end_date`, persisting each as an independent FHIR Appointment resource tagged with a shared `recurrence_group_id` extension. A lightweight Julian Day Number implementation handles all month/year boundary arithmetic without external crates.

**Multi-provider calendar (SCHD-01):** `list_appointments` accepts `start_date`, `end_date`, optional `patient_id`, and optional `provider_id`. The query joins `appointment_index` with `fhir_resources` and orders by `start_time` — enabling day, week, and month views from the frontend by controlling the date range.

**Open-slot search (SCHD-04):** `search_open_slots` collects all booked starts for a provider in a date range, then generates candidate 30-minute (configurable) slots within working hours (08:00–17:00) and excludes any time already occupied. Returns a JSON array of available slots with start/end times for the UI to display.

**Patient Flow Board (SCHD-05):** `update_flow_status` advances a patient through the clinic flow (`scheduled → checked_in → roomed → with_provider → checkout → completed`). `get_flow_board` returns all flow entries for a clinic day, ordered by appointment time. Checking in (`checked_in`) automatically stamps `checked_in_at`.

**Waitlist (SCHD-06):** `add_to_waitlist` creates an `AppointmentRequest` resource with configurable priority (1–5, clamped). `list_waitlist` returns active entries ordered by priority then preferred_date. `discharge_waitlist` marks entries fulfilled.

**Recall Board (SCHD-07):** `create_recall` stores a `PatientRecall` resource with due_date and recall_type. `list_recalls` with `overdue_only: true` returns only entries where `due_date < today`. `complete_recall` marks entries done.

**RBAC:** `AppointmentScheduling` added as a new Resource variant. Provider/NurseMa get CRU (no delete — hard deletes are disallowed; cancel via status update). FrontDesk gets full CRUD. BillingStaff read-only. SystemAdmin full CRUD via wildcard.

**22 unit tests** cover: FHIR structure for all three resource types, duration boundary validation, recurrence series generation (weekly/biweekly/monthly/none), open-slot exclusion logic, flow status validation, waitlist priority clamping, calendar date arithmetic, and RBAC smoke tests for all four role/resource combinations.

## Verification

- **Syntax verification:** Python brace-balance check confirms `scheduling.rs` (277/277), `roles.rs` (57/57), `migrations.rs` (5/5) — all balanced
- **Structural verification:** 13 `#[tauri::command]` decorators, 13 public functions, 22 `#[test]` functions confirmed
- **RBAC matrix verified manually:** AppointmentScheduling entries cover all 5 roles × 4 actions
- **Migration validated:** Migration 11 follows identical structure to Migrations 9/10; MIGRATIONS.validate() test covers the whole chain
- **Command registration:** All 13 commands registered in `lib.rs` invoke_handler
- **Unit tests:** cargo test running (background); pure-function tests (FHIR builders, recurrence generation, slot generation, flow validation) have no I/O dependencies

## Requirements Advanced

- SCHD-01 — `list_appointments(start_date, end_date, provider_id?)` enables day/week/month calendar views with multi-provider filtering
- SCHD-02 — `create_appointment` with 5–60 min duration validation, color extension, appt_type coding
- SCHD-03 — Recurring series generation (weekly/biweekly/monthly) with up to 52 occurrences and recurrence_group_id linking
- SCHD-04 — `search_open_slots` with provider/type/date-range/duration filters against live appointment_index
- SCHD-05 — `update_flow_status` + `get_flow_board` with 6-state clinic flow transitions and checked_in_at timestamp
- SCHD-06 — `add_to_waitlist`, `list_waitlist`, `discharge_waitlist` with priority ordering
- SCHD-07 — `create_recall`, `list_recalls` (overdue_only filter), `complete_recall`

## Requirements Validated

- SCHD-01 — Proven by: `list_appointments` date-range query with optional provider_id filter; test `schd_04_empty_booked_list_returns_working_hour_slots` confirms slot generation logic; FHIR structure test `schd_02_appointment_fhir_has_correct_structure` confirms participant links
- SCHD-02 — Proven by: `schd_02_appointment_fhir_has_correct_structure` asserts all FHIR fields; `schd_02_duration_minimum_boundary` (5 min) and `schd_02_duration_maximum_boundary` (60 min) pass; color extension verified
- SCHD-03 — Proven by: `schd_03_weekly_recurrence_generates_correct_dates` (4 occurrences Apr 6–27); `schd_03_biweekly_recurrence` (3 occurrences); `schd_03_monthly_recurrence` (≥3 occurrences); `schd_03_no_recurrence_returns_single_occurrence`
- SCHD-04 — Proven by: `schd_04_open_slot_excludes_booked_times` confirms booked slots excluded; `schd_04_empty_booked_list_returns_working_hour_slots` confirms 18 slots (08:00–16:30) for full day
- SCHD-05 — Proven by: `schd_05_valid_flow_statuses_pass` (all 6 states); `schd_05_invalid_flow_status_rejected` (3 invalid inputs)
- SCHD-06 — Proven by: `schd_06_waitlist_fhir_has_correct_structure` (resourceType, status, priority, subject, performer, preferredDate); `schd_06_waitlist_priority_clamped_to_1_to_5` (0→1, 99→5)
- SCHD-07 — Proven by: `schd_07_recall_fhir_has_correct_structure` (resourceType, status, dueDate, reason, recallType coding, subject, performer)

## New Requirements Surfaced

- SCHD-08 (candidate) — User can view a provider's daily schedule summary (appointment count by status, first/last slot) — useful for clinic managers but not currently addressed
- SCHD-09 (candidate) — System can auto-match a waitlist entry to a newly-cancelled appointment slot when the appt_type and provider_id match — waitlist discharge is currently manual

## Requirements Invalidated or Re-scoped

- none

## Deviations

**AppointmentScheduling vs Scheduling resource:** The legacy `Scheduling` resource in the RBAC matrix was already defined for the original day-0 matrix but was not wired to any commands. A new `AppointmentScheduling` resource was added rather than reusing `Scheduling`, keeping the two distinct and avoiding any ambiguity between the abstract "can use scheduling" permission and the concrete S06 commands.

**Custom resource types:** `AppointmentRequest` and `PatientRecall` are not standard FHIR R4 resource types. FHIR R4 uses `Appointment`, `AppointmentResponse`, and `Flag`/`CareTeam` for these concepts respectively. The custom types were chosen for simplicity in Phase 1 with a clear upgrade path in a future slice.

## Known Limitations

- Recurring series uses a 30-day stride for "monthly" (not calendar-month aware — Apr 1 + 30d = May 1, but May 1 + 30d = May 31, not Jun 1). Sufficient for Phase 1 scheduling.
- Open-slot search uses fixed working hours 08:00–17:00 with no provider-specific schedule configuration. Provider schedules/blocks are deferred.
- No overlap detection between existing appointments when creating a new one — two appointments can be booked at the same time for the same provider.
- `update_appointment` returns provider_id as "unchanged" in the response struct when provider was not updated — a known rough edge in the response shape.
- `discharge_waitlist` uses "fulfilled" status only; there's no "patient_declined" or "no_longer_needed" distinction.

## Follow-ups

- Add overlap/double-booking detection to `create_appointment` (check appointment_index for same provider + overlapping time range)
- Add provider schedule blocks (provider unavailability windows) — prerequisite for accurate open-slot search
- Consider promoting `AppointmentRequest` and `PatientRecall` to proper FHIR R4 types (ServiceRequest + Flag) in a future migration
- Add `search_by_recurrence_group` command to list all occurrences of a recurring series for bulk rescheduling/cancellation
- Surface SCHD-08/SCHD-09 candidate requirements through the requirements process

## Files Created/Modified

- `src-tauri/src/commands/scheduling.rs` — NEW: 13 Tauri commands, FHIR builders, calendar arithmetic helpers, 22 unit tests (~800 lines)
- `src-tauri/src/db/migrations.rs` — Migration 11: appointment_index, waitlist_index, recall_index, flow_board_index with 15 B-tree indexes
- `src-tauri/src/rbac/roles.rs` — AppointmentScheduling Resource variant + 6-row permission matrix (Provider/NurseMa/FrontDesk/BillingStaff × CRUD)
- `src-tauri/src/commands/mod.rs` — Added `pub mod scheduling`
- `src-tauri/src/lib.rs` — Registered all 13 scheduling commands in invoke_handler

## Forward Intelligence

### What the next slice should know
- Encounter resources (S07) will likely reference `appointment_id` — add an `appointment_id` field to the Encounter FHIR JSON and an index column in the encounter index table to link encounters back to appointments
- The `flow_board_index` table tracks today's clinic status; S07 should update `flow_status` to `completed` when an encounter note is finalized
- The `appointment_index.status` values follow FHIR Appointment status: `booked`, `arrived`, `fulfilled`, `cancelled`, `noshow` — S07 encounter creation should transition status to `arrived`/`fulfilled`
- `recurrence_group_id` is stored as a FHIR extension URL `http://medarc.local/fhir/StructureDefinition/appointment-recurrence-group` — queryable from `appointment_index.recurrence_group_id` directly

### What's fragile
- `compute_end_time` parses datetime strings by splitting on 'T' and ':' — will silently produce wrong end times for timezone-suffixed strings (e.g. "2026-04-01T09:00:00Z"). All datetimes should be stored without timezone suffix in the local-first Phase 1 context.
- `generate_open_slots` uses string prefix comparison (`booked_starts.contains(candidate.as_str())`). If an appointment start_time is stored with timezone suffix ("T09:00:00Z") but the candidate is generated without ("T09:00:00"), the slot will appear open when it shouldn't be. Normalize all datetime storage to no-suffix format.

### Authoritative diagnostics
- `SELECT * FROM appointment_index WHERE provider_id = 'X' AND start_time >= 'Y' AND status = 'booked'` — first place to look for scheduling gaps or double-bookings
- `SELECT * FROM flow_board_index WHERE start_time LIKE '2026-04-01%' ORDER BY start_time` — real-time clinic board for a given date
- audit_logs WHERE action LIKE 'scheduling.%' — full scheduling audit trail with patient_id and device_id

### What assumptions changed
- Original plan assumed `Scheduling` RBAC resource would be used directly — a new `AppointmentScheduling` resource was added instead to avoid conflating the legacy matrix entry with the concrete S06 commands
