/**
 * claims.ts — TypeScript types for Electronic Claims Submission 837P (M004/S02)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/claims.rs.
 * All field names are camelCase to match Tauri's serde(rename_all = "camelCase").
 */

// ─── Payer Configuration ─────────────────────────────────────────────────────

/** Clearinghouse partner options. */
export type Clearinghouse = "office_ally" | "availity" | "trizetto" | "manual";

/** Input for creating or updating a payer configuration. */
export interface PayerInput {
  name: string;
  /** EDI payer identifier used in ISA/GS segments. */
  ediPayerId?: string | null;
  clearinghouse?: Clearinghouse | null;
  /** Which 8-minute rule method applies to this payer. */
  billingRule: "medicare" | "ama";
  phone?: string | null;
  address?: string | null;
}

/** A payer configuration record as stored in the database. */
export interface PayerRecord {
  payerId: string;
  name: string;
  ediPayerId: string | null;
  clearinghouse: Clearinghouse | null;
  billingRule: "medicare" | "ama";
  phone: string | null;
  address: string | null;
  createdAt: string;
}

// ─── Claim Lifecycle ─────────────────────────────────────────────────────────

/**
 * Claim status lifecycle.
 * draft → validated → submitted → accepted → paid
 *                              ↘ denied → appealed
 */
export type ClaimStatus =
  | "draft"
  | "validated"
  | "submitted"
  | "accepted"
  | "paid"
  | "denied"
  | "appealed";

/** Input for creating a new claim. */
export interface CreateClaimInput {
  encounterBillingId: string;
  payerId: string;
  patientId: string;
}

/** A claim record as stored in the database. */
export interface ClaimRecord {
  claimId: string;
  encounterBillingId: string;
  payerId: string;
  patientId: string;
  status: ClaimStatus;
  /** Full 837P EDI text, populated after generate_837p is called. */
  ediContent: string | null;
  /** Filesystem path to the saved .edi file. */
  ediFilePath: string | null;
  /** ISA13 interchange control number for payer correlation. */
  controlNumber: string | null;
  submittedAt: string | null;
  responseAt: string | null;
  paidAmount: number | null;
  adjustmentAmount: number | null;
  denialReason: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

// ─── Validation ──────────────────────────────────────────────────────────────

/** Result of claim validation — lists any blocking errors. */
export interface ValidationResult {
  valid: boolean;
  /** Human-readable error messages that must be resolved before submission. */
  errors: string[];
}

// ─── 837P Generation ─────────────────────────────────────────────────────────

/** Result of 837P EDI generation. */
export interface EdiGenerationResult {
  claimId: string;
  /** Complete 837P EDI text with ~ segment terminators. */
  ediContent: string;
  /** Path to the saved .edi file. */
  ediFilePath: string;
  /** Number of segments in the ST-SE transaction envelope. */
  segmentCount: number;
  /** ISA13 interchange control number. */
  controlNumber: string;
}

// ─── Claim List / Filters ────────────────────────────────────────────────────

/** Filters for listing claims. All are optional. */
export interface ClaimListFilter {
  patientId?: string | null;
  status?: ClaimStatus | null;
  payerId?: string | null;
}

// ─── Status Update ───────────────────────────────────────────────────────────

/** Input for manually updating a claim's status. */
export interface UpdateClaimStatusInput {
  status: ClaimStatus;
  notes?: string | null;
  paidAmount?: number | null;
  adjustmentAmount?: number | null;
  denialReason?: string | null;
}
