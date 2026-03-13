/**
 * TypeScript types for patient demographics, care team, related persons,
 * and clinical data (allergies, problems, medications, immunizations).
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 *
 * Patient and clinical structs are co-located in this file because clinical
 * data (allergies, problems, medications, immunizations) is patient-scoped
 * and accessed together in the patient chart views.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Patient types (PTNT-01 through PTNT-05)
// ─────────────────────────────────────────────────────────────────────────────

/** Insurance plan information for one coverage tier. */
export interface InsuranceInput {
  payerName: string;
  planName: string | null;
  memberId: string;
  groupNumber: string | null;
  subscriberName: string | null;
  subscriberDob: string | null;
  relationshipToSubscriber: string | null;
}

/** Employer information. */
export interface EmployerInput {
  employerName: string;
  occupation: string | null;
  employerPhone: string | null;
  employerAddress: string | null;
}

/** Social Determinants of Health screen answers. */
export interface SdohInput {
  housingStatus: string | null;
  foodSecurity: string | null;
  transportationAccess: string | null;
  educationLevel: string | null;
  notes: string | null;
}

/** Input for creating or updating a patient record. */
export interface PatientInput {
  /** Family (last) name */
  familyName: string;
  /** List of given names (first, middle, …) */
  givenNames: string[];
  /** ISO 8601 date of birth, e.g. "1990-03-15" */
  birthDate: string | null;
  /** FHIR biological sex: "male" | "female" | "other" | "unknown" */
  gender: string | null;
  /** Administrative gender / gender identity (free text, e.g. "non-binary") */
  genderIdentity: string | null;
  /** Phone number (primary) */
  phone: string | null;
  /** Email address */
  email: string | null;
  /** Street address */
  addressLine: string | null;
  /** City */
  city: string | null;
  /** State / province */
  state: string | null;
  /** Postal code */
  postalCode: string | null;
  /** Country (default "US") */
  country: string | null;
  /** URL or base64 data URI for the patient photo */
  photoUrl: string | null;
  /** Medical Record Number (assigned at creation if blank) */
  mrn: string | null;
  /** Provider user-id of the primary care provider */
  primaryProviderId: string | null;
  /** Primary insurance coverage */
  insurancePrimary: InsuranceInput | null;
  /** Secondary insurance coverage */
  insuranceSecondary: InsuranceInput | null;
  /** Tertiary insurance coverage */
  insuranceTertiary: InsuranceInput | null;
  employer: EmployerInput | null;
  sdoh: SdohInput | null;
}

/** Summary record returned by search queries (avoids sending full JSON). */
export interface PatientSummary {
  id: string;
  mrn: string;
  familyName: string;
  givenNames: string[];
  birthDate: string | null;
  gender: string | null;
  phone: string | null;
  primaryProviderId: string | null;
}

/** Full patient record as stored and returned by get_patient. */
export interface PatientRecord {
  id: string;
  mrn: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
  createdAt: string;
}

/** Search query parameters. */
export interface PatientSearchQuery {
  /** Free-text name search (family or given) */
  name: string | null;
  /** Exact MRN */
  mrn: string | null;
  /** ISO date of birth "YYYY-MM-DD" */
  birthDate: string | null;
  /** Maximum results to return (default 50) */
  limit: number | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Care Team types (PTNT-07)
// ─────────────────────────────────────────────────────────────────────────────

/** One care team member assignment. */
export interface CareTeamMemberInput {
  /** patient_id this care team belongs to */
  patientId: string;
  /** User ID of the provider/staff member */
  memberId: string;
  /** Display name of the member (denormalised for FHIR) */
  memberName: string;
  /** Role in the care team, e.g. "primary_care", "nurse", "specialist" */
  role: string;
  /** Optional note */
  note: string | null;
}

export interface CareTeamRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Related Person types (PTNT-06)
// ─────────────────────────────────────────────────────────────────────────────

export interface RelatedPersonInput {
  /** patient_id this person is related to */
  patientId: string;
  familyName: string;
  givenNames: string[];
  /** FHIR relationship code, e.g. "emergency_contact", "next_of_kin", "guarantor" */
  relationship: string;
  phone: string | null;
  email: string | null;
  addressLine: string | null;
  city: string | null;
  state: string | null;
  postalCode: string | null;
}

export interface RelatedPersonRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Allergy types (PTNT-08)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating an AllergyIntolerance. */
export interface AllergyInput {
  /** The patient this allergy belongs to. */
  patientId: string;
  /** "drug" | "food" | "environment" | "biologic" */
  category: string;
  /** Free-text or coded allergen name (e.g. "Penicillin", "Peanuts") */
  substance: string;
  /** SNOMED or RxNorm code for the substance (optional) */
  substanceCode: string | null;
  /** Code system for substanceCode (e.g. "http://www.nlm.nih.gov/research/umls/rxnorm") */
  substanceSystem: string | null;
  /** "active" | "inactive" | "resolved" */
  clinicalStatus: string | null;
  /** "allergy" | "intolerance" */
  allergyType: string | null;
  /** "mild" | "moderate" | "severe" | "life-threatening" */
  severity: string | null;
  /** Free-text description of the reaction (e.g. "hives", "anaphylaxis") */
  reaction: string | null;
  /** ISO 8601 date of onset (e.g. "2024-01-15") */
  onsetDate: string | null;
  /** Additional notes */
  notes: string | null;
}

/** Stored allergy record returned to callers. */
export interface AllergyRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem types (PTNT-09)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating a Condition (problem list entry). */
export interface ProblemInput {
  /** The patient this problem belongs to. */
  patientId: string;
  /** ICD-10 code (e.g. "J06.9", "I10") */
  icd10Code: string;
  /** Human-readable display for the ICD-10 code */
  display: string;
  /** "active" | "inactive" | "resolved" */
  clinicalStatus: string | null;
  /** ISO 8601 date of onset (e.g. "2024-03-01") */
  onsetDate: string | null;
  /** ISO 8601 date resolved/abated (if applicable) */
  abatementDate: string | null;
  /** Additional notes */
  notes: string | null;
}

/** Stored problem record returned to callers. */
export interface ProblemRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Medication types (PTNT-10)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating a MedicationStatement. */
export interface MedicationInput {
  /** The patient this medication belongs to. */
  patientId: string;
  /** RxNorm code (e.g. "1049502") */
  rxnormCode: string | null;
  /** Drug name for display (e.g. "Amoxicillin 500 MG Oral Capsule") */
  display: string;
  /** "active" | "completed" | "entered-in-error" | "intended" | "stopped" | "on-hold" | "unknown" | "not-taken" */
  status: string | null;
  /** Dosage instructions (e.g. "500mg TID x 10 days") */
  dosage: string | null;
  /** ISO 8601 effective start date */
  effectiveStart: string | null;
  /** ISO 8601 effective end date (if stopped/completed) */
  effectiveEnd: string | null;
  /** Prescribing provider ID */
  prescriberId: string | null;
  /** Reason for medication (ICD-10 code or free text) */
  reason: string | null;
  /** Additional notes */
  notes: string | null;
}

/** Stored medication record returned to callers. */
export interface MedicationRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Immunization types (PTNT-11)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating or updating an Immunization record. */
export interface ImmunizationInput {
  /** The patient this immunization belongs to. */
  patientId: string;
  /** CVX code (e.g. "158" for influenza, "208" for COVID-19 Pfizer) */
  cvxCode: string;
  /** Vaccine name for display (e.g. "Influenza, seasonal, injectable") */
  display: string;
  /** ISO 8601 date of administration (e.g. "2024-10-15") */
  occurrenceDate: string;
  /** Lot number of the vaccine vial */
  lotNumber: string | null;
  /** Expiration date of the lot (ISO 8601) */
  expirationDate: string | null;
  /** Administering site (e.g. "left arm", "right deltoid") */
  site: string | null;
  /** Route of administration (e.g. "intramuscular", "subcutaneous") */
  route: string | null;
  /** Dose number in series (e.g. 1, 2, 3) */
  doseNumber: number | null;
  /** "completed" | "entered-in-error" | "not-done" */
  status: string | null;
  /** Additional notes */
  notes: string | null;
}

/** Stored immunization record returned to callers. */
export interface ImmunizationRecord {
  id: string;
  patientId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}
