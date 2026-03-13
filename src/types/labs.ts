/**
 * TypeScript types for lab results, lab orders, lab catalogue, and
 * patient document management.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 * serde_json::Value maps to Record<string, unknown>.
 *
 * Key mappings from Rust:
 *   - i64 (file_size_bytes)  → number
 *   - bool (has_abnormal)    → boolean
 *   - f64 (value_quantity)   → number
 */

// ─────────────────────────────────────────────────────────────────────────────
// Lab Catalogue types (LABS-02)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for adding a procedure to the lab catalogue. */
export interface LabCatalogueInput {
  /** LOINC code (e.g. "2345-7" for glucose). */
  loincCode: string;
  /** Human-readable display name (e.g. "Glucose [Mass/volume] in Serum or Plasma"). */
  displayName: string;
  /** Category: "laboratory" | "radiology" | "pathology" | "microbiology" */
  category: string | null;
  /** Specimen type (e.g. "venous blood", "urine", "swab"). */
  specimenType: string | null;
  /** Unit of measure (e.g. "mg/dL", "mmol/L"). */
  unit: string | null;
  /** Reference range as free text (e.g. "70-100 mg/dL"). */
  referenceRange: string | null;
}

/** Lab catalogue entry returned to callers. */
export interface LabCatalogueRecord {
  id: string;
  loincCode: string;
  displayName: string;
  category: string;
  resource: Record<string, unknown>;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Lab Order types (LABS-03)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for creating a lab order (FHIR ServiceRequest). */
export interface LabOrderInput {
  /** Patient the lab is ordered for. */
  patientId: string;
  /** Provider (user ID) signing the order. */
  providerId: string;
  /** LOINC code for the ordered test. */
  loincCode: string;
  /** Human-readable display name for the test. */
  displayName: string;
  /** Order priority: "routine" | "urgent" | "stat" | "asap" */
  priority: string | null;
  /** Clinical indication / reason for test. */
  reasonText: string | null;
  /** Special instructions for the laboratory. */
  note: string | null;
  /** ISO 8601 timestamp of when order was placed. */
  orderedAt: string | null;
}

/** Lab order record returned to callers. */
export interface LabOrderRecord {
  id: string;
  patientId: string;
  providerId: string;
  status: string;
  loincCode: string;
  priority: string;
  resource: Record<string, unknown>;
  lastUpdated: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Lab Result types (LABS-01, LABS-04)
// ─────────────────────────────────────────────────────────────────────────────

/** A single observed value in a lab result. */
export interface LabObservation {
  /** LOINC code for this observation. */
  loincCode: string;
  /** Human-readable display name. */
  displayName: string;
  /** Numeric result value (if applicable). */
  valueQuantity: number | null;
  /** Unit for the value (e.g. "mg/dL"). */
  unit: string | null;
  /** Free-text result (for qualitative tests). */
  valueString: string | null;
  /** Reference range as free text (e.g. "70–100 mg/dL"). */
  referenceRange: string | null;
  /** Abnormal interpretation: "N" | "H" | "L" | "HH" | "LL" | "A" | "AA" */
  interpretation: string | null;
}

/** Input for entering lab results (FHIR DiagnosticReport). */
export interface LabResultInput {
  /** Patient the results belong to. */
  patientId: string;
  /** Linked lab order ID (ServiceRequest), if any. */
  orderId: string | null;
  /** Provider who is entering / verifying the results. */
  providerId: string;
  /** Primary LOINC code for the panel/test (e.g. "24323-8" for CMP). */
  loincCode: string;
  /** Human-readable test name. */
  displayName: string;
  /** Report status: "preliminary" | "final" | "amended" | "corrected" */
  status: string;
  /** ISO 8601 timestamp of when results were reported. */
  reportedAt: string | null;
  /** Lab performing the test. */
  performingLab: string | null;
  /** Individual observed values. */
  observations: LabObservation[];
  /** Overall conclusion / impression. */
  conclusion: string | null;
}

/** Lab result record returned to callers. */
export interface LabResultRecord {
  id: string;
  patientId: string;
  orderId: string | null;
  status: string;
  /** True if any observation has an abnormal interpretation flag. */
  hasAbnormal: boolean;
  loincCode: string;
  resource: Record<string, unknown>;
  lastUpdated: string;
}

/** Input for a provider sign-off action on a lab result. */
export interface SignLabResultInput {
  /** ID of the DiagnosticReport to sign. */
  resultId: string;
  /** Provider signing the result. */
  providerId: string;
  /** Optional comment / clinical action note. */
  comment: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Document management types (DOCS-01, DOCS-02, DOCS-03)
// ─────────────────────────────────────────────────────────────────────────────

/** Input for uploading a patient document (FHIR DocumentReference). */
export interface DocumentUploadInput {
  /** Patient the document belongs to. */
  patientId: string;
  /** Document title (e.g. "CT Chest 2026-03-11"). */
  title: string;
  /** Category: "clinical-note" | "imaging" | "lab-report" | "consent" | "referral" | "other" */
  category: string | null;
  /** MIME type (e.g. "application/pdf", "image/jpeg"). */
  contentType: string;
  /** Base64-encoded file content. */
  contentBase64: string;
  /** File size in bytes (validated against DOCS-01 64 MB limit). Maps from Rust i64. */
  fileSizeBytes: number;
  /** Provider/user uploading the document. */
  uploadedBy: string;
}

/** Document record returned to callers. */
export interface DocumentRecord {
  id: string;
  patientId: string;
  title: string;
  category: string;
  contentType: string;
  /** File size in bytes. Maps from Rust i64. */
  fileSizeBytes: number;
  sha1Checksum: string;
  uploadedAt: string;
  uploadedBy: string;
  resource: Record<string, unknown>;
}

/** Result of a SHA-1 integrity verification check. */
export interface IntegrityCheckResult {
  documentId: string;
  storedSha1: string;
  computedSha1: string;
  integrityOk: boolean;
}
