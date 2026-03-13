/**
 * TypeScript types for Physical Therapy note data model.
 *
 * Conventions:
 *   - Field names are camelCase, matching the Rust structs'
 *     `#[serde(rename_all = "camelCase")]` serialisation.
 *   - `Option<T>` in Rust maps to `T | null` here — never `T | undefined`.
 *   - `serde_json::Value` maps to `Record<string, unknown>`.
 *   - Enum types are string literal unions matching Rust's
 *     `#[serde(rename_all = "snake_case")]` — do NOT use numeric enums.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Discriminant types
// ─────────────────────────────────────────────────────────────────────────────

/**
 * PT note type discriminant.
 * Mirrors Rust enum `PtNoteType` with `#[serde(rename_all = "snake_case")]`.
 */
export type PtNoteType =
  | "initial_eval"
  | "progress_note"
  | "discharge_summary";

/**
 * PT note lifecycle status.
 * Mirrors Rust string values: "draft" → "signed" → "locked".
 */
export type PtNoteStatus = "draft" | "signed" | "locked";

// ─────────────────────────────────────────────────────────────────────────────
// Field shapes per note type
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Fields for a PT Initial Evaluation note.
 * Mirrors Rust struct `InitialEvalFields`.
 */
export interface InitialEvalFields {
  chiefComplaint: string | null;
  mechanismOfInjury: string | null;
  priorLevelOfFunction: string | null;
  /** Pain NRS stored as string "0"–"10" for flexibility. */
  painNrs: string | null;
  functionalLimitations: string | null;
  icd10Codes: string | null;
  physicalExamFindings: string | null;
  shortTermGoals: string | null;
  longTermGoals: string | null;
  planOfCare: string | null;
  frequencyDuration: string | null;
  cptCodes: string | null;
  referringPhysician: string | null;
  referralDocumentId: string | null;
}

/**
 * Fields for a PT Daily Progress Note.
 * Mirrors Rust struct `ProgressNoteFields`.
 */
export interface ProgressNoteFields {
  subjective: string | null;
  patientReportPainNrs: string | null;
  /** Home exercise programme compliance: "yes" | "no" | "partial". */
  hepCompliance: string | null;
  barriers: string | null;
  treatments: string | null;
  exercises: string | null;
  assessment: string | null;
  progressTowardGoals: string | null;
  plan: string | null;
  hepUpdates: string | null;
  totalTreatmentMinutes: string | null;
}

/**
 * Fields for a PT Discharge Summary note.
 * Mirrors Rust struct `DischargeSummaryFields`.
 * `outcomeComparisonPlaceholder` is reserved for S02 outcome-measure integration.
 */
export interface DischargeSummaryFields {
  totalVisitsAttended: string | null;
  totalVisitsAuthorized: string | null;
  treatmentSummary: string | null;
  goalAchievement: string | null;
  /** Placeholder for outcome-measure comparison data wired in S02. */
  outcomeComparisonPlaceholder: string | null;
  dischargeRecommendations: string | null;
  hepNarrative: string | null;
  returnToCare: string | null;
}

/**
 * Discriminated union of all PT note field shapes.
 * Mirrors Rust enum `PtNoteFields` with `#[serde(tag = "noteType", content = "fields")]`.
 */
export type PtNoteFields =
  | InitialEvalFields
  | ProgressNoteFields
  | DischargeSummaryFields;

// ─────────────────────────────────────────────────────────────────────────────
// Input / Record types
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Input for creating or updating a PT note.
 * Mirrors Rust struct `PtNoteInput`.
 */
export interface PtNoteInput {
  patientId: string;
  encounterId: string | null;
  noteType: PtNoteType;
  fields: PtNoteFields | null;
  addendumOf: string | null;
}

/**
 * PT note record returned from the backend.
 * Mirrors Rust struct `PtNoteRecord`.
 */
export interface PtNoteRecord {
  id: string;
  patientId: string;
  encounterId: string | null;
  noteType: PtNoteType;
  status: PtNoteStatus;
  providerId: string;
  /** Serialised FHIR-like resource blob stored in the database. */
  resource: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
  addendumOf: string | null;
}
