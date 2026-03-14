/**
 * ReferralPanel.tsx — Referral tracking panel for the patient chart.
 *
 * Renders a list of referrals for a patient as cards, with an "Add Referral"
 * button and an add/edit modal form. Visible to Provider and SystemAdmin roles
 * only (RBAC gated by PatientDetailPage).
 *
 * On mount: calls listReferrals(patientId) to load referral data.
 * Add: calls createReferral(input)
 * Edit: calls updateReferral(referralId, input)
 *
 * The modal follows the same overlay pattern as AllergyFormModal (fixed inset-0
 * bg-black/40 z-50) and uses the same input/label class constants.
 *
 * Observability:
 *   - console.error tagged [ReferralPanel] on fetch/mutation failure
 *   - Inline error banners visible without DevTools
 *   - loading / submitting state visible in React DevTools
 */
import { useState, useEffect, useCallback, type FormEvent } from "react";
import { commands } from "../../lib/tauri";
import type { ReferralRecord, ReferralInput } from "../../types/documents";
import type { DocumentRecord } from "../../types/labs";

// ─── Shared style constants (mirror AllergyFormModal.tsx) ────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ───────────────────────────────────────────────────────────────────

interface ReferralPanelProps {
  patientId: string;
  role: string;
}

// ─── Referral card sub-component ─────────────────────────────────────────────

function ReferralCard({
  referral,
  canEdit,
  onEdit,
}: {
  referral: ReferralRecord;
  canEdit: boolean;
  onEdit: (r: ReferralRecord) => void;
}) {
  return (
    <div className="rounded-md border border-gray-200 bg-white p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          {/* Provider name — prominent */}
          <p className="text-sm font-semibold text-gray-900">
            {referral.referringProviderName}
          </p>
          {/* NPI */}
          {referral.referringProviderNpi && (
            <p className="text-xs text-gray-500">
              NPI: {referral.referringProviderNpi}
            </p>
          )}
          {/* Practice */}
          {referral.practiceName && (
            <p className="mt-0.5 text-xs text-gray-600">
              {referral.practiceName}
            </p>
          )}
        </div>
        {canEdit && (
          <button
            type="button"
            onClick={() => onEdit(referral)}
            className="shrink-0 rounded px-2 py-0.5 text-xs font-medium text-indigo-600 hover:bg-indigo-50 focus:outline-none focus:ring-1 focus:ring-indigo-500"
          >
            Edit
          </button>
        )}
      </div>

      {/* Detail grid */}
      <div className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
        {referral.referralDate && (
          <>
            <span className="text-gray-500">Referral Date</span>
            <span className="text-gray-700">{referral.referralDate}</span>
          </>
        )}
        {referral.authorizedVisitCount != null && (
          <>
            <span className="text-gray-500">Authorized Visits</span>
            <span className="text-gray-700">{referral.authorizedVisitCount}</span>
          </>
        )}
        {referral.diagnosisIcd10 && (
          <>
            <span className="text-gray-500">ICD-10 Diagnosis</span>
            <span className="font-mono text-gray-700">
              {referral.diagnosisIcd10}
            </span>
          </>
        )}
        {referral.phone && (
          <>
            <span className="text-gray-500">Phone</span>
            <span className="text-gray-700">{referral.phone}</span>
          </>
        )}
        {referral.fax && (
          <>
            <span className="text-gray-500">Fax</span>
            <span className="text-gray-700">{referral.fax}</span>
          </>
        )}
        {referral.linkedDocumentId && (
          <>
            <span className="text-gray-500">Linked Document</span>
            <span className="text-indigo-600">{referral.linkedDocumentId}</span>
          </>
        )}
      </div>

      {/* Notes */}
      {referral.notes && (
        <p className="mt-2 text-xs text-gray-500 italic">{referral.notes}</p>
      )}
    </div>
  );
}

// ─── Referral Form Modal ─────────────────────────────────────────────────────

interface ReferralFormModalProps {
  patientId: string;
  initial: ReferralRecord | null;
  documents: DocumentRecord[];
  onSuccess: () => void;
  onClose: () => void;
}

function ReferralFormModal({
  patientId,
  initial,
  documents,
  onSuccess,
  onClose,
}: ReferralFormModalProps) {
  const isEdit = initial !== null;

  // Form state — pre-populate in edit mode
  const [providerName, setProviderName] = useState(
    initial?.referringProviderName ?? "",
  );
  const [providerNpi, setProviderNpi] = useState(
    initial?.referringProviderNpi ?? "",
  );
  const [practiceName, setPracticeName] = useState(
    initial?.practiceName ?? "",
  );
  const [phone, setPhone] = useState(initial?.phone ?? "");
  const [fax, setFax] = useState(initial?.fax ?? "");
  const [referralDate, setReferralDate] = useState(
    initial?.referralDate ?? "",
  );
  const [authorizedVisitCount, setAuthorizedVisits] = useState(
    initial?.authorizedVisitCount?.toString() ?? "",
  );
  const [diagnosisIcd10, setIcd10Diagnosis] = useState(
    initial?.diagnosisIcd10 ?? "",
  );
  const [linkedDocumentId, setLinkedDocumentId] = useState(
    initial?.linkedDocumentId ?? "",
  );
  const [notes, setNotes] = useState(initial?.notes ?? "");

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  function buildInput(): ReferralInput {
    const visitsNum = authorizedVisitCount.trim()
      ? parseInt(authorizedVisitCount.trim(), 10)
      : null;
    return {
      patientId,
      referringProviderName: providerName.trim(),
      referringProviderNpi: providerNpi.trim() || null,
      practiceName: practiceName.trim() || null,
      phone: phone.trim() || null,
      fax: fax.trim() || null,
      referralDate: referralDate.trim() || null,
      authorizedVisitCount:
        visitsNum !== null && !isNaN(visitsNum) ? visitsNum : null,
      diagnosisIcd10: diagnosisIcd10.trim() || null,
      linkedDocumentId: linkedDocumentId.trim() || null,
      notes: notes.trim() || null,
    };
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!providerName.trim()) {
      setSubmitError("Referring provider name is required.");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      const input = buildInput();
      if (isEdit && initial) {
        await commands.updateReferral(initial.referralId, input);
      } else {
        await commands.createReferral(input);
      }
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[ReferralPanel] referral mutation failed:", msg);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="mx-4 max-h-screen w-full max-w-lg overflow-y-auto rounded-lg bg-white p-6 shadow-xl">
        {/* Header */}
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-gray-900">
            {isEdit ? "Edit Referral" : "Add Referral"}
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
          {/* Referring Provider Name */}
          <div>
            <label htmlFor="ref-provider-name" className={LABEL_CLS}>
              Referring Provider Name *
            </label>
            <input
              id="ref-provider-name"
              type="text"
              value={providerName}
              onChange={(e) => setProviderName(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. Dr. Jane Smith"
              autoFocus
              required
            />
          </div>

          {/* Referring Provider NPI */}
          <div>
            <label htmlFor="ref-provider-npi" className={LABEL_CLS}>
              Referring Provider NPI
            </label>
            <input
              id="ref-provider-npi"
              type="text"
              value={providerNpi}
              onChange={(e) => setProviderNpi(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 1234567890"
              maxLength={10}
            />
          </div>

          {/* Practice Name */}
          <div>
            <label htmlFor="ref-practice" className={LABEL_CLS}>
              Practice Name
            </label>
            <input
              id="ref-practice"
              type="text"
              value={practiceName}
              onChange={(e) => setPracticeName(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. City Medical Group"
            />
          </div>

          {/* Phone / Fax row */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label htmlFor="ref-phone" className={LABEL_CLS}>
                Phone
              </label>
              <input
                id="ref-phone"
                type="tel"
                value={phone}
                onChange={(e) => setPhone(e.target.value)}
                className={INPUT_CLS}
                placeholder="(555) 123-4567"
              />
            </div>
            <div>
              <label htmlFor="ref-fax" className={LABEL_CLS}>
                Fax
              </label>
              <input
                id="ref-fax"
                type="tel"
                value={fax}
                onChange={(e) => setFax(e.target.value)}
                className={INPUT_CLS}
                placeholder="(555) 123-4568"
              />
            </div>
          </div>

          {/* Referral Date */}
          <div>
            <label htmlFor="ref-date" className={LABEL_CLS}>
              Referral Date
            </label>
            <input
              id="ref-date"
              type="date"
              value={referralDate}
              onChange={(e) => setReferralDate(e.target.value)}
              className={INPUT_CLS}
            />
          </div>

          {/* Authorized Visits */}
          <div>
            <label htmlFor="ref-visits" className={LABEL_CLS}>
              Authorized Visit Count
            </label>
            <input
              id="ref-visits"
              type="number"
              min={0}
              value={authorizedVisitCount}
              onChange={(e) => setAuthorizedVisits(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. 12"
            />
          </div>

          {/* ICD-10 Diagnosis */}
          <div>
            <label htmlFor="ref-icd10" className={LABEL_CLS}>
              ICD-10 Diagnosis
            </label>
            <input
              id="ref-icd10"
              type="text"
              value={diagnosisIcd10}
              onChange={(e) => setIcd10Diagnosis(e.target.value)}
              className={INPUT_CLS}
              placeholder="e.g. M54.5 (format: letter + digits + optional dot)"
            />
          </div>

          {/* Linked Document */}
          <div>
            <label htmlFor="ref-document" className={LABEL_CLS}>
              Linked Document (optional)
            </label>
            <select
              id="ref-document"
              value={linkedDocumentId}
              onChange={(e) => setLinkedDocumentId(e.target.value)}
              className={INPUT_CLS}
            >
              <option value="">-- None --</option>
              {documents.map((doc) => (
                <option key={doc.id} value={doc.id}>
                  {doc.title} ({doc.category})
                </option>
              ))}
            </select>
          </div>

          {/* Notes */}
          <div>
            <label htmlFor="ref-notes" className={LABEL_CLS}>
              Notes
            </label>
            <textarea
              id="ref-notes"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className={INPUT_CLS}
              rows={3}
              placeholder="Additional referral notes"
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
              {submitting
                ? "Saving…"
                : isEdit
                  ? "Save Changes"
                  : "Add Referral"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

export function ReferralPanel({ patientId, role }: ReferralPanelProps) {
  const [referrals, setReferrals] = useState<ReferralRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshCounter, setRefreshCounter] = useState(0);

  // Document list for the linked document dropdown
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);

  // Modal state
  const [addOpen, setAddOpen] = useState(false);
  const [editRecord, setEditRecord] = useState<ReferralRecord | null>(null);

  const canWrite = role === "Provider" || role === "SystemAdmin";

  // ── Fetch referrals ────────────────────────────────────────────────────
  const fetchReferrals = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await commands.listReferrals(patientId);
      setReferrals(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[ReferralPanel] listReferrals failed:", msg);
      setError(msg);
      setReferrals([]);
    } finally {
      setLoading(false);
    }
  }, [patientId, refreshCounter]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    let mounted = true;
    fetchReferrals().then(() => {
      if (!mounted) return;
    });
    return () => {
      mounted = false;
    };
  }, [fetchReferrals]);

  // ── Fetch documents for dropdown ───────────────────────────────────────
  useEffect(() => {
    let mounted = true;
    commands
      .listDocuments(patientId, null, null)
      .then((docs) => {
        if (mounted) setDocuments(docs);
      })
      .catch((e) => {
        // Non-critical — log but don't block the panel
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[ReferralPanel] listDocuments failed:", msg);
        if (mounted) setDocuments([]);
      });
    return () => {
      mounted = false;
    };
  }, [patientId]);

  // ── Modal handlers ─────────────────────────────────────────────────────
  function handleSuccess() {
    setAddOpen(false);
    setEditRecord(null);
    setRefreshCounter((n) => n + 1);
  }

  function handleClose() {
    setAddOpen(false);
    setEditRecord(null);
  }

  // ── Referral summary for header badge ──────────────────────────────────
  const latestReferral =
    referrals.length > 0 ? referrals[0] : null;

  return (
    <div className="space-y-4">
      {/* ── Referral badge summary ──────────────────────────────────────── */}
      {!loading && latestReferral && (
        <div className="rounded-md border border-indigo-100 bg-indigo-50 px-3 py-2 text-sm text-indigo-800">
          <span className="font-medium">Referral:</span>{" "}
          {latestReferral.referringProviderName}
          {latestReferral.authorizedVisitCount != null && (
            <span>
              {" "}
              / {latestReferral.authorizedVisitCount} visit
              {latestReferral.authorizedVisitCount !== 1 ? "s" : ""} authorized
            </span>
          )}
        </div>
      )}

      {/* ── Controls row ────────────────────────────────────────────────── */}
      {canWrite && (
        <div className="flex items-center justify-end">
          <button
            type="button"
            onClick={() => setAddOpen(true)}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
          >
            + Add Referral
          </button>
        </div>
      )}

      {/* ── Error banner ────────────────────────────────────────────────── */}
      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          <p className="font-semibold">Failed to load referrals</p>
          <p className="mt-0.5">{error}</p>
          <button
            type="button"
            onClick={() => setRefreshCounter((n) => n + 1)}
            className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
          >
            Retry
          </button>
        </div>
      )}

      {/* ── Referral list ───────────────────────────────────────────────── */}
      {loading ? (
        <p className="text-sm text-gray-500">Loading referrals…</p>
      ) : referrals.length === 0 && !error ? (
        <p className="text-sm text-gray-500">No referrals on file.</p>
      ) : (
        <div className="space-y-3">
          {referrals.map((ref) => (
            <ReferralCard
              key={ref.referralId}
              referral={ref}
              canEdit={canWrite}
              onEdit={(r) => setEditRecord(r)}
            />
          ))}
        </div>
      )}

      {/* ── Add / Edit Modal ────────────────────────────────────────────── */}
      {(addOpen || editRecord !== null) && (
        <ReferralFormModal
          patientId={patientId}
          initial={editRecord}
          documents={documents}
          onSuccess={handleSuccess}
          onClose={handleClose}
        />
      )}
    </div>
  );
}
