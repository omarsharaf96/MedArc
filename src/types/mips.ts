/**
 * mips.ts — TypeScript types for MIPS Quality Measure Capture (M004/S07)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/mips_reporting.rs.
 * All field names are camelCase to match Tauri's serde(rename_all = "camelCase").
 */

// ─── Measure IDs ─────────────────────────────────────────────────────────────

export type MipsMeasureId =
  | "182"
  | "217"
  | "220"
  | "221"
  | "134"
  | "155"
  | "128";

// ─── Performance tier ─────────────────────────────────────────────────────────

/** Color-coded tier based on performance rate. */
export type PerformanceTier = "Green" | "Amber" | "Red" | "NoData";

// ─── Per-measure performance ──────────────────────────────────────────────────

/** Per-measure MIPS performance data (numerator/denominator/rate). */
export interface MipsPerformance {
  measureId: string;
  measureName: string;
  numerator: number;
  denominator: number;
  /** null when denominator is 0. */
  performanceRate: number | null;
  performanceYear: number;
}

// ─── Eligible patients ────────────────────────────────────────────────────────

/** A patient in the denominator of a specific MIPS measure. */
export interface EligiblePatient {
  patientId: string;
  displayName: string;
  /** true if the patient also satisfies the numerator criteria. */
  inNumerator: boolean;
}

// ─── Screening records ────────────────────────────────────────────────────────

/** Valid MIPS screening measure types. */
export type MipsScreeningType = "phq2" | "phq9" | "falls_risk" | "bmi";

/** A recorded screening entry (PHQ-2, PHQ-9, falls risk, or BMI). */
export interface MipsScreening {
  screeningId: string;
  patientId: string;
  encounterId: string | null;
  measureType: MipsScreeningType;
  /** Numeric score (PHQ-2: 0-6, PHQ-9: 0-27, BMI: real). */
  score: number | null;
  /** "positive" | "negative" for screening results. */
  result: string | null;
  followUpPlan: string | null;
  performanceYear: number;
  screenedAt: string;
}

// ─── Dashboard ────────────────────────────────────────────────────────────────

/** A single measure card shown on the MIPS dashboard. */
export interface MipsMeasureCard {
  measureId: string;
  measureName: string;
  numerator: number;
  denominator: number;
  performanceRate: number | null;
  tier: PerformanceTier;
  performanceYear: number;
}

/** Full MIPS dashboard payload. */
export interface MipsDashboard {
  performanceYear: number;
  measures: MipsMeasureCard[];
  /** Average performance rate across all measures with data. null when no data. */
  projectedCompositeScore: number | null;
  computedAt: string;
}

// ─── User management ─────────────────────────────────────────────────────────

/** User list entry for the admin users panel. */
export interface UserListEntry {
  id: string;
  username: string;
  displayName: string;
  role: string;
  isActive: boolean;
  createdAt: string;
}
