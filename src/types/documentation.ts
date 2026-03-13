/**
 * TypeScript types for clinical documentation: encounters, SOAP notes,
 * vitals, review of systems, physical exam, templates, co-sign workflow,
 * and drug-allergy CDS alerts.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 *
 * RosStatus is a string literal union matching the Rust enum's
 * #[serde(rename_all = "snake_case")] serialisation — do NOT use a numeric enum.
 */

// ─────────────────────────────────────────────────────────────────────────────
// ROS Status
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Status for each Review-of-Systems organ system.
 * Mirrors Rust enum RosStatus with #[serde(rename_all = "snake_case")].
 */
export type RosStatus = "positive" | "negative" | "not_reviewed";

// ─────────────────────────────────────────────────────────────────────────────
// Encounter types (CLIN-01)
// ─────────────────────────────────────────────────────────────────────────────

/** Structured SOAP note sections. */
export interface SoapInput {
  /** Subjective — patient-reported symptoms, HPI, chief complaint narrative. */
  subjective: string | null;
  /** Objective — exam findings, vitals summary. */
  objective: string | null;
  /** Assessment — diagnoses, ICD-10 codes, clinical impressions. */
  assessment: string | null;
  /** Plan — treatment orders, prescriptions, referrals, follow-up. */
  plan: string | null;
}

/** Input for creating a new encounter. */
export interface EncounterInput {
  /** Patient the encounter belongs to. */
  patientId: string;
  /** Provider (user ID) who conducted the encounter. */
  providerId: string;
  /** ISO 8601 date-time of the encounter (e.g. "2026-04-01T09:00:00"). */
  encounterDate: string;
  /** Encounter type: "office_visit" | "telehealth" | "urgent_care" | "follow_up" | "preventive" | "procedure" */
  encounterType: string;
  /** Chief complaint (free text). */
  chiefComplaint: string | null;
  /** Template ID to pre-populate the note structure (from list_templates). */
  templateId: string | null;
  /** SOAP note sections. */
  soap: SoapInput | null;
}

/** Encounter record returned to callers. */
export interface EncounterRecord {
  id: string;
  patientId: string;
  providerId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

/** Input for updating an existing encounter / SOAP note. */
export interface UpdateEncounterInput {
  /** New encounter status: "in-progress" | "finished" | "cancelled" */
  status: string | null;
  /** Updated SOAP note sections. */
  soap: SoapInput | null;
  /** Updated chief complaint. */
  chiefComplaint: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Vitals types (CLIN-02)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for recording a vitals observation set. */
export interface VitalsInput {
  /** Patient the vitals belong to. */
  patientId: string;
  /** Encounter this vitals set is attached to. */
  encounterId: string;
  /** ISO 8601 datetime when vitals were recorded. */
  recordedAt: string;
  /** Systolic BP in mmHg. */
  systolicBp: number | null;
  /** Diastolic BP in mmHg. */
  diastolicBp: number | null;
  /** Heart rate in bpm. */
  heartRate: number | null;
  /** Respiratory rate in breaths/min. */
  respiratoryRate: number | null;
  /** Temperature in degrees Celsius. */
  temperatureCelsius: number | null;
  /** SpO2 as a percentage (0–100). */
  spo2Percent: number | null;
  /** Weight in kilograms. */
  weightKg: number | null;
  /** Height in centimeters. */
  heightCm: number | null;
  /** Pain score on 0–10 NRS scale. */
  painScore: number | null;
  /** Additional notes. */
  notes: string | null;
}

/** Vitals record returned to callers. */
export interface VitalsRecord {
  id: string;
  patientId: string;
  encounterId: string;
  /** Calculated BMI (kg/m²), null if weight or height missing. */
  bmi: number | null;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Review of Systems types (CLIN-03)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Review of Systems across 14 standard organ systems.
 * Each system has a RosStatus | null status field and a string | null notes field.
 * Total: patientId + encounterId + 14×status + 14×notes = 30 fields.
 */
export interface ReviewOfSystemsInput {
  /** Patient the ROS belongs to. */
  patientId: string;
  /** Encounter this ROS is attached to. */
  encounterId: string;

  /** 1. Constitutional (fever, chills, fatigue, weight change) */
  constitutional: RosStatus | null;
  constitutionalNotes: string | null;
  /** 2. Eyes (vision changes, diplopia, pain) */
  eyes: RosStatus | null;
  eyesNotes: string | null;
  /** 3. ENT / Head (headache, sinus, hearing, throat) */
  ent: RosStatus | null;
  entNotes: string | null;
  /** 4. Cardiovascular (chest pain, palpitations, edema) */
  cardiovascular: RosStatus | null;
  cardiovascularNotes: string | null;
  /** 5. Respiratory (cough, dyspnea, wheezing) */
  respiratory: RosStatus | null;
  respiratoryNotes: string | null;
  /** 6. Gastrointestinal (nausea, vomiting, diarrhea, pain) */
  gastrointestinal: RosStatus | null;
  gastrointestinalNotes: string | null;
  /** 7. Genitourinary (dysuria, frequency, hematuria) */
  genitourinary: RosStatus | null;
  genitourinaryNotes: string | null;
  /** 8. Musculoskeletal (joint pain, stiffness, swelling) */
  musculoskeletal: RosStatus | null;
  musculoskeletalNotes: string | null;
  /** 9. Integumentary / Skin (rash, lesions, pruritus) */
  integumentary: RosStatus | null;
  integumentaryNotes: string | null;
  /** 10. Neurological (dizziness, syncope, numbness, seizures) */
  neurological: RosStatus | null;
  neurologicalNotes: string | null;
  /** 11. Psychiatric (mood, anxiety, sleep, cognition) */
  psychiatric: RosStatus | null;
  psychiatricNotes: string | null;
  /** 12. Endocrine (heat/cold intolerance, polyuria, polydipsia) */
  endocrine: RosStatus | null;
  endocrineNotes: string | null;
  /** 13. Hematologic / Lymphatic (easy bruising, bleeding, lymphadenopathy) */
  hematologic: RosStatus | null;
  hematologicNotes: string | null;
  /** 14. Allergic / Immunologic (seasonal allergies, drug reactions) */
  allergicImmunologic: RosStatus | null;
  allergicImmunologicNotes: string | null;
}

/** ROS record returned to callers. */
export interface RosRecord {
  id: string;
  patientId: string;
  encounterId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Physical Exam types (CLIN-04)
// ─────────────────────────────────────────────────────────────────────────────

/** Physical exam findings per body system. */
export interface PhysicalExamInput {
  /** Patient the exam belongs to. */
  patientId: string;
  /** Encounter this exam is attached to. */
  encounterId: string;
  /** General appearance (e.g. "Well-appearing, no acute distress"). */
  general: string | null;
  /** HEENT — Head, Eyes, Ears, Nose, Throat. */
  heent: string | null;
  /** Neck — lymphadenopathy, thyroid, JVD. */
  neck: string | null;
  /** Cardiovascular — heart sounds, murmurs, pulses. */
  cardiovascular: string | null;
  /** Pulmonary — breath sounds, work of breathing. */
  pulmonary: string | null;
  /** Abdomen — tenderness, organomegaly, bowel sounds. */
  abdomen: string | null;
  /** Extremities — edema, pulses, cyanosis. */
  extremities: string | null;
  /** Neurological — motor, sensory, reflexes, cranial nerves. */
  neurological: string | null;
  /** Skin — color, turgor, lesions, rash. */
  skin: string | null;
  /** Psychiatric — orientation, affect, mood. */
  psychiatric: string | null;
  /** Musculoskeletal — ROM, strength, tenderness. */
  musculoskeletal: string | null;
  /** Genitourinary — (optional, specialty-specific). */
  genitourinary: string | null;
  /** Rectal — (optional, specialty-specific). */
  rectal: string | null;
  /** Additional free-text exam notes. */
  additionalNotes: string | null;
}

/** Physical exam record returned to callers. */
export interface PhysicalExamRecord {
  id: string;
  patientId: string;
  encounterId: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Template types (CLIN-05)
// ─────────────────────────────────────────────────────────────────────────────

/** A clinical note template record. */
export interface TemplateRecord {
  id: string;
  name: string;
  specialty: string;
  description: string;
  defaultSoap: SoapInput;
  defaultExamSections: string[];
  rosSystems: string[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Co-sign types (CLIN-06)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for a co-sign request (NP/PA requesting supervising physician signature). */
export interface CosignRequestInput {
  /** Encounter to be co-signed. */
  encounterId: string;
  /** Supervising physician's user ID. */
  supervisingProviderId: string;
  /** Optional note to the supervisor. */
  message: string | null;
}

/** Co-sign record returned to callers. */
export interface CosignRecord {
  id: string;
  encounterId: string;
  requestingProviderId: string;
  supervisingProviderId: string;
  status: string;
  requestedAt: string;
  signedAt: string | null;
  resource: Record<string, unknown>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Drug-Allergy CDS types (CLIN-07)
// ─────────────────────────────────────────────────────────────────────────────

/** A passive clinical decision support alert for drug-allergy interaction. */
export interface DrugAllergyAlert {
  /** The medication that triggered the alert. */
  medicationId: string;
  medicationName: string;
  medicationRxnorm: string | null;
  /** The allergy that conflicts with the medication. */
  allergyId: string;
  allergySubstance: string;
  allergySeverity: string | null;
  allergyReaction: string | null;
  /** Alert severity: "warning" | "contraindicated" */
  alertSeverity: string;
  /** Human-readable alert message. */
  message: string;
}
