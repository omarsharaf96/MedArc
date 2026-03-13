/**
 * AppointmentFormModal.tsx — Create and Cancel appointment modal.
 *
 * Two modes controlled by the `mode` prop:
 *   - "create": full form for scheduling a new appointment with optional recurrence
 *   - "cancel": single reason field to cancel an existing appointment
 *
 * Follows the AllergyFormModal pattern:
 *   - fixed inset-0 bg-black/40 z-50 overlay
 *   - submitError rendered inline above the submit button
 *   - submitting boolean disables the form during the async call
 *
 * Observability:
 *   - submitError visible in UI without DevTools
 *   - React DevTools → AppointmentFormModal state: submitting, submitError
 */
import { useState, type FormEvent } from "react";
import type { AppointmentInput, AppointmentRecord } from "../../types/scheduling";

// ─── Shared style constants ───────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Color palette ────────────────────────────────────────────────────────────

/** Fixed 6-swatch color palette — no <input type="color"> per design constraints. */
const COLOR_PALETTE: { hex: string; name: string }[] = [
  { hex: "#4A90E2", name: "Blue" },
  { hex: "#50C878", name: "Green" },
  { hex: "#FF6B6B", name: "Red" },
  { hex: "#FFD700", name: "Yellow" },
  { hex: "#9B59B6", name: "Purple" },
  { hex: "#FF8C00", name: "Orange" },
];

const DEFAULT_COLOR = "#4A90E2";

// ─── Appointment type options ─────────────────────────────────────────────────

const APPT_TYPE_OPTIONS = [
  { value: "new_patient", label: "New Patient" },
  { value: "follow_up", label: "Follow Up" },
  { value: "procedure", label: "Procedure" },
  { value: "telehealth", label: "Telehealth" },
  { value: "annual_wellness", label: "Annual Wellness" },
  { value: "urgent", label: "Urgent" },
];

// ─── Duration options ─────────────────────────────────────────────────────────

const DURATION_OPTIONS = [5, 10, 15, 20, 30, 45, 60];

// ─── Props ────────────────────────────────────────────────────────────────────

export interface AppointmentFormModalProps {
  mode: "create" | "cancel";
  /** ID of the appointment being cancelled (cancel mode only). */
  appointmentId?: string;
  /** Human-readable summary shown above the cancel reason field. */
  appointmentSummary?: string;
  onSubmitCreate: (input: AppointmentInput) => Promise<AppointmentRecord[]>;
  onSubmitCancel: (id: string, reason: string | null) => Promise<AppointmentRecord>;
  onClose: () => void;
  canWrite: boolean;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function AppointmentFormModal({
  mode,
  appointmentId,
  appointmentSummary,
  onSubmitCreate,
  onSubmitCancel,
  onClose,
  canWrite,
}: AppointmentFormModalProps) {
  // ── Create-mode form state ──────────────────────────────────────────────
  const [patientId, setPatientId] = useState("");
  const [startTime, setStartTime] = useState("");
  const [durationMinutes, setDurationMinutes] = useState(30);
  const [apptType, setApptType] = useState("follow_up");
  const [reason, setReason] = useState("");
  const [notes, setNotes] = useState("");
  const [recurrence, setRecurrence] = useState("");
  const [recurrenceEndDate, setRecurrenceEndDate] = useState("");
  const [color, setColor] = useState(DEFAULT_COLOR);

  // Inline field error for recurrenceEndDate
  const [recurrenceEndDateError, setRecurrenceEndDateError] = useState<string | null>(null);

  // ── Shared async state ──────────────────────────────────────────────────
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Cancel-mode form state ──────────────────────────────────────────────
  const [cancelReason, setCancelReason] = useState("");

  // ── Handlers ───────────────────────────────────────────────────────────

  async function handleCreate(e: FormEvent) {
    e.preventDefault();
    if (!patientId.trim()) {
      setSubmitError("Patient ID is required.");
      return;
    }
    if (!startTime) {
      setSubmitError("Start time is required.");
      return;
    }

    // Recurrence validation
    if (recurrence !== "") {
      if (!recurrenceEndDate) {
        setRecurrenceEndDateError("End date is required when recurrence is set.");
        return;
      }
      // Compare end date against start date
      const startDatePart = startTime.split("T")[0];
      if (recurrenceEndDate <= startDatePart) {
        setRecurrenceEndDateError("End date must be after the appointment date.");
        return;
      }
    }
    setRecurrenceEndDateError(null);

    // Normalize startTime: strip seconds if browser includes them (HH:MM:SS → HH:MM:SS)
    // datetime-local gives "YYYY-MM-DDTHH:MM" — we want "YYYY-MM-DDTHH:MM:SS"
    let normalizedStart = startTime;
    const timePart = startTime.split("T")[1] ?? "";
    const timeSections = timePart.split(":");
    if (timeSections.length === 2) {
      // No seconds — append :00
      normalizedStart = startTime + ":00";
    } else if (timeSections.length >= 3) {
      // Has seconds — keep only HH:MM:SS (drop any sub-seconds)
      normalizedStart = `${startTime.split("T")[0]}T${timeSections[0]}:${timeSections[1]}:${timeSections[2].slice(0, 2)}`;
    }

    setSubmitting(true);
    setSubmitError(null);
    try {
      const input: AppointmentInput = {
        patientId: patientId.trim(),
        providerId: "", // Provider is derived server-side from the authenticated session
        startTime: normalizedStart,
        durationMinutes,
        apptType,
        color: color || null,
        reason: reason.trim() || null,
        recurrence: recurrence || null,
        recurrenceEndDate: recurrence && recurrenceEndDate ? recurrenceEndDate : null,
        notes: notes.trim() || null,
      };
      await onSubmitCreate(input);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCancel(e: FormEvent) {
    e.preventDefault();
    if (!appointmentId) {
      setSubmitError("No appointment selected for cancellation.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      // cancelAppointment always passes reason ?? null — enforced here
      await onSubmitCancel(appointmentId, cancelReason.trim() || null);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Read-only guard ─────────────────────────────────────────────────────
  if (!canWrite) {
    return null;
  }

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-lg p-6 mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-gray-900">
            {mode === "create" ? "New Appointment" : "Cancel Appointment"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="rounded-md p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1 disabled:opacity-50"
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        {/* ── Create mode ──────────────────────────────────────────────── */}
        {mode === "create" && (
          <form onSubmit={handleCreate} noValidate className="space-y-4">
            {/* Patient ID */}
            <div>
              <label htmlFor="appt-patient-id" className={LABEL_CLS}>
                Patient ID *
              </label>
              <input
                id="appt-patient-id"
                type="text"
                value={patientId}
                onChange={(e) => setPatientId(e.target.value)}
                className={INPUT_CLS}
                placeholder="Patient UUID"
                autoFocus
                required
              />
            </div>

            {/* Start time */}
            <div>
              <label htmlFor="appt-start-time" className={LABEL_CLS}>
                Start Time *
              </label>
              <input
                id="appt-start-time"
                type="datetime-local"
                value={startTime}
                onChange={(e) => setStartTime(e.target.value)}
                className={INPUT_CLS}
                required
              />
            </div>

            {/* Duration */}
            <div>
              <label htmlFor="appt-duration" className={LABEL_CLS}>
                Duration
              </label>
              <select
                id="appt-duration"
                value={durationMinutes}
                onChange={(e) => setDurationMinutes(Number(e.target.value))}
                className={INPUT_CLS}
              >
                {DURATION_OPTIONS.map((d) => (
                  <option key={d} value={d}>
                    {d} min
                  </option>
                ))}
              </select>
            </div>

            {/* Appointment type */}
            <div>
              <label htmlFor="appt-type" className={LABEL_CLS}>
                Appointment Type
              </label>
              <select
                id="appt-type"
                value={apptType}
                onChange={(e) => setApptType(e.target.value)}
                className={INPUT_CLS}
              >
                {APPT_TYPE_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>

            {/* Reason */}
            <div>
              <label htmlFor="appt-reason" className={LABEL_CLS}>
                Reason
              </label>
              <input
                id="appt-reason"
                type="text"
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                className={INPUT_CLS}
                placeholder="Reason for visit"
              />
            </div>

            {/* Notes */}
            <div>
              <label htmlFor="appt-notes" className={LABEL_CLS}>
                Notes
              </label>
              <textarea
                id="appt-notes"
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                className={INPUT_CLS}
                rows={2}
                placeholder="Additional notes"
              />
            </div>

            {/* Recurrence */}
            <div>
              <label htmlFor="appt-recurrence" className={LABEL_CLS}>
                Recurrence
              </label>
              <select
                id="appt-recurrence"
                value={recurrence}
                onChange={(e) => {
                  setRecurrence(e.target.value);
                  setRecurrenceEndDateError(null);
                  if (!e.target.value) setRecurrenceEndDate("");
                }}
                className={INPUT_CLS}
              >
                <option value="">None</option>
                <option value="weekly">Weekly</option>
                <option value="biweekly">Biweekly</option>
                <option value="monthly">Monthly</option>
              </select>
            </div>

            {/* Recurrence end date — shown only when recurrence is set */}
            {recurrence !== "" && (
              <div>
                <label htmlFor="appt-recurrence-end" className={LABEL_CLS}>
                  Recurrence End Date *
                </label>
                <input
                  id="appt-recurrence-end"
                  type="date"
                  value={recurrenceEndDate}
                  onChange={(e) => {
                    setRecurrenceEndDate(e.target.value);
                    setRecurrenceEndDateError(null);
                  }}
                  className={`${INPUT_CLS} ${recurrenceEndDateError ? "border-red-400 focus:border-red-500 focus:ring-red-500" : ""}`}
                  required
                />
                {recurrenceEndDateError && (
                  <p className="mt-1 text-xs text-red-600">
                    {recurrenceEndDateError}
                  </p>
                )}
              </div>
            )}

            {/* Color palette */}
            <div>
              <span className={LABEL_CLS}>Color</span>
              <div className="flex gap-2 mt-1">
                {COLOR_PALETTE.map((swatch) => (
                  <button
                    key={swatch.hex}
                    type="button"
                    onClick={() => setColor(swatch.hex)}
                    title={swatch.name}
                    aria-label={`Select ${swatch.name}`}
                    className={`w-8 h-8 rounded-full border-2 transition-transform hover:scale-110 focus:outline-none focus:ring-2 focus:ring-offset-1 focus:ring-blue-500 ${
                      color === swatch.hex
                        ? "border-gray-800 scale-110"
                        : "border-transparent"
                    }`}
                    style={{ backgroundColor: swatch.hex }}
                  />
                ))}
              </div>
            </div>

            {/* Submit error */}
            {submitError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                <p className="font-semibold">Failed to create appointment</p>
                <p className="mt-0.5">{submitError}</p>
              </div>
            )}

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 border-t border-gray-100 pt-4">
              <button
                type="button"
                onClick={onClose}
                disabled={submitting}
                className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={submitting}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
              >
                {submitting ? "Creating…" : "Create Appointment"}
              </button>
            </div>
          </form>
        )}

        {/* ── Cancel mode ──────────────────────────────────────────────── */}
        {mode === "cancel" && (
          <form onSubmit={handleCancel} noValidate className="space-y-4">
            {/* Appointment summary */}
            {appointmentSummary && (
              <div className="rounded-md bg-gray-50 border border-gray-200 px-4 py-3 text-sm text-gray-700">
                <p className="font-medium text-gray-900 mb-0.5">
                  Appointment to cancel
                </p>
                <p>{appointmentSummary}</p>
              </div>
            )}

            {/* Cancellation reason (optional) */}
            <div>
              <label htmlFor="cancel-reason" className={LABEL_CLS}>
                Reason for cancellation (optional)
              </label>
              <input
                id="cancel-reason"
                type="text"
                value={cancelReason}
                onChange={(e) => setCancelReason(e.target.value)}
                className={INPUT_CLS}
                placeholder="e.g. Patient requested"
                autoFocus
              />
            </div>

            {/* Submit error */}
            {submitError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                <p className="font-semibold">Failed to cancel appointment</p>
                <p className="mt-0.5">{submitError}</p>
              </div>
            )}

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 border-t border-gray-100 pt-4">
              <button
                type="button"
                onClick={onClose}
                disabled={submitting}
                className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
              >
                Go Back
              </button>
              <button
                type="submit"
                disabled={submitting}
                className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
              >
                {submitting ? "Cancelling…" : "Cancel Appointment"}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
