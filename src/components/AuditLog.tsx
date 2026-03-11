/**
 * AuditLog.tsx — Role-scoped audit log viewer.
 *
 * Renders a paginated table of audit entries. Behaviour by role:
 * - Provider: sees only their own entries (enforced backend-side).
 * - SystemAdmin: sees all entries plus a "Verify Chain" button that checks
 *   cryptographic hash-chain integrity.
 *
 * Columns: Timestamp · Action · Resource Type · Resource ID · Success
 *
 * AUDT-04 / AUDT-05 compliance: backend enforces the scope; this component
 * renders the data and surfaces chain-verification results to the admin.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type { AuditEntry, AuditLogPage, ChainVerificationResult } from "../types/audit";

const PAGE_SIZE = 20;

/** Format an ISO-8601 timestamp as a short local-time string. */
function formatTimestamp(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
  } catch {
    return iso;
  }
}

/** A badge for the action string, colour-coded by category. */
function ActionBadge({ action }: { action: string }) {
  let colour = "bg-gray-100 text-gray-700";
  if (action.startsWith("fhir.")) colour = "bg-blue-100 text-blue-800";
  else if (action.startsWith("auth.")) colour = "bg-purple-100 text-purple-800";
  else if (action.startsWith("break_glass.")) colour = "bg-orange-100 text-orange-800";
  return (
    <span className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${colour}`}>
      {action}
    </span>
  );
}

/** Green ✓ or red ✗ badge for success/failure. */
function SuccessBadge({ success }: { success: boolean }) {
  return success ? (
    <span className="inline-flex items-center gap-1 rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-800">
      ✓ OK
    </span>
  ) : (
    <span className="inline-flex items-center gap-1 rounded-full bg-red-100 px-2 py-0.5 text-xs font-medium text-red-800">
      ✗ Failed
    </span>
  );
}

/** Displays chain verification status returned from verify_audit_chain_cmd. */
function ChainStatus({ result }: { result: ChainVerificationResult }) {
  if (result.valid) {
    return (
      <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-800">
        <span className="text-lg leading-none">🔒</span>
        <span>
          Chain valid — <strong>{result.rowsChecked}</strong>{" "}
          {result.rowsChecked === 1 ? "row" : "rows"} verified, no tampering detected.
        </span>
      </div>
    );
  }
  return (
    <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-800">
      <span className="text-lg leading-none">⚠️</span>
      <div>
        <p className="font-semibold">Chain integrity broken</p>
        {result.error && (
          <p className="mt-0.5 font-mono text-xs">{result.error}</p>
        )}
        <p className="mt-0.5 text-xs text-red-600">
          {result.rowsChecked} rows checked before first broken link.
        </p>
      </div>
    </div>
  );
}

interface AuditLogProps {
  /** The authenticated user's role — controls which features are shown. */
  role: string;
}

export default function AuditLog({ role }: AuditLogProps) {
  const [page, setPage] = useState<AuditLogPage | null>(null);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Chain verification state (SystemAdmin only)
  const [verifying, setVerifying] = useState(false);
  const [chainResult, setChainResult] = useState<ChainVerificationResult | null>(null);
  const [chainError, setChainError] = useState<string | null>(null);

  const fetchPage = useCallback(async (pageOffset: number) => {
    setLoading(true);
    setError(null);
    try {
      const result = await commands.getAuditLog({
        limit: PAGE_SIZE,
        offset: pageOffset,
      });
      setPage(result);
      setOffset(pageOffset);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // Load first page on mount.
  useEffect(() => {
    fetchPage(0);
  }, [fetchPage]);

  const handleVerifyChain = async () => {
    setVerifying(true);
    setChainResult(null);
    setChainError(null);
    try {
      const result = await commands.verifyAuditChain();
      setChainResult(result);
    } catch (err) {
      setChainError(err instanceof Error ? err.message : String(err));
    } finally {
      setVerifying(false);
    }
  };

  const totalPages = page ? Math.ceil(page.total / PAGE_SIZE) : 0;
  const currentPage = Math.floor(offset / PAGE_SIZE) + 1;

  return (
    <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
        <div>
          <h2 className="text-lg font-semibold text-gray-900">Audit Log</h2>
          <p className="mt-0.5 text-xs text-gray-500">
            {role === "SystemAdmin"
              ? "All system activity — tamper-proof hash chain"
              : "Your ePHI access history"}
          </p>
        </div>

        <div className="flex items-center gap-2">
          {/* Refresh */}
          <button
            onClick={() => fetchPage(offset)}
            disabled={loading}
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            {loading ? "Loading…" : "↻ Refresh"}
          </button>

          {/* Verify chain — SystemAdmin only */}
          {role === "SystemAdmin" && (
            <button
              onClick={handleVerifyChain}
              disabled={verifying}
              className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
            >
              {verifying ? "Verifying…" : "🔒 Verify Chain"}
            </button>
          )}
        </div>
      </div>

      {/* Chain verification result banner */}
      {(chainResult || chainError) && (
        <div className="border-b border-gray-200 px-6 py-3">
          {chainResult && <ChainStatus result={chainResult} />}
          {chainError && (
            <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-800">
              Verification error: {chainError}
            </div>
          )}
        </div>
      )}

      {/* Error state */}
      {error && (
        <div className="border-b border-red-200 bg-red-50 px-6 py-3 text-sm text-red-700">
          Failed to load audit log: {error}
        </div>
      )}

      {/* Table */}
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-gray-100 bg-gray-50 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
              <th className="px-4 py-3">Timestamp</th>
              {role === "SystemAdmin" && <th className="px-4 py-3">User ID</th>}
              <th className="px-4 py-3">Action</th>
              <th className="px-4 py-3">Resource Type</th>
              <th className="px-4 py-3">Resource ID</th>
              <th className="px-4 py-3">Result</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50">
            {!page || page.entries.length === 0 ? (
              <tr>
                <td
                  colSpan={role === "SystemAdmin" ? 6 : 5}
                  className="px-4 py-8 text-center text-gray-400"
                >
                  {loading ? "Loading audit entries…" : "No audit entries found."}
                </td>
              </tr>
            ) : (
              page.entries.map((entry: AuditEntry) => (
                <tr
                  key={entry.id}
                  className={`transition-colors hover:bg-gray-50 ${
                    !entry.success ? "bg-red-50/30" : ""
                  }`}
                >
                  <td className="whitespace-nowrap px-4 py-2.5 font-mono text-xs text-gray-600">
                    {formatTimestamp(entry.timestamp)}
                  </td>
                  {role === "SystemAdmin" && (
                    <td className="px-4 py-2.5 font-mono text-xs text-gray-500">
                      <span title={entry.userId}>
                        {entry.userId.length > 12
                          ? entry.userId.slice(0, 8) + "…"
                          : entry.userId}
                      </span>
                    </td>
                  )}
                  <td className="px-4 py-2.5">
                    <ActionBadge action={entry.action} />
                  </td>
                  <td className="px-4 py-2.5 text-gray-700">{entry.resourceType}</td>
                  <td className="px-4 py-2.5 font-mono text-xs text-gray-500">
                    {entry.resourceId ?? (
                      <span className="italic text-gray-300">—</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5">
                    <SuccessBadge success={entry.success} />
                    {entry.details && (
                      <p className="mt-0.5 text-xs text-gray-400" title={entry.details}>
                        {entry.details.length > 40
                          ? entry.details.slice(0, 40) + "…"
                          : entry.details}
                      </p>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* Pagination footer */}
      {page && page.total > 0 && (
        <div className="flex items-center justify-between border-t border-gray-100 px-6 py-3 text-xs text-gray-500">
          <span>
            Showing {offset + 1}–{Math.min(offset + PAGE_SIZE, page.total)} of{" "}
            {page.total} entries
          </span>
          <div className="flex items-center gap-1">
            <button
              onClick={() => fetchPage(Math.max(0, offset - PAGE_SIZE))}
              disabled={offset === 0 || loading}
              className="rounded px-2 py-1 hover:bg-gray-100 disabled:opacity-40"
            >
              ← Prev
            </button>
            <span className="px-2">
              Page {currentPage} / {totalPages}
            </span>
            <button
              onClick={() => fetchPage(offset + PAGE_SIZE)}
              disabled={offset + PAGE_SIZE >= page.total || loading}
              className="rounded px-2 py-1 hover:bg-gray-100 disabled:opacity-40"
            >
              Next →
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
