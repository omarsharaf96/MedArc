/**
 * ClaimsPage.tsx — Electronic Claims Submission UI (M004/S02)
 *
 * Claim list with status/payer/patient filters, color-coded status badges,
 * create claim from encounter billing, validate, submit, and EDI preview.
 *
 * Route: { page: "claims"; patientId?: string }
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type {
  ClaimRecord,
  ClaimStatus,
  PayerRecord,
  CreateClaimInput,
  UpdateClaimStatusInput,
  ValidationResult,
  EdiGenerationResult,
} from "../types/claims";

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  patientId?: string;
  role: string;
}

// ─── Tailwind helpers ────────────────────────────────────────────────────────

const BTN_PRIMARY =
  "rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50";
const BTN_SECONDARY =
  "rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50";
const BTN_SUCCESS =
  "rounded-md bg-green-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50";
const BTN_DANGER =
  "rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50";
const INPUT_CLS =
  "rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-xs font-medium text-gray-600";

// ─── Status badge colors ─────────────────────────────────────────────────────

const STATUS_COLORS: Record<ClaimStatus, string> = {
  draft: "bg-gray-100 text-gray-700",
  validated: "bg-blue-100 text-blue-700",
  submitted: "bg-yellow-100 text-yellow-700",
  accepted: "bg-cyan-100 text-cyan-700",
  paid: "bg-green-100 text-green-700",
  denied: "bg-red-100 text-red-700",
  appealed: "bg-orange-100 text-orange-700",
};

function StatusBadge({ status }: { status: string }) {
  const colorClass =
    STATUS_COLORS[status as ClaimStatus] ?? "bg-gray-100 text-gray-600";
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium capitalize ${colorClass}`}
    >
      {status}
    </span>
  );
}

// ─── Create Claim Modal ──────────────────────────────────────────────────────

interface CreateClaimModalProps {
  payers: PayerRecord[];
  patientId: string;
  onClose: () => void;
  onCreated: (claim: ClaimRecord) => void;
}

function CreateClaimModal({
  payers,
  patientId,
  onClose,
  onCreated,
}: CreateClaimModalProps) {
  const [encounterBillingId, setEncounterBillingId] = useState("");
  const [payerId, setPayerId] = useState(payers[0]?.payerId ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!encounterBillingId.trim()) {
      setError("Encounter billing ID is required");
      return;
    }
    if (!payerId) {
      setError("Select a payer");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const input: CreateClaimInput = {
        encounterBillingId: encounterBillingId.trim(),
        payerId,
        patientId,
      };
      const claim = await commands.createClaim(input);
      onCreated(claim);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">
          Create New Claim
        </h2>

        {error && (
          <div className="mb-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
            {error}
          </div>
        )}

        <div className="space-y-4">
          <div>
            <label className={LABEL_CLS}>Encounter Billing ID</label>
            <input
              type="text"
              className={`${INPUT_CLS} w-full`}
              placeholder="billing_id from encounter_billing"
              value={encounterBillingId}
              onChange={(e) => setEncounterBillingId(e.target.value)}
            />
          </div>

          <div>
            <label className={LABEL_CLS}>Payer</label>
            <select
              className={`${INPUT_CLS} w-full`}
              value={payerId}
              onChange={(e) => setPayerId(e.target.value)}
            >
              {payers.map((p) => (
                <option key={p.payerId} value={p.payerId}>
                  {p.name} {p.ediPayerId ? `(${p.ediPayerId})` : ""}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-3">
          <button className={BTN_SECONDARY} onClick={onClose} disabled={loading}>
            Cancel
          </button>
          <button
            className={BTN_PRIMARY}
            onClick={handleSubmit}
            disabled={loading}
          >
            {loading ? "Creating..." : "Create Claim"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── EDI Preview Modal ───────────────────────────────────────────────────────

function EdiPreviewModal({
  result,
  onClose,
}: {
  result: EdiGenerationResult;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="flex h-4/5 w-full max-w-3xl flex-col rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
          <div>
            <h2 className="text-lg font-semibold text-gray-900">837P EDI Preview</h2>
            <p className="mt-0.5 text-xs text-gray-500">
              Control: {result.controlNumber} — {result.segmentCount} segments
            </p>
          </div>
          <button onClick={onClose} className="text-gray-400 hover:text-gray-600">
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 overflow-auto p-6">
          <pre className="whitespace-pre-wrap rounded-md bg-gray-50 p-4 font-mono text-xs text-gray-800">
            {result.ediContent}
          </pre>
        </div>
        <div className="border-t border-gray-200 px-6 py-4">
          <p className="text-xs text-gray-500">File: {result.ediFilePath}</p>
        </div>
      </div>
    </div>
  );
}

// ─── Validation Result Panel ─────────────────────────────────────────────────

function ValidationPanel({
  result,
  onClose,
}: {
  result: ValidationResult;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-lg rounded-lg bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-center gap-3">
          {result.valid ? (
            <div className="flex h-8 w-8 items-center justify-center rounded-full bg-green-100">
              <svg className="h-5 w-5 text-green-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
              </svg>
            </div>
          ) : (
            <div className="flex h-8 w-8 items-center justify-center rounded-full bg-red-100">
              <svg className="h-5 w-5 text-red-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </div>
          )}
          <h2 className="text-lg font-semibold text-gray-900">
            {result.valid ? "Claim Validated" : "Validation Failed"}
          </h2>
        </div>

        {result.valid ? (
          <p className="text-sm text-gray-600">
            All required fields are present. Claim status updated to validated.
          </p>
        ) : (
          <ul className="space-y-2">
            {result.errors.map((err, i) => (
              <li
                key={i}
                className="flex items-start gap-2 rounded-md bg-red-50 p-2.5 text-sm text-red-700"
              >
                <span className="mt-0.5 shrink-0 text-red-400">•</span>
                {err}
              </li>
            ))}
          </ul>
        )}

        <div className="mt-6 flex justify-end">
          <button className={BTN_SECONDARY} onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Component ──────────────────────────────────────────────────────────

export function ClaimsPage({ patientId, role }: Props) {
  const { goBack } = useNav();

  const [claims, setClaims] = useState<ClaimRecord[]>([]);
  const [payers, setPayers] = useState<PayerRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  // Filters
  const [statusFilter, setStatusFilter] = useState<string>("");
  const [payerFilter, setPayerFilter] = useState<string>("");

  // Modals
  const [showCreate, setShowCreate] = useState(false);
  const [validationResult, setValidationResult] = useState<ValidationResult | null>(null);
  const [ediResult, setEdiResult] = useState<EdiGenerationResult | null>(null);

  // Can modify — billing staff, provider, system admin
  const canModify = ["SystemAdmin", "Provider", "BillingStaff"].includes(role);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [claimList, payerList] = await Promise.all([
        commands.listClaims(
          patientId ?? null,
          (statusFilter as ClaimStatus) || null,
          payerFilter || null,
        ),
        commands.listPayers(),
      ]);
      setClaims(claimList);
      setPayers(payerList);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [patientId, statusFilter, payerFilter]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleValidate = async (claim: ClaimRecord) => {
    setActionLoading(claim.claimId);
    try {
      const result = await commands.validateClaim(claim.claimId);
      setValidationResult(result);
      if (result.valid) {
        await loadData();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setActionLoading(null);
    }
  };

  const handleGenerate837p = async (claim: ClaimRecord) => {
    setActionLoading(claim.claimId);
    try {
      const result = await commands.generate837p(
        claim.encounterBillingId,
        claim.payerId,
      );
      setEdiResult(result);
      await loadData();
    } catch (e) {
      setError(String(e));
    } finally {
      setActionLoading(null);
    }
  };

  const handleSubmit = async (claim: ClaimRecord) => {
    if (!confirm("Submit this claim? This will mark it as submitted.")) return;
    setActionLoading(claim.claimId);
    try {
      await commands.submitClaim(claim.claimId);
      await loadData();
    } catch (e) {
      setError(String(e));
    } finally {
      setActionLoading(null);
    }
  };

  const handleStatusUpdate = async (
    claim: ClaimRecord,
    newStatus: ClaimStatus,
  ) => {
    const input: UpdateClaimStatusInput = { status: newStatus };
    setActionLoading(claim.claimId);
    try {
      await commands.updateClaimStatus(claim.claimId, input);
      await loadData();
    } catch (e) {
      setError(String(e));
    } finally {
      setActionLoading(null);
    }
  };

  const payerMap = Object.fromEntries(payers.map((p) => [p.payerId, p]));

  return (
    <div className="flex h-full flex-col bg-gray-50">
      {/* Header */}
      <div className="border-b border-gray-200 bg-white px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <button
              onClick={goBack}
              className="flex items-center gap-1 text-sm text-gray-500 hover:text-gray-700"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
              Back
            </button>
            <div>
              <h1 className="text-xl font-semibold text-gray-900">
                Claims
              </h1>
              {patientId && (
                <p className="text-xs text-gray-500">Patient: {patientId}</p>
              )}
            </div>
          </div>

          {canModify && payers.length > 0 && (
            <button
              className={BTN_PRIMARY}
              onClick={() => setShowCreate(true)}
            >
              + New Claim
            </button>
          )}
          {canModify && payers.length === 0 && (
            <p className="text-xs text-amber-600">
              Configure a payer first to create claims.
            </p>
          )}
        </div>
      </div>

      {/* Filters */}
      <div className="border-b border-gray-200 bg-white px-6 py-3">
        <div className="flex flex-wrap gap-4">
          <div>
            <label className={LABEL_CLS}>Status</label>
            <select
              className={INPUT_CLS}
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
            >
              <option value="">All Statuses</option>
              {(
                [
                  "draft",
                  "validated",
                  "submitted",
                  "accepted",
                  "paid",
                  "denied",
                  "appealed",
                ] as ClaimStatus[]
              ).map((s) => (
                <option key={s} value={s}>
                  {s.charAt(0).toUpperCase() + s.slice(1)}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className={LABEL_CLS}>Payer</label>
            <select
              className={INPUT_CLS}
              value={payerFilter}
              onChange={(e) => setPayerFilter(e.target.value)}
            >
              <option value="">All Payers</option>
              {payers.map((p) => (
                <option key={p.payerId} value={p.payerId}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6">
        {error && (
          <div className="mb-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
            {error}
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center py-12">
            <div className="h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
          </div>
        ) : claims.length === 0 ? (
          <div className="rounded-lg border border-gray-200 bg-white p-12 text-center">
            <p className="text-sm text-gray-500">
              No claims found.
              {canModify && payers.length > 0
                ? " Create one from an encounter billing record."
                : ""}
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {claims.map((claim) => {
              const payer = payerMap[claim.payerId];
              const isActing = actionLoading === claim.claimId;

              return (
                <div
                  key={claim.claimId}
                  className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm"
                >
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    {/* Left: claim info */}
                    <div className="flex-1 min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <StatusBadge status={claim.status} />
                        <span className="font-mono text-xs text-gray-500">
                          {claim.claimId.slice(0, 8)}…
                        </span>
                        {claim.controlNumber && (
                          <span className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-xs text-gray-600">
                            CTL: {claim.controlNumber}
                          </span>
                        )}
                      </div>

                      <div className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-sm text-gray-600 sm:grid-cols-3">
                        <div>
                          <span className="font-medium">Payer: </span>
                          {payer?.name ?? claim.payerId}
                        </div>
                        <div>
                          <span className="font-medium">Billing: </span>
                          <span className="font-mono text-xs">
                            {claim.encounterBillingId.slice(0, 12)}…
                          </span>
                        </div>
                        {claim.submittedAt && (
                          <div>
                            <span className="font-medium">Submitted: </span>
                            {new Date(claim.submittedAt).toLocaleDateString()}
                          </div>
                        )}
                        {claim.paidAmount != null && (
                          <div>
                            <span className="font-medium">Paid: </span>
                            ${claim.paidAmount.toFixed(2)}
                          </div>
                        )}
                        {claim.denialReason && (
                          <div className="col-span-2">
                            <span className="font-medium text-red-600">
                              Denial:{" "}
                            </span>
                            {claim.denialReason}
                          </div>
                        )}
                      </div>
                    </div>

                    {/* Right: action buttons */}
                    {canModify && (
                      <div className="flex flex-wrap gap-2">
                        {claim.status === "draft" && (
                          <>
                            <button
                              className={BTN_SECONDARY}
                              disabled={isActing}
                              onClick={() => handleValidate(claim)}
                            >
                              {isActing ? "..." : "Validate"}
                            </button>
                            <button
                              className={BTN_SECONDARY}
                              disabled={isActing}
                              onClick={() => handleGenerate837p(claim)}
                            >
                              {isActing ? "..." : "Generate 837P"}
                            </button>
                          </>
                        )}

                        {claim.status === "validated" && (
                          <>
                            <button
                              className={BTN_SECONDARY}
                              disabled={isActing}
                              onClick={() => handleGenerate837p(claim)}
                            >
                              {isActing ? "..." : "Generate 837P"}
                            </button>
                            <button
                              className={BTN_SUCCESS}
                              disabled={isActing}
                              onClick={() => handleSubmit(claim)}
                            >
                              {isActing ? "..." : "Submit"}
                            </button>
                          </>
                        )}

                        {claim.status === "submitted" && (
                          <div className="flex gap-2">
                            <button
                              className={BTN_SUCCESS}
                              disabled={isActing}
                              onClick={() =>
                                handleStatusUpdate(claim, "accepted")
                              }
                            >
                              Mark Accepted
                            </button>
                            <button
                              className={BTN_DANGER}
                              disabled={isActing}
                              onClick={() => handleStatusUpdate(claim, "denied")}
                            >
                              Mark Denied
                            </button>
                          </div>
                        )}

                        {claim.status === "accepted" && (
                          <button
                            className={BTN_SUCCESS}
                            disabled={isActing}
                            onClick={() => handleStatusUpdate(claim, "paid")}
                          >
                            Mark Paid
                          </button>
                        )}

                        {claim.status === "denied" && (
                          <button
                            className={BTN_SECONDARY}
                            disabled={isActing}
                            onClick={() =>
                              handleStatusUpdate(claim, "appealed")
                            }
                          >
                            Appeal
                          </button>
                        )}

                        {claim.ediContent && (
                          <button
                            className={BTN_SECONDARY}
                            onClick={() =>
                              setEdiResult({
                                claimId: claim.claimId,
                                ediContent: claim.ediContent!,
                                ediFilePath: claim.ediFilePath ?? "",
                                segmentCount: 0,
                                controlNumber: claim.controlNumber ?? "",
                              })
                            }
                          >
                            View EDI
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Modals */}
      {showCreate && patientId && (
        <CreateClaimModal
          payers={payers}
          patientId={patientId}
          onClose={() => setShowCreate(false)}
          onCreated={async (_claim) => {
            setShowCreate(false);
            await loadData();
          }}
        />
      )}

      {validationResult && (
        <ValidationPanel
          result={validationResult}
          onClose={() => setValidationResult(null)}
        />
      )}

      {ediResult && (
        <EdiPreviewModal
          result={ediResult}
          onClose={() => setEdiResult(null)}
        />
      )}
    </div>
  );
}
