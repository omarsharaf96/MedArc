/**
 * billing.ts — TypeScript types for the CPT Billing Engine (M004/S01)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/billing.rs.
 * All field names are camelCase to match Tauri's serde(rename_all = "camelCase").
 */

// ─── CPT Code Library ────────────────────────────────────────────────────────

/** Category of a CPT code for Physical Therapy billing. */
export type CptCategory = "evaluation" | "timed" | "untimed";

/** A single CPT code entry from the PT code library. */
export interface CptCode {
  code: string;
  description: string;
  isTimed: boolean;
  defaultMinutes: number;
  category: CptCategory;
}

// ─── Billing Rule ────────────────────────────────────────────────────────────

/**
 * Which 8-minute rule calculation method to apply.
 * - `medicare`: Pools all timed minutes across services (CMS requirement)
 * - `ama`:      Calculates each service independently (commercial payers)
 */
export type BillingRule = "medicare" | "ama";

// ─── Unit Calculation ────────────────────────────────────────────────────────

/** A single timed service submitted for unit calculation. */
export interface ServiceMinutes {
  cptCode: string;
  minutes: number;
}

/** Result of the 8-minute rule calculation for one service. */
export interface UnitCalculationResult {
  cptCode: string;
  minutes: number;
  units: number;
}

// ─── Fee Schedule ────────────────────────────────────────────────────────────

/** Input for creating a fee schedule entry. */
export interface FeeScheduleInput {
  /** null means self-pay / default fee schedule */
  payerId: string | null;
  cptCode: string;
  description?: string | null;
  allowedAmount: number;
  isTimed: boolean;
  effectiveDate: string; // ISO date: "YYYY-MM-DD"
}

/** A fee schedule entry as stored in the database. */
export interface FeeScheduleEntry {
  feeId: string;
  payerId: string | null;
  cptCode: string;
  description: string | null;
  allowedAmount: number;
  isTimed: boolean;
  effectiveDate: string;
  createdAt: string;
}

// ─── Encounter Billing ───────────────────────────────────────────────────────

/** A billing line item for submission / retrieval. */
export interface BillingLineItemInput {
  cptCode: string;
  /** Comma-separated modifier codes, e.g. "KX" or "CQ,KX" */
  modifiers?: string | null;
  /** Timed minutes for this code (0 for untimed codes) */
  minutes: number;
  /** Pre-calculated units (use calculateBillingUnits to derive) */
  units: number;
  /** Charge amount in dollars */
  charge: number;
  /** ICD-10 diagnosis pointer(s), e.g. "A,B" */
  dxPointers?: string | null;
}

/** A billing line item as returned from the database. */
export interface BillingLineItem {
  lineId: string;
  billingId: string;
  cptCode: string;
  modifiers: string | null;
  minutes: number;
  units: number;
  charge: number;
  dxPointers: string | null;
  createdAt: string;
}

/** Input for saving encounter billing. */
export interface SaveEncounterBillingInput {
  encounterId: string;
  patientId: string;
  payerId?: string | null;
  billingRule: BillingRule;
  services: BillingLineItemInput[];
}

/** Status of an encounter billing record. */
export type BillingStatus = "draft" | "ready" | "submitted" | "paid";

/** Complete encounter billing record with line items. */
export interface EncounterBilling {
  billingId: string;
  encounterId: string;
  patientId: string;
  payerId: string | null;
  billingRule: string;
  totalCharge: number;
  totalUnits: number;
  totalMinutes: number;
  status: BillingStatus;
  lineItems: BillingLineItem[];
  createdAt: string;
  updatedAt: string;
}
