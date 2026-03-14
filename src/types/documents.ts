/**
 * TypeScript types for the Document Center upgrade (M003/S04).
 *
 * Covers three feature areas:
 *   1. Document Categories — PT-specific categorized document management
 *   2. Intake Survey Builder — survey templates and responses
 *   3. Referral Tracking — referring provider records
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Document Category types
// ─────────────────────────────────────────────────────────────────────────────

/** PT-specific document categories. */
export type DocumentCategory =
  | "referral_rx"
  | "imaging"
  | "consent_forms"
  | "intake_surveys"
  | "insurance"
  | "legal"
  | "home_exercise_program"
  | "other";

/** Input for uploading a categorized patient document. */
export interface CategorizedDocumentInput {
  /** Patient the document belongs to. */
  patientId: string;
  /** PT-specific category. */
  category: DocumentCategory;
  /** File name (e.g. "referral_dr_smith.pdf"). */
  fileName: string;
  /** Base64-encoded file content. */
  fileDataBase64: string;
  /** MIME type (e.g. "application/pdf", "image/jpeg"). */
  mimeType: string;
}

/** Categorized document record returned from the backend. */
export interface CategorizedDocument {
  documentId: string;
  patientId: string;
  category: DocumentCategory;
  fileName: string;
  mimeType: string;
  fileSize: number;
  sha1Hash: string;
  uploadedAt: string;
  resource: Record<string, unknown>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Survey types
// ─────────────────────────────────────────────────────────────────────────────

/** Supported field types in intake surveys. */
export type SurveyFieldType = "text" | "number" | "yes_no" | "pain_scale" | "date";

/** A single field in a survey template. */
export interface SurveyField {
  fieldId: string;
  fieldType: SurveyFieldType;
  label: string;
  required: boolean;
  order: number;
}

/** Input for creating a custom survey template. */
export interface SurveyTemplateInput {
  name: string;
  fields: SurveyField[];
}

/** Survey template record returned from the backend. */
export interface SurveyTemplate {
  templateId: string;
  name: string;
  isBuiltin: boolean;
  fieldCount: number;
  fields: SurveyField[];
  createdAt: string;
  updatedAt: string;
  resource: Record<string, unknown>;
}

/** Input for submitting a survey response. */
export interface SurveyResponseInput {
  templateId: string;
  patientId: string;
  responses: Record<string, unknown>;
}

/** Survey response record returned from the backend. */
export interface SurveyResponse {
  responseId: string;
  templateId: string;
  patientId: string;
  responses: Record<string, unknown>;
  completedAt: string;
  resource: Record<string, unknown>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Referral types
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating a referral record. */
export interface ReferralInput {
  patientId: string;
  referringProviderName: string;
  referringProviderNpi: string | null;
  practiceName: string | null;
  phone: string | null;
  fax: string | null;
  referralDate: string | null;
  authorizedVisitCount: number | null;
  diagnosisIcd10: string | null;
  linkedDocumentId: string | null;
  notes: string | null;
}

/** Referral record returned from the backend. */
export interface ReferralRecord {
  referralId: string;
  patientId: string;
  referringProviderName: string;
  referringProviderNpi: string | null;
  practiceName: string | null;
  phone: string | null;
  fax: string | null;
  referralDate: string | null;
  authorizedVisitCount: number | null;
  diagnosisIcd10: string | null;
  linkedDocumentId: string | null;
  notes: string | null;
  createdAt: string;
  resource: Record<string, unknown>;
}
