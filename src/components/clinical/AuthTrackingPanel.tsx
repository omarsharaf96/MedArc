/**
 * AuthTrackingPanel.tsx — Authorization & Visit Tracking UI.
 *
 * Displays authorization records for a patient with:
 *   - List of auth records with visit counter ("X of Y visits used")
 *   - Add/edit modal for auth record CRUD
 *   - Alert banners for expiring, expired, exhausted, low-visits conditions
 *   - Re-authorization letter generation and preview
 *
 * RBAC:
 *   - Provider / SystemAdmin: full CRUD
 *   - NurseMa: Create + Read + Update (no delete)
 *   - BillingStaff: Read-only
 *   - FrontDesk: hidden (no access)
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../../lib/tauri";
import type {
  AuthRecord,
  AuthRecordInput,
  AuthAlert,
} from "../../types/auth-tracking";

// ─── Props ───────────────────────────────────────────────────────────────────

interface AuthTrackingPanelProps {
  patientId: string;
  role: string;
}

// ─── Tailwind class constants ─────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";
const BTN_PRIMARY =
  "rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60";
const BTN_SECONDARY =
  "rounded-md bg-gray-100 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2";

// ─── Alert Banner Component ──────────────────────────────────────────────────

export function AuthAlertBanner({
  patientId,
}: {
  patientId: string;
}) {
  const [alerts, setAlerts] = useState<AuthAlert[]>([]);

  useEffect(() => {
    let mounted = true;
    commands
      .getAuthAlerts(patientId)
      .then((result) => {
        if (mounted) setAlerts(result);
      })
      .catch((e) => {
        console.error("[AuthAlertBanner] getAuthAlerts failed:", e);
      });
    return () => {
      mounted = false;
    };
  }, [patientId]);

  if (alerts.length === 0) return null;

  return (
    <div className="space-y-2">
      {alerts.map((alert, idx) => {
        const isError = alert.severity === "error";
        return (
          <div
            key={`${alert.authId}-${alert.alertType}-${idx}`}
            className={[
              "rounded-md border px-4 py-3 text-sm",
              isError
                ? "border-red-200 bg-red-50 text-red-700"
                : "border-amber-200 bg-amber-50 text-amber-700",
            ].join(" ")}
          >
            <p className="font-semibold">
              {alert.payerName}
              {alert.authNumber ? ` (${alert.authNumber})` : ""}
            </p>
            <p className="mt-0.5">{alert.message}</p>
          </div>
        );
      })}
    </div>
  );
}

// ─── Auth Record Form Modal ──────────────────────────────────────────────────

interface AuthFormModalProps {
  patientId: string;
  existingRecord: AuthRecord | null;
  onSuccess: () => void;
  onClose: () => void;
}

function AuthFormModal({
  patientId,
  existingRecord,
  onSuccess,
  onClose,
}: AuthFormModalProps) {
  const isEdit = existingRecord !== null;

  const [payerName, setPayerName] = useState(existingRecord?.payerName ?? "");
  const [payerPhone, setPayerPhone] = useState(existingRecord?.payerPhone ?? "");
  const [authNumber, setAuthNumber] = useState(existingRecord?.authNumber ?? "");
  const [authorizedVisits, setAuthorizedVisits] = useState(
    existingRecord?.authorizedVisits?.toString() ?? ""
  );
  const [cptCodes, setCptCodes] = useState(
    existingRecord?.authorizedCptCodes?.join(", ") ?? ""
  );
  const [startDate, setStartDate] = useState(existingRecord?.startDate ?? "");
  const [endDate, setEndDate] = useState(existingRecord?.endDate ?? "");
  const [notes, setNotes] = useState(existingRecord?.notes ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setError(null);

    const parsedVisits = parseInt(authorizedVisits, 10);
    if (isNaN(parsedVisits) || parsedVisits < 0) {
      setError("Authorized visits must be a non-negative number.");
      setSaving(false);
      return;
    }

    const parsedCptCodes = cptCodes
      .split(",")
      .map((c) => c.trim())
      .filter((c) => c.length > 0);

    const input: AuthRecordInput = {
      patientId,
      payerName: payerName.trim(),
      payerPhone: payerPhone.trim() || null,
      authNumber: authNumber.trim() || null,
      authorizedVisits: parsedVisits,
      authorizedCptCodes: parsedCptCodes.length > 0 ? parsedCptCodes : null,
      startDate,
      endDate,
      notes: notes.trim() || null,
    };

    try {
      if (isEdit && existingRecord) {
        await commands.updateAuthRecord(existingRecord.authId, input);
      } else {
        await commands.createAuthRecord(input);
      }
      onSuccess();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[AuthFormModal] save failed:", msg);
      setError(msg);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-lg rounded-lg bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-gray-900">
            {isEdit ? "Edit Authorization" : "Add Authorization"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-gray-400 hover:text-gray-600"
            aria-label="Close"
          >
            X
          </button>
        </div>

        {error && (
          <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className={LABEL_CLS}>Payer Name *</label>
            <input
              type="text"
              value={payerName}
              onChange={(e) => setPayerName(e.target.value)}
              className={INPUT_CLS}
              required
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className={LABEL_CLS}>Payer Phone</label>
              <input
                type="text"
                value={payerPhone}
                onChange={(e) => setPayerPhone(e.target.value)}
                className={INPUT_CLS}
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Auth Number</label>
              <input
                type="text"
                value={authNumber}
                onChange={(e) => setAuthNumber(e.target.value)}
                className={INPUT_CLS}
              />
            </div>
          </div>

          <div>
            <label className={LABEL_CLS}>Authorized Visits *</label>
            <input
              type="number"
              min="0"
              value={authorizedVisits}
              onChange={(e) => setAuthorizedVisits(e.target.value)}
              className={INPUT_CLS}
              required
            />
          </div>

          <div>
            <label className={LABEL_CLS}>Authorized CPT Codes (comma-separated)</label>
            <input
              type="text"
              value={cptCodes}
              onChange={(e) => setCptCodes(e.target.value)}
              className={INPUT_CLS}
              placeholder="97110, 97140, 97530"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className={LABEL_CLS}>Start Date *</label>
              <input
                type="date"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
                className={INPUT_CLS}
                required
              />
            </div>
            <div>
              <label className={LABEL_CLS}>End Date *</label>
              <input
                type="date"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
                className={INPUT_CLS}
                required
              />
            </div>
          </div>

          <div>
            <label className={LABEL_CLS}>Notes</label>
            <textarea
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className={INPUT_CLS}
              rows={3}
            />
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <button type="button" onClick={onClose} className={BTN_SECONDARY}>
              Cancel
            </button>
            <button type="submit" disabled={saving} className={BTN_PRIMARY}>
              {saving ? "Saving..." : isEdit ? "Update" : "Create"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Re-Auth Letter Preview Modal ────────────────────────────────────────────

interface ReauthLetterModalProps {
  letterContent: string;
  onClose: () => void;
}

function ReauthLetterModal({ letterContent, onClose }: ReauthLetterModalProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-2xl rounded-lg bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-gray-900">
            Re-Authorization Letter Preview
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-gray-400 hover:text-gray-600"
            aria-label="Close"
          >
            X
          </button>
        </div>

        <pre className="max-h-96 overflow-auto whitespace-pre-wrap rounded-md border border-gray-200 bg-gray-50 p-4 text-sm font-mono text-gray-800">
          {letterContent}
        </pre>

        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onClose} className={BTN_SECONDARY}>
            Close
          </button>
          <button
            type="button"
            className="rounded-md bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2"
            onClick={() => {
              // Placeholder for fax send dialog integration
              alert("Fax send dialog would open here. Feature coming soon.");
            }}
          >
            Send via Fax
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Panel Component ────────────────────────────────────────────────────

export function AuthTrackingPanel({ patientId, role }: AuthTrackingPanelProps) {
  const [records, setRecords] = useState<AuthRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  // Modal state
  const [formOpen, setFormOpen] = useState(false);
  const [editingRecord, setEditingRecord] = useState<AuthRecord | null>(null);

  // Re-auth letter state
  const [letterContent, setLetterContent] = useState<string | null>(null);
  const [generatingLetter, setGeneratingLetter] = useState<string | null>(null);

  const canEdit =
    role === "Provider" || role === "SystemAdmin" || role === "NurseMa";

  const reload = useCallback(() => setRefreshKey((k) => k + 1), []);

  // ── Fetch records ──────────────────────────────────────────────────────
  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setError(null);

    commands
      .listAuthRecords(patientId)
      .then((result) => {
        if (mounted) setRecords(result);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[AuthTrackingPanel] listAuthRecords failed:", msg);
        setError(msg);
        setRecords([]);
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [patientId, refreshKey]);

  // ── Generate re-auth letter ────────────────────────────────────────────
  async function handleGenerateReauth(authId: string) {
    setGeneratingLetter(authId);
    try {
      const letter = await commands.generateReauthLetter(authId, patientId);
      setLetterContent(letter);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[AuthTrackingPanel] generateReauthLetter failed:", msg);
      alert(`Failed to generate letter: ${msg}`);
    } finally {
      setGeneratingLetter(null);
    }
  }

  // ── Visit counter display helper ───────────────────────────────────────
  function visitCounterDisplay(record: AuthRecord): string {
    const remaining = record.authorizedVisits - record.visitsUsed;
    return `${record.visitsUsed} of ${record.authorizedVisits} visits used (${remaining} remaining)`;
  }

  // ── Status badge ───────────────────────────────────────────────────────
  function statusBadge(status: string) {
    const cls =
      status === "active"
        ? "bg-green-100 text-green-800"
        : status === "expired"
          ? "bg-red-100 text-red-800"
          : "bg-gray-100 text-gray-700";
    return (
      <span
        className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}
      >
        {status.charAt(0).toUpperCase() + status.slice(1)}
      </span>
    );
  }

  // ── Loading ────────────────────────────────────────────────────────────
  if (loading) {
    return <p className="text-sm text-gray-500">Loading authorizations...</p>;
  }

  // ── Error ──────────────────────────────────────────────────────────────
  if (error) {
    return (
      <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
        <p className="font-semibold">Failed to load authorizations</p>
        <p className="mt-0.5">{error}</p>
        <button
          type="button"
          onClick={reload}
          className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Alert banners */}
      <AuthAlertBanner patientId={patientId} />

      {/* Header with add button */}
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-gray-700">
          Authorizations ({records.length})
        </h3>
        {canEdit && (
          <button
            type="button"
            onClick={() => {
              setEditingRecord(null);
              setFormOpen(true);
            }}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
          >
            + Add Authorization
          </button>
        )}
      </div>

      {/* Records list */}
      {records.length === 0 ? (
        <p className="text-sm text-gray-500">No authorizations on file.</p>
      ) : (
        <div className="space-y-3">
          {records.map((record) => (
            <div
              key={record.authId}
              className="rounded-md border border-gray-200 bg-gray-50 p-4"
            >
              <div className="flex items-start justify-between">
                <div>
                  <p className="text-sm font-medium text-gray-900">
                    {record.payerName}
                    {record.authNumber ? ` — #${record.authNumber}` : ""}
                  </p>
                  <p className="mt-1 text-xs text-gray-500">
                    {record.startDate} to {record.endDate}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  {statusBadge(record.status)}
                </div>
              </div>

              {/* Visit counter */}
              <div className="mt-3">
                <div className="flex items-center gap-2">
                  <div className="h-2 flex-1 rounded-full bg-gray-200">
                    <div
                      className={[
                        "h-2 rounded-full transition-all",
                        record.visitsUsed >= record.authorizedVisits
                          ? "bg-red-500"
                          : record.authorizedVisits - record.visitsUsed <= 2
                            ? "bg-amber-500"
                            : "bg-green-500",
                      ].join(" ")}
                      style={{
                        width: `${Math.min(100, (record.visitsUsed / record.authorizedVisits) * 100)}%`,
                      }}
                    />
                  </div>
                </div>
                <p className="mt-1 text-xs text-gray-600">
                  {visitCounterDisplay(record)}
                </p>
              </div>

              {/* CPT codes */}
              {record.authorizedCptCodes &&
                record.authorizedCptCodes.length > 0 && (
                  <p className="mt-2 text-xs text-gray-500">
                    <span className="font-medium">CPT:</span>{" "}
                    {record.authorizedCptCodes.join(", ")}
                  </p>
                )}

              {/* Notes */}
              {record.notes && (
                <p className="mt-1 text-xs text-gray-500 italic">
                  {record.notes}
                </p>
              )}

              {/* Payer phone */}
              {record.payerPhone && (
                <p className="mt-1 text-xs text-gray-500">
                  Payer phone: {record.payerPhone}
                </p>
              )}

              {/* Action buttons */}
              {canEdit && (
                <div className="mt-3 flex gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setEditingRecord(record);
                      setFormOpen(true);
                    }}
                    className="rounded bg-gray-100 px-2.5 py-1 text-xs font-medium text-gray-700 hover:bg-gray-200"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => handleGenerateReauth(record.authId)}
                    disabled={generatingLetter === record.authId}
                    className="rounded bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700 hover:bg-blue-100 disabled:opacity-60"
                  >
                    {generatingLetter === record.authId
                      ? "Generating..."
                      : "Request Re-Auth"}
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Add/Edit modal */}
      {formOpen && (
        <AuthFormModal
          patientId={patientId}
          existingRecord={editingRecord}
          onSuccess={() => {
            setFormOpen(false);
            setEditingRecord(null);
            reload();
          }}
          onClose={() => {
            setFormOpen(false);
            setEditingRecord(null);
          }}
        />
      )}

      {/* Re-auth letter preview modal */}
      {letterContent !== null && (
        <ReauthLetterModal
          letterContent={letterContent}
          onClose={() => setLetterContent(null)}
        />
      )}
    </div>
  );
}
