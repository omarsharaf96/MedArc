/**
 * AnalyticsDashboardPage.tsx — Analytics & Outcomes Dashboard (M003/S02)
 *
 * Four-section grid layout:
 *   1. Operational KPIs  — visits, cancellation rate, units/visit, new patients
 *   2. Financial KPIs    — revenue/visit, net collection rate, days in A/R, charges vs collections
 *   3. Clinical Outcomes — MCID achievement bar chart, avg improvement (inline SVG)
 *   4. Payer Mix         — revenue by payer as SVG donut chart with legend
 *
 * All charts use inline SVG — no npm chart packages.
 * Follows the ScoreChart pattern from ObjectiveMeasuresPage.tsx.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  OperationalKPIs,
  FinancialKPIs,
  ClinicalOutcomes,
  PayerMix,
  MeasureOutcome,
  PayerBreakdown,
} from "../types/analytics";
import { MIPSDashboardPage } from "./MIPSDashboardPage";

// ─── Tailwind constants ──────────────────────────────────────────────────────

const INPUT_CLS =
  "rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";

const CARD_CLS = "rounded-lg border border-gray-200 bg-white p-5 shadow-sm";

// ─── Color helpers ───────────────────────────────────────────────────────────

/** Cancellation / no-show rate: green <10%, amber 10-15%, red >15% */
function cancelRateColor(rate: number): string {
  if (rate < 10) return "text-green-600";
  if (rate <= 15) return "text-amber-600";
  return "text-red-600";
}

/** Days in A/R: green <35, amber 35-50, red >50 */
function daysArColor(days: number): string {
  if (days < 35) return "text-green-600";
  if (days <= 50) return "text-amber-600";
  return "text-red-600";
}

/** Net collection rate: green >95%, amber 90-95%, red <90% */
function collectionRateColor(rate: number): string {
  if (rate >= 95) return "text-green-600";
  if (rate >= 90) return "text-amber-600";
  return "text-red-600";
}

// ─── KPI card ────────────────────────────────────────────────────────────────

interface KpiCardProps {
  label: string;
  value: string;
  subtext?: string;
  valueClass?: string;
}

function KpiCard({ label, value, subtext, valueClass = "text-gray-900" }: KpiCardProps) {
  return (
    <div className={CARD_CLS}>
      <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">{label}</p>
      <p className={`text-2xl font-bold ${valueClass}`}>{value}</p>
      {subtext && <p className="mt-1 text-xs text-gray-400">{subtext}</p>}
    </div>
  );
}

// ─── MCID Achievement Bar Chart (inline SVG) ─────────────────────────────────

interface McidBarChartProps {
  measures: MeasureOutcome[];
}

function McidBarChart({ measures }: McidBarChartProps) {
  if (measures.length === 0) {
    return (
      <div className="flex h-40 items-center justify-center text-sm text-gray-400">
        No outcome data for this period
      </div>
    );
  }

  const width = 480;
  const height = 160;
  const paddingLeft = 60;
  const paddingRight = 16;
  const paddingTop = 12;
  const paddingBottom = 32;
  const plotWidth = width - paddingLeft - paddingRight;
  const plotHeight = height - paddingTop - paddingBottom;
  const barWidth = Math.min(36, plotWidth / measures.length - 8);
  const gap = plotWidth / measures.length;

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="w-full"
      role="img"
      aria-label="MCID achievement rate by measure"
    >
      {/* Y-axis label */}
      <text
        x={8}
        y={height / 2}
        textAnchor="middle"
        fontSize="9"
        fill="#9ca3af"
        transform={`rotate(-90, 8, ${height / 2})`}
      >
        % Achieved
      </text>

      {/* Y-axis grid lines at 0%, 25%, 50%, 75%, 100% */}
      {[0, 25, 50, 75, 100].map((pct) => {
        const y = paddingTop + plotHeight - (pct / 100) * plotHeight;
        return (
          <g key={pct}>
            <line
              x1={paddingLeft}
              y1={y}
              x2={width - paddingRight}
              y2={y}
              stroke="#f3f4f6"
              strokeWidth="1"
            />
            <text x={paddingLeft - 4} y={y + 3} textAnchor="end" fontSize="8" fill="#9ca3af">
              {pct}
            </text>
          </g>
        );
      })}

      {/* Bars */}
      {measures.map((m, i) => {
        const rate = Math.min(m.mcidAchievementRate, 100);
        const barHeight = (rate / 100) * plotHeight;
        const x = paddingLeft + i * gap + (gap - barWidth) / 2;
        const y = paddingTop + plotHeight - barHeight;
        const barColor = rate >= 60 ? "#22c55e" : rate >= 40 ? "#f59e0b" : "#ef4444";

        return (
          <g key={m.measureType}>
            {/* Bar */}
            <rect
              x={x}
              y={y}
              width={barWidth}
              height={barHeight}
              fill={barColor}
              rx={3}
              opacity={0.85}
            />
            {/* Percentage label above bar */}
            {barHeight > 0 && (
              <text
                x={x + barWidth / 2}
                y={y - 3}
                textAnchor="middle"
                fontSize="9"
                fill={barColor}
                fontWeight="600"
              >
                {rate.toFixed(0)}%
              </text>
            )}
            {/* Measure label below bar */}
            <text
              x={x + barWidth / 2}
              y={paddingTop + plotHeight + 14}
              textAnchor="middle"
              fontSize="9"
              fill="#6b7280"
            >
              {m.measureType.toUpperCase()}
            </text>
            {/* Patient count */}
            <text
              x={x + barWidth / 2}
              y={paddingTop + plotHeight + 24}
              textAnchor="middle"
              fontSize="7"
              fill="#9ca3af"
            >
              n={m.patientCount}
            </text>
          </g>
        );
      })}

      {/* Axis line */}
      <line
        x1={paddingLeft}
        y1={paddingTop + plotHeight}
        x2={width - paddingRight}
        y2={paddingTop + plotHeight}
        stroke="#e5e7eb"
        strokeWidth="1"
      />
    </svg>
  );
}

// ─── Average Improvement Bar Chart (inline SVG) ───────────────────────────────

interface ImprovementBarChartProps {
  measures: MeasureOutcome[];
}

function ImprovementBarChart({ measures }: ImprovementBarChartProps) {
  if (measures.length === 0) return null;

  const positiveMax = Math.max(...measures.map((m) => Math.max(m.avgImprovement, 0)), 1);
  const negativeMin = Math.min(...measures.map((m) => Math.min(m.avgImprovement, 0)), 0);
  const totalRange = positiveMax - negativeMin;

  const width = 480;
  const height = 130;
  const paddingLeft = 60;
  const paddingRight = 16;
  const paddingTop = 12;
  const paddingBottom = 28;
  const plotWidth = width - paddingLeft - paddingRight;
  const plotHeight = height - paddingTop - paddingBottom;
  const barWidth = Math.min(36, plotWidth / measures.length - 8);
  const gap = plotWidth / measures.length;

  const zeroY = paddingTop + plotHeight * (positiveMax / totalRange);

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="w-full"
      role="img"
      aria-label="Average score improvement by measure"
    >
      {/* Zero line */}
      <line
        x1={paddingLeft}
        y1={zeroY}
        x2={width - paddingRight}
        y2={zeroY}
        stroke="#9ca3af"
        strokeWidth="1"
        strokeDasharray="3,2"
      />

      {measures.map((m, i) => {
        const imp = m.avgImprovement;
        const barHeight = Math.abs((imp / totalRange) * plotHeight);
        const isPositive = imp >= 0;
        const x = paddingLeft + i * gap + (gap - barWidth) / 2;
        const y = isPositive ? zeroY - barHeight : zeroY;
        const barColor = isPositive ? "#6366f1" : "#ef4444";

        return (
          <g key={m.measureType}>
            <rect
              x={x}
              y={y}
              width={barWidth}
              height={barHeight}
              fill={barColor}
              rx={3}
              opacity={0.75}
            />
            <text
              x={x + barWidth / 2}
              y={isPositive ? y - 3 : y + barHeight + 10}
              textAnchor="middle"
              fontSize="8"
              fill={barColor}
              fontWeight="600"
            >
              {imp > 0 ? "+" : ""}
              {imp.toFixed(1)}
            </text>
            <text
              x={x + barWidth / 2}
              y={paddingTop + plotHeight + 14}
              textAnchor="middle"
              fontSize="9"
              fill="#6b7280"
            >
              {m.measureType.toUpperCase()}
            </text>
          </g>
        );
      })}

      <text x={paddingLeft - 4} y={paddingTop + 4} textAnchor="end" fontSize="8" fill="#9ca3af">
        +{positiveMax.toFixed(0)}
      </text>
    </svg>
  );
}

// ─── Payer Mix Donut Chart (inline SVG) ──────────────────────────────────────

const DONUT_COLORS = [
  "#6366f1",
  "#22c55e",
  "#f59e0b",
  "#ef4444",
  "#14b8a6",
  "#8b5cf6",
  "#f97316",
  "#64748b",
];

interface DonutChartProps {
  payers: PayerBreakdown[];
  totalPayments: number;
}

function DonutChart({ payers, totalPayments }: DonutChartProps) {
  if (payers.length === 0 || totalPayments === 0) {
    return (
      <div className="flex h-40 items-center justify-center text-sm text-gray-400">
        No payer data for this period
      </div>
    );
  }

  const cx = 80;
  const cy = 80;
  const outerR = 70;
  const innerR = 42;

  // Build SVG arc segments
  interface Segment {
    path: string;
    color: string;
    payer: PayerBreakdown;
  }

  const segments: Segment[] = [];
  let cumulativeAngle = -Math.PI / 2; // start at top

  for (let i = 0; i < payers.length; i++) {
    const payer = payers[i];
    const fraction = totalPayments > 0 ? payer.totalPayments / totalPayments : 0;
    if (fraction <= 0) continue;

    const sweepAngle = fraction * 2 * Math.PI;
    const startAngle = cumulativeAngle;
    const endAngle = cumulativeAngle + sweepAngle;

    const x1 = cx + outerR * Math.cos(startAngle);
    const y1 = cy + outerR * Math.sin(startAngle);
    const x2 = cx + outerR * Math.cos(endAngle);
    const y2 = cy + outerR * Math.sin(endAngle);
    const x3 = cx + innerR * Math.cos(endAngle);
    const y3 = cy + innerR * Math.sin(endAngle);
    const x4 = cx + innerR * Math.cos(startAngle);
    const y4 = cy + innerR * Math.sin(startAngle);

    const largeArc = sweepAngle > Math.PI ? 1 : 0;

    const path = [
      `M ${x1} ${y1}`,
      `A ${outerR} ${outerR} 0 ${largeArc} 1 ${x2} ${y2}`,
      `L ${x3} ${y3}`,
      `A ${innerR} ${innerR} 0 ${largeArc} 0 ${x4} ${y4}`,
      "Z",
    ].join(" ");

    segments.push({
      path,
      color: DONUT_COLORS[i % DONUT_COLORS.length],
      payer,
    });

    cumulativeAngle = endAngle;
  }

  const fmtCurrency = (v: number) =>
    v >= 1_000_000
      ? `$${(v / 1_000_000).toFixed(1)}M`
      : v >= 1_000
      ? `$${(v / 1_000).toFixed(0)}K`
      : `$${v.toFixed(0)}`;

  return (
    <div className="flex flex-wrap items-start gap-6">
      {/* Donut */}
      <svg viewBox="0 0 160 160" className="w-36 shrink-0" role="img" aria-label="Payer mix donut chart">
        {segments.map((seg, i) => (
          <path key={i} d={seg.path} fill={seg.color} opacity={0.9} />
        ))}
        {/* Center total label */}
        <text x={cx} y={cy - 4} textAnchor="middle" fontSize="9" fill="#6b7280">
          Total
        </text>
        <text x={cx} y={cy + 8} textAnchor="middle" fontSize="11" fill="#111827" fontWeight="600">
          {fmtCurrency(totalPayments)}
        </text>
      </svg>

      {/* Legend */}
      <ul className="flex-1 space-y-2 text-xs">
        {segments.map((seg, i) => (
          <li key={i} className="flex items-center gap-2">
            <span
              className="inline-block h-3 w-3 shrink-0 rounded-sm"
              style={{ backgroundColor: seg.color }}
            />
            <span className="min-w-[80px] font-medium text-gray-700 truncate max-w-[140px]">
              {seg.payer.payerName}
            </span>
            <span className="text-gray-500">{seg.payer.revenuePercentage.toFixed(1)}%</span>
            <span className="ml-auto text-gray-400">{fmtCurrency(seg.payer.totalPayments)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

// ─── A/R Aging horizontal bar ────────────────────────────────────────────────

interface ArAgingBarProps {
  financial: FinancialKPIs;
}

function ArAgingBar({ financial }: ArAgingBarProps) {
  const buckets = [
    { label: "0-30d", value: financial.arAging030, color: "#22c55e" },
    { label: "31-60d", value: financial.arAging3160, color: "#f59e0b" },
    { label: "61-90d", value: financial.arAging6190, color: "#f97316" },
    { label: "91+d", value: financial.arAging91Plus, color: "#ef4444" },
  ];

  const total = buckets.reduce((s, b) => s + b.value, 0);
  if (total === 0) {
    return <p className="text-xs text-gray-400">No outstanding A/R</p>;
  }

  const fmtCurrency = (v: number) =>
    v >= 1_000 ? `$${(v / 1_000).toFixed(0)}K` : `$${v.toFixed(0)}`;

  return (
    <div className="space-y-3">
      {/* Stacked bar */}
      <div className="flex h-4 overflow-hidden rounded">
        {buckets.map((b) => {
          const pct = (b.value / total) * 100;
          if (pct < 0.5) return null;
          return (
            <div
              key={b.label}
              title={`${b.label}: ${fmtCurrency(b.value)} (${pct.toFixed(1)}%)`}
              style={{ width: `${pct}%`, backgroundColor: b.color }}
            />
          );
        })}
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-3 text-xs">
        {buckets.map((b) => {
          const pct = total > 0 ? (b.value / total) * 100 : 0;
          return (
            <span key={b.label} className="flex items-center gap-1">
              <span
                className="inline-block h-2.5 w-2.5 rounded-sm"
                style={{ backgroundColor: b.color }}
              />
              <span className="text-gray-600">{b.label}</span>
              <span className="font-medium text-gray-900">{fmtCurrency(b.value)}</span>
              <span className="text-gray-400">({pct.toFixed(0)}%)</span>
            </span>
          );
        })}
      </div>
    </div>
  );
}

// ─── Main page component ─────────────────────────────────────────────────────

interface Props {
  role: string;
}

export function AnalyticsDashboardPage({ role: _role }: Props) {
  // ── Tab state ──────────────────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<"analytics" | "mips">("analytics");

  // ── Date range state (default: current month) ──────────────────────────
  const now = new Date();
  const defaultStart = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
  const lastDay = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();
  const defaultEnd = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(lastDay).padStart(2, "0")}`;

  const [startDate, setStartDate] = useState(defaultStart);
  const [endDate, setEndDate] = useState(defaultEnd);

  // ── Data state ─────────────────────────────────────────────────────────
  const [operational, setOperational] = useState<OperationalKPIs | null>(null);
  const [financial, setFinancial] = useState<FinancialKPIs | null>(null);
  const [clinical, setClinical] = useState<ClinicalOutcomes | null>(null);
  const [payerMix, setPayerMix] = useState<PayerMix | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // ── Fetch all sections ─────────────────────────────────────────────────
  const fetchAll = useCallback(async () => {
    if (!startDate || !endDate) return;
    setLoading(true);
    setError(null);
    try {
      const [op, fin, clin, pm] = await Promise.all([
        commands.getOperationalKpis(startDate, endDate, null),
        commands.getFinancialKpis(startDate, endDate, null),
        commands.getClinicalOutcomes(startDate, endDate, null, null),
        commands.getPayerMix(startDate, endDate),
      ]);
      setOperational(op);
      setFinancial(fin);
      setClinical(clin);
      setPayerMix(pm);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to load analytics data");
    } finally {
      setLoading(false);
    }
  }, [startDate, endDate]);

  useEffect(() => {
    void fetchAll();
  }, [fetchAll]);

  // ── Formatting helpers ─────────────────────────────────────────────────
  const fmtPct = (v: number) => `${v.toFixed(1)}%`;
  const fmtCurrency = (v: number) =>
    new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", maximumFractionDigits: 0 }).format(v);
  const fmtNumber = (v: number) => v.toLocaleString("en-US");
  const fmtDecimal = (v: number, d = 1) => v.toFixed(d);

  // ── Render ─────────────────────────────────────────────────────────────
  return (
    <div className="flex flex-col h-full bg-gray-50">
      {/* Header with tab bar */}
      <div className="border-b border-gray-200 bg-white">
        <div className="flex items-center justify-between px-6 py-4">
          <div>
            <h1 className="text-lg font-semibold text-gray-900">Analytics & Outcomes Dashboard</h1>
            <p className="text-sm text-gray-500">
              KPIs, financial performance, clinical outcomes, and MIPS quality
            </p>
          </div>

          {/* Date range picker — only shown on Analytics tab */}
          {activeTab === "analytics" && (
            <div className="flex items-center gap-3">
              <label className="text-sm font-medium text-gray-700">From</label>
              <input
                type="date"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
                className={INPUT_CLS}
              />
              <label className="text-sm font-medium text-gray-700">To</label>
              <input
                type="date"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
                className={INPUT_CLS}
              />
              <button
                type="button"
                onClick={() => void fetchAll()}
                disabled={loading}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
              >
                {loading ? "Loading\u2026" : "Refresh"}
              </button>
            </div>
          )}
        </div>

        {/* Tab bar */}
        <div className="flex gap-0 px-6">
          <button
            type="button"
            onClick={() => setActiveTab("analytics")}
            className={[
              "px-4 py-2 text-sm font-medium border-b-2 transition-colors",
              activeTab === "analytics"
                ? "border-blue-600 text-blue-700"
                : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300",
            ].join(" ")}
          >
            Analytics
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("mips")}
            className={[
              "px-4 py-2 text-sm font-medium border-b-2 transition-colors",
              activeTab === "mips"
                ? "border-blue-600 text-blue-700"
                : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300",
            ].join(" ")}
          >
            MIPS Quality
          </button>
        </div>
      </div>

      {/* MIPS Quality tab */}
      {activeTab === "mips" && (
        <div className="flex-1 overflow-y-auto">
          <MIPSDashboardPage role={_role} />
        </div>
      )}

      {/* Analytics tab */}
      {activeTab === "analytics" && <>

      {/* Error banner */}
      {error && (
        <div className="mx-6 mt-4 rounded-md bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700">
          {error}
        </div>
      )}

      {/* Main content */}
      <div className="flex-1 overflow-y-auto px-6 py-6 space-y-8">

        {/* ── Section 1: Operational KPIs ─────────────────────────────── */}
        <section>
          <h2 className="mb-4 text-sm font-semibold uppercase tracking-wide text-gray-500">
            Operational KPIs
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <KpiCard
              label="Visits This Period"
              value={operational ? fmtNumber(operational.totalVisits) : "—"}
              subtext="Completed encounters"
            />
            <KpiCard
              label="Cancellation Rate"
              value={operational ? fmtPct(operational.cancellationRate) : "—"}
              subtext="Of scheduled appointments"
              valueClass={operational ? cancelRateColor(operational.cancellationRate) : "text-gray-900"}
            />
            <KpiCard
              label="Avg Units / Visit"
              value={operational ? fmtDecimal(operational.avgUnitsPerVisit) : "—"}
              subtext="Timed CPT units billed"
            />
            <KpiCard
              label="New Patients"
              value={operational ? fmtNumber(operational.newPatients) : "—"}
              subtext="First encounter in period"
            />
          </div>

          {/* No-show rate secondary metric */}
          {operational && operational.noShowRate > 0 && (
            <p className="mt-2 text-xs text-gray-500">
              No-show rate:{" "}
              <span className={cancelRateColor(operational.noShowRate)}>
                {fmtPct(operational.noShowRate)}
              </span>
            </p>
          )}
        </section>

        {/* ── Section 2: Financial KPIs ───────────────────────────────── */}
        <section>
          <h2 className="mb-4 text-sm font-semibold uppercase tracking-wide text-gray-500">
            Financial KPIs
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <KpiCard
              label="Revenue / Visit"
              value={financial ? fmtCurrency(financial.revenuePerVisit) : "—"}
              subtext="Avg collected per encounter"
            />
            <KpiCard
              label="Net Collection Rate"
              value={financial ? fmtPct(financial.netCollectionRate) : "—"}
              subtext="Payments / (Charges - Adj)"
              valueClass={financial ? collectionRateColor(financial.netCollectionRate) : "text-gray-900"}
            />
            <KpiCard
              label="Days in A/R"
              value={financial ? fmtDecimal(financial.daysInAr, 0) : "—"}
              subtext="Avg submission to payment"
              valueClass={financial ? daysArColor(financial.daysInAr) : "text-gray-900"}
            />
            <KpiCard
              label="Charges / Visit"
              value={financial ? fmtCurrency(financial.chargesPerVisit) : "—"}
              subtext="Avg billed per encounter"
            />
          </div>

          {/* Charges vs Collections summary row */}
          {financial && (
            <div className="mt-4 rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
              <div className="flex flex-wrap gap-6 text-sm">
                <div>
                  <span className="text-gray-500">Total Charges: </span>
                  <span className="font-semibold text-gray-900">
                    {fmtCurrency(financial.totalCharges)}
                  </span>
                </div>
                <div>
                  <span className="text-gray-500">Total Collections: </span>
                  <span className="font-semibold text-green-600">
                    {fmtCurrency(financial.totalPayments)}
                  </span>
                </div>
                <div>
                  <span className="text-gray-500">Total Adjustments: </span>
                  <span className="font-semibold text-amber-600">
                    {fmtCurrency(financial.totalAdjustments)}
                  </span>
                </div>
              </div>

              {/* A/R Aging distribution */}
              <div className="mt-4">
                <p className="mb-2 text-xs font-medium text-gray-500">A/R Aging Distribution</p>
                <ArAgingBar financial={financial} />
              </div>
            </div>
          )}
        </section>

        {/* ── Section 3: Clinical Outcomes ────────────────────────────── */}
        <section>
          <h2 className="mb-4 text-sm font-semibold uppercase tracking-wide text-gray-500">
            Clinical Outcomes
          </h2>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {/* MCID Achievement Rate */}
            <div className={CARD_CLS}>
              <p className="mb-3 text-sm font-medium text-gray-700">
                MCID Achievement Rate by Measure
              </p>
              <McidBarChart measures={clinical?.measureOutcomes ?? []} />
              {clinical && (
                <p className="mt-2 text-xs text-gray-400">
                  Color: green ≥60%, amber 40-59%, red &lt;40%
                </p>
              )}
            </div>

            {/* Average Score Improvement */}
            <div className={CARD_CLS}>
              <p className="mb-3 text-sm font-medium text-gray-700">
                Average Score Improvement by Measure
              </p>
              <ImprovementBarChart measures={clinical?.measureOutcomes ?? []} />
              {clinical && clinical.measureOutcomes.length > 0 && (
                <p className="mt-2 text-xs text-gray-400">
                  Positive = improvement. LEFS/PSFS: higher is better. DASH/NDI/Oswestry/FABQ: lower is better.
                </p>
              )}
            </div>
          </div>

          {/* Discharge rate + patient count summary */}
          {clinical && (
            <div className="mt-4 flex flex-wrap gap-4">
              <div className={`${CARD_CLS} flex-1 min-w-[140px]`}>
                <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">
                  Discharge Rate
                </p>
                <p className="text-2xl font-bold text-blue-600">
                  {fmtPct(clinical.dischargeRate)}
                </p>
                <p className="mt-1 text-xs text-gray-400">
                  Patients completing plan of care
                </p>
              </div>
              <div className={`${CARD_CLS} flex-1 min-w-[140px]`}>
                <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">
                  Patients With Outcomes
                </p>
                <p className="text-2xl font-bold text-gray-900">
                  {fmtNumber(clinical.totalPatientsWithOutcomes)}
                </p>
                <p className="mt-1 text-xs text-gray-400">
                  Patients with scored outcome measures
                </p>
              </div>

              {/* Measure detail table */}
              {clinical.measureOutcomes.filter((m) => m.patientCount > 0).length > 0 && (
                <div className="w-full overflow-x-auto">
                  <table className="w-full text-xs text-left">
                    <thead>
                      <tr className="border-b border-gray-200 bg-gray-50">
                        <th className="px-3 py-2 font-medium text-gray-500">Measure</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">Patients</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">MCID Achieved</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">Rate</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">Avg Initial</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">Avg Final</th>
                        <th className="px-3 py-2 font-medium text-gray-500 text-right">Avg Change</th>
                      </tr>
                    </thead>
                    <tbody>
                      {clinical.measureOutcomes
                        .filter((m) => m.patientCount > 0)
                        .map((m) => (
                          <tr key={m.measureType} className="border-b border-gray-100 hover:bg-gray-50">
                            <td className="px-3 py-2 font-medium text-gray-900">
                              {m.measureType.toUpperCase()}
                            </td>
                            <td className="px-3 py-2 text-right text-gray-600">{m.patientCount}</td>
                            <td className="px-3 py-2 text-right text-gray-600">{m.mcidAchievedCount}</td>
                            <td
                              className={`px-3 py-2 text-right font-medium ${
                                m.mcidAchievementRate >= 60
                                  ? "text-green-600"
                                  : m.mcidAchievementRate >= 40
                                  ? "text-amber-600"
                                  : "text-red-600"
                              }`}
                            >
                              {fmtPct(m.mcidAchievementRate)}
                            </td>
                            <td className="px-3 py-2 text-right text-gray-600">
                              {fmtDecimal(m.avgInitialScore)}
                            </td>
                            <td className="px-3 py-2 text-right text-gray-600">
                              {fmtDecimal(m.avgFinalScore)}
                            </td>
                            <td
                              className={`px-3 py-2 text-right font-medium ${
                                m.avgImprovement >= 0 ? "text-green-600" : "text-red-600"
                              }`}
                            >
                              {m.avgImprovement > 0 ? "+" : ""}
                              {fmtDecimal(m.avgImprovement)}
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
        </section>

        {/* ── Section 4: Payer Mix ────────────────────────────────────── */}
        <section>
          <h2 className="mb-4 text-sm font-semibold uppercase tracking-wide text-gray-500">
            Payer Mix
          </h2>
          <div className={CARD_CLS}>
            <DonutChart
              payers={payerMix?.payers ?? []}
              totalPayments={payerMix?.totalPayments ?? 0}
            />

            {/* Payer detail table */}
            {payerMix && payerMix.payers.length > 0 && (
              <div className="mt-5 overflow-x-auto">
                <table className="w-full text-xs text-left">
                  <thead>
                    <tr className="border-b border-gray-200 bg-gray-50">
                      <th className="px-3 py-2 font-medium text-gray-500">Payer</th>
                      <th className="px-3 py-2 font-medium text-gray-500 text-right">Visits</th>
                      <th className="px-3 py-2 font-medium text-gray-500 text-right">Charges</th>
                      <th className="px-3 py-2 font-medium text-gray-500 text-right">Collections</th>
                      <th className="px-3 py-2 font-medium text-gray-500 text-right">Revenue %</th>
                      <th className="px-3 py-2 font-medium text-gray-500 text-right">Avg / Visit</th>
                    </tr>
                  </thead>
                  <tbody>
                    {payerMix.payers.map((p) => (
                      <tr key={p.payerId} className="border-b border-gray-100 hover:bg-gray-50">
                        <td className="px-3 py-2 font-medium text-gray-900">{p.payerName}</td>
                        <td className="px-3 py-2 text-right text-gray-600">{fmtNumber(p.visitCount)}</td>
                        <td className="px-3 py-2 text-right text-gray-600">{fmtCurrency(p.totalCharges)}</td>
                        <td className="px-3 py-2 text-right text-green-600 font-medium">
                          {fmtCurrency(p.totalPayments)}
                        </td>
                        <td className="px-3 py-2 text-right font-medium text-blue-600">
                          {fmtPct(p.revenuePercentage)}
                        </td>
                        <td className="px-3 py-2 text-right text-gray-600">
                          {fmtCurrency(p.avgReimbursementPerVisit)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr className="border-t-2 border-gray-300 bg-gray-50 font-semibold">
                      <td className="px-3 py-2 text-gray-900">Total</td>
                      <td className="px-3 py-2 text-right text-gray-900">
                        {fmtNumber(payerMix.totalVisits)}
                      </td>
                      <td className="px-3 py-2 text-right text-gray-900">
                        {fmtCurrency(payerMix.totalCharges)}
                      </td>
                      <td className="px-3 py-2 text-right text-green-700">
                        {fmtCurrency(payerMix.totalPayments)}
                      </td>
                      <td className="px-3 py-2 text-right text-gray-900">100%</td>
                      <td className="px-3 py-2 text-right text-gray-600">
                        {payerMix.totalVisits > 0
                          ? fmtCurrency(payerMix.totalPayments / payerMix.totalVisits)
                          : "—"}
                      </td>
                    </tr>
                  </tfoot>
                </table>
              </div>
            )}
          </div>
        </section>
      </div>

      </>}
    </div>
  );
}
