/**
 * MedicationFormModal.tsx — Add / Edit medication (MedicationStatement) modal.
 *
 * Add path:    addMedication({ patientId, ...formState }) via onAdd prop
 * Edit path:   updateMedication(initial.id, { patientId, ...formState }) via onUpdate prop
 * No physical delete — "Stop Medication" quick-action sets status to "stopped".
 *
 * Status select has all 8 valid FHIR MedicationStatement status values,
 * defaulting to "active".
 *
 * Pre-population from extractMedicationDisplay(initial.resource) in edit mode.
 *
 * Observability:
 *   - submitError rendered inline above submit button
 *   - console.error tagged [ClinicalSidebar] on mutation failure
 */
import { useState, type FormEvent } from "react";
import { extractMedicationDisplay } from "../../lib/fhirExtract";
import type { MedicationRecord, MedicationInput } from "../../types/patient";

// ─── Shared style constants ───────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface MedicationFormModalProps {
  patientId: string;
  /** null → Add mode; non-null → Edit mode */
  initial: MedicationRecord | null;
  onAdd: (input: MedicationInput) => Promise<void>;
  onUpdate: (id: string, input: MedicationInput) => Promise<void>;
  onSuccess: () => void;
  onClose: () => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function MedicationFormModal({
  patientId,
  initial,
  onAdd,
  onUpdate,
  onSuccess,
  onClose,
}: MedicationFormModalProps) {
  const isEdit = initial !== null;

  // Pre-populate from FHIR resource in edit mode.
  const prefill = initial ? extractMedicationDisplay(initial.resource) : null;

  const [display, setDisplay] = useState(prefill?.drugName ?? "");
  const [rxnormCode, setRxnormCode] = useState(prefill?.rxnormCode ?? "");
  const [status, setStatus] = useState(prefill?.status ?? "active");
  const [dosage, setDosage] = useState(prefill?.dosage ?? "");
  const [effectiveStart, setEffectiveStart] = useState(
    prefill?.effectiveStart ?? "",
  );
  const [effectiveEnd, setEffectiveEnd] = useState(prefill?.effectiveEnd ?? "");
  const [prescriberId, setPrescriberId] = useState("");
  const [reason, setReason] = useState("");
  const [notes, setNotes] = useState("");

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Build MedicationInput ───────────────────────────────────────────────

  function buildInput(overrideStatus?: string): MedicationInput {
    return {
      patientId,
      display: display.trim(),
      rxnormCode: rxnormCode.trim() || null,
      status: (overrideStatus ?? status).trim() || null,
      dosage: dosage.trim() || null,
      effectiveStart: effectiveStart.trim() || null,
      effectiveEnd: effectiveEnd.trim() || null,
      prescriberId: prescriberId.trim() || null,
      reason: reason.trim() || null,
      notes: notes.trim() || null,
    };
  }

  // ── Submit ──────────────────────────────────────────────────────────────

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!display.trim()) {
      setSubmitError("Drug name is required.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      const input = buildInput();
      if (isEdit && initial) {
        await onUpdate(initial.id, input);
      } else {
        await onAdd(input);
      }
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[ClinicalSidebar] medication mutation failed:", msg);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Stop Medication ─────────────────────────────────────────────────────

  async function handleStop() {
    if (!initial) return;
    if (!display.trim()) {
      setSubmitError("Drug name is required.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onUpdate(initial.id, buildInput("stopped"));
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[ClinicalSidebar] medication stop failed:", msg);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-lg p-6 mx-4 max-h-screen overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-gray-900">
            {isEdit ? "Edit Medication" : "Add Medication"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} noValidate className="space-y-4">
          {/* Drug Name */}
          <div>
            <label htmlFor="med-display" className={LABEL_CLS}>
              Drug Name *
            </label>
            <input
              id="med-display"
              type="text"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. Amoxicillin 500 MG Oral Capsule"
              autoFocus
              required
            />
          </div>

          {/* RxNorm Code */}
          <div>
            <label htmlFor="med-rxnorm" className={LABEL_CLS}>
              RxNorm Code (optional)
            </label>
            <input
              id="med-rxnorm"
              type="text"
              value={rxnormCode}
              onChange={(e) => setRxnormCode(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 723 (penicillin)"
            />
          </div>

          {/* Status — all 8 valid FHIR MedicationStatement status values */}
          <div>
            <label htmlFor="med-status" className={LABEL_CLS}>
              Status
            </label>
            <select
              id="med-status"
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="active">Active</option>
              <option value="completed">Completed</option>
              <option value="entered-in-error">Entered in Error</option>
              <option value="intended">Intended</option>
              <option value="stopped">Stopped</option>
              <option value="on-hold">On Hold</option>
              <option value="unknown">Unknown</option>
              <option value="not-taken">Not Taken</option>
            </select>
          </div>

          {/* Dosage */}
          <div>
            <label htmlFor="med-dosage" className={LABEL_CLS}>
              Dosage
            </label>
            <input
              id="med-dosage"
              type="text"
              value={dosage}
              onChange={(e) => setDosage(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 500mg TID x 10 days"
            />
          </div>

          {/* Effective Start */}
          <div>
            <label htmlFor="med-effective-start" className={LABEL_CLS}>
              Effective Start
            </label>
            <input
              id="med-effective-start"
              type="date"
              value={effectiveStart}
              onChange={(e) => setEffectiveStart(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Effective End */}
          <div>
            <label htmlFor="med-effective-end" className={LABEL_CLS}>
              Effective End
            </label>
            <input
              id="med-effective-end"
              type="date"
              value={effectiveEnd}
              onChange={(e) => setEffectiveEnd(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Prescriber ID */}
          <div>
            <label htmlFor="med-prescriber" className={LABEL_CLS}>
              Prescriber ID (optional)
            </label>
            <input
              id="med-prescriber"
              type="text"
              value={prescriberId}
              onChange={(e) => setPrescriberId(e.target.value)}
              className={INPUT_CLS}
              placeholder="Provider user ID"
            />
          </div>

          {/* Reason */}
          <div>
            <label htmlFor="med-reason" className={LABEL_CLS}>
              Reason (optional)
            </label>
            <input
              id="med-reason"
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className={INPUT_CLS}
              placeholder="ICD-10 code or free text"
            />
          </div>

          {/* Notes */}
          <div>
            <label htmlFor="med-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="med-notes"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className={INPUT_CLS}
              rows={3}
              placeholder="Additional clinical notes"
            />
          </div>

          {/* Submit error */}
          {submitError && (
            <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
              <p className="font-semibold">Failed to save</p>
              <p className="mt-0.5">{submitError}</p>
            </div>
          )}

          {/* Footer */}
          <div className="flex items-center justify-between gap-3 border-t border-gray-100 pt-4">
            {/* Stop Medication quick-action — edit mode only, when not already stopped */}
            <div>
              {isEdit && status !== "stopped" && status !== "completed" && (
                <button
                  type="button"
                  onClick={handleStop}
                  disabled={submitting}
                  className="rounded-md border border-amber-300 bg-white px-4 py-2 text-sm font-medium text-amber-700 shadow-sm hover:bg-amber-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-amber-500 focus:ring-offset-2"
                >
                  Stop Medication
                </button>
              )}
            </div>
            <div className="flex items-center gap-3">
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
                className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
              >
                {submitting
                  ? "Saving…"
                  : isEdit
                    ? "Save Changes"
                    : "Add Medication"}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
