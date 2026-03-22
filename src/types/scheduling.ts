/**
 * TypeScript types for appointment scheduling, waitlist, recall, and
 * patient flow board management.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Appointment types (SCHD-01 through SCHD-04)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating a new appointment. */
export interface AppointmentInput {
  /** Patient the appointment is for. */
  patientId: string;
  /** Provider (user ID) who will see the patient. */
  providerId: string;
  /** ISO 8601 datetime for the appointment start (e.g. "2026-04-01T09:00:00"). */
  startTime: string;
  /** Duration in minutes — must be 5–60 (inclusive). */
  durationMinutes: number;
  /** Category/type — e.g. "new_patient", "follow_up", "procedure", "telehealth". */
  apptType: string;
  /** Hex color code for calendar display (e.g. "#4A90E2"). */
  color: string | null;
  /** Free-text reason for the visit. */
  reason: string | null;
  /** Recurrence rule: None | "weekly" | "biweekly" | "monthly" */
  recurrence: string | null;
  /** If recurring — ISO 8601 date on which recurrence ends (e.g. "2026-12-31"). */
  recurrenceEndDate: string | null;
  /** Additional notes. */
  notes: string | null;
}

/** Appointment record returned to callers. */
export interface AppointmentRecord {
  id: string;
  patientId: string;
  providerId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

/** Input for updating an appointment (status, time, duration, etc.). */
export interface UpdateAppointmentInput {
  /** New start time (ISO 8601 datetime), if changing. */
  startTime: string | null;
  /** New duration in minutes, if changing. */
  durationMinutes: number | null;
  /** New status: "proposed" | "pending" | "booked" | "arrived" | "fulfilled" | "cancelled" | "noshow" */
  status: string | null;
  /** New reason. */
  reason: string | null;
  /** New notes. */
  notes: string | null;
  /** New provider, if reassigning. */
  providerId: string | null;
  /** New color. */
  color: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Waitlist types (SCHD-06)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for adding a patient to the waitlist. */
export interface WaitlistInput {
  /** Patient to add to the waitlist. */
  patientId: string;
  /** Preferred provider (user ID), if any. */
  providerId: string | null;
  /** Preferred appointment type. */
  apptType: string;
  /** ISO 8601 date — earliest date patient can be seen (e.g. "2026-04-01"). */
  preferredDate: string;
  /** Priority: 1 (urgent) – 5 (routine). Defaults to 3. */
  priority: number | null;
  /** Reason for the visit. */
  reason: string | null;
  /** Additional notes. */
  notes: string | null;
}

/** Waitlist record returned to callers. */
export interface WaitlistRecord {
  id: string;
  patientId: string;
  providerId: string | null;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Recall types (SCHD-07)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating a recall entry. */
export interface RecallInput {
  /** Patient who needs to be recalled. */
  patientId: string;
  /** Provider who should see the patient. */
  providerId: string | null;
  /** ISO 8601 date by which the patient should return (e.g. "2026-07-01"). */
  dueDate: string;
  /** Type of follow-up: "routine", "urgent", "post_procedure", "preventive", etc. */
  recallType: string;
  /** Reason / clinical indication for the recall. */
  reason: string;
  /** Additional notes. */
  notes: string | null;
}

/** Recall record returned to callers. */
export interface RecallRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow Board types (SCHD-05)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for updating a patient's flow board status. */
export interface UpdateFlowStatusInput {
  /** The appointment being tracked. */
  appointmentId: string;
  /** New flow status: "scheduled" | "checked_in" | "roomed" | "with_provider" | "checkout" | "completed" */
  flowStatus: string;
  /** Room number or name, if applicable. */
  room: string | null;
  /** Notes about the status transition. */
  notes: string | null;
}

/** Patient Flow Board entry returned to callers. */
export interface FlowBoardEntry {
  appointmentId: string;
  patientId: string;
  providerId: string;
  flowStatus: string;
  startTime: string;
  apptType: string;
  color: string | null;
  room: string | null;
  checkedInAt: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Calendar Settings
// ─────────────────────────────────────────────────────────────────────────────

/** Calendar display settings stored in app_settings. */
export interface CalendarSettings {
  /** Whether to show Saturday in the week view. */
  showSaturday: boolean;
  /** Whether to show Sunday in the week view. */
  showSunday: boolean;
  /** Start hour for the calendar grid (5-10, default 6). */
  startHour: number;
  /** End hour for the calendar grid (17-22, default 20). */
  endHour: number;
  /** Default appointment duration in minutes (15/30/45/60, default 60). */
  defaultDurationMinutes: number;
  /** Default calendar view: "day" or "week". */
  defaultView: string;
  /** Height in pixels per hour in the calendar grid (40/60/80, default 60). */
  hourHeightPx: number;
  /** Whether to show dotted half-hour lines in the calendar grid. */
  showHalfHourLines: boolean;
}
