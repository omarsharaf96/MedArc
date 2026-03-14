/**
 * TypeScript types for intake surveys: templates, fields, responses.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Survey field types
// ─────────────────────────────────────────────────────────────────────────────

/** The supported input types for a survey field. */
export type SurveyFieldType = "Text" | "Number" | "YesNo" | "PainScale" | "Date";

/** A single field definition within a survey template. */
export interface SurveyField {
  /** Unique identifier for this field (UUID). */
  id: string;
  /** Display label shown to the patient. */
  label: string;
  /** Input type controlling how this field renders. */
  fieldType: SurveyFieldType;
  /** Whether the patient must answer this field before submitting. */
  required: boolean;
  /** Display order (0-based). */
  order: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Survey template
// ─────────────────────────────────────────────────────────────────────────────

/** A survey template containing an ordered list of fields. */
export interface SurveyTemplate {
  /** Unique identifier (UUID). */
  id: string;
  /** Human-readable template name (e.g. "Pain and Function Intake"). */
  name: string;
  /** Ordered list of field definitions. */
  fields: SurveyField[];
  /** Whether this is a built-in (non-deletable) template. */
  builtIn: boolean;
  /** ISO 8601 datetime when the template was created. */
  createdAt: string;
  /** User ID of the template creator. */
  createdBy: string;
}

/** Input for creating a new survey template. */
export interface SurveyTemplateInput {
  /** Template name. */
  name: string;
  /** Ordered list of field definitions. */
  fields: SurveyField[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Survey response
// ─────────────────────────────────────────────────────────────────────────────

/** A single field answer within a survey response. */
export interface SurveyFieldResponse {
  /** The field ID this answer corresponds to. */
  fieldId: string;
  /** The patient's answer as a string (all types serialised to string). */
  value: string;
}

/** Input for submitting a completed survey. */
export interface SurveyResponseInput {
  /** The template this response is for. */
  templateId: string;
  /** The patient who filled out the survey. */
  patientId: string;
  /** Ordered list of field answers. */
  responses: SurveyFieldResponse[];
}

/** A saved survey response record. */
export interface SurveyResponse {
  /** Unique identifier (UUID). */
  id: string;
  /** The template this response was filled from. */
  templateId: string;
  /** The patient who completed the survey. */
  patientId: string;
  /** Ordered list of field answers. */
  responses: SurveyFieldResponse[];
  /** ISO 8601 datetime when the survey was submitted. */
  submittedAt: string;
}
