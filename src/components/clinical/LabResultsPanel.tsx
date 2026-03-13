/**
 * LabResultsPanel.tsx — Patient-scoped lab orders and lab results panel.
 *
 * Renders two sub-sections within a single panel:
 *   1. Orders — lists FHIR ServiceRequest lab orders for the patient.
 *   2. Results — lists FHIR DiagnosticReport lab results; abnormal rows
 *      are highlighted in amber. Providers/SystemAdmins can sign
 *      preliminary results. Any non-FrontDesk role can enter new results.
 *
 * Fetch domains are isolated: an orders fetch failure shows an error only
 * in the Orders sub-section; a results fetch failure shows an error only
 * in the Results sub-section.
 *
 * Observability:
 *   - console.error("[LabResultsPanel] orders fetch failed:", msg)
 *   - console.error("[LabResultsPanel] results fetch failed:", msg)
 *   - console.error("[LabResultsPanel] sign failed:", msg)
 *   - Inline red <p> for each sub-section error, visible without DevTools
 *   - ordersError / resultsError visible in React DevTools
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../../lib/tauri";
import { extractLabOrderDisplay, extractLabResultDisplay } from "../../lib/fhirExtract";
import type { LabOrderRecord, LabResultRecord } from "../../types/labs";

// ─── Props ───────────────────────────────────────────────────────────────────

interface LabResultsPanelProps {
  patientId: string;
  userId: string;
  role: string;
}

// ─── Enter Result modal form state ────────────────────────────────────────────

interface EnterForm {
  loincCode: string;
  displayName: string;
  status: "preliminary" | "final";
  value: string;
  unit: string;
  referenceRange: string;
  orderId: string;
}

const defaultEnterForm = (): EnterForm => ({
  loincCode: "",
  displayName: "",
  status: "preliminary",
  value: "",
  unit: "",
  referenceRange: "",
  orderId: "",
});

// ─── Sub-section error banner ─────────────────────────────────────────────────

function SectionErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
      <p className="font-semibold">Failed to load data</p>
      <p className="mt-0.5 text-xs">{message}</p>
      <button
        type="button"
        onClick={onRetry}
        className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1"
      >
        Retry
      </button>
    </div>
  );
}

// ─── Enter Result Modal ───────────────────────────────────────────────────────

function EnterResultModal({
  onClose,
  onSubmit,
  submitError,
  submitting,
}: {
  onClose: () => void;
  onSubmit: (form: EnterForm) => Promise<void>;
  submitError: string | null;
  submitting: boolean;
}) {
  const [form, setForm] = useState<EnterForm>(defaultEnterForm);

  function set<K extends keyof EnterForm>(key: K, value: EnterForm[K]) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    await onSubmit(form);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="w-full max-w-lg rounded-lg bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h3 className="text-base font-semibold text-gray-900">Enter Lab Result</h3>
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-gray-400 disabled:opacity-50"
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* LOINC Code */}
          <div>
            <label
              htmlFor="lr-loinc"
              className="block text-xs font-medium text-gray-700"
            >
              LOINC Code <span className="text-red-500">*</span>
            </label>
            <input
              id="lr-loinc"
              type="text"
              required
              value={form.loincCode}
              onChange={(e) => set("loincCode", e.target.value)}
              placeholder="e.g. 2345-7"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Display Name */}
          <div>
            <label
              htmlFor="lr-display"
              className="block text-xs font-medium text-gray-700"
            >
              Display Name <span className="text-red-500">*</span>
            </label>
            <input
              id="lr-display"
              type="text"
              required
              value={form.displayName}
              onChange={(e) => set("displayName", e.target.value)}
              placeholder="e.g. Glucose"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Status */}
          <div>
            <label
              htmlFor="lr-status"
              className="block text-xs font-medium text-gray-700"
            >
              Status
            </label>
            <select
              id="lr-status"
              value={form.status}
              onChange={(e) =>
                set("status", e.target.value as "preliminary" | "final")
              }
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            >
              <option value="preliminary">Preliminary</option>
              <option value="final">Final</option>
            </select>
          </div>

          {/* Value */}
          <div>
            <label
              htmlFor="lr-value"
              className="block text-xs font-medium text-gray-700"
            >
              Value
            </label>
            <input
              id="lr-value"
              type="text"
              value={form.value}
              onChange={(e) => set("value", e.target.value)}
              placeholder="e.g. 95 or Positive"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Unit */}
          <div>
            <label
              htmlFor="lr-unit"
              className="block text-xs font-medium text-gray-700"
            >
              Unit (optional)
            </label>
            <input
              id="lr-unit"
              type="text"
              value={form.unit}
              onChange={(e) => set("unit", e.target.value)}
              placeholder="e.g. mg/dL"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Reference Range */}
          <div>
            <label
              htmlFor="lr-range"
              className="block text-xs font-medium text-gray-700"
            >
              Reference Range (optional)
            </label>
            <input
              id="lr-range"
              type="text"
              value={form.referenceRange}
              onChange={(e) => set("referenceRange", e.target.value)}
              placeholder="e.g. 70–100 mg/dL"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Linked Order ID */}
          <div>
            <label
              htmlFor="lr-order"
              className="block text-xs font-medium text-gray-700"
            >
              Linked Order ID (optional)
            </label>
            <input
              id="lr-order"
              type="text"
              value={form.orderId}
              onChange={(e) => set("orderId", e.target.value)}
              placeholder="Leave blank if no order"
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-900 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </div>

          {/* Submit error */}
          {submitError && (
            <p className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
              {submitError}
            </p>
          )}

          {/* Actions */}
          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1 disabled:opacity-60"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1 disabled:opacity-60"
            >
              {submitting ? "Saving…" : "Save Result"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

/**
 * LabResultsPanel — Patient-scoped lab orders and lab results.
 * Rendered inside PatientDetailPage for roles that can read LabResults
 * (i.e. everyone except FrontDesk — gate applied in PatientDetailPage).
 */
export function LabResultsPanel({ patientId, userId, role }: LabResultsPanelProps) {
  // ── Data state ─────────────────────────────────────────────────────────
  const [orders, setOrders] = useState<LabOrderRecord[]>([]);
  const [ordersError, setOrdersError] = useState<string | null>(null);
  const [results, setResults] = useState<LabResultRecord[]>([]);
  const [resultsError, setResultsError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // ── Refresh counter (same pattern as useEncounter) ─────────────────────
  const [refreshCounter, setRefreshCounter] = useState(0);

  // ── Modal state ────────────────────────────────────────────────────────
  const [showEnterModal, setShowEnterModal] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // ── Per-row sign state: resultId → boolean ─────────────────────────────
  const [signingId, setSigningId] = useState<string | null>(null);
  const [signError, setSignError] = useState<string | null>(null);

  // ── Reload callback ────────────────────────────────────────────────────
  const reload = useCallback(() => {
    setRefreshCounter((n) => n + 1);
  }, []);

  // ── Fetch orders and results (parallel, independent error isolation) ────
  useEffect(() => {
    let mounted = true;
    setLoading(true);

    const fetchOrders = commands
      .listLabOrders(patientId, null)
      .then((data) => {
        if (mounted) {
          setOrders(data);
          setOrdersError(null);
        }
      })
      .catch((e) => {
        if (mounted) {
          const msg = e instanceof Error ? e.message : String(e);
          console.error("[LabResultsPanel] orders fetch failed:", msg);
          setOrdersError(msg);
          setOrders([]);
        }
      });

    const fetchResults = commands
      .listLabResults(patientId, null, null)
      .then((data) => {
        if (mounted) {
          setResults(data);
          setResultsError(null);
        }
      })
      .catch((e) => {
        if (mounted) {
          const msg = e instanceof Error ? e.message : String(e);
          console.error("[LabResultsPanel] results fetch failed:", msg);
          setResultsError(msg);
          setResults([]);
        }
      });

    Promise.allSettled([fetchOrders, fetchResults]).finally(() => {
      if (mounted) setLoading(false);
    });

    return () => {
      mounted = false;
    };
  }, [patientId, refreshCounter]);

  // ── Sign a result ──────────────────────────────────────────────────────
  async function handleSign(resultId: string) {
    setSigningId(resultId);
    setSignError(null);
    try {
      await commands.signLabResult({ resultId, providerId: userId, comment: null });
      reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[LabResultsPanel] sign failed:", msg);
      setSignError(msg);
    } finally {
      setSigningId(null);
    }
  }

  // ── Submit Enter Result modal ──────────────────────────────────────────
  async function handleEnterResult(form: EnterForm) {
    setSubmitting(true);
    setSubmitError(null);
    try {
      await commands.enterLabResult({
        patientId,
        orderId: form.orderId.trim() || null,
        providerId: userId,
        loincCode: form.loincCode.trim(),
        displayName: form.displayName.trim(),
        status: form.status,
        reportedAt: new Date().toISOString(),
        performingLab: null,
        observations: form.value.trim()
          ? [
              {
                loincCode: form.loincCode.trim(),
                displayName: form.displayName.trim(),
                valueQuantity: null,
                unit: form.unit.trim() || null,
                valueString: form.value.trim(),
                referenceRange: form.referenceRange.trim() || null,
                interpretation: null,
              },
            ]
          : [],
        conclusion: null,
      });
      setShowEnterModal(false);
      reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  const canEnterResult = role !== "FrontDesk";
  const canSign = role === "Provider" || role === "SystemAdmin";

  // ── Loading skeleton ───────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="animate-pulse space-y-3 py-2">
        <div className="h-4 w-1/3 rounded bg-gray-200" />
        <div className="h-4 w-2/3 rounded bg-gray-200" />
        <div className="h-4 w-1/2 rounded bg-gray-200" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* ── Orders sub-section ───────────────────────────────────────── */}
      <div>
        <h3 className="mb-3 text-sm font-semibold text-gray-700 uppercase tracking-wide">
          Orders
        </h3>

        {ordersError ? (
          <SectionErrorBanner message={ordersError} onRetry={reload} />
        ) : orders.length === 0 ? (
          <p className="text-sm text-gray-500">No lab orders on record.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  <th className="pb-2 pr-4">Test Name</th>
                  <th className="pb-2 pr-4">LOINC</th>
                  <th className="pb-2 pr-4">Status</th>
                  <th className="pb-2 pr-4">Priority</th>
                  <th className="pb-2">Last Updated</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-50">
                {orders.map((order) => {
                  const d = extractLabOrderDisplay(order.resource);
                  return (
                    <tr key={order.id} className="hover:bg-gray-50">
                      <td className="py-2 pr-4 text-gray-900">
                        {d.displayName ?? "—"}
                      </td>
                      <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                        {d.loincCode ?? order.loincCode ?? "—"}
                      </td>
                      <td className="py-2 pr-4">
                        <OrderStatusBadge status={d.status ?? order.status} />
                      </td>
                      <td className="py-2 pr-4 text-gray-600 capitalize">
                        {d.priority ?? order.priority ?? "—"}
                      </td>
                      <td className="py-2 text-gray-600">
                        {formatDate(order.lastUpdated)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ── Results sub-section ──────────────────────────────────────── */}
      <div>
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">
            Results
          </h3>
          {canEnterResult && (
            <button
              type="button"
              onClick={() => {
                setSubmitError(null);
                setShowEnterModal(true);
              }}
              className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
            >
              + Enter Result
            </button>
          )}
        </div>

        {/* Sign error (shown below header) */}
        {signError && (
          <p className="mb-2 rounded-md border border-red-200 bg-red-50 px-3 py-1.5 text-xs text-red-700">
            Sign failed: {signError}
          </p>
        )}

        {resultsError ? (
          <SectionErrorBanner message={resultsError} onRetry={reload} />
        ) : results.length === 0 ? (
          <p className="text-sm text-gray-500">No lab results on record.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  <th className="pb-2 pr-4">Test Name</th>
                  <th className="pb-2 pr-4">LOINC</th>
                  <th className="pb-2 pr-4">Status</th>
                  <th className="pb-2 pr-4">Abnormal</th>
                  <th className="pb-2 pr-4">Last Updated</th>
                  {canSign && <th className="pb-2" />}
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-50">
                {results.map((result) => {
                  const d = extractLabResultDisplay(result);
                  const isAbnormal = result.hasAbnormal === true;
                  const rowCls = isAbnormal
                    ? "bg-amber-50 border-l-4 border-amber-400"
                    : "hover:bg-gray-50";
                  const isPreliminary = result.status === "preliminary";
                  const showSign = canSign && isPreliminary;
                  const isSigning = signingId === result.id;

                  return (
                    <tr key={result.id} className={rowCls}>
                      <td className="py-2 pr-4 text-gray-900">
                        {/* displayName not in denormalized fields — show loincCode as fallback */}
                        {d.loincCode ?? "—"}
                      </td>
                      <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                        {d.loincCode ?? result.loincCode ?? "—"}
                      </td>
                      <td className="py-2 pr-4">
                        <ResultStatusBadge status={d.status ?? result.status} />
                      </td>
                      <td className="py-2 pr-4">
                        {isAbnormal ? (
                          <span className="inline-flex items-center rounded-full bg-amber-100 px-2 py-0.5 text-xs font-semibold text-amber-800">
                            Abnormal
                          </span>
                        ) : (
                          <span className="text-xs text-gray-400">Normal</span>
                        )}
                      </td>
                      <td className="py-2 pr-4 text-gray-600">
                        {formatDate(result.lastUpdated)}
                      </td>
                      {canSign && (
                        <td className="py-2 text-right">
                          {showSign && (
                            <button
                              type="button"
                              onClick={() => handleSign(result.id)}
                              disabled={isSigning}
                              className="rounded px-2 py-0.5 text-xs font-medium text-indigo-600 hover:bg-indigo-50 focus:outline-none focus:ring-1 focus:ring-indigo-500 disabled:opacity-60"
                            >
                              {isSigning ? "Signing…" : "Sign"}
                            </button>
                          )}
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ── Enter Result modal ────────────────────────────────────────── */}
      {showEnterModal && (
        <EnterResultModal
          onClose={() => setShowEnterModal(false)}
          onSubmit={handleEnterResult}
          submitError={submitError}
          submitting={submitting}
        />
      )}
    </div>
  );
}

// ─── Status badge helpers ─────────────────────────────────────────────────────

function OrderStatusBadge({ status }: { status: string | null }) {
  if (!status) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    status === "active"
      ? "bg-blue-100 text-blue-800"
      : status === "completed"
        ? "bg-green-100 text-green-800"
        : status === "cancelled"
          ? "bg-red-100 text-red-800"
          : status === "draft"
            ? "bg-gray-100 text-gray-600"
            : "bg-gray-100 text-gray-600";
  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}
    >
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

function ResultStatusBadge({ status }: { status: string | null }) {
  if (!status) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    status === "final"
      ? "bg-green-100 text-green-800"
      : status === "preliminary"
        ? "bg-yellow-100 text-yellow-800"
        : status === "amended" || status === "corrected"
          ? "bg-blue-100 text-blue-800"
          : "bg-gray-100 text-gray-600";
  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}
    >
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

// ─── Date formatter ───────────────────────────────────────────────────────────

function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  if (iso.length >= 10) return iso.slice(0, 10);
  return iso;
}
