/**
 * TypeScript types for authorization & visit tracking.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Auth Record types
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating/updating an authorization record. */
export interface AuthRecordInput {
  /** Patient the authorization belongs to. */
  patientId: string;
  /** Insurance payer name. */
  payerName: string;
  /** Payer phone number. */
  payerPhone: string | null;
  /** Authorization number from the payer. */
  authNumber: string | null;
  /** Total number of visits authorized. */
  authorizedVisits: number;
  /** JSON array of authorized CPT codes (e.g. ["97110","97140"]). */
  authorizedCptCodes: string[] | null;
  /** Authorization start date (ISO 8601 date, e.g. "2026-01-01"). */
  startDate: string;
  /** Authorization end date (ISO 8601 date, e.g. "2026-06-30"). */
  endDate: string;
  /** Free-text notes. */
  notes: string | null;
}

/** Auth record returned from the backend. */
export interface AuthRecord {
  authId: string;
  patientId: string;
  payerName: string;
  payerPhone: string | null;
  authNumber: string | null;
  authorizedVisits: number;
  visitsUsed: number;
  authorizedCptCodes: string[] | null;
  startDate: string;
  endDate: string;
  status: AuthRecordStatus;
  notes: string | null;
  createdAt: string;
  resource: Record<string, unknown>;
}

/** Possible statuses for an auth record. */
export type AuthRecordStatus = "active" | "expired" | "exhausted";

// ─────────────────────────────────────────────────────────────────────────────
// Alert types
// ─────────────────────────────────────────────────────────────────────────────

/** Type of authorization alert. */
export type AuthAlertType =
  | "expired"
  | "exhausted"
  | "expiring_soon"
  | "low_visits";

/** Alert severity level. */
export type AuthAlertSeverity = "error" | "warning";

/** An authorization alert for a patient. */
export interface AuthAlert {
  authId: string;
  alertType: AuthAlertType;
  severity: AuthAlertSeverity;
  message: string;
  payerName: string;
  authNumber: string | null;
}
