/**
 * MIPSDashboardPage.tsx — MIPS Quality Measure Capture Dashboard (M004/S07)
 *
 * Displays MIPS quality measure performance rates for the selected year:
 *   - Performance year selector
 *   - Measure cards: name, numerator/denominator, rate %, color-coded tier
 *     Green >= 75% | Amber 50-74% | Red < 50% | Gray = no data
 *   - PHQ-2 depression screening entry form (Measure #134)
 *   - Falls risk screening entry form (Measure #155)
 *
 * Route: { page: "mips" }
 */

import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  MipsDashboard,
  MipsMeasureCard,
  PerformanceTier,
  MipsScreening,
} from "../types/mips";

// ─── Props ────────────────────────────────────────────────────────────────────

interface MIPSDashboardPageProps {
  role: string;
}

// ─── Tier badge ───────────────────────────────────────────────────────────────

function tierClasses(tier: PerformanceTier): string {
  const base = "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-semibold";
  switch (tier) {
    case "Green":
      return `${base} bg-green-100 text-green-800`;
    case "Amber":
      return `${base} bg-amber-100 text-amber-800`;
    case "Red":
      return `${base} bg-red-100 text-red-700`;
    case "NoData":
    default:
      return `${base} bg-gray-100 text-gray-500`;
  }
}

function tierLabel(tier: PerformanceTier): string {
  switch (tier) {
    case "Green":
      return "On Track";
    case "Amber":
      return "Needs Attention";
    case "Red":
      return "Below Threshold";
    case "NoData":
    default:
      return "No Data";
  }
}

function rateCardBorder(tier: PerformanceTier): string {
  switch (tier) {
    case "Green":
      return "border-green-200";
    case "Amber":
      return "border-amber-200";
    case "Red":
      return "border-red-200";
    default:
      return "border-gray-200";
  }
}

// ─── Measure card ─────────────────────────────────────────────────────────────

function MeasureCard({ card }: { card: MipsMeasureCard }) {
  const rateDisplay =
    card.performanceRate !== null
      ? `${card.performanceRate.toFixed(1)}%`
      : "—";

  return (
    <div
      className={[
        "rounded-lg border bg-white p-5 shadow-sm",
        rateCardBorder(card.tier),
      ].join(" ")}
    >
      <div className="mb-3 flex items-start justify-between gap-2">
        <div>
          <p className="text-xs font-medium text-gray-500">
            Measure #{card.measureId}
          </p>
          <p className="mt-0.5 text-sm font-semibold text-gray-900">
            {card.measureName}
          </p>
        </div>
        <span className={tierClasses(card.tier)}>{tierLabel(card.tier)}</span>
      </div>

      {/* Rate */}
      <p className="text-3xl font-bold text-gray-900">{rateDisplay}</p>

      {/* Numerator / denominator */}
      <p className="mt-1 text-xs text-gray-500">
        {card.numerator} / {card.denominator} patients
      </p>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export function MIPSDashboardPage({ role }: MIPSDashboardPageProps) {
  const currentYear = new Date().getFullYear();
  const [selectedYear, setSelectedYear] = useState(currentYear);
  const [dashboard, setDashboard] = useState<MipsDashboard | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // PHQ-2 screening form
  const [phq2PatientId, setPhq2PatientId] = useState("");
  const [phq2Score, setPhq2Score] = useState("");
  const [phq2EncounterId, setPhq2EncounterId] = useState("");
  const [phq2Submitting, setPhq2Submitting] = useState(false);
  const [phq2Error, setPhq2Error] = useState<string | null>(null);
  const [phq2Result, setPhq2Result] = useState<MipsScreening | null>(null);

  // Falls risk screening form
  const [fallsPatientId, setFallsPatientId] = useState("");
  const [fallsResult, setFallsResult] = useState<"positive" | "negative">("negative");
  const [fallsPlanDocumented, setFallsPlanDocumented] = useState(false);
  const [fallsEncounterId, setFallsEncounterId] = useState("");
  const [fallsSubmitting, setFallsSubmitting] = useState(false);
  const [fallsError, setFallsError] = useState<string | null>(null);
  const [fallsSuccess, setFallsSuccess] = useState<string | null>(null);

  // ── Load dashboard ──────────────────────────────────────────────────────────

  const loadDashboard = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await (commands as unknown as {
        getMipsDashboard: (year: number) => Promise<MipsDashboard>;
      }).getMipsDashboard(selectedYear);
      setDashboard(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [selectedYear]);

  useEffect(() => {
    loadDashboard();
  }, [loadDashboard]);

  // ── PHQ-2 submission ───────────────────────────────────────────────────────

  const handlePhq2Submit = useCallback(async () => {
    if (!phq2PatientId.trim() || phq2Score === "") return;
    const scoreNum = parseFloat(phq2Score);
    if (isNaN(scoreNum) || scoreNum < 0 || scoreNum > 6) {
      setPhq2Error("Score must be a number between 0 and 6.");
      return;
    }
    setPhq2Submitting(true);
    setPhq2Error(null);
    setPhq2Result(null);
    try {
      const result = await (commands as unknown as {
        recordPhq2Screening: (
          patientId: string,
          score: number,
          encounterId: string | null
        ) => Promise<MipsScreening>;
      }).recordPhq2Screening(
        phq2PatientId.trim(),
        scoreNum,
        phq2EncounterId.trim() || null
      );
      setPhq2Result(result);
      setPhq2PatientId("");
      setPhq2Score("");
      setPhq2EncounterId("");
      loadDashboard();
    } catch (e) {
      setPhq2Error(e instanceof Error ? e.message : String(e));
    } finally {
      setPhq2Submitting(false);
    }
  }, [phq2PatientId, phq2Score, phq2EncounterId, loadDashboard]);

  // ── Falls risk submission ──────────────────────────────────────────────────

  const handleFallsSubmit = useCallback(async () => {
    if (!fallsPatientId.trim()) return;
    setFallsSubmitting(true);
    setFallsError(null);
    setFallsSuccess(null);
    try {
      await (commands as unknown as {
        recordFallsScreening: (
          patientId: string,
          result: string,
          planDocumented: boolean,
          encounterId: string | null
        ) => Promise<MipsScreening>;
      }).recordFallsScreening(
        fallsPatientId.trim(),
        fallsResult,
        fallsPlanDocumented,
        fallsEncounterId.trim() || null
      );
      setFallsSuccess("Falls risk screening recorded successfully.");
      setFallsPatientId("");
      setFallsResult("negative");
      setFallsPlanDocumented(false);
      setFallsEncounterId("");
      loadDashboard();
    } catch (e) {
      setFallsError(e instanceof Error ? e.message : String(e));
    } finally {
      setFallsSubmitting(false);
    }
  }, [fallsPatientId, fallsResult, fallsPlanDocumented, fallsEncounterId, loadDashboard]);

  // ── Year options ───────────────────────────────────────────────────────────

  const yearOptions = [currentYear, currentYear - 1, currentYear - 2];

  // ── Render ─────────────────────────────────────────────────────────────────

  const isProvider = role === "Provider" || role === "SystemAdmin";

  return (
    <div className="flex h-full flex-col p-6">
      {/* Header */}
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">
            MIPS Quality Dashboard
          </h1>
          <p className="mt-1 text-sm text-gray-500">
            Merit-based Incentive Payment System — performance year{" "}
            {selectedYear}
          </p>
        </div>

        {/* Year selector */}
        <div className="flex items-center gap-2">
          <label className="text-sm font-medium text-gray-700">
            Performance Year
          </label>
          <select
            value={selectedYear}
            onChange={(e) => setSelectedYear(Number(e.target.value))}
            className="rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            {yearOptions.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={loadDashboard}
            disabled={loading}
            className="rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:opacity-50"
          >
            {loading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>

      {error && (
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-8">
        {/* ── Projected composite score ──────────────────────────────────────── */}
        {dashboard && (
          <section>
            <div className="mb-4 flex items-center gap-3">
              <h2 className="text-base font-semibold text-gray-900">
                Projected Composite Score
              </h2>
              {dashboard.projectedCompositeScore !== null ? (
                <span
                  className={[
                    "text-2xl font-bold",
                    dashboard.projectedCompositeScore >= 75
                      ? "text-green-700"
                      : dashboard.projectedCompositeScore >= 50
                        ? "text-amber-600"
                        : "text-red-600",
                  ].join(" ")}
                >
                  {dashboard.projectedCompositeScore.toFixed(1)}%
                </span>
              ) : (
                <span className="text-sm text-gray-400">No data yet</span>
              )}
            </div>

            {/* ── Measure cards grid ────────────────────────────────────────── */}
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
              {dashboard.measures.map((card) => (
                <MeasureCard key={card.measureId} card={card} />
              ))}
            </div>

            <p className="mt-3 text-xs text-gray-400">
              Last computed: {new Date(dashboard.computedAt).toLocaleString()}
            </p>
          </section>
        )}

        {loading && !dashboard && (
          <p className="text-sm text-gray-500">Loading measures…</p>
        )}

        {/* ── PHQ-2 Screening Entry (Measure #134) ──────────────────────────── */}
        {isProvider && (
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm max-w-xl">
            <h2 className="mb-1 text-base font-semibold text-gray-900">
              PHQ-2 Depression Screening (Measure #134)
            </h2>
            <p className="mb-4 text-sm text-gray-500">
              Record a PHQ-2 score for a patient. Scores ≥ 3 automatically flag
              for PHQ-9 follow-up.
            </p>

            {phq2Result && (
              <div className="mb-4 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                Screening recorded.{" "}
                {phq2Result.result === "positive" ? (
                  <span className="font-medium text-amber-700">
                    PHQ-9 follow-up required.
                  </span>
                ) : (
                  "Screen negative."
                )}
              </div>
            )}

            {phq2Error && (
              <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {phq2Error}
              </div>
            )}

            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Patient ID
                </label>
                <input
                  type="text"
                  value={phq2PatientId}
                  onChange={(e) => setPhq2PatientId(e.target.value)}
                  placeholder="Patient UUID…"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>

              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  PHQ-2 Score (0–6)
                </label>
                <input
                  type="number"
                  min={0}
                  max={6}
                  step={1}
                  value={phq2Score}
                  onChange={(e) => setPhq2Score(e.target.value)}
                  placeholder="0"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>

              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Encounter ID (optional)
                </label>
                <input
                  type="text"
                  value={phq2EncounterId}
                  onChange={(e) => setPhq2EncounterId(e.target.value)}
                  placeholder="Encounter UUID…"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>

              <button
                type="button"
                onClick={handlePhq2Submit}
                disabled={
                  phq2Submitting ||
                  !phq2PatientId.trim() ||
                  phq2Score === ""
                }
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {phq2Submitting ? "Recording…" : "Record PHQ-2 Screening"}
              </button>
            </div>
          </section>
        )}

        {/* ── Falls Risk Screening Entry (Measure #155) ─────────────────────── */}
        {isProvider && (
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm max-w-xl">
            <h2 className="mb-1 text-base font-semibold text-gray-900">
              Falls Risk Screening (Measure #155)
            </h2>
            <p className="mb-4 text-sm text-gray-500">
              Document falls risk screening for patients aged 65+. Record result
              and whether a plan of care was documented.
            </p>

            {fallsSuccess && (
              <div className="mb-4 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {fallsSuccess}
              </div>
            )}

            {fallsError && (
              <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {fallsError}
              </div>
            )}

            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Patient ID
                </label>
                <input
                  type="text"
                  value={fallsPatientId}
                  onChange={(e) => setFallsPatientId(e.target.value)}
                  placeholder="Patient UUID…"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>

              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Screening Result
                </label>
                <select
                  value={fallsResult}
                  onChange={(e) =>
                    setFallsResult(e.target.value as "positive" | "negative")
                  }
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                >
                  <option value="negative">Negative</option>
                  <option value="positive">Positive</option>
                </select>
              </div>

              <div className="flex items-center gap-3">
                <input
                  type="checkbox"
                  id="falls-plan"
                  checked={fallsPlanDocumented}
                  onChange={(e) => setFallsPlanDocumented(e.target.checked)}
                  className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                />
                <label
                  htmlFor="falls-plan"
                  className="text-sm font-medium text-gray-700"
                >
                  Plan of care documented
                </label>
              </div>

              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Encounter ID (optional)
                </label>
                <input
                  type="text"
                  value={fallsEncounterId}
                  onChange={(e) => setFallsEncounterId(e.target.value)}
                  placeholder="Encounter UUID…"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>

              <button
                type="button"
                onClick={handleFallsSubmit}
                disabled={fallsSubmitting || !fallsPatientId.trim()}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {fallsSubmitting ? "Recording…" : "Record Falls Screening"}
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
