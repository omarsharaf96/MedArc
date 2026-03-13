/**
 * fhirExtract.ts — Pure FHIR JSON extraction helpers.
 *
 * All FHIR JSON path knowledge is isolated here. Components never navigate
 * the `Record<string, unknown>` blob directly — they call extractPatientDisplay()
 * and receive a typed PatientDisplay object with all nullable string fields.
 *
 * FHIR extension URLs are authoritative from patient.rs:
 *   http://hl7.org/fhir/StructureDefinition/patient-genderIdentity
 *   http://medarc.local/photo-url
 *   http://medarc.local/insurance/{tier}
 *   http://medarc.local/employer
 *   http://medarc.local/sdoh
 *   http://medarc.local/mrn
 *   http://medarc.local/primary-provider
 */

// ─── Types ───────────────────────────────────────────────────────────────────

/** Insurance plan information extracted from one coverage tier extension. */
export interface InsuranceDisplay {
  payerName: string | null;
  planName: string | null;
  memberId: string | null;
  groupNumber: string | null;
  subscriberName: string | null;
  subscriberDob: string | null;
  relationshipToSubscriber: string | null;
}

/** All displayable fields extracted from a patient FHIR resource. */
export interface PatientDisplay {
  familyName: string | null;
  givenNames: string[];
  dob: string | null;
  gender: string | null;
  genderIdentity: string | null;
  photoUrl: string | null;
  phone: string | null;
  email: string | null;
  addressLine: string | null;
  city: string | null;
  state: string | null;
  postalCode: string | null;
  country: string | null;
  mrn: string | null;
  primaryProviderId: string | null;
  insurancePrimary: InsuranceDisplay | null;
  insuranceSecondary: InsuranceDisplay | null;
  insuranceTertiary: InsuranceDisplay | null;
  employer: Record<string, string | null> | null;
  sdoh: Record<string, string | null> | null;
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/** Return an all-null PatientDisplay for when the resource is absent. */
function emptyDisplay(): PatientDisplay {
  return {
    familyName: null,
    givenNames: [],
    dob: null,
    gender: null,
    genderIdentity: null,
    photoUrl: null,
    phone: null,
    email: null,
    addressLine: null,
    city: null,
    state: null,
    postalCode: null,
    country: null,
    mrn: null,
    primaryProviderId: null,
    insurancePrimary: null,
    insuranceSecondary: null,
    insuranceTertiary: null,
    employer: null,
    sdoh: null,
  };
}

/**
 * Extract a single sub-extension valueString by key from an extension's
 * nested `extension` array.
 */
function subExtValue(
  subExts: Array<Record<string, unknown>> | undefined,
  key: string,
): string | null {
  if (!subExts) return null;
  const match = subExts.find((e) => e["url"] === key);
  if (!match) return null;
  const v = match["valueString"];
  if (typeof v === "string" && v !== "") return v;
  return null;
}

/**
 * Find the top-level extension with the given URL and return its nested
 * `extension` array (or undefined if not found / not an array).
 */
function findExtensionSubArray(
  extensions: Array<Record<string, unknown>> | undefined,
  url: string,
): Array<Record<string, unknown>> | undefined {
  if (!extensions) return undefined;
  const ext = extensions.find((e) => e["url"] === url);
  if (!ext) return undefined;
  const nested = ext["extension"];
  if (!Array.isArray(nested)) return undefined;
  return nested as Array<Record<string, unknown>>;
}

/**
 * Extract an insurance tier from the top-level extension array.
 * Returns null if the tier extension is absent.
 */
function extractInsurance(
  extensions: Array<Record<string, unknown>> | undefined,
  tier: "primary" | "secondary" | "tertiary",
): InsuranceDisplay | null {
  const url = `http://medarc.local/insurance/${tier}`;
  const subExts = findExtensionSubArray(extensions, url);
  if (!subExts) return null;

  return {
    payerName: subExtValue(subExts, "payerName"),
    planName: subExtValue(subExts, "planName"),
    memberId: subExtValue(subExts, "memberId"),
    groupNumber: subExtValue(subExts, "groupNumber"),
    subscriberName: subExtValue(subExts, "subscriberName"),
    subscriberDob: subExtValue(subExts, "subscriberDob"),
    relationshipToSubscriber: subExtValue(subExts, "relationshipToSubscriber"),
  };
}

/**
 * Extract a flat group extension into a Record<key, string | null>.
 * Each sub-extension's `url` becomes the key; `valueString` becomes the value.
 * Returns null if the extension is absent.
 */
function extractExtensionGroup(
  extensions: Array<Record<string, unknown>> | undefined,
  url: string,
  keys: string[],
): Record<string, string | null> | null {
  const subExts = findExtensionSubArray(extensions, url);
  if (!subExts) return null;

  const result: Record<string, string | null> = {};
  for (const key of keys) {
    result[key] = subExtValue(subExts, key);
  }
  return result;
}

// ─── Public API ──────────────────────────────────────────────────────────────

// ─── SOAP extraction ──────────────────────────────────────────────────────────

/**
 * SOAP section URLs stored as FHIR extension valueCode on Annotation items.
 * Must match the Rust patient.rs / encounter.rs constants exactly.
 */
const SOAP_SECTION_URL = "http://medarc.local/fhir/ext/soap-section";

/** Structured SOAP section output — all sections nullable (missing ↔ null). */
export interface SoapSections {
  subjective: string | null;
  objective: string | null;
  assessment: string | null;
  plan: string | null;
}

/**
 * Re-hydrate SOAP note sections from a saved FHIR Encounter `resource.note` blob.
 *
 * The `note` array follows FHIR Annotation shape where each item has:
 *   - `text`: the section content
 *   - `extension`: array of `{ url, valueCode }` items; the item with
 *     `url === SOAP_SECTION_URL` holds the section key
 *     ("subjective" | "objective" | "assessment" | "plan").
 *
 * If `resource.note` is absent, empty, or malformed, returns all-null SoapSections.
 * Never throws — safe to call on any unknown resource blob.
 *
 * @param resource - Raw `EncounterRecord.resource` blob (or null/undefined).
 * @returns SoapSections with each field as string | null.
 */
export function extractSoapSections(
  resource: Record<string, unknown> | null | undefined,
): SoapSections {
  const empty: SoapSections = {
    subjective: null,
    objective: null,
    assessment: null,
    plan: null,
  };

  if (!resource) return empty;

  const note = resource["note"];
  if (!Array.isArray(note) || note.length === 0) return empty;

  const result: SoapSections = { ...empty };

  for (const item of note as Array<Record<string, unknown>>) {
    // Each annotation item must have a text field
    const text = item["text"];
    if (typeof text !== "string") continue;

    // Find our SOAP section extension on this annotation item
    const ext = item["extension"];
    if (!Array.isArray(ext)) continue;

    const sectionExt = (ext as Array<Record<string, unknown>>).find(
      (e) => e["url"] === SOAP_SECTION_URL,
    );
    if (!sectionExt) continue;

    const sectionCode = sectionExt["valueCode"];
    if (typeof sectionCode !== "string") continue;

    switch (sectionCode) {
      case "subjective":
        result.subjective = text;
        break;
      case "objective":
        result.objective = text;
        break;
      case "assessment":
        result.assessment = text;
        break;
      case "plan":
        result.plan = text;
        break;
      default:
        // Unknown section code — ignore silently
        break;
    }
  }

  return result;
}

// ─── Clinical display structs ─────────────────────────────────────────────────

/** Displayable fields extracted from a FHIR R4 AllergyIntolerance resource. */
export interface AllergyDisplay {
  substance: string | null;
  category: string | null;
  clinicalStatus: string | null;
  allergyType: string | null;
  severity: string | null;
  reaction: string | null;
  onsetDate: string | null;
  substanceCode: string | null;
  substanceSystem: string | null;
}

/** Displayable fields extracted from a FHIR R4 Condition (problem) resource. */
export interface ProblemDisplay {
  icd10Code: string | null;
  display: string | null;
  clinicalStatus: string | null;
  onsetDate: string | null;
  abatementDate: string | null;
}

/** Displayable fields extracted from a FHIR R4 MedicationStatement resource. */
export interface MedicationDisplay {
  drugName: string | null;
  status: string | null;
  rxnormCode: string | null;
  dosage: string | null;
  effectiveStart: string | null;
  effectiveEnd: string | null;
}

/** Displayable fields extracted from a FHIR R4 Immunization resource. */
export interface ImmunizationDisplay {
  vaccineName: string | null;
  cvxCode: string | null;
  occurrenceDate: string | null;
  lotNumber: string | null;
  status: string | null;
}

// ─── Clinical extract functions ───────────────────────────────────────────────

/**
 * Extract displayable fields from a FHIR R4 AllergyIntolerance resource.
 *
 * FHIR paths (authoritative from build_allergy_fhir in clinical.rs):
 *   substance:      code.text
 *   category:       category[0]
 *   clinicalStatus: clinicalStatus.coding[0].code
 *   allergyType:    type
 *   severity:       reaction[0].severity
 *   reaction:       reaction[0].manifestation[0].text
 *   onsetDate:      onsetDateTime
 *   substanceCode:  code.coding[0].code
 *   substanceSystem: code.coding[0].system
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractAllergyDisplay(
  resource: Record<string, unknown> | null | undefined,
): AllergyDisplay {
  const empty: AllergyDisplay = {
    substance: null,
    category: null,
    clinicalStatus: null,
    allergyType: null,
    severity: null,
    reaction: null,
    onsetDate: null,
    substanceCode: null,
    substanceSystem: null,
  };

  if (!resource) return empty;

  const code = resource["code"] as Record<string, unknown> | undefined;
  const substance =
    typeof code?.["text"] === "string" ? code["text"] : null;

  const categoryArr = resource["category"] as Array<unknown> | undefined;
  const category =
    Array.isArray(categoryArr) && typeof categoryArr[0] === "string"
      ? categoryArr[0]
      : null;

  const clinicalStatusCoding = (
    resource["clinicalStatus"] as Record<string, unknown> | undefined
  )?.["coding"] as Array<Record<string, unknown>> | undefined;
  const clinicalStatus =
    typeof clinicalStatusCoding?.[0]?.["code"] === "string"
      ? clinicalStatusCoding[0]["code"]
      : null;

  const allergyType =
    typeof resource["type"] === "string" ? resource["type"] : null;

  const reactionArr = resource["reaction"] as
    | Array<Record<string, unknown>>
    | undefined;
  const rxn0 = Array.isArray(reactionArr) ? reactionArr[0] : undefined;
  const severity =
    typeof rxn0?.["severity"] === "string" ? rxn0["severity"] : null;
  const manifestationArr = rxn0?.["manifestation"] as
    | Array<Record<string, unknown>>
    | undefined;
  const reaction =
    typeof manifestationArr?.[0]?.["text"] === "string"
      ? manifestationArr[0]["text"]
      : null;

  const onsetDate =
    typeof resource["onsetDateTime"] === "string"
      ? resource["onsetDateTime"]
      : null;

  const codeCoding = code?.["coding"] as
    | Array<Record<string, unknown>>
    | undefined;
  const substanceCode =
    typeof codeCoding?.[0]?.["code"] === "string"
      ? codeCoding[0]["code"]
      : null;
  const substanceSystem =
    typeof codeCoding?.[0]?.["system"] === "string"
      ? codeCoding[0]["system"]
      : null;

  return {
    substance,
    category,
    clinicalStatus,
    allergyType,
    severity,
    reaction,
    onsetDate,
    substanceCode,
    substanceSystem,
  };
}

/**
 * Extract displayable fields from a FHIR R4 Condition resource.
 *
 * FHIR paths (authoritative from build_problem_fhir in clinical.rs):
 *   icd10Code:      code.coding[0].code
 *   display:        code.text
 *   clinicalStatus: clinicalStatus.coding[0].code
 *   onsetDate:      onsetDateTime
 *   abatementDate:  abatementDateTime
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractProblemDisplay(
  resource: Record<string, unknown> | null | undefined,
): ProblemDisplay {
  const empty: ProblemDisplay = {
    icd10Code: null,
    display: null,
    clinicalStatus: null,
    onsetDate: null,
    abatementDate: null,
  };

  if (!resource) return empty;

  const code = resource["code"] as Record<string, unknown> | undefined;
  const codeCoding = code?.["coding"] as
    | Array<Record<string, unknown>>
    | undefined;
  const icd10Code =
    typeof codeCoding?.[0]?.["code"] === "string"
      ? codeCoding[0]["code"]
      : null;
  const display =
    typeof code?.["text"] === "string" ? code["text"] : null;

  const clinicalStatusCoding = (
    resource["clinicalStatus"] as Record<string, unknown> | undefined
  )?.["coding"] as Array<Record<string, unknown>> | undefined;
  const clinicalStatus =
    typeof clinicalStatusCoding?.[0]?.["code"] === "string"
      ? clinicalStatusCoding[0]["code"]
      : null;

  const onsetDate =
    typeof resource["onsetDateTime"] === "string"
      ? resource["onsetDateTime"]
      : null;
  const abatementDate =
    typeof resource["abatementDateTime"] === "string"
      ? resource["abatementDateTime"]
      : null;

  return { icd10Code, display, clinicalStatus, onsetDate, abatementDate };
}

/**
 * Extract displayable fields from a FHIR R4 MedicationStatement resource.
 *
 * FHIR paths (authoritative from build_medication_fhir in clinical.rs):
 *   drugName:       medication.concept.text
 *   status:         status
 *   rxnormCode:     medication.concept.coding[0].code
 *   dosage:         dosage[0].text
 *   effectiveStart: effectivePeriod.start
 *   effectiveEnd:   effectivePeriod.end
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractMedicationDisplay(
  resource: Record<string, unknown> | null | undefined,
): MedicationDisplay {
  const empty: MedicationDisplay = {
    drugName: null,
    status: null,
    rxnormCode: null,
    dosage: null,
    effectiveStart: null,
    effectiveEnd: null,
  };

  if (!resource) return empty;

  const medication = resource["medication"] as
    | Record<string, unknown>
    | undefined;
  const concept = medication?.["concept"] as
    | Record<string, unknown>
    | undefined;
  const drugName =
    typeof concept?.["text"] === "string" ? concept["text"] : null;

  const status =
    typeof resource["status"] === "string" ? resource["status"] : null;

  const conceptCoding = concept?.["coding"] as
    | Array<Record<string, unknown>>
    | undefined;
  const rxnormCode =
    typeof conceptCoding?.[0]?.["code"] === "string"
      ? conceptCoding[0]["code"]
      : null;

  const dosageArr = resource["dosage"] as
    | Array<Record<string, unknown>>
    | undefined;
  const dosage =
    typeof dosageArr?.[0]?.["text"] === "string" ? dosageArr[0]["text"] : null;

  const effectivePeriod = resource["effectivePeriod"] as
    | Record<string, unknown>
    | undefined;
  const effectiveStart =
    typeof effectivePeriod?.["start"] === "string"
      ? effectivePeriod["start"]
      : null;
  const effectiveEnd =
    typeof effectivePeriod?.["end"] === "string"
      ? effectivePeriod["end"]
      : null;

  return { drugName, status, rxnormCode, dosage, effectiveStart, effectiveEnd };
}

/**
 * Extract displayable fields from a FHIR R4 Immunization resource.
 *
 * FHIR paths (authoritative from build_immunization_fhir in clinical.rs):
 *   vaccineName:    vaccineCode.text
 *   cvxCode:        vaccineCode.coding[0].code
 *   occurrenceDate: occurrenceDateTime
 *   lotNumber:      lotNumber
 *   status:         status
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractImmunizationDisplay(
  resource: Record<string, unknown> | null | undefined,
): ImmunizationDisplay {
  const empty: ImmunizationDisplay = {
    vaccineName: null,
    cvxCode: null,
    occurrenceDate: null,
    lotNumber: null,
    status: null,
  };

  if (!resource) return empty;

  const vaccineCode = resource["vaccineCode"] as
    | Record<string, unknown>
    | undefined;
  const vaccineName =
    typeof vaccineCode?.["text"] === "string" ? vaccineCode["text"] : null;

  const vaccineCodeCoding = vaccineCode?.["coding"] as
    | Array<Record<string, unknown>>
    | undefined;
  const cvxCode =
    typeof vaccineCodeCoding?.[0]?.["code"] === "string"
      ? vaccineCodeCoding[0]["code"]
      : null;

  const occurrenceDate =
    typeof resource["occurrenceDateTime"] === "string"
      ? resource["occurrenceDateTime"]
      : null;
  const lotNumber =
    typeof resource["lotNumber"] === "string" ? resource["lotNumber"] : null;
  const status =
    typeof resource["status"] === "string" ? resource["status"] : null;

  return { vaccineName, cvxCode, occurrenceDate, lotNumber, status };
}

// ─── Appointment display struct ───────────────────────────────────────────────

/**
 * All displayable fields extracted from a FHIR Appointment resource.
 * Built by build_appointment_fhir in scheduling.rs.
 */
export interface AppointmentDisplay {
  /** ISO 8601 datetime string — appointment start. */
  start: string | null;
  /** ISO 8601 datetime string — appointment end (computed from duration). */
  end: string | null;
  /** Duration in minutes (number, not string). */
  durationMin: number | null;
  /** FHIR appointment status: "booked" | "cancelled" | "fulfilled" | "noshow" | … */
  status: string | null;
  /** serviceType[0].coding[0].code — e.g. "follow_up". */
  apptType: string | null;
  /** serviceType[0].coding[0].display — human-readable label. */
  apptTypeDisplay: string | null;
  /** reason[0].text — free-text reason for visit. */
  reason: string | null;
  /** extension appointment-color → valueString. */
  color: string | null;
  /** extension appointment-recurrence → valueString. */
  recurrence: string | null;
  /** extension appointment-recurrence-group → valueId. */
  recurrenceGroup: string | null;
  /** extension appointment-notes → valueString. */
  notes: string | null;
}

/**
 * Extract displayable fields from a FHIR Appointment resource blob.
 *
 * FHIR paths (authoritative from build_appointment_fhir in scheduling.rs):
 *   start:            resource.start (string)
 *   end:              resource.end (string)
 *   durationMin:      resource.minutesDuration (number)
 *   status:           resource.status (string)
 *   apptType:         resource.serviceType[0].coding[0].code
 *   apptTypeDisplay:  resource.serviceType[0].coding[0].display
 *   reason:           resource.reason[0].text
 *   color:            extension url="…/appointment-color" → valueString
 *   recurrence:       extension url="…/appointment-recurrence" → valueString
 *   recurrenceGroup:  extension url="…/appointment-recurrence-group" → valueId
 *   notes:            extension url="…/appointment-notes" → valueString
 *
 * Never throws. Returns all-null struct on null/undefined input.
 * No `as any`. All extension accesses use Array.isArray guard + .find().
 */
export function extractAppointmentDisplay(
  resource: Record<string, unknown> | null | undefined,
): AppointmentDisplay {
  const empty: AppointmentDisplay = {
    start: null,
    end: null,
    durationMin: null,
    status: null,
    apptType: null,
    apptTypeDisplay: null,
    reason: null,
    color: null,
    recurrence: null,
    recurrenceGroup: null,
    notes: null,
  };

  if (!resource) return empty;

  const start =
    typeof resource["start"] === "string" ? resource["start"] : null;
  const end =
    typeof resource["end"] === "string" ? resource["end"] : null;

  const rawDuration = resource["minutesDuration"];
  const durationMin =
    typeof rawDuration === "number" ? rawDuration : null;

  const status =
    typeof resource["status"] === "string" ? resource["status"] : null;

  // serviceType[0].coding[0].code / .display
  let apptType: string | null = null;
  let apptTypeDisplay: string | null = null;
  const serviceType = resource["serviceType"];
  if (Array.isArray(serviceType) && serviceType.length > 0) {
    const st0 = serviceType[0] as Record<string, unknown>;
    const coding = st0["coding"];
    if (Array.isArray(coding) && coding.length > 0) {
      const coding0 = coding[0] as Record<string, unknown>;
      apptType =
        typeof coding0["code"] === "string" ? coding0["code"] : null;
      apptTypeDisplay =
        typeof coding0["display"] === "string" ? coding0["display"] : null;
    }
  }

  // reason[0].text
  let reason: string | null = null;
  const reasonArr = resource["reason"];
  if (Array.isArray(reasonArr) && reasonArr.length > 0) {
    const r0 = reasonArr[0] as Record<string, unknown>;
    reason = typeof r0["text"] === "string" ? r0["text"] : null;
  }

  // Extensions
  let color: string | null = null;
  let recurrence: string | null = null;
  let recurrenceGroup: string | null = null;
  let notes: string | null = null;

  const extensions = resource["extension"];
  if (Array.isArray(extensions)) {
    const extArr = extensions as Array<Record<string, unknown>>;

    const colorExt = extArr.find(
      (e) =>
        e["url"] ===
        "http://medarc.local/fhir/StructureDefinition/appointment-color",
    );
    if (colorExt && typeof colorExt["valueString"] === "string") {
      color = colorExt["valueString"];
    }

    const recurrenceExt = extArr.find(
      (e) =>
        e["url"] ===
        "http://medarc.local/fhir/StructureDefinition/appointment-recurrence",
    );
    if (recurrenceExt && typeof recurrenceExt["valueString"] === "string") {
      recurrence = recurrenceExt["valueString"];
    }

    const recurrenceGroupExt = extArr.find(
      (e) =>
        e["url"] ===
        "http://medarc.local/fhir/StructureDefinition/appointment-recurrence-group",
    );
    if (
      recurrenceGroupExt &&
      typeof recurrenceGroupExt["valueId"] === "string"
    ) {
      recurrenceGroup = recurrenceGroupExt["valueId"];
    }

    const notesExt = extArr.find(
      (e) =>
        e["url"] ===
        "http://medarc.local/fhir/StructureDefinition/appointment-notes",
    );
    if (notesExt && typeof notesExt["valueString"] === "string") {
      notes = notesExt["valueString"];
    }
  }

  return {
    start,
    end,
    durationMin,
    status,
    apptType,
    apptTypeDisplay,
    reason,
    color,
    recurrence,
    recurrenceGroup,
    notes,
  };
}

// ─── Physical Exam display struct ────────────────────────────────────────────

/**
 * Displayable fields extracted from a FHIR ClinicalImpression resource
 * representing a Physical Exam record. All 13 body-system fields + additionalNotes.
 */
export interface ExtractedPhysicalExam {
  general: string | null;
  heent: string | null;
  neck: string | null;
  cardiovascular: string | null;
  pulmonary: string | null;
  abdomen: string | null;
  extremities: string | null;
  neurological: string | null;
  skin: string | null;
  psychiatric: string | null;
  musculoskeletal: string | null;
  genitourinary: string | null;
  rectal: string | null;
  additionalNotes: string | null;
}

/** System codes that map finding.itemCodeableConcept.coding[0].code → field key. */
const PHYSICAL_EXAM_SYSTEM_CODES = new Set([
  "general",
  "heent",
  "neck",
  "cardiovascular",
  "pulmonary",
  "abdomen",
  "extremities",
  "neurological",
  "skin",
  "psychiatric",
  "musculoskeletal",
  "genitourinary",
  "rectal",
  "additionalNotes",
]);

/**
 * Extract displayable fields from a FHIR ClinicalImpression resource storing
 * physical exam findings.
 *
 * FHIR paths (authoritative from physical_exam.rs):
 *   Each body system is stored as a `finding` entry:
 *     finding[].itemCodeableConcept.coding[0].code  — system key
 *     finding[].itemCodeableConcept.text            — free-text finding
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractPhysicalExamDisplay(
  resource: Record<string, unknown> | null | undefined,
): ExtractedPhysicalExam {
  const empty: ExtractedPhysicalExam = {
    general: null,
    heent: null,
    neck: null,
    cardiovascular: null,
    pulmonary: null,
    abdomen: null,
    extremities: null,
    neurological: null,
    skin: null,
    psychiatric: null,
    musculoskeletal: null,
    genitourinary: null,
    rectal: null,
    additionalNotes: null,
  };

  if (!resource) return empty;

  const findings = resource["finding"];
  if (!Array.isArray(findings)) return empty;

  const result: ExtractedPhysicalExam = { ...empty };

  for (const finding of findings as Array<Record<string, unknown>>) {
    const itemCodeableConcept = finding["itemCodeableConcept"] as
      | Record<string, unknown>
      | undefined;
    if (!itemCodeableConcept) continue;

    // Get the system code from coding[0].code
    const coding = itemCodeableConcept["coding"] as
      | Array<Record<string, unknown>>
      | undefined;
    const code =
      Array.isArray(coding) && coding.length > 0
        ? (coding[0]["code"] as string | undefined)
        : undefined;
    if (!code || !PHYSICAL_EXAM_SYSTEM_CODES.has(code)) continue;

    // Get the free-text finding from itemCodeableConcept.text
    const text = itemCodeableConcept["text"];
    if (typeof text !== "string") continue;

    (result as unknown as Record<string, unknown>)[code] = text;
  }

  return result;
}

// ─── Lab Order display struct ─────────────────────────────────────────────────

/**
 * Displayable fields extracted from a FHIR ServiceRequest (lab order) resource.
 * All fields are nullable; never throws on bad input.
 */
export interface ExtractedLabOrder {
  loincCode: string | null;
  displayName: string | null;
  status: string | null;
  priority: string | null;
  lastUpdated: string | null;
}

/**
 * Extract displayable fields from a FHIR ServiceRequest resource blob
 * representing a lab order.
 *
 * FHIR paths (authoritative from lab_order.rs):
 *   loincCode:   code.coding[0].code
 *   displayName: code.coding[0].display
 *   status:      status
 *   priority:    priority
 *
 * lastUpdated is not stored in the FHIR resource; the LabOrderRecord carries
 * it as a top-level field — pass it in or leave null.
 *
 * Never throws. Returns all-null struct on null/undefined input.
 */
export function extractLabOrderDisplay(
  resource: Record<string, unknown> | null | undefined,
): ExtractedLabOrder {
  const empty: ExtractedLabOrder = {
    loincCode: null,
    displayName: null,
    status: null,
    priority: null,
    lastUpdated: null,
  };

  if (!resource) return empty;

  const code = resource["code"] as Record<string, unknown> | undefined;
  const coding = code?.["coding"] as
    | Array<Record<string, unknown>>
    | undefined;
  const loincCode =
    typeof coding?.[0]?.["code"] === "string" ? coding[0]["code"] : null;
  const displayName =
    typeof coding?.[0]?.["display"] === "string"
      ? coding[0]["display"]
      : null;

  const status =
    typeof resource["status"] === "string" ? resource["status"] : null;
  const priority =
    typeof resource["priority"] === "string" ? resource["priority"] : null;

  return { loincCode, displayName, status, priority, lastUpdated: null };
}

// ─── Lab Result display struct ────────────────────────────────────────────────

/**
 * Displayable fields extracted from a LabResultRecord (denormalized fields,
 * not the raw FHIR resource blob).
 * All fields are nullable/false by default; never throws on bad input.
 */
export interface ExtractedLabResult {
  loincCode: string | null;
  displayName: string | null;
  status: string | null;
  hasAbnormal: boolean;
  lastUpdated: string | null;
}

/**
 * Extract displayable fields from a LabResultRecord's denormalized fields.
 *
 * Reads directly from the denormalized `LabResultRecord` fields:
 *   loincCode    — already a top-level string on the record
 *   status       — already a top-level string on the record
 *   hasAbnormal  — already a top-level boolean on the record
 *   lastUpdated  — already a top-level string on the record
 *
 * Note: displayName is NOT stored in the denormalized fields — it lives in
 * the FHIR resource blob. We return null for displayName and let the caller
 * fall back to loincCode or another display strategy.
 *
 * Never throws. Returns all-false/null struct on null/undefined input.
 */
export function extractLabResultDisplay(
  record:
    | {
        loincCode: string;
        status: string;
        hasAbnormal: boolean;
        lastUpdated: string;
      }
    | null
    | undefined,
): ExtractedLabResult {
  const empty: ExtractedLabResult = {
    loincCode: null,
    displayName: null,
    status: null,
    hasAbnormal: false,
    lastUpdated: null,
  };

  if (!record) return empty;

  return {
    loincCode: record.loincCode ?? null,
    displayName: null,
    status: record.status ?? null,
    hasAbnormal: record.hasAbnormal === true,
    lastUpdated: record.lastUpdated ?? null,
  };
}

// ─── Patient display extract ──────────────────────────────────────────────────

/**
 * Extract all displayable fields from a raw FHIR Patient resource blob.
 *
 * Guards against null/undefined resource — returns all-null PatientDisplay
 * rather than throwing. All paths use optional chaining; no `as any` casts.
 *
 * @param resource - The raw `resource` field from a PatientRecord, or null/undefined.
 * @returns PatientDisplay with all fields populated or null/[].
 */
export function extractPatientDisplay(
  resource: Record<string, unknown> | null | undefined,
): PatientDisplay {
  // Null/undefined guard — return safe defaults rather than throwing.
  if (!resource) return emptyDisplay();

  const names = resource["name"] as Array<Record<string, unknown>> | undefined;
  const firstName = names?.[0];
  const familyName =
    typeof firstName?.["family"] === "string" ? firstName["family"] : null;
  const givenRaw = firstName?.["given"];
  const givenNames = Array.isArray(givenRaw)
    ? (givenRaw.filter((g) => typeof g === "string") as string[])
    : [];

  const dob =
    typeof resource["birthDate"] === "string" ? resource["birthDate"] : null;
  const gender =
    typeof resource["gender"] === "string" ? resource["gender"] : null;

  // Top-level extensions array
  const extensions = resource["extension"] as
    | Array<Record<string, unknown>>
    | undefined;

  // Gender identity: extension url="http://hl7.org/fhir/StructureDefinition/patient-genderIdentity" → valueString
  let genderIdentity: string | null = null;
  if (extensions) {
    const giExt = extensions.find(
      (e) =>
        e["url"] ===
        "http://hl7.org/fhir/StructureDefinition/patient-genderIdentity",
    );
    if (giExt && typeof giExt["valueString"] === "string") {
      genderIdentity = giExt["valueString"];
    }
  }

  // Photo URL: extension url="http://medarc.local/photo-url" → valueUrl
  let photoUrl: string | null = null;
  if (extensions) {
    const photoExt = extensions.find(
      (e) => e["url"] === "http://medarc.local/photo-url",
    );
    if (photoExt && typeof photoExt["valueUrl"] === "string") {
      photoUrl = photoExt["valueUrl"];
    }
  }

  // Telecom
  const telecom = resource["telecom"] as
    | Array<Record<string, unknown>>
    | undefined;
  let phone: string | null = null;
  let email: string | null = null;
  if (telecom) {
    const phoneEntry = telecom.find((t) => t["system"] === "phone");
    if (phoneEntry && typeof phoneEntry["value"] === "string") {
      phone = phoneEntry["value"];
    }
    const emailEntry = telecom.find((t) => t["system"] === "email");
    if (emailEntry && typeof emailEntry["value"] === "string") {
      email = emailEntry["value"];
    }
  }

  // Address
  const address = resource["address"] as
    | Array<Record<string, unknown>>
    | undefined;
  const addr0 = address?.[0];
  const lineArr = addr0?.["line"] as Array<unknown> | undefined;
  const addressLine =
    Array.isArray(lineArr) && typeof lineArr[0] === "string"
      ? lineArr[0]
      : null;
  const city = typeof addr0?.["city"] === "string" ? addr0["city"] : null;
  const state = typeof addr0?.["state"] === "string" ? addr0["state"] : null;
  const postalCode =
    typeof addr0?.["postalCode"] === "string" ? addr0["postalCode"] : null;
  const country =
    typeof addr0?.["country"] === "string" ? addr0["country"] : null;

  // Identifiers
  const identifier = resource["identifier"] as
    | Array<Record<string, unknown>>
    | undefined;
  let mrn: string | null = null;
  let primaryProviderId: string | null = null;
  if (identifier) {
    const mrnEntry = identifier.find(
      (id) => id["system"] === "http://medarc.local/mrn",
    );
    if (mrnEntry && typeof mrnEntry["value"] === "string") {
      mrn = mrnEntry["value"];
    }
    const ppEntry = identifier.find(
      (id) => id["system"] === "http://medarc.local/primary-provider",
    );
    if (ppEntry && typeof ppEntry["value"] === "string") {
      primaryProviderId = ppEntry["value"];
    }
  }

  // Insurance tiers
  const insurancePrimary = extractInsurance(extensions, "primary");
  const insuranceSecondary = extractInsurance(extensions, "secondary");
  const insuranceTertiary = extractInsurance(extensions, "tertiary");

  // Employer group
  const employer = extractExtensionGroup(
    extensions,
    "http://medarc.local/employer",
    ["employerName", "occupation", "employerPhone", "employerAddress"],
  );

  // SDOH group
  const sdoh = extractExtensionGroup(
    extensions,
    "http://medarc.local/sdoh",
    [
      "housingStatus",
      "foodSecurity",
      "transportationAccess",
      "educationLevel",
      "notes",
    ],
  );

  return {
    familyName,
    givenNames,
    dob,
    gender,
    genderIdentity,
    photoUrl,
    phone,
    email,
    addressLine,
    city,
    state,
    postalCode,
    country,
    mrn,
    primaryProviderId,
    insurancePrimary,
    insuranceSecondary,
    insuranceTertiary,
    employer,
    sdoh,
  };
}

// ─── Document display extraction ─────────────────────────────────────────────

/** Displayable fields extracted from a DocumentRecord's denormalized fields. */
export interface ExtractedDocument {
  title: string | null;
  category: string | null;
  contentType: string | null;
  fileSizeBytes: number | null;
  uploadedAt: string | null;
  uploadedBy: string | null;
}

/** All-null ExtractedDocument for absent records. */
function emptyDocument(): ExtractedDocument {
  return {
    title: null,
    category: null,
    contentType: null,
    fileSizeBytes: null,
    uploadedAt: null,
    uploadedBy: null,
  };
}

/**
 * Extract displayable fields from a DocumentRecord's denormalized fields.
 * Reads directly from the record (not the FHIR resource blob).
 * Returns all-null struct on null/undefined input. Never throws.
 */
export function extractDocumentDisplay(
  record:
    | {
        title: string;
        category: string;
        contentType: string;
        fileSizeBytes: number;
        uploadedAt: string;
        uploadedBy: string;
      }
    | null
    | undefined,
): ExtractedDocument {
  if (record == null) return emptyDocument();
  try {
    return {
      title: record.title ?? null,
      category: record.category ?? null,
      contentType: record.contentType ?? null,
      fileSizeBytes: typeof record.fileSizeBytes === "number" ? record.fileSizeBytes : null,
      uploadedAt: record.uploadedAt ?? null,
      uploadedBy: record.uploadedBy ?? null,
    };
  } catch {
    return emptyDocument();
  }
}
