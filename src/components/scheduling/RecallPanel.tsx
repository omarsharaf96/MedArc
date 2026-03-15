/**
 * RecallPanel.tsx — Recall list + inline create form + complete action.
 *
 * Receives all data and mutations as props from SchedulePage (backed by
 * useSchedule). completeRecall returns void — no return value is read.
 *
 * extractRecallDisplay is intentionally local (not in fhirExtract.ts) because
 * RecallRecord.resource is a PatientRecall FHIR blob with scheduling-domain
 * conventions, not a standard FHIR Appointment.
 *
 * Observability:
 *   - Loading, error, and empty states all rendered inline
 *   - submitError visible above submit button without DevTools
 */
import { useState, type FormEvent } from "react";
import type { RecallRecord, RecallInput } from "../../types/scheduling";

// ─── Local display extractor ──────────────────────────────────────────────────

interface RecallDisplay {
  status: string | null;
  dueDate: string | null;
  recallType: string | null;
  reason: string | null;
}

/**
 * Extract display fields from a raw RecallRecord resource blob.
 * Reads status, dueDate, recallType, and reason from the PatientRecall shape.
 * Never throws.
 */
function extractRecallDisplay(
  resource: Record<string, unknown> | null | undefined,
): RecallDisplay {
  if (!resource) {
    return { status: null, dueDate: null, recallType: null, reason: null };
  }
  const r = resource as Record<string, unknown>;

  const status = typeof r["status"] === "string" ? r["status"] : null;
  const dueDate = typeof r["dueDate"] === "string" ? r["dueDate"] : null;
  const recallType =
    typeof r["recallType"] === "string" ? r["recallType"] : null;
  const reason = typeof r["reason"] === "string" ? r["reason"] : null;

  return { status, dueDate, recallType, reason };
}

// ─── Style constants ──────────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Recall type options ──────────────────────────────────────────────────────

const RECALL_TYPE_OPTIONS = [
  { value: "routine", label: "Routine" },
  { value: "urgent", label: "Urgent" },
  { value: "post_procedure", label: "Post Procedure" },
  { value: "preventive", label: "Preventive" },
];

// ─── Props ────────────────────────────────────────────────────────────────────

export interface RecallPanelProps {
  recalls: RecallRecord[];
  loading: boolean;
  error: string | null;
  canWrite: boolean;
  onCreateRecall: (input: RecallInput) => Promise<void>;
  onCompleteRecall: (id: string, notes: string | null) => Promise<void>;
  patientLabel?: (patientId: string) => string;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function RecallPanel({
  recalls,
  loading,
  error,
  canWrite,
  onCreateRecall,
  onCompleteRecall,
  patientLabel,
}: RecallPanelProps) {
  // ── Add form toggle + state ─────────────────────────────────────────────
  const [addOpen, setAddOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Form fields
  const [patientId, setPatientId] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [recallType, setRecallType] = useState("routine");
  const [reason, setReason] = useState("");
  const [notes, setNotes] = useState("");
  const [providerId, setProviderId] = useState("");

  // ── Complete action ─────────────────────────────────────────────────────

  async function handleComplete(record: RecallRecord) {
    try {
      // completeRecall returns void — do not read a return value
      await onCompleteRecall(record.id, null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      alert(`Failed to complete recall: ${msg}`);
    }
  }

  // ── Create form submit ──────────────────────────────────────────────────

  async function handleCreate(e: FormEvent) {
    e.preventDefault();
    if (!patientId.trim()) {
      setSubmitError("Patient ID is required.");
      return;
    }
    if (!dueDate) {
      setSubmitError("Due date is required.");
      return;
    }
    if (!reason.trim()) {
      setSubmitError("Reason is required.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onCreateRecall({
        patientId: patientId.trim(),
        providerId: providerId.trim() || null,
        dueDate,
        recallType,
        reason: reason.trim(),
        notes: notes.trim() || null,
      });
      // Reset form
      setPatientId("");
      setDueDate("");
      setRecallType("routine");
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
        <h2 className="text-lg font-semibold text-gray-800">Recall Board</h2>
        {canWrite && !addOpen && (
          <button
            type="button"
            onClick={() => {
              setAddOpen(true);
              setSubmitError(null);
            }}
            className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            Create Recall
          </button>
        )}
      </div>

      {/* Inline create form */}
      {canWrite && addOpen && (
        <form
          onSubmit={handleCreate}
          noValidate
          className="rounded-lg border border-green-200 bg-green-50 p-4 space-y-3"
        >
          <h3 className="text-sm font-semibold text-green-900">
            New Recall Entry
          </h3>

          <div>
            <label htmlFor="rc-patient-id" className={LABEL_CLS}>
              Patient ID *
            </label>
            <input
              id="rc-patient-id"
              type="text"
              value={patientId}
              onChange={(e) => setPatientId(e.target.value)}
              className={INPUT_CLS}
              placeholder="Patient UUID"
              autoFocus
            />
          </div>

          <div>
            <label htmlFor="rc-due-date" className={LABEL_CLS}>
              Due Date *
            </label>
            <input
              id="rc-due-date"
              type="date"
              value={dueDate}
              onChange={(e) => setDueDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          <div>
            <label htmlFor="rc-recall-type" className={LABEL_CLS}>
              Recall Type
            </label>
            <select
              id="rc-recall-type"
              value={recallType}
              onChange={(e) => setRecallType(e.target.value)}
              className={INPUT_CLS}
            >
              {RECALL_TYPE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label htmlFor="rc-reason" className={LABEL_CLS}>
              Reason *
            </label>
            <input
              id="rc-reason"
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className={INPUT_CLS}
              placeholder="Clinical reason for recall"
            />
          </div>

          <div>
            <label htmlFor="rc-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="rc-notes"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className={INPUT_CLS}
              rows={2}
              placeholder="Additional notes"
            />
          </div>

          <div>
            <label htmlFor="rc-provider-id" className={LABEL_CLS}>
              Provider ID (optional)
            </label>
            <input
              id="rc-provider-id"
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
              className="rounded-md bg-green-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-1"
            >
              {submitting ? "Creating…" : "Create Recall"}
            </button>
          </div>
        </form>
      )}

      {/* Loading */}
      {loading && (
        <div className="space-y-2" aria-label="Loading recall board">
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
          <span className="font-medium">Recall board error: </span>
          {error}
        </div>
      )}

      {/* Empty state */}
      {!loading && !error && recalls.length === 0 && (
        <p className="text-sm text-gray-500 italic">No recall entries.</p>
      )}

      {/* Recall rows */}
      {!loading && !error && recalls.length > 0 && (
        <div className="space-y-2">
          {recalls.map((record) => {
            const display = extractRecallDisplay(record.resource);

            // dueDate: prefer resource field; split on "T" for display
            const dueDateDisplay = display.dueDate
              ? display.dueDate.split("T")[0]
              : "—";

            const typeLabel =
              display.recallType?.replace(/_/g, " ") ?? "—";

            return (
              <div
                key={record.id}
                className="flex items-center gap-3 rounded-lg border border-gray-200 bg-white px-4 py-3 shadow-sm"
              >
                {/* Type badge */}
                <span className="shrink-0 rounded-full bg-purple-100 text-purple-700 px-2.5 py-0.5 text-xs font-medium capitalize">
                  {typeLabel}
                </span>

                {/* Main info */}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-gray-900">
                    {patientLabel ? patientLabel(record.patientId) : record.patientId}
                  </p>
                  <p className="text-xs text-gray-500">
                    Due: {dueDateDisplay}
                    {display.status ? ` · ${display.status}` : ""}
                    {display.reason ? ` · ${display.reason}` : ""}
                  </p>
                </div>

                {/* Complete button */}
                {canWrite && (
                  <button
                    type="button"
                    onClick={() => handleComplete(record)}
                    className="shrink-0 rounded-md border border-green-300 bg-white px-3 py-1 text-xs font-medium text-green-700 hover:bg-green-50 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-1"
                  >
                    Complete
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
