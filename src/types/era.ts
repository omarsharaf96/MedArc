/**
 * era.ts — TypeScript types for ERA/835 Remittance Processing (M003/S02)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/era_processing.rs.
 * All field names are camelCase to match Tauri's serde(rename_all = "camelCase").
 */

// ─── 835 EDI Parsed Types ─────────────────────────────────────────────────────

/** A single adjustment code + amount pair (from a CAS segment). */
export interface AdjustmentCode {
  /** Group code: CO (contractual), PR (patient responsibility), OA (other). */
  groupCode: string;
  /** CARC reason code (e.g. "4", "97", "1", "2", "3"). */
  reasonCode: string;
  /** Dollar amount adjusted. */
  amount: number;
}

/** Service-level payment detail from an SVC segment. */
export interface ServiceLinePayment {
  /** CPT/HCPCS procedure code. */
  procedureCode: string;
  /** Submitted charge amount. */
  submittedCharge: number;
  /** Paid amount for this service line. */
  paidAmount: number;
  /** Revenue code, if present. */
  revenueCode: string | null;
  /** Adjustments applied to this service line. */
  adjustments: AdjustmentCode[];
}

/** Claim-level payment detail from a CLP loop. */
export interface ClaimPaymentDetail {
  /** CLP01 — matches claims.controlNumber for auto-posting. */
  claimControlNumber: string;
  /**
   * CLP02 — claim status code.
   * 1 = Processed as Primary, 2 = Processed as Secondary,
   * 4 = Denied, 19 = Primary Forwarded.
   */
  claimStatusCode: string;
  /** Total charge billed. */
  totalCharge: number;
  /** Amount paid by payer. */
  paidAmount: number;
  /** Patient responsibility (deductible + coinsurance + copay). */
  patientResponsibility: number;
  /** CLP06 — claim filing indicator (e.g. "MB" = Medicare Part B). */
  claimFilingIndicator: string | null;
  /** Claim-level CAS adjustments. */
  adjustments: AdjustmentCode[];
  /** SVC loop service-level payment details. */
  serviceLines: ServiceLinePayment[];
}

/** Parsed representation of a complete 835 remittance advice. */
export interface RemittanceAdvice {
  /** Payer name from N1*PR. */
  payerName: string | null;
  /** Payer identifier. */
  payerId: string | null;
  /** Payee NPI or tax ID. */
  payeeId: string | null;
  /** BPR02 — total payment amount. */
  paymentAmount: number;
  /** BPR16 — payment effective date (YYYYMMDD). */
  paymentDate: string | null;
  /** TRN02 — check/EFT trace number. */
  traceNumber: string | null;
  /** All CLP loops parsed from the file. */
  claims: ClaimPaymentDetail[];
}

// ─── Database Records ─────────────────────────────────────────────────────────

/** A saved remittance_advice record. */
export interface RemittanceRecord {
  remittanceId: string;
  payerId: string | null;
  traceNumber: string | null;
  paymentAmount: number;
  paymentDate: string | null;
  filePath: string | null;
  /** Whether auto-posting has been performed for this ERA. */
  posted: boolean;
  createdAt: string;
}

/** A claim_payments record. */
export interface ClaimPaymentRecord {
  paymentId: string;
  claimId: string;
  remittanceId: string | null;
  paidAmount: number;
  adjustmentAmount: number;
  patientResponsibility: number;
  /** CSV of adjustment codes, e.g. "CO-45:50.00,PR-2:25.00". */
  adjustmentCodes: string | null;
  postedAt: string;
}

// ─── Auto-Posting ─────────────────────────────────────────────────────────────

/** Result of calling auto_post_remittance. */
export interface AutoPostResult {
  /** Number of claims successfully matched and posted. */
  matchedCount: number;
  /** Number of claim_payments records created. */
  paymentsCreated: number;
  /** CLP control numbers that could not be matched to a claim. */
  unmatchedControlNumbers: string[];
}

// ─── Denial Management ───────────────────────────────────────────────────────

/** A denial queue entry. */
export interface DenialRecord {
  claimId: string;
  patientId: string;
  payerId: string;
  status: string;
  denialReason: string | null;
  /** CSV of adjustment codes from claim_payments. */
  adjustmentCodes: string | null;
  paidAmount: number | null;
  createdAt: string;
  updatedAt: string;
}

// ─── A/R Aging ───────────────────────────────────────────────────────────────

/** One bucket in the A/R aging report. */
export interface AgingBucket {
  /** "0-30" | "31-60" | "61-90" | "91-120" | "120+" */
  label: string;
  /** Total outstanding dollar amount in this bucket. */
  totalAmount: number;
  /** Number of claims in this bucket. */
  claimCount: number;
}

/** Full A/R aging report. */
export interface ArAgingReport {
  buckets: AgingBucket[];
  /** Grand total outstanding across all buckets. */
  totalOutstanding: number;
}

// ─── Patient Balance ─────────────────────────────────────────────────────────

/** Per-claim balance breakdown. */
export interface ClaimBalance {
  claimId: string;
  totalCharge: number;
  totalPaid: number;
  outstanding: number;
  patientResponsibility: number;
}

/** Patient balance summary. */
export interface PatientBalance {
  patientId: string;
  totalBilled: number;
  totalInsurancePaid: number;
  totalPatientResponsibility: number;
  outstandingBalance: number;
  claimDetails: ClaimBalance[];
}

// ─── CARC Code Descriptions ──────────────────────────────────────────────────

/**
 * Common CARC code descriptions for display in the denial queue.
 * Mirrors the carc_description() function in era_processing.rs.
 */
export const CARC_DESCRIPTIONS: Record<string, string> = {
  "1": "Deductible amount",
  "2": "Coinsurance amount",
  "3": "Copay amount",
  "4": "Procedure not covered / inconsistent modifier",
  "5": "Procedure inconsistent with place of service",
  "6": "Procedure inconsistent with patient age",
  "7": "Procedure inconsistent with patient gender",
  "18": "Exact duplicate claim/service",
  "22": "May be covered by another payer (COB)",
  "29": "Timely filing limit expired",
  "45": "Charge exceeds fee schedule / max allowable",
  "50": "Not medically necessary",
  "96": "Non-covered charge",
  "97": "Benefit included in payment for another service",
  "109": "Claim not covered by this payer",
  "119": "Benefit maximum reached",
  "167": "Diagnosis not covered",
  "170": "Not covered for this provider/facility type",
  "197": "Missing prior authorization",
};

export function carcDescription(reasonCode: string): string {
  return CARC_DESCRIPTIONS[reasonCode] ?? `CARC-${reasonCode}`;
}
