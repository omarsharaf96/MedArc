/**
 * therapy-cap.ts — TypeScript types for Therapy Cap & KX Modifier Monitoring (M004/S02)
 *
 * Mirrors the Rust structs in src-tauri/src/commands/therapy_cap.rs.
 * All field names are camelCase to match serde(rename_all = "camelCase").
 *
 * Thresholds (2026):
 *   PT + SLP combined cap:       $2,480  → KX modifier required
 *   Targeted Medical Review:     $3,000  → separate alert
 *   Approaching alert (amber):   $2,280  (within $200 of cap)
 */

// ─── Therapy Cap Status ───────────────────────────────────────────────────────

/** Payer type for therapy cap tracking. */
export type CapPayerType = "medicare" | "medicaid" | "commercial";

/** Full therapy cap tracking record for a patient in a calendar year. */
export interface TherapyCapStatus {
  trackingId: string;
  patientId: string;
  calendarYear: number;
  payerType: CapPayerType;
  cumulativeCharges: number;
  thresholdAmount: number;
  remaining: number;
  kxRequired: boolean;
  kxAppliedDate: string | null;
  reviewThresholdReached: boolean;
  createdAt: string;
  updatedAt: string;
}

/** Result of check_therapy_cap — lightweight computed status. */
export interface TherapyCapCheck {
  patientId: string;
  calendarYear: number;
  cumulativeCharges: number;
  thresholdAmount: number;
  remaining: number;
  kxRequired: boolean;
  reviewThresholdReached: boolean;
}

// ─── Therapy Cap Alerts ───────────────────────────────────────────────────────

/**
 * Alert type for therapy cap status.
 *
 * - `approaching_therapy_cap`: within $200 of cap (amber)
 * - `kx_modifier_required`: at or above $2,480 (red)
 * - `targeted_medical_review`: at or above $3,000 (red)
 */
export type TherapyCapAlertType =
  | "approaching_therapy_cap"
  | "kx_modifier_required"
  | "targeted_medical_review";

/** Severity level for therapy cap alerts. */
export type TherapyCapAlertSeverity = "warning" | "error";

/** A therapy cap alert for a patient. */
export interface TherapyCapAlert {
  patientId: string;
  alertType: TherapyCapAlertType;
  severity: TherapyCapAlertSeverity;
  message: string;
  cumulativeCharges: number;
  thresholdAmount: number;
}

// ─── ABN (Advance Beneficiary Notice) ────────────────────────────────────────

/**
 * Reason for issuing an ABN (CMS-R-131).
 *
 * - `therapy_cap_approaching`: Medicare therapy cap is approaching or reached
 * - `auth_expired`: Insurance authorization has expired
 * - `non_covered_service`: Service is not covered by the payer
 * - `frequency_limit`: Service frequency limit reached
 */
export type AbnReason =
  | "therapy_cap_approaching"
  | "auth_expired"
  | "non_covered_service"
  | "frequency_limit";

/**
 * Patient's choice on the ABN form.
 *
 * - `option1_pay`: Patient wants the service and agrees to pay if Medicare denies
 * - `option2_dont_pay`: Patient wants the service but will not pay if denied
 * - `option3_dont_provide`: Patient does not want the service
 */
export type AbnPatientChoice =
  | "option1_pay"
  | "option2_dont_pay"
  | "option3_dont_provide";

/** Input for creating a new ABN record. */
export interface AbnInput {
  patientId: string;
  reason: AbnReason;
  /** CPT codes for the services potentially not covered. */
  services: string[];
  createdBy: string;
}

/** Input for recording a patient's ABN choice and signature. */
export interface AbnChoiceInput {
  abnId: string;
  patientChoice: AbnPatientChoice;
  /** ISO 8601 date the patient signed (e.g. "2026-03-15"). */
  signedDate: string;
}

/** An ABN record as returned from the backend. */
export interface AbnRecord {
  abnId: string;
  patientId: string;
  reason: AbnReason;
  services: string[];
  patientChoice: AbnPatientChoice | null;
  signedDate: string | null;
  createdBy: string;
  createdAt: string;
}

// ─── PTA CQ Modifier ─────────────────────────────────────────────────────────

/** Result of checking whether an encounter's provider is a PTA. */
export interface PtaModifierCheck {
  encounterId: string;
  providerId: string;
  /** True if the treating provider is a Physical Therapist Assistant. */
  isPta: boolean;
  /** True when CQ modifier must be added to all service lines. */
  cqModifierRequired: boolean;
  message: string;
}
