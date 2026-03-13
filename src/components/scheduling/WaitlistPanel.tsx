/**
 * WaitlistPanel.tsx — Waitlist list + inline add form + discharge action.
 *
 * Receives all data and mutations as props from SchedulePage (backed by
 * useSchedule). Discharge uses window.confirm for confirmation — consistent
 * with the AllergyFormModal delete pattern.
 *
 * extractWaitlistDisplay is intentionally local (not in fhirExtract.ts) because
 * WaitlistRecord.resource is an AppointmentRequest, not a standard FHIR
 * Appointment, and its shape is scheduling-domain-specific.
 *
 * Observability:
 *   - Loading, error, and empty states all rendered inline
 *   - submitError visible above submit button without DevTools
 */
import { useState, type FormEvent } from "react";
import type { WaitlistRecord, WaitlistInput } from "../../types/scheduling";

// ─── Local display extractor ──────────────────────────────────────────────────

interface WaitlistDisplay {
  status: string | null;
  priority: string | null;
}

/**
 * Extract display fields from a raw WaitlistRecord resource blob.
 * Never throws. Reads status and priority from AppointmentRequest shape.
 */
function extractWaitlistDisplay(
  resource: Record<string, unknown> | null | undefined,
): WaitlistDisplay {
  if (!resource) return { status: null, priority: null };

  const status =
    typeof (resource as Record<string, unknown>)["status"] === "string"
      ? ((resource as Record<string, unknown>)["status"] as string)
      : null;

  // priority[0].coding[0].code path for AppointmentRequest
  let priority: string | null = null;
  try {
    const priorityArr = (resource as Record<string, unknown>)["priority"];
    if (Array.isArray(priorityArr) && priorityArr.length > 0) {
      const first = priorityArr[0] as Record<string, unknown>;
      const coding = first["coding"];
      if (Array.isArray(coding) && coding.length > 0) {
        const code = (coding[0] as Record<string, unknown>)["code"];
        if (typeof code === "string" || typeof code === "number") {
          priority = String(code);
        }
      }
    }
  } catch {
    priority = null;
  }

  return { status, priority };
}

// ─── Style constants ──────────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Appointment type options (same as AppointmentFormModal) ──────────────────

const APPT_TYPE_OPTIONS = [
  { value: "new_patient", label: "New Patient" },
  { value: "follow_up", label: "Follow Up" },
  { value: "procedure", label: "Procedure" },
  { value: "telehealth", label: "Telehealth" },
  { value: "annual_wellness", label: "Annual Wellness" },
  { value: "urgent", label: "Urgent" },
];

// ─── Props ────────────────────────────────────────────────────────────────────

export interface WaitlistPanelProps {
  waitlist: WaitlistRecord[];
  loading: boolean;
  error: string | null;
  canWrite: boolean;
  onAdd: (input: WaitlistInput) => Promise<void>;
  onDischarge: (id: string, reason: string | null) => Promise<void>;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function WaitlistPanel({
  waitlist,
  loading,
  error,
  canWrite,
  onAdd,
  onDischarge,
}: WaitlistPanelProps) {
  // ── Add form toggle + state ─────────────────────────────────────────────
  const [addOpen, setAddOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Form fields
  const [patientId, setPatientId] = useState("");
  const [apptType, setApptType] = useState("follow_up");
  const [preferredDate, setPreferredDate] = useState("");
  const [priority, setPriority] = useState(3);
  const [reason, setReason] = useState("");
  const [notes, setNotes] = useState("");
  const [providerId, setProviderId] = useState("");

  // ── Discharge ──────────────────────────────────────────────────────────

  async function handleDischarge(record: WaitlistRecord) {
    const confirmed = window.confirm(
      `Discharge patient ${record.patientId} from the waitlist? This cannot be undone.`,
    );
    if (!confirmed) return;
    try {
      await onDischarge(record.id, null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      alert(`Discharge failed: ${msg}`);
    }
  }

  // ── Add form submit ─────────────────────────────────────────────────────

  async function handleAdd(e: FormEvent) {
    e.preventDefault();
    if (!patientId.trim()) {
      setSubmitError("Patient ID is required.");
      return;
    }
    if (!preferredDate) {
      setSubmitError("Preferred date is required.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onAdd({
        patientId: patientId.trim(),
        providerId: providerId.trim() || null,
        apptType,
        preferredDate,
        priority: priority || null,
        reason: reason.trim() || null,
        notes: notes.trim() || null,
      });
      // Reset form
      setPatientId("");
      setApptType("follow_up");
      setPreferredDate("");
      setPriority(3);
      setReason("");
      setNotes("");
      setProviderId("");
      setAddOpen(false);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="space-y-4">
      {/* Header + Add button */}
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-800">Waitlist</h2>
        {canWrite && !addOpen && (
          <button
            type="button"
            onClick={() => {
              setAddOpen(true);
              setSubmitError(null);
            }}
            className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            Add to Waitlist
          </button>
        )}
      </div>

      {/* Inline add form */}
      {canWrite && addOpen && (
        <form
          onSubmit={handleAdd}
          noValidate
          className="rounded-lg border border-blue-200 bg-blue-50 p-4 space-y-3"
        >
          <h3 className="text-sm font-semibold text-blue-900">
            Add Waitlist Entry
          </h3>

          <div>
            <label htmlFor="wl-patient-id" className={LABEL_CLS}>
              Patient ID *
            </label>
            <input
              id="wl-patient-id"
              type="text"
              value={patientId}
              onChange={(e) => setPatientId(e.target.value)}
              className={INPUT_CLS}
              placeholder="Patient UUID"
              autoFocus
            />
          </div>

          <div>
            <label htmlFor="wl-appt-type" className={LABEL_CLS}>
              Appointment Type
            </label>
            <select
              id="wl-appt-type"
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

          <div>
            <label htmlFor="wl-preferred-date" className={LABEL_CLS}>
              Preferred Date *
            </label>
            <input
              id="wl-preferred-date"
              type="date"
              value={preferredDate}
              onChange={(e) => setPreferredDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          <div>
            <label htmlFor="wl-priority" className={LABEL_CLS}>
              Priority (1 = urgent, 5 = routine)
            </label>
            <select
              id="wl-priority"
              value={priority}
              onChange={(e) => setPriority(Number(e.target.value))}
              className={INPUT_CLS}
            >
              {[1, 2, 3, 4, 5].map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label htmlFor="wl-reason" className={LABEL_CLS}>
              Reason
            </label>
            <input
              id="wl-reason"
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className={INPUT_CLS}
              placeholder="Reason for visit"
            />
          </div>

          <div>
            <label htmlFor="wl-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="wl-notes"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className={INPUT_CLS}
              rows={2}
              placeholder="Additional notes"
            />
          </div>

          <div>
            <label htmlFor="wl-provider-id" className={LABEL_CLS}>
              Provider ID (optional)
            </label>
            <input
              id="wl-provider-id"
              type="text"
              value={providerId}
              onChange={(e) => setProviderId(e.target.value)}
              className={INPUT_CLS}
              placeholder="Provider UUID"
            />
          </div>

          {/* Submit error */}
          {submitError && (
            <p className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
              {submitError}
            </p>
          )}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={() => {
                setAddOpen(false);
                setSubmitError(null);
              }}
              disabled={submitting}
              className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-1"
            >
              {submitting ? "Adding…" : "Add Entry"}
            </button>
          </div>
        </form>
      )}

      {/* Loading */}
      {loading && (
        <div className="space-y-2" aria-label="Loading waitlist">
          {[1, 2].map((i) => (
            <div key={i} className="h-12 rounded-lg bg-gray-100 animate-pulse" />
          ))}
        </div>
      )}

      {/* Error */}
      {!loading && error && (
        <div
          role="alert"
          className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
        >
          <span className="font-medium">Waitlist error: </span>
          {error}
        </div>
      )}

      {/* Empty state */}
      {!loading && !error && waitlist.length === 0 && (
        <p className="text-sm text-gray-500 italic">No patients on the waitlist.</p>
      )}

      {/* Waitlist rows */}
      {!loading && !error && waitlist.length > 0 && (
        <div className="space-y-2">
          {waitlist.map((record) => {
            const display = extractWaitlistDisplay(record.resource);

            // Preferred date: split on "T" to get date part — no Date construction
            const preferredDateDisplay =
              record.resource &&
              typeof (record.resource as Record<string, unknown>)["preferredDate"] === "string"
                ? ((record.resource as Record<string, unknown>)["preferredDate"] as string).split("T")[0]
                : "—";

            // apptType from resource
            const apptTypeRaw =
              record.resource &&
              typeof (record.resource as Record<string, unknown>)["apptType"] === "string"
                ? ((record.resource as Record<string, unknown>)["apptType"] as string)
                : "—";

            return (
              <div
                key={record.id}
                className="flex items-center gap-3 rounded-lg border border-gray-200 bg-white px-4 py-3 shadow-sm"
              >
                {/* Priority badge */}
                <span className="shrink-0 rounded-full bg-orange-100 text-orange-700 px-2.5 py-0.5 text-xs font-medium">
                  P{display.priority ?? "?"}
                </span>

                {/* Main info */}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-gray-900">
                    {record.patientId}
                  </p>
                  <p className="text-xs text-gray-500">
                    {apptTypeRaw} · {preferredDateDisplay}
                    {display.status ? ` · ${display.status}` : ""}
                  </p>
                </div>

                {/* Discharge button */}
                {canWrite && (
                  <button
                    type="button"
                    onClick={() => handleDischarge(record)}
                    className="shrink-0 rounded-md border border-red-300 bg-white px-3 py-1 text-xs font-medium text-red-700 hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1"
                  >
                    Discharge
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
