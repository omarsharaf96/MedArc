/**
 * analytics.ts — TypeScript types for the Analytics & Outcomes Dashboard (M003/S02)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/analytics.rs.
 * All field names are camelCase to match Tauri's serde(rename_all = "camelCase").
 */

// ─── Operational KPIs ────────────────────────────────────────────────────────

/** Operational KPIs for a date range. */
export interface OperationalKPIs {
  totalVisits: number;
  cancellationRate: number;
  noShowRate: number;
  avgUnitsPerVisit: number;
  newPatients: number;
  periodStart: string;
  periodEnd: string;
  providerId: string | null;
}

// ─── Financial KPIs ──────────────────────────────────────────────────────────

/** Financial KPIs for a date range. */
export interface FinancialKPIs {
  totalCharges: number;
  totalPayments: number;
  totalAdjustments: number;
  revenuePerVisit: number;
  netCollectionRate: number;
  daysInAr: number;
  chargesPerVisit: number;
  arAging030: number;
  arAging3160: number;
  arAging6190: number;
  arAging91Plus: number;
  periodStart: string;
  periodEnd: string;
  payerId: string | null;
}

// ─── Clinical Outcomes ───────────────────────────────────────────────────────

/** MCID achievement and average improvement for one measure type. */
export interface MeasureOutcome {
  measureType: string;
  patientCount: number;
  mcidAchievedCount: number;
  mcidAchievementRate: number;
  avgInitialScore: number;
  avgFinalScore: number;
  avgImprovement: number;
}

/** Outcomes grouped by provider. */
export interface ProviderOutcome {
  providerId: string;
  patientCount: number;
  avgImprovement: number;
  dischargeCount: number;
}

/** Aggregated clinical outcomes for a date range. */
export interface ClinicalOutcomes {
  measureOutcomes: MeasureOutcome[];
  providerOutcomes: ProviderOutcome[];
  dischargeRate: number;
  totalPatientsWithOutcomes: number;
  periodStart: string;
  periodEnd: string;
  measureTypeFilter: string | null;
  providerIdFilter: string | null;
}

// ─── Payer Mix ────────────────────────────────────────────────────────────────

/** Revenue and visit breakdown for one payer. */
export interface PayerBreakdown {
  payerId: string;
  payerName: string;
  visitCount: number;
  totalCharges: number;
  totalPayments: number;
  revenuePercentage: number;
  avgReimbursementPerVisit: number;
}

/** Payer mix for a date range. */
export interface PayerMix {
  payers: PayerBreakdown[];
  totalVisits: number;
  totalCharges: number;
  totalPayments: number;
  periodStart: string;
  periodEnd: string;
}

// ─── KPI Snapshot ─────────────────────────────────────────────────────────────

/** Valid snapshot period cadences. */
export type PeriodType = "daily" | "weekly" | "monthly" | "quarterly" | "yearly";

/** A persisted KPI snapshot for historical trend retrieval. */
export interface KpiSnapshot {
  snapshotId: string;
  periodType: PeriodType;
  periodStart: string;
  periodEnd: string;
  providerId: string | null;
  kpiData: Record<string, unknown>;
  computedAt: string;
}

// ─── Dashboard Summary ────────────────────────────────────────────────────────

/** All KPI sections combined (returned by get_dashboard_summary). */
export interface DashboardSummary {
  operational: OperationalKPIs;
  financial: FinancialKPIs;
  clinical: ClinicalOutcomes;
  payerMix: PayerMix;
  periodStart: string;
  periodEnd: string;
}
