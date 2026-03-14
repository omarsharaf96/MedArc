/**
 * TypeScript types for referral tracking.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 *
 * These types will be wired to Tauri commands (createReferral, getReferral,
 * listReferrals, updateReferral) once the backend is implemented.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Referral types
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating a referral record. */
export interface ReferralInput {
  /** The patient this referral belongs to. */
  patientId: string;
  /** Name of the referring provider (required). */
  referringProviderName: string;
  /** NPI of the referring provider. */
  referringProviderNpi: string | null;
  /** Practice name of the referring provider. */
  practiceName: string | null;
  /** Phone number for the referring practice. */
  phone: string | null;
  /** Fax number for the referring practice. */
  fax: string | null;
  /** ISO 8601 date of referral (e.g. "2026-03-14"). */
  referralDate: string | null;
  /** Number of authorized visits. */
  authorizedVisits: number | null;
  /** ICD-10 diagnosis code associated with the referral. */
  icd10Diagnosis: string | null;
  /** Linked document ID (optional). */
  linkedDocumentId: string | null;
  /** Additional notes. */
  notes: string | null;
}

/** Stored referral record returned to callers. */
export interface ReferralRecord {
  id: string;
  patientId: string;
  referringProviderName: string;
  referringProviderNpi: string | null;
  practiceName: string | null;
  phone: string | null;
  fax: string | null;
  referralDate: string | null;
  authorizedVisits: number | null;
  icd10Diagnosis: string | null;
  linkedDocumentId: string | null;
  notes: string | null;
  createdAt: string;
  lastUpdated: string;
}
