/**
 * ProblemFormModal.tsx — Add / Edit problem (Condition) modal.
 *
 * Add path:    addProblem({ patientId, ...formState }) via onAdd prop
 * Edit path:   updateProblem(initial.id, { patientId, ...formState }) via onUpdate prop
 * No physical delete — only status transitions.
 *
 * "Resolve" quick-action button (edit mode only): sets clinicalStatus to
 * "resolved" and submits immediately. abatementDate field is shown only when
 * clinicalStatus is "resolved" or "inactive".
 *
 * Pre-population from extractProblemDisplay(initial.resource) in edit mode.
 *
 * Observability:
 *   - submitError rendered inline above submit button
 *   - console.error tagged [ClinicalSidebar] on mutation failure
 */
import { useState, type FormEvent } from "react";
import { extractProblemDisplay } from "../../lib/fhirExtract";
import type { ProblemRecord, ProblemInput } from "../../types/patient";

// ─── Shared style constants ───────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface ProblemFormModalProps {
  patientId: string;
  /** null → Add mode; non-null → Edit mode */
  initial: ProblemRecord | null;
  onAdd: (input: ProblemInput) => Promise<void>;
  onUpdate: (id: string, input: ProblemInput) => Promise<void>;
  onSuccess: () => void;
  onClose: () => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function ProblemFormModal({
  patientId,
  initial,
  onAdd,
  onUpdate,
  onSuccess,
  onClose,
}: ProblemFormModalProps) {
  const isEdit = initial !== null;

  // Pre-populate from FHIR resource in edit mode.
  const prefill = initial ? extractProblemDisplay(initial.resource) : null;

  const [icd10Code, setIcd10Code] = useState(prefill?.icd10Code ?? "");
  const [display, setDisplay] = useState(prefill?.display ?? "");
  const [clinicalStatus, setClinicalStatus] = useState(
    prefill?.clinicalStatus ?? "active",
  );
  const [onsetDate, setOnsetDate] = useState(prefill?.onsetDate ?? "");
  const [abatementDate, setAbatementDate] = useState(
    prefill?.abatementDate ?? "",
  );
  const [notes, setNotes] = useState("");

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Build ProblemInput ──────────────────────────────────────────────────

  function buildInput(overrideStatus?: string): ProblemInput {
    const status = overrideStatus ?? clinicalStatus;
    return {
      patientId,
      icd10Code: icd10Code.trim(),
      display: display.trim(),
      clinicalStatus: status.trim() || null,
      onsetDate: onsetDate.trim() || null,
      abatementDate:
        (status === "resolved" || status === "inactive") && abatementDate.trim()
          ? abatementDate.trim()
          : null,
      notes: notes.trim() || null,
    };
  }

  // ── Validate ────────────────────────────────────────────────────────────

  function validate(): string | null {
    if (!icd10Code.trim()) return "ICD-10 code is required.";
    if (!display.trim()) return "Diagnosis display is required.";
    return null;
  }

  // ── Submit ──────────────────────────────────────────────────────────────

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const err = validate();
    if (err) {
      setSubmitError(err);
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
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error("[ClinicalSidebar] problem mutation failed:", msg);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Quick Resolve ───────────────────────────────────────────────────────

  async function handleResolve() {
    if (!initial) return;
    const err = validate();
    if (err) {
      setSubmitError(err);
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onUpdate(initial.id, buildInput("resolved"));
      onSuccess();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error("[ClinicalSidebar] problem resolve failed:", msg);
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
            {isEdit ? "Edit Problem" : "Add Problem"}
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
          {/* ICD-10 Code */}
          <div>
            <label htmlFor="problem-icd10" className={LABEL_CLS}>
              ICD-10 Code *
            </label>
            <input
              id="problem-icd10"
              type="text"
              value={icd10Code}
              onChange={(e) => setIcd10Code(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. J06.9"
              autoFocus
              required
            />
          </div>

          {/* Display */}
          <div>
            <label htmlFor="problem-display" className={LABEL_CLS}>
              Diagnosis *
            </label>
            <input
              id="problem-display"
              type="text"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. Acute upper respiratory infection"
              required
            />
          </div>

          {/* Clinical Status */}
          <div>
            <label htmlFor="problem-status" className={LABEL_CLS}>
              Clinical Status
            </label>
            <select
              id="problem-status"
              value={clinicalStatus}
              onChange={(e) => setClinicalStatus(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="active">Active</option>
              <option value="inactive">Inactive</option>
              <option value="resolved">Resolved</option>
            </select>
          </div>

          {/* Onset Date */}
          <div>
            <label htmlFor="problem-onset-date" className={LABEL_CLS}>
              Onset Date
            </label>
            <input
              id="problem-onset-date"
              type="date"
              value={onsetDate}
              onChange={(e) => setOnsetDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Abatement Date — shown when status is resolved or inactive */}
          {(clinicalStatus === "resolved" || clinicalStatus === "inactive") && (
            <div>
              <label htmlFor="problem-abatement-date" className={LABEL_CLS}>
                Abatement / Resolved Date
              </label>
              <input
                id="problem-abatement-date"
                type="date"
                value={abatementDate}
                onChange={(e) => setAbatementDate(e.target.value)}
                className={INPUT_CLS}
              />
            </div>
          )}

          {/* Notes */}
          <div>
            <label htmlFor="problem-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="problem-notes"
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
            {/* Resolve quick-action — edit mode only */}
            <div>
              {isEdit && clinicalStatus !== "resolved" && (
                <button
                  type="button"
                  onClick={handleResolve}
                  disabled={submitting}
                  className="rounded-md border border-green-300 bg-white px-4 py-2 text-sm font-medium text-green-700 shadow-sm hover:bg-green-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2"
                >
                  Mark Resolved
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
                {submitting ? "Saving…" : isEdit ? "Save Changes" : "Add Problem"}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
