/**
 * ImmunizationFormModal.tsx — Add-only immunization modal.
 *
 * Immunizations are append-only: no edit, no delete.
 * Add path: addImmunization({ patientId, ...formState }) via onAdd prop.
 *
 * Required fields: cvxCode, display, occurrenceDate.
 * Optional fields: lotNumber, expirationDate, site, route, doseNumber, status, notes.
 *
 * Observability:
 *   - submitError rendered inline above submit button
 *   - console.error tagged [ClinicalSidebar] on mutation failure
 */
import { useState, type FormEvent } from "react";
import type { ImmunizationInput } from "../../types/patient";

// ─── Shared style constants ───────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface ImmunizationFormModalProps {
  patientId: string;
  onAdd: (input: ImmunizationInput) => Promise<void>;
  onSuccess: () => void;
  onClose: () => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function ImmunizationFormModal({
  patientId,
  onAdd,
  onSuccess,
  onClose,
}: ImmunizationFormModalProps) {
  const [cvxCode, setCvxCode] = useState("");
  const [display, setDisplay] = useState("");
  const [occurrenceDate, setOccurrenceDate] = useState("");
  const [lotNumber, setLotNumber] = useState("");
  const [expirationDate, setExpirationDate] = useState("");
  const [site, setSite] = useState("");
  const [route, setRoute] = useState("");
  const [doseNumber, setDoseNumber] = useState<string>("");
  const [status, setStatus] = useState("completed");
  const [notes, setNotes] = useState("");

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Validate ────────────────────────────────────────────────────────────

  function validate(): string | null {
    if (!cvxCode.trim()) return "CVX code is required.";
    if (!display.trim()) return "Vaccine name is required.";
    if (!occurrenceDate.trim()) return "Occurrence date is required.";
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
      const parsedDose = doseNumber.trim() ? parseInt(doseNumber.trim(), 10) : null;
      const input: ImmunizationInput = {
        patientId,
        cvxCode: cvxCode.trim(),
        display: display.trim(),
        occurrenceDate: occurrenceDate.trim(),
        lotNumber: lotNumber.trim() || null,
        expirationDate: expirationDate.trim() || null,
        site: site.trim() || null,
        route: route.trim() || null,
        doseNumber: parsedDose !== null && !isNaN(parsedDose) ? parsedDose : null,
        status: status.trim() || null,
        notes: notes.trim() || null,
      };
      await onAdd(input);
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[ClinicalSidebar] immunization add failed:", msg);
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
            Add Immunization
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
          {/* CVX Code */}
          <div>
            <label htmlFor="imm-cvx" className={LABEL_CLS}>
              CVX Code *
            </label>
            <input
              id="imm-cvx"
              type="text"
              value={cvxCode}
              onChange={(e) => setCvxCode(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 158 (influenza)"
              autoFocus
              required
            />
          </div>

          {/* Vaccine Name */}
          <div>
            <label htmlFor="imm-display" className={LABEL_CLS}>
              Vaccine Name *
            </label>
            <input
              id="imm-display"
              type="text"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. Influenza, seasonal, injectable"
              required
            />
          </div>

          {/* Occurrence Date */}
          <div>
            <label htmlFor="imm-occurrence" className={LABEL_CLS}>
              Date Administered *
            </label>
            <input
              id="imm-occurrence"
              type="date"
              value={occurrenceDate}
              onChange={(e) => setOccurrenceDate(e.target.value)}
              className={INPUT_CLS}
              required
            />
          </div>

          {/* Status */}
          <div>
            <label htmlFor="imm-status" className={LABEL_CLS}>
              Status
            </label>
            <select
              id="imm-status"
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="completed">Completed</option>
              <option value="entered-in-error">Entered in Error</option>
              <option value="not-done">Not Done</option>
            </select>
          </div>

          {/* Lot Number */}
          <div>
            <label htmlFor="imm-lot" className={LABEL_CLS}>
              Lot Number
            </label>
            <input
              id="imm-lot"
              type="text"
              value={lotNumber}
              onChange={(e) => setLotNumber(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. A1234B"
            />
          </div>

          {/* Expiration Date */}
          <div>
            <label htmlFor="imm-expiration" className={LABEL_CLS}>
              Lot Expiration Date
            </label>
            <input
              id="imm-expiration"
              type="date"
              value={expirationDate}
              onChange={(e) => setExpirationDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Site */}
          <div>
            <label htmlFor="imm-site" className={LABEL_CLS}>
              Site
            </label>
            <input
              id="imm-site"
              type="text"
              value={site}
              onChange={(e) => setSite(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. left deltoid"
            />
          </div>

          {/* Route */}
          <div>
            <label htmlFor="imm-route" className={LABEL_CLS}>
              Route
            </label>
            <input
              id="imm-route"
              type="text"
              value={route}
              onChange={(e) => setRoute(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. intramuscular"
            />
          </div>

          {/* Dose Number */}
          <div>
            <label htmlFor="imm-dose" className={LABEL_CLS}>
              Dose Number in Series
            </label>
            <input
              id="imm-dose"
              type="number"
              min={1}
              value={doseNumber}
              onChange={(e) => setDoseNumber(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 1"
            />
          </div>

          {/* Notes */}
          <div>
            <label htmlFor="imm-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="imm-notes"
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
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
            >
              {submitting ? "Saving…" : "Add Immunization"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
