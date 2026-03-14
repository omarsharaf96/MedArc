/**
 * TypeScript types for the Workers' Compensation module.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 */

// ─────────────────────────────────────────────────────────────────────────────
// WC Case types
// ─────────────────────────────────────────────────────────────────────────────

/** Possible statuses for a workers' comp case. */
export type WcCaseStatus = "open" | "closed" | "settled" | "disputed";

/** Input for creating or updating a workers' comp case. */
export interface WcCaseInput {
  patientId: string;
  employerName: string;
  employerContact: string | null;
  injuryDate: string;
  injuryDescription: string | null;
  /** Array of body part identifiers, e.g. ["lumbar_spine", "left_shoulder"] */
  bodyParts: string[] | null;
  claimNumber: string | null;
  state: string;
  status: WcCaseStatus | null;
  mmiDate: string | null;
}

/** A workers' comp case record returned from the backend. */
export interface WcCaseRecord {
  caseId: string;
  resourceId: string;
  patientId: string;
  employerName: string;
  employerContact: string | null;
  injuryDate: string;
  injuryDescription: string | null;
  bodyParts: string[] | null;
  claimNumber: string | null;
  state: string;
  status: WcCaseStatus;
  mmiDate: string | null;
  createdAt: string;
  updatedAt: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// WC Contact types
// ─────────────────────────────────────────────────────────────────────────────

/** Possible roles for a WC case contact. */
export type WcContactRole =
  | "adjuster"
  | "attorney"
  | "nurse_case_manager"
  | "employer_rep";

/** Input for adding or updating a WC case contact. */
export interface WcContactInput {
  role: WcContactRole;
  name: string;
  company: string | null;
  phone: string | null;
  email: string | null;
  fax: string | null;
  notes: string | null;
}

/** A WC contact record returned from the backend. */
export interface WcContactRecord {
  contactId: string;
  caseId: string;
  role: WcContactRole;
  name: string;
  company: string | null;
  phone: string | null;
  email: string | null;
  fax: string | null;
  notes: string | null;
  createdAt: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// FROI types
// ─────────────────────────────────────────────────────────────────────────────

/** Result of FROI document generation. */
export interface FroiResult {
  caseId: string;
  /** Full structured text of the First Report of Injury. */
  content: string;
  /** Document title for display / fax cover sheet. */
  title: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fee Schedule types
// ─────────────────────────────────────────────────────────────────────────────

/** Result of a WC fee schedule lookup. */
export interface WcFeeResult {
  state: string;
  cptCode: string;
  maxAllowable: number;
  effectiveDate: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Impairment Rating types
// ─────────────────────────────────────────────────────────────────────────────

/** AMA Guides editions supported for impairment ratings. */
export type AmaGuidesEdition = "3rd_rev" | "4th" | "5th" | "6th";

/** Input for recording an impairment rating. */
export interface ImpairmentRatingInput {
  bodyPart: string;
  amaGuidesEdition: AmaGuidesEdition | null;
  impairmentClass: string | null;
  gradeModifier: string | null;
  /** Whole person impairment percentage (0–100). */
  wholePersonPct: number;
  evaluator: string | null;
  evaluationDate: string | null;
}

/** An impairment rating record returned from the backend. */
export interface ImpairmentRatingRecord {
  ratingId: string;
  caseId: string;
  bodyPart: string;
  amaGuidesEdition: AmaGuidesEdition | null;
  impairmentClass: string | null;
  gradeModifier: string | null;
  wholePersonPct: number;
  evaluator: string | null;
  evaluationDate: string | null;
  createdAt: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Communication Log types
// ─────────────────────────────────────────────────────────────────────────────

/** Communication direction. */
export type WcCommDirection = "inbound" | "outbound";

/** Communication method. */
export type WcCommMethod =
  | "phone"
  | "email"
  | "fax"
  | "letter"
  | "in_person";

/** Input for logging a WC communication. */
export interface WcCommunicationInput {
  contactId: string | null;
  direction: WcCommDirection;
  method: WcCommMethod;
  subject: string | null;
  content: string | null;
  commDate: string | null;
}

/** A WC communication log entry returned from the backend. */
export interface WcCommunicationRecord {
  commId: string;
  caseId: string;
  contactId: string | null;
  direction: WcCommDirection;
  method: WcCommMethod;
  subject: string | null;
  content: string | null;
  commDate: string;
  createdAt: string;
}
