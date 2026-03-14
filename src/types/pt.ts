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
 * Mirrors Rust string values: "draft" -> "signed" -> "locked".
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
  /** Pain NRS stored as string "0"-"10" for flexibility. */
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

// ─────────────────────────────────────────────────────────────────────────────
// M003/S02 — Objective Measures & Outcome Scores
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Supported outcome measure types.
 */
export type MeasureType = "lefs" | "dash" | "ndi" | "oswestry" | "psfs" | "fabq";

/**
 * Episode phase for outcome scoring.
 */
export type EpisodePhase = "initial" | "mid" | "discharge";

/**
 * Input for recording an outcome score.
 * Mirrors Rust struct `OutcomeScoreInput`.
 */
export interface OutcomeScoreInput {
  patientId: string;
  encounterId: string | null;
  measureType: MeasureType;
  items: number[];
  episodePhase: EpisodePhase | null;
}

/**
 * Outcome score record returned from the backend.
 * Mirrors Rust struct `OutcomeScoreRecord`.
 */
export interface OutcomeScoreRecord {
  scoreId: string;
  resourceId: string;
  patientId: string;
  encounterId: string | null;
  measureType: MeasureType;
  score: number;
  scoreSecondary: number | null;
  severity: string | null;
  episodePhase: EpisodePhase | null;
  loincCode: string | null;
  recordedAt: string;
}

/**
 * Input for recording objective measures (ROM, MMT, ortho tests).
 * Mirrors Rust struct `ObjectiveMeasuresInput`.
 */
export interface ObjectiveMeasuresInput {
  patientId: string;
  encounterId: string;
  data: Record<string, unknown>;
}

/**
 * Objective measures record returned from the backend.
 * Mirrors Rust struct `ObjectiveMeasuresRecord`.
 */
export interface ObjectiveMeasuresRecord {
  resourceId: string;
  patientId: string;
  encounterId: string;
  data: Record<string, unknown>;
  recordedAt: string;
}

/**
 * Comparison data for a single measure type (earliest vs latest).
 * Mirrors Rust struct `OutcomeComparisonMeasure`.
 */
export interface OutcomeComparisonMeasure {
  measureType: MeasureType;
  initialScore: number | null;
  initialDate: string | null;
  latestScore: number | null;
  latestDate: string | null;
  change: number | null;
  mcid: number | null;
  mcidMet: boolean | null;
}

/**
 * Full outcome comparison for a patient across all measure types.
 * Mirrors Rust struct `OutcomeComparison`.
 */
export interface OutcomeComparison {
  patientId: string;
  measures: OutcomeComparisonMeasure[];
}
