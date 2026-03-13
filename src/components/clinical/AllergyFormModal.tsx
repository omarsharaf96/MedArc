/**
 * AllergyFormModal.tsx — Add / Edit / Delete allergy modal.
 *
 * Add path:    addAllergy({ patientId, ...formState }) via onSuccess callback
 * Edit path:   updateAllergy(initial.id, { patientId, ...formState }) via onSuccess callback
 * Delete path: deleteAllergy(initial.id, patientId) — Provider/SystemAdmin only,
 *              requires window.confirm before proceeding.
 *
 * Conditional RxNorm fields appear only when category === "drug".
 * Pre-population from extractAllergyDisplay(initial.resource) in edit mode.
 *
 * Observability:
 *   - submitError rendered inline above submit button — visible without DevTools
 *   - submitting / submitError inspectable in React DevTools
 *   - console.error tagged [ClinicalSidebar] on mutation failure
 *
 * Overlay pattern: fixed inset-0 bg-black/40 z-50 (same as PatientFormModal).
 */
import { useState, type FormEvent } from "react";
import { extractAllergyDisplay } from "../../lib/fhirExtract";
import type { AllergyRecord, AllergyInput } from "../../types/patient";

// ─── Shared style constants (mirror EncounterWorkspace.tsx) ──────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface AllergyFormModalProps {
  patientId: string;
  /** null → Add mode; non-null → Edit mode */
  initial: AllergyRecord | null;
  role: string;
  /** Called with mutation callbacks so parent wires add/update/delete */
  onAdd: (input: AllergyInput) => Promise<void>;
  onUpdate: (id: string, input: AllergyInput) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onSuccess: () => void;
  onClose: () => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function AllergyFormModal({
  patientId,
  initial,
  role,
  onAdd,
  onUpdate,
  onDelete,
  onSuccess,
  onClose,
}: AllergyFormModalProps) {
  const isEdit = initial !== null;
  const canDelete = isEdit && (role === "Provider" || role === "SystemAdmin");

  // Pre-populate from extracted FHIR data in edit mode.
  const prefill = initial ? extractAllergyDisplay(initial.resource) : null;

  const [substance, setSubstance] = useState(prefill?.substance ?? "");
  const [category, setCategory] = useState(prefill?.category ?? "drug");
  const [clinicalStatus, setClinicalStatus] = useState(
    prefill?.clinicalStatus ?? "active",
  );
  const [allergyType, setAllergyType] = useState(
    prefill?.allergyType ?? "allergy",
  );
  const [severity, setSeverity] = useState(prefill?.severity ?? "");
  const [reaction, setReaction] = useState(prefill?.reaction ?? "");
  const [onsetDate, setOnsetDate] = useState(prefill?.onsetDate ?? "");
  const [notes, setNotes] = useState("");
  // RxNorm fields — drug category only
  const [substanceCode, setSubstanceCode] = useState(
    prefill?.substanceCode ?? "",
  );
  const [substanceSystem, setSubstanceSystem] = useState(
    prefill?.substanceSystem ?? "",
  );

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Build AllergyInput from form state ──────────────────────────────────

  function buildInput(): AllergyInput {
    return {
      patientId,
      substance: substance.trim(),
      category: category.trim(),
      clinicalStatus: clinicalStatus.trim() || null,
      allergyType: allergyType.trim() || null,
      severity: severity.trim() || null,
      reaction: reaction.trim() || null,
      onsetDate: onsetDate.trim() || null,
      substanceCode: category === "drug" && substanceCode.trim() ? substanceCode.trim() : null,
      substanceSystem:
        category === "drug" && substanceSystem.trim() ? substanceSystem.trim() : null,
      notes: notes.trim() || null,
    };
  }

  // ── Submit ──────────────────────────────────────────────────────────────

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!substance.trim()) {
      setSubmitError("Substance is required.");
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
      console.error("[ClinicalSidebar] allergy mutation failed:", msg);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Delete ──────────────────────────────────────────────────────────────

  async function handleDelete() {
    if (!initial) return;
    const confirmed = window.confirm(
      "Delete this allergy? This cannot be undone.",
    );
    if (!confirmed) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onDelete(initial.id);
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[ClinicalSidebar] allergy delete failed:", msg);
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
            {isEdit ? "Edit Allergy" : "Add Allergy"}
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
          {/* Substance */}
          <div>
            <label htmlFor="allergy-substance" className={LABEL_CLS}>
              Substance *
            </label>
            <input
              id="allergy-substance"
              type="text"
              value={substance}
              onChange={(e) => setSubstance(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. Penicillin"
              autoFocus
              required
            />
          </div>

          {/* Category */}
          <div>
            <label htmlFor="allergy-category" className={LABEL_CLS}>
              Category
            </label>
            <select
              id="allergy-category"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="drug">Drug</option>
              <option value="food">Food</option>
              <option value="environment">Environment</option>
              <option value="biologic">Biologic</option>
            </select>
          </div>

          {/* Conditional RxNorm fields — only for drug category */}
          {category === "drug" && (
            <>
              <div>
                <label htmlFor="allergy-substance-code" className={LABEL_CLS}>
                  RxNorm Code (optional)
                </label>
                <input
                  id="allergy-substance-code"
                  type="text"
                  value={substanceCode}
                  onChange={(e) => setSubstanceCode(e.target.value)}
                  className={INPUT_CLS}
                  placeholder="e.g. 7980"
                />
              </div>
              <div>
                <label htmlFor="allergy-substance-system" className={LABEL_CLS}>
                  Substance Code System (optional)
                </label>
                <input
                  id="allergy-substance-system"
                  type="text"
                  value={substanceSystem}
                  onChange={(e) => setSubstanceSystem(e.target.value)}
                  className={INPUT_CLS}
                  placeholder="e.g. http://www.nlm.nih.gov/research/umls/rxnorm"
                />
              </div>
            </>
          )}

          {/* Clinical Status */}
          <div>
            <label htmlFor="allergy-clinical-status" className={LABEL_CLS}>
              Clinical Status
            </label>
            <select
              id="allergy-clinical-status"
              value={clinicalStatus}
              onChange={(e) => setClinicalStatus(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="active">Active</option>
              <option value="inactive">Inactive</option>
              <option value="resolved">Resolved</option>
            </select>
          </div>

          {/* Allergy Type */}
          <div>
            <label htmlFor="allergy-type" className={LABEL_CLS}>
              Type
            </label>
            <select
              id="allergy-type"
              value={allergyType}
              onChange={(e) => setAllergyType(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="allergy">Allergy</option>
              <option value="intolerance">Intolerance</option>
            </select>
          </div>

          {/* Severity */}
          <div>
            <label htmlFor="allergy-severity" className={LABEL_CLS}>
              Severity
            </label>
            <select
              id="allergy-severity"
              value={severity}
              onChange={(e) => setSeverity(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="">— (not specified)</option>
              <option value="mild">Mild</option>
              <option value="moderate">Moderate</option>
              <option value="severe">Severe</option>
              <option value="life-threatening">Life-threatening</option>
            </select>
          </div>

          {/* Reaction */}
          <div>
            <label htmlFor="allergy-reaction" className={LABEL_CLS}>
              Reaction
            </label>
            <input
              id="allergy-reaction"
              type="text"
              value={reaction}
              onChange={(e) => setReaction(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. hives, anaphylaxis"
            />
          </div>

          {/* Onset Date */}
          <div>
            <label htmlFor="allergy-onset-date" className={LABEL_CLS}>
              Onset Date
            </label>
            <input
              id="allergy-onset-date"
              type="date"
              value={onsetDate}
              onChange={(e) => setOnsetDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Notes */}
          <div>
            <label htmlFor="allergy-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="allergy-notes"
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
            {/* Delete — only for edit mode and Provider/SystemAdmin */}
            <div>
              {canDelete && (
                <button
                  type="button"
                  onClick={handleDelete}
                  disabled={submitting}
                  className="rounded-md border border-red-300 bg-white px-4 py-2 text-sm font-medium text-red-700 shadow-sm hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
                >
                  Delete Allergy
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
                {submitting ? "Saving…" : isEdit ? "Save Changes" : "Add Allergy"}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
