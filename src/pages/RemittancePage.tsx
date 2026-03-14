/**
 * RemittancePage.tsx — ERA/835 Remittance Processing UI (M003/S02)
 *
 * Features:
 *   - Import 835 file via Tauri file dialog
 *   - Remittance list with payment amounts and posted status
 *   - Auto-post button with match preview modal
 *   - Denial queue with CARC code descriptions
 *   - A/R aging summary cards
 *
 * Route: { page: "remittance" }
 */
import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../lib/tauri";
import type {
  RemittanceRecord,
  DenialRecord,
  ArAgingReport,
  AutoPostResult,
} from "../types/era";
import { carcDescription } from "../types/era";

// ─── Tailwind class constants ─────────────────────────────────────────────────

const BTN_PRIMARY =
  "rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50";
const BTN_SECONDARY =
  "rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50";
const BTN_SUCCESS =
  "rounded-md bg-green-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50";
const CARD_CLS = "rounded-lg border border-gray-200 bg-white p-4 shadow-sm";
const INPUT_CLS =
  "rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-xs font-medium text-gray-600";

// ─── Tab type ─────────────────────────────────────────────────────────────────

type Tab = "remittances" | "denials" | "aging";

// ─── Auto-Post Result Modal ───────────────────────────────────────────────────

interface AutoPostModalProps {
  result: AutoPostResult;
  onClose: () => void;
}

function AutoPostModal({ result, onClose }: AutoPostModalProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">
          Auto-Post Complete
        </h2>
        <div className="space-y-3">
          <div className="flex items-center justify-between rounded-md bg-green-50 p-3">
            <span className="text-sm font-medium text-green-700">Claims matched</span>
            <span className="text-lg font-bold text-green-700">{result.matchedCount}</span>
          </div>
          <div className="flex items-center justify-between rounded-md bg-blue-50 p-3">
            <span className="text-sm font-medium text-blue-700">Payment records created</span>
            <span className="text-lg font-bold text-blue-700">{result.paymentsCreated}</span>
          </div>
          {result.unmatchedControlNumbers.length > 0 && (
            <div className="rounded-md bg-yellow-50 p-3">
              <p className="mb-2 text-sm font-medium text-yellow-700">
                Unmatched control numbers ({result.unmatchedControlNumbers.length})
              </p>
              <ul className="space-y-1">
                {result.unmatchedControlNumbers.map((cn) => (
                  <li key={cn} className="text-xs text-yellow-600 font-mono">
                    {cn}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
        <div className="mt-5 flex justify-end">
          <button className={BTN_PRIMARY} onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Remittance List Tab ──────────────────────────────────────────────────────

interface RemittanceListTabProps {
  remittances: RemittanceRecord[];
  loading: boolean;
  onImport: () => void;
  onAutoPost: (remittanceId: string) => void;
  postingId: string | null;
}

function RemittanceListTab({
  remittances,
  loading,
  onImport,
  onAutoPost,
  postingId,
}: RemittanceListTabProps) {
  function formatDate(dateStr: string | null): string {
    if (!dateStr) return "—";
    // YYYYMMDD → YYYY-MM-DD or ISO string
    if (dateStr.length === 8) {
      return `${dateStr.slice(0, 4)}-${dateStr.slice(4, 6)}-${dateStr.slice(6, 8)}`;
    }
    return dateStr.split("T")[0];
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-gray-500">
          {remittances.length} remittance{remittances.length !== 1 ? "s" : ""} found
        </p>
        <button className={BTN_PRIMARY} onClick={onImport} disabled={loading}>
          Import 835 File
        </button>
      </div>

      {loading ? (
        <div className="py-8 text-center text-sm text-gray-400">Loading...</div>
      ) : remittances.length === 0 ? (
        <div className="py-12 text-center text-sm text-gray-400">
          No remittance files imported yet. Click "Import 835 File" to begin.
        </div>
      ) : (
        <div className="overflow-hidden rounded-lg border border-gray-200">
          <table className="min-w-full divide-y divide-gray-200 text-sm">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  Trace Number
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  Payer
                </th>
                <th className="px-4 py-3 text-right text-xs font-medium uppercase tracking-wide text-gray-500">
                  Payment Amount
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  Payment Date
                </th>
                <th className="px-4 py-3 text-center text-xs font-medium uppercase tracking-wide text-gray-500">
                  Status
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  Imported
                </th>
                <th className="px-4 py-3" />
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 bg-white">
              {remittances.map((r) => (
                <tr key={r.remittanceId} className="hover:bg-gray-50">
                  <td className="px-4 py-3 font-mono text-xs text-gray-700">
                    {r.traceNumber ?? "—"}
                  </td>
                  <td className="px-4 py-3 text-gray-700">
                    {r.payerId ?? "—"}
                  </td>
                  <td className="px-4 py-3 text-right font-medium text-gray-900">
                    ${r.paymentAmount.toFixed(2)}
                  </td>
                  <td className="px-4 py-3 text-gray-600">
                    {formatDate(r.paymentDate)}
                  </td>
                  <td className="px-4 py-3 text-center">
                    {r.posted ? (
                      <span className="inline-flex items-center rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-medium text-green-700">
                        Posted
                      </span>
                    ) : (
                      <span className="inline-flex items-center rounded-full bg-yellow-100 px-2.5 py-0.5 text-xs font-medium text-yellow-700">
                        Pending
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs text-gray-400">
                    {r.createdAt.split("T")[0]}
                  </td>
                  <td className="px-4 py-3 text-right">
                    {!r.posted && (
                      <button
                        className={BTN_SUCCESS}
                        disabled={postingId === r.remittanceId}
                        onClick={() => onAutoPost(r.remittanceId)}
                      >
                        {postingId === r.remittanceId ? "Posting..." : "Auto-Post"}
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ─── Denial Queue Tab ─────────────────────────────────────────────────────────

interface DenialQueueTabProps {
  denials: DenialRecord[];
  loading: boolean;
  statusFilter: string;
  onStatusFilterChange: (v: string) => void;
}

function parseAdjCodes(raw: string | null): Array<{ label: string }> {
  if (!raw) return [];
  return raw.split(",").map((part) => {
    // Format: "CO-45:50.00"
    const [codeGroup, amtStr] = part.split(":");
    const parts = codeGroup.split("-");
    const reasonCode = parts.slice(1).join("-");
    const amount = parseFloat(amtStr ?? "0");
    const desc = carcDescription(reasonCode);
    return { label: `${codeGroup}: ${desc} ($${amount.toFixed(2)})` };
  });
}

function DenialQueueTab({
  denials,
  loading,
  statusFilter,
  onStatusFilterChange,
}: DenialQueueTabProps) {
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <div>
          <label className={LABEL_CLS}>Filter by Status</label>
          <select
            className={INPUT_CLS}
            value={statusFilter}
            onChange={(e) => onStatusFilterChange(e.target.value)}
          >
            <option value="">All</option>
            <option value="denied">Denied</option>
            <option value="appealed">Appealed</option>
          </select>
        </div>
        <div className="mt-4 text-sm text-gray-500">
          {denials.length} denial{denials.length !== 1 ? "s" : ""} in queue
        </div>
      </div>

      {loading ? (
        <div className="py-8 text-center text-sm text-gray-400">Loading...</div>
      ) : denials.length === 0 ? (
        <div className="py-12 text-center text-sm text-gray-400">
          No denials found. Great work!
        </div>
      ) : (
        <div className="space-y-3">
          {denials.map((d) => {
            const adjCodes = parseAdjCodes(d.adjustmentCodes);
            return (
              <div key={d.claimId} className={CARD_CLS}>
                <div className="flex items-start justify-between">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-xs text-gray-500">
                        {d.claimId.slice(0, 8)}...
                      </span>
                      <span
                        className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium capitalize ${
                          d.status === "denied"
                            ? "bg-red-100 text-red-700"
                            : d.status === "appealed"
                            ? "bg-orange-100 text-orange-700"
                            : "bg-gray-100 text-gray-600"
                        }`}
                      >
                        {d.status}
                      </span>
                    </div>
                    <p className="text-sm text-gray-700">
                      Patient: <span className="font-medium">{d.patientId.slice(0, 8)}...</span>
                      {" · "}
                      Payer: <span className="font-medium">{d.payerId.slice(0, 8)}...</span>
                    </p>
                    {d.denialReason && (
                      <p className="text-sm text-red-600">{d.denialReason}</p>
                    )}
                    {adjCodes.length > 0 && (
                      <ul className="mt-1 space-y-0.5">
                        {adjCodes.map((ac, i) => (
                          <li key={i} className="text-xs text-gray-500">
                            {ac.label}
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                  <div className="text-right">
                    {d.paidAmount != null && (
                      <p className="text-sm font-medium text-gray-700">
                        Paid: ${d.paidAmount.toFixed(2)}
                      </p>
                    )}
                    <p className="text-xs text-gray-400">{d.updatedAt.split("T")[0]}</p>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ─── A/R Aging Tab ────────────────────────────────────────────────────────────

interface AgingTabProps {
  report: ArAgingReport | null;
  loading: boolean;
  onRefresh: () => void;
}

const BUCKET_COLORS: Record<string, string> = {
  "0-30": "bg-green-50 border-green-200 text-green-700",
  "31-60": "bg-yellow-50 border-yellow-200 text-yellow-700",
  "61-90": "bg-orange-50 border-orange-200 text-orange-700",
  "91-120": "bg-red-50 border-red-200 text-red-700",
  "120+": "bg-red-100 border-red-300 text-red-800",
};

function AgingTab({ report, loading, onRefresh }: AgingTabProps) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-gray-500">
          Aging is calculated from claim submission date to today for unpaid claims.
        </p>
        <button className={BTN_SECONDARY} onClick={onRefresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {loading ? (
        <div className="py-8 text-center text-sm text-gray-400">Loading...</div>
      ) : !report ? (
        <div className="py-12 text-center text-sm text-gray-400">
          No aging data available.
        </div>
      ) : (
        <>
          {/* Summary Card */}
          <div className="rounded-lg border border-blue-200 bg-blue-50 p-4">
            <p className="text-sm font-medium text-blue-700">Total Outstanding A/R</p>
            <p className="mt-1 text-3xl font-bold text-blue-900">
              ${report.totalOutstanding.toFixed(2)}
            </p>
          </div>

          {/* Bucket Cards */}
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-5">
            {report.buckets.map((bucket) => {
              const colorCls =
                BUCKET_COLORS[bucket.label] ?? "bg-gray-50 border-gray-200 text-gray-700";
              return (
                <div
                  key={bucket.label}
                  className={`rounded-lg border p-4 ${colorCls}`}
                >
                  <p className="text-xs font-semibold uppercase tracking-wide">
                    {bucket.label} days
                  </p>
                  <p className="mt-2 text-2xl font-bold">
                    ${bucket.totalAmount.toFixed(0)}
                  </p>
                  <p className="mt-1 text-xs">
                    {bucket.claimCount} claim{bucket.claimCount !== 1 ? "s" : ""}
                  </p>
                </div>
              );
            })}
          </div>

          {/* Bar chart visualization */}
          {report.totalOutstanding > 0 && (
            <div className={CARD_CLS}>
              <p className="mb-3 text-sm font-medium text-gray-700">Distribution</p>
              <div className="space-y-2">
                {report.buckets.map((bucket) => {
                  const pct =
                    report.totalOutstanding > 0
                      ? (bucket.totalAmount / report.totalOutstanding) * 100
                      : 0;
                  const barColor =
                    bucket.label === "0-30"
                      ? "bg-green-400"
                      : bucket.label === "31-60"
                      ? "bg-yellow-400"
                      : bucket.label === "61-90"
                      ? "bg-orange-400"
                      : "bg-red-500";
                  return (
                    <div key={bucket.label} className="flex items-center gap-3">
                      <span className="w-16 text-right text-xs font-medium text-gray-600">
                        {bucket.label}
                      </span>
                      <div className="flex-1 rounded-full bg-gray-100 h-4 overflow-hidden">
                        <div
                          className={`h-full rounded-full transition-all ${barColor}`}
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="w-20 text-right text-xs text-gray-500">
                        ${bucket.totalAmount.toFixed(0)} ({pct.toFixed(0)}%)
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ─── Main Page Component ──────────────────────────────────────────────────────

interface Props {
  role: string;
}

export function RemittancePage({ role }: Props) {
  const [activeTab, setActiveTab] = useState<Tab>("remittances");

  // Remittances tab state
  const [remittances, setRemittances] = useState<RemittanceRecord[]>([]);
  const [remittancesLoading, setRemittancesLoading] = useState(false);
  const [postingId, setPostingId] = useState<string | null>(null);
  const [autoPostResult, setAutoPostResult] = useState<AutoPostResult | null>(null);

  // Denials tab state
  const [denials, setDenials] = useState<DenialRecord[]>([]);
  const [denialsLoading, setDenialsLoading] = useState(false);
  const [statusFilter, setStatusFilter] = useState("");

  // Aging tab state
  const [agingReport, setAgingReport] = useState<ArAgingReport | null>(null);
  const [agingLoading, setAgingLoading] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);

  // ── Load remittances ──────────────────────────────────────────────────────
  const loadRemittances = useCallback(async () => {
    setRemittancesLoading(true);
    setError(null);
    try {
      const data = await commands.listRemittances();
      setRemittances(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setRemittancesLoading(false);
    }
  }, []);

  // ── Load denials ──────────────────────────────────────────────────────────
  const loadDenials = useCallback(async () => {
    setDenialsLoading(true);
    setError(null);
    try {
      const data = await commands.listDenials(statusFilter || null);
      setDenials(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setDenialsLoading(false);
    }
  }, [statusFilter]);

  // ── Load aging ────────────────────────────────────────────────────────────
  const loadAging = useCallback(async () => {
    setAgingLoading(true);
    setError(null);
    try {
      const data = await commands.getArAging();
      setAgingReport(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setAgingLoading(false);
    }
  }, []);

  // ── Initial loads ─────────────────────────────────────────────────────────
  useEffect(() => {
    loadRemittances();
  }, [loadRemittances]);

  useEffect(() => {
    if (activeTab === "denials") loadDenials();
  }, [activeTab, loadDenials]);

  useEffect(() => {
    if (activeTab === "aging") loadAging();
  }, [activeTab, loadAging]);

  useEffect(() => {
    if (activeTab === "denials") loadDenials();
  }, [statusFilter]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Import 835 ────────────────────────────────────────────────────────────
  async function handleImport() {
    setError(null);
    try {
      const selected = await open({
        filters: [
          { name: "835 EDI Files", extensions: ["835", "edi", "txt", "x12"] },
          { name: "All Files", extensions: ["*"] },
        ],
        multiple: false,
      });

      if (!selected || typeof selected !== "string") return;

      const record = await commands.import835(selected);
      setSuccessMsg(
        `Imported ERA: trace ${record.traceNumber ?? "N/A"}, amount $${record.paymentAmount.toFixed(2)}`
      );
      await loadRemittances();
    } catch (e) {
      setError(String(e));
    }
  }

  // ── Auto-post ─────────────────────────────────────────────────────────────
  async function handleAutoPost(remittanceId: string) {
    setError(null);
    setPostingId(remittanceId);
    try {
      const result = await commands.autoPostRemittance(remittanceId);
      setAutoPostResult(result);
      await loadRemittances();
    } catch (e) {
      setError(String(e));
    } finally {
      setPostingId(null);
    }
  }

  const isReadOnly = role === "nurse_ma" || role === "front_desk";

  const tabs: { id: Tab; label: string }[] = [
    { id: "remittances", label: "Remittances" },
    { id: "denials", label: "Denial Queue" },
    { id: "aging", label: "A/R Aging" },
  ];

  return (
    <div className="p-6">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Remittance Processing</h1>
        <p className="mt-1 text-sm text-gray-500">
          Import 835 ERA files, auto-post payments, manage denials, and review A/R aging.
        </p>
      </div>

      {/* Alerts */}
      {error && (
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error}
          <button
            className="ml-2 font-medium underline"
            onClick={() => setError(null)}
          >
            Dismiss
          </button>
        </div>
      )}
      {successMsg && (
        <div className="mb-4 rounded-md border border-green-200 bg-green-50 p-3 text-sm text-green-700">
          {successMsg}
          <button
            className="ml-2 font-medium underline"
            onClick={() => setSuccessMsg(null)}
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Tab Bar */}
      <div className="mb-6 border-b border-gray-200">
        <nav className="-mb-px flex gap-4">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`pb-3 text-sm font-medium transition-colors ${
                activeTab === tab.id
                  ? "border-b-2 border-blue-600 text-blue-600"
                  : "text-gray-500 hover:text-gray-700"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      {activeTab === "remittances" && (
        <RemittanceListTab
          remittances={remittances}
          loading={remittancesLoading}
          onImport={isReadOnly ? () => {} : handleImport}
          onAutoPost={handleAutoPost}
          postingId={postingId}
        />
      )}
      {activeTab === "denials" && (
        <DenialQueueTab
          denials={denials}
          loading={denialsLoading}
          statusFilter={statusFilter}
          onStatusFilterChange={setStatusFilter}
        />
      )}
      {activeTab === "aging" && (
        <AgingTab
          report={agingReport}
          loading={agingLoading}
          onRefresh={loadAging}
        />
      )}

      {/* Auto-post result modal */}
      {autoPostResult && (
        <AutoPostModal
          result={autoPostResult}
          onClose={() => setAutoPostResult(null)}
        />
      )}
    </div>
  );
}
