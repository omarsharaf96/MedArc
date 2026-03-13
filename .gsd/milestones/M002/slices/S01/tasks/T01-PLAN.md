---
estimated_steps: 4
estimated_files: 4
---

# T01: Create TypeScript type files for all M001 command structs

**Slice:** S01 — Navigation Shell & Type System
**Milestone:** M002

## Description

Create four new TypeScript type files that mirror every Rust struct crossing the Tauri IPC boundary for the patient, clinical, scheduling, documentation, and labs/documents modules. These types are the hard dependency of T02 (invoke wrappers) and every page component in S02–S07. All field names must be `camelCase` (matching Rust's `#[serde(rename_all = "camelCase")]`). `Option<T>` maps to `T | null`. `serde_json::Value` maps to `Record<string, unknown>`. No `any`. The `RosStatus` enum maps to a string literal union matching Rust's `#[serde(rename_all = "snake_case")]`.

Patient and clinical structs are co-located in `patient.ts` per the architectural decision — clinical data (allergies, problems, medications, immunizations) is all patient-scoped and accessed together in S02–S04.

Backup commands do not exist in `lib.rs` yet — do not create a `backup.ts` file.

## Steps

1. **Create `src/types/patient.ts`** — combining patient + clinical structs. Add a JSDoc header matching `src/types/auth.ts` style. Include all of:
   - `InsuranceInput` (payerName, planName | null, memberId, groupNumber | null, subscriberName | null, subscriberDob | null, relationshipToSubscriber | null)
   - `EmployerInput` (employerName, occupation | null, employerPhone | null, employerAddress | null)
   - `SdohInput` (housingStatus | null, foodSecurity | null, transportationAccess | null, educationLevel | null, notes | null)
   - `PatientInput` (familyName, givenNames: string[], birthDate | null, gender | null, genderIdentity | null, phone | null, email | null, addressLine | null, city | null, state | null, postalCode | null, country | null, photoUrl | null, mrn | null, primaryProviderId | null, insurancePrimary: InsuranceInput | null, insuranceSecondary: InsuranceInput | null, insuranceTertiary: InsuranceInput | null, employer: EmployerInput | null, sdoh: SdohInput | null)
   - `PatientSummary` (id, mrn, familyName, givenNames: string[], birthDate | null, gender | null, phone | null, primaryProviderId | null)
   - `PatientRecord` (id, mrn, resource: Record\<string,unknown\>, versionId: number, lastUpdated, createdAt)
   - `PatientSearchQuery` (name | null, mrn | null, birthDate | null, limit: number | null)
   - `CareTeamMemberInput` (patientId, memberId, memberName, role, note | null)
   - `CareTeamRecord` (id, patientId, resource: Record\<string,unknown\>, lastUpdated)
   - `RelatedPersonInput` (patientId, familyName, givenNames: string[], relationship, phone | null, email | null, addressLine | null, city | null, state | null, postalCode | null)
   - `RelatedPersonRecord` (id, patientId, resource: Record\<string,unknown\>, lastUpdated)
   - `AllergyInput` (patientId, category, substance, substanceCode | null, substanceSystem | null, clinicalStatus | null, allergyType | null, severity | null, reaction | null, onsetDate | null, notes | null)
   - `AllergyRecord` (id, patientId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `ProblemInput` (patientId, icd10Code, display, clinicalStatus | null, onsetDate | null, abatementDate | null, notes | null)
   - `ProblemRecord` (id, patientId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `MedicationInput` (patientId, rxnormCode | null, display, status | null, dosage | null, effectiveStart | null, effectiveEnd | null, prescriberId | null, reason | null, notes | null)
   - `MedicationRecord` (id, patientId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `ImmunizationInput` (patientId, cvxCode, display, occurrenceDate, lotNumber | null, expirationDate | null, site | null, route | null, doseNumber: number | null, status | null, notes | null)
   - `ImmunizationRecord` (id, patientId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)

2. **Create `src/types/scheduling.ts`** with interfaces:
   - `AppointmentInput` (patientId, providerId, startTime, durationMinutes: number, apptType, color | null, reason | null, recurrence | null, recurrenceEndDate | null, notes | null)
   - `AppointmentRecord` (id, patientId, providerId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `UpdateAppointmentInput` (startTime | null, durationMinutes: number | null, status | null, reason | null, notes | null, providerId | null, color | null)
   - `WaitlistInput` (patientId, providerId | null, apptType, preferredDate, priority: number | null, reason | null, notes | null)
   - `WaitlistRecord` (id, patientId, providerId | null, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `RecallInput` (patientId, providerId | null, dueDate, recallType, reason, notes | null)
   - `RecallRecord` (id, patientId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `UpdateFlowStatusInput` (appointmentId, flowStatus, room | null, notes | null)
   - `FlowBoardEntry` (appointmentId, patientId, providerId, flowStatus, startTime, apptType, color | null, room | null, checkedInAt | null)

3. **Create `src/types/documentation.ts`** with:
   - `export type RosStatus = "positive" | "negative" | "not_reviewed"` (string literal union — do NOT use a numeric enum)
   - `SoapInput` (subjective | null, objective | null, assessment | null, plan | null)
   - `EncounterInput` (patientId, providerId, encounterDate, encounterType, chiefComplaint | null, templateId | null, soap: SoapInput | null)
   - `EncounterRecord` (id, patientId, providerId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `UpdateEncounterInput` (status | null, soap: SoapInput | null, chiefComplaint | null)
   - `VitalsInput` (patientId, encounterId, recordedAt, systolicBp: number | null, diastolicBp: number | null, heartRate: number | null, respiratoryRate: number | null, temperatureCelsius: number | null, spo2Percent: number | null, weightKg: number | null, heightCm: number | null, painScore: number | null, notes | null)
   - `VitalsRecord` (id, patientId, encounterId, bmi: number | null, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `ReviewOfSystemsInput` (patientId, encounterId, plus 14 system fields each typed as `RosStatus | null` and 14 `*Notes: string | null` fields — constitutional, eyes, ent, cardiovascular, respiratory, gastrointestinal, genitourinary, musculoskeletal, integumentary, neurological, psychiatric, endocrine, hematologic, allergicImmunologic, plus all corresponding *Notes fields)
   - `RosRecord` (id, patientId, encounterId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `PhysicalExamInput` (patientId, encounterId, general | null, heent | null, neck | null, cardiovascular | null, pulmonary | null, abdomen | null, extremities | null, neurological | null, skin | null, psychiatric | null, musculoskeletal | null, genitourinary | null, rectal | null, additionalNotes | null)
   - `PhysicalExamRecord` (id, patientId, encounterId, resource: Record\<string,unknown\>, versionId: number, lastUpdated)
   - `TemplateRecord` (id, name, specialty, description, defaultSoap: SoapInput, defaultExamSections: string[], rosSystems: string[])
   - `CosignRequestInput` (encounterId, supervisingProviderId, message | null)
   - `CosignRecord` (id, encounterId, requestingProviderId, supervisingProviderId, status, requestedAt, signedAt | null, resource: Record\<string,unknown\>)
   - `DrugAllergyAlert` (medicationId, medicationName, medicationRxnorm | null, allergyId, allergySubstance, allergySeverity | null, allergyReaction | null, alertSeverity, message)

4. **Create `src/types/labs.ts`** with interfaces:
   - `LabCatalogueInput` (loincCode, displayName, category | null, specimenType | null, unit | null, referenceRange | null)
   - `LabCatalogueRecord` (id, loincCode, displayName, category, resource: Record\<string,unknown\>, lastUpdated)
   - `LabOrderInput` (patientId, providerId, loincCode, displayName, priority | null, reasonText | null, note | null, orderedAt | null)
   - `LabOrderRecord` (id, patientId, providerId, status, loincCode, priority, resource: Record\<string,unknown\>, lastUpdated)
   - `LabObservation` (loincCode, displayName, valueQuantity: number | null, unit | null, valueString | null, referenceRange | null, interpretation | null)
   - `LabResultInput` (patientId, orderId | null, providerId, loincCode, displayName, status, reportedAt | null, performingLab | null, observations: LabObservation[], conclusion | null)
   - `LabResultRecord` (id, patientId, orderId | null, status, hasAbnormal: boolean, loincCode, resource: Record\<string,unknown\>, lastUpdated)
   - `SignLabResultInput` (resultId, providerId, comment | null)
   - `DocumentUploadInput` (patientId, title, category | null, contentType, contentBase64, fileSizeBytes: number, uploadedBy)
   - `DocumentRecord` (id, patientId, title, category, contentType, fileSizeBytes: number, sha1Checksum, uploadedAt, uploadedBy, resource: Record\<string,unknown\>)
   - `IntegrityCheckResult` (documentId, storedSha1, computedSha1, integrityOk: boolean)

## Must-Haves

- [ ] Every Rust struct that appears in `src-tauri/src/lib.rs`'s invoke_handler has a corresponding TypeScript interface
- [ ] `RosStatus` is `type RosStatus = "positive" | "negative" | "not_reviewed"` — NOT a numeric enum
- [ ] All `Option<T>` Rust fields become `T | null` (not `T | undefined`)
- [ ] All `serde_json::Value` Rust fields become `Record<string, unknown>`
- [ ] `DocumentUploadInput.fileSizeBytes` is `number` (maps from Rust `i64`)
- [ ] `LabResultRecord.hasAbnormal` is `boolean` (maps from Rust `bool`)
- [ ] No `any` type in any file
- [ ] Each file has a JSDoc header noting field name convention (camelCase matching Rust serde)
- [ ] `tsc --noEmit` exits 0 after all four files are created
- [ ] No backup types created (backup commands not in lib.rs)

## Verification

- `npx tsc --noEmit 2>&1 | head -30` — expect zero errors
- `grep -rn " any" src/types/patient.ts src/types/scheduling.ts src/types/documentation.ts src/types/labs.ts` — must return no matches
- Manual cross-check: `ReviewOfSystemsInput` has 14 system `RosStatus | null` fields + 14 `*Notes: string | null` fields + `patientId` + `encounterId` = 30 fields total
- Manual cross-check: `PatientInput` field count matches `src-tauri/src/commands/patient.rs` struct definition

## Observability Impact

- Signals added/changed: None at runtime — compile-time type declarations only
- How a future agent inspects this: `npx tsc --noEmit` surfaces any field mismatches between frontend types and Rust structs; `grep -c "interface\|^export type" src/types/*.ts` gives interface count per file
- Failure state exposed: TypeScript compile errors pinpoint exact field mismatches when backend structs change; strict `T | null` vs `T | undefined` discipline catches incorrect optional field handling at compile time

## Inputs

- `src-tauri/src/commands/patient.rs` — canonical field names and types for patient + care team + related person structs
- `src-tauri/src/commands/clinical.rs` — canonical fields for AllergyInput, ProblemInput, MedicationInput, ImmunizationInput
- `src-tauri/src/commands/scheduling.rs` — canonical fields for all scheduling structs including `FlowBoardEntry` (appointmentId, patientId, providerId, flowStatus, startTime, apptType, color | null, room | null, checkedInAt | null)
- `src-tauri/src/commands/documentation.rs` — canonical fields; note `RosStatus` enum `#[serde(rename_all = "snake_case")]`; `ReviewOfSystemsInput` has 14 systems × 2 fields; `TemplateRecord.defaultSoap` is `SoapInput`
- `src-tauri/src/commands/labs.rs` — canonical fields; `LabResultRecord.hasAbnormal` is `bool`; `DocumentUploadInput.fileSizeBytes` is `i64`
- `src/types/auth.ts` — JSDoc header style and field convention template to follow

## Expected Output

- `src/types/patient.ts` — 19 interfaces (patient + clinical combined), ~230 lines
- `src/types/scheduling.ts` — 9 interfaces, ~110 lines
- `src/types/documentation.ts` — `RosStatus` type + 14 interfaces, ~210 lines
- `src/types/labs.ts` — 11 interfaces, ~140 lines
- `npx tsc --noEmit` exits 0 with zero errors
