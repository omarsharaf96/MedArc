---
estimated_steps: 5
estimated_files: 1
---

# T02: Extend `src/lib/tauri.ts` with all 60 net-new command wrappers

**Slice:** S01 — Navigation Shell & Type System
**Milestone:** M002

## Description

Append five new grouped sections to the existing flat `commands` object in `src/lib/tauri.ts`, covering all 60 Rust commands not yet wrapped: patient (9), clinical (12), scheduling (13), documentation (16), and labs/documents (10). Backup commands do not exist in `lib.rs` — do not add them. The flat object structure is the locked architectural decision (Option A per DECISIONS.md). All existing wrappers must remain untouched. Invoke parameter names must be snake_case matching Rust function parameter names exactly — Tauri 2 deserializes IPC params by Rust function parameter names, not by camelCase field names.

**Critical rule**: TypeScript parameter names use camelCase; invoke object keys use the exact snake_case Rust parameter names. Example: a wrapper takes `patientId: string` as a TypeScript parameter, but passes it to invoke as `{ patient_id: patientId }`.

**Key corrected signatures from code review of Rust sources:**
- `create_appointment` returns `Vec<AppointmentRecord>` → TypeScript return type is `AppointmentRecord[]`
- `cancel_appointment` has `reason: Option<String>` param → wrapper must include `reason: reason ?? null`
- `list_lab_catalogue` Rust param is `category_filter` (not `category`)
- `list_lab_orders` Rust param is `status_filter` (not `status`)
- `get_ros` requires BOTH `encounter_id: String` AND `patient_id: String` params
- `get_physical_exam` requires BOTH `encounter_id: String` AND `patient_id: String` params
- `list_vitals` has an optional `encounter_id: Option<String>` second param
- `list_encounters` has optional `start_date`, `end_date`, `encounter_type` params
- `list_recalls` params are `provider_id`, `overdue_only`, `status` — no `patient_id`
- `list_waitlist` params are `provider_id`, `appt_type`, `status` — no `patient_id`
- `list_pending_cosigns` takes `supervising_provider_id: Option<String>` (not required)
- `discharge_waitlist` has `reason: Option<String>` second param

## Steps

1. **Add import statements** at the top of `src/lib/tauri.ts` for all types needed by the new wrappers. Import only what is used — `noUnusedLocals` is enforced. Group imports by file:
   - From `"../types/patient"`: `PatientInput`, `PatientRecord`, `PatientSummary`, `PatientSearchQuery`, `CareTeamMemberInput`, `CareTeamRecord`, `RelatedPersonInput`, `RelatedPersonRecord`, `AllergyInput`, `AllergyRecord`, `ProblemInput`, `ProblemRecord`, `MedicationInput`, `MedicationRecord`, `ImmunizationInput`, `ImmunizationRecord`
   - From `"../types/scheduling"`: `AppointmentInput`, `AppointmentRecord`, `UpdateAppointmentInput`, `WaitlistInput`, `WaitlistRecord`, `RecallInput`, `RecallRecord`, `UpdateFlowStatusInput`, `FlowBoardEntry`
   - From `"../types/documentation"`: `EncounterInput`, `EncounterRecord`, `UpdateEncounterInput`, `VitalsInput`, `VitalsRecord`, `ReviewOfSystemsInput`, `RosRecord`, `PhysicalExamInput`, `PhysicalExamRecord`, `TemplateRecord`, `CosignRequestInput`, `CosignRecord`, `DrugAllergyAlert`
   - From `"../types/labs"`: `LabCatalogueInput`, `LabCatalogueRecord`, `LabOrderInput`, `LabOrderRecord`, `LabResultInput`, `LabResultRecord`, `SignLabResultInput`, `DocumentUploadInput`, `DocumentRecord`, `IntegrityCheckResult`

2. **Append `// ─── Patient commands ───` section** (9 wrappers). Exact invoke param mapping derived from Rust function signatures in `patient.rs`:
   ```typescript
   createPatient: (input: PatientInput) =>
     invoke<PatientRecord>("create_patient", { input }),
   getPatient: (patientId: string) =>
     invoke<PatientRecord>("get_patient", { patient_id: patientId }),
   updatePatient: (patientId: string, input: PatientInput) =>
     invoke<PatientRecord>("update_patient", { patient_id: patientId, input }),
   searchPatients: (query: PatientSearchQuery) =>
     invoke<PatientSummary[]>("search_patients", { query }),
   deletePatient: (patientId: string) =>
     invoke<void>("delete_patient", { patient_id: patientId }),
   upsertCareTeam: (input: CareTeamMemberInput) =>
     invoke<CareTeamRecord>("upsert_care_team", { input }),
   getCareTeam: (patientId: string) =>
     invoke<CareTeamRecord | null>("get_care_team", { patient_id: patientId }),
   addRelatedPerson: (input: RelatedPersonInput) =>
     invoke<RelatedPersonRecord>("add_related_person", { input }),
   listRelatedPersons: (patientId: string) =>
     invoke<RelatedPersonRecord[]>("list_related_persons", { patient_id: patientId }),
   ```

3. **Append `// ─── Clinical commands ───` section** (12 wrappers). Key param names from `clinical.rs`:
   ```typescript
   addAllergy: (input: AllergyInput) =>
     invoke<AllergyRecord>("add_allergy", { input }),
   listAllergies: (patientId: string) =>
     invoke<AllergyRecord[]>("list_allergies", { patient_id: patientId }),
   updateAllergy: (allergyId: string, input: AllergyInput) =>
     invoke<AllergyRecord>("update_allergy", { allergy_id: allergyId, input }),
   deleteAllergy: (allergyId: string, patientId: string) =>
     invoke<void>("delete_allergy", { allergy_id: allergyId, patient_id: patientId }),
   addProblem: (input: ProblemInput) =>
     invoke<ProblemRecord>("add_problem", { input }),
   listProblems: (patientId: string, statusFilter?: string | null) =>
     invoke<ProblemRecord[]>("list_problems", { patient_id: patientId, status_filter: statusFilter ?? null }),
   updateProblem: (problemId: string, input: ProblemInput) =>
     invoke<ProblemRecord>("update_problem", { problem_id: problemId, input }),
   addMedication: (input: MedicationInput) =>
     invoke<MedicationRecord>("add_medication", { input }),
   listMedications: (patientId: string, statusFilter?: string | null) =>
     invoke<MedicationRecord[]>("list_medications", { patient_id: patientId, status_filter: statusFilter ?? null }),
   updateMedication: (medicationId: string, input: MedicationInput) =>
     invoke<MedicationRecord>("update_medication", { medication_id: medicationId, input }),
   addImmunization: (input: ImmunizationInput) =>
     invoke<ImmunizationRecord>("add_immunization", { input }),
   listImmunizations: (patientId: string) =>
     invoke<ImmunizationRecord[]>("list_immunizations", { patient_id: patientId }),
   ```

4. **Append `// ─── Scheduling commands ───` section** (13 wrappers). All scheduling commands are `async fn` in Rust — invoke() handles this transparently. Key corrected signatures:
   ```typescript
   // NOTE: create_appointment returns Vec<AppointmentRecord> (recurring generates multiple)
   createAppointment: (input: AppointmentInput) =>
     invoke<AppointmentRecord[]>("create_appointment", { input }),
   listAppointments: (startDate: string, endDate: string, patientId?: string | null, providerId?: string | null) =>
     invoke<AppointmentRecord[]>("list_appointments", { start_date: startDate, end_date: endDate, patient_id: patientId ?? null, provider_id: providerId ?? null }),
   updateAppointment: (appointmentId: string, input: UpdateAppointmentInput) =>
     invoke<AppointmentRecord>("update_appointment", { appointment_id: appointmentId, input }),
   // NOTE: cancel_appointment has a reason param and returns AppointmentRecord
   cancelAppointment: (appointmentId: string, reason?: string | null) =>
     invoke<AppointmentRecord>("cancel_appointment", { appointment_id: appointmentId, reason: reason ?? null }),
   searchOpenSlots: (startDate: string, endDate: string, providerId: string, apptType?: string | null, durationMinutes?: number | null) =>
     invoke<Record<string, unknown>[]>("search_open_slots", { start_date: startDate, end_date: endDate, provider_id: providerId, appt_type: apptType ?? null, duration_minutes: durationMinutes ?? null }),
   updateFlowStatus: (input: UpdateFlowStatusInput) =>
     invoke<FlowBoardEntry>("update_flow_status", { input }),
   getFlowBoard: (date: string, providerId?: string | null) =>
     invoke<FlowBoardEntry[]>("get_flow_board", { date, provider_id: providerId ?? null }),
   addToWaitlist: (input: WaitlistInput) =>
     invoke<WaitlistRecord>("add_to_waitlist", { input }),
   // NOTE: list_waitlist params are provider_id, appt_type, status (no patient_id)
   listWaitlist: (providerId?: string | null, apptType?: string | null, status?: string | null) =>
     invoke<WaitlistRecord[]>("list_waitlist", { provider_id: providerId ?? null, appt_type: apptType ?? null, status: status ?? null }),
   // NOTE: discharge_waitlist has reason param
   dischargeWaitlist: (waitlistId: string, reason?: string | null) =>
     invoke<void>("discharge_waitlist", { waitlist_id: waitlistId, reason: reason ?? null }),
   createRecall: (input: RecallInput) =>
     invoke<RecallRecord>("create_recall", { input }),
   // NOTE: list_recalls params are provider_id, overdue_only, status (no patient_id)
   listRecalls: (providerId?: string | null, overdueOnly?: boolean | null, status?: string | null) =>
     invoke<RecallRecord[]>("list_recalls", { provider_id: providerId ?? null, overdue_only: overdueOnly ?? null, status: status ?? null }),
   // NOTE: complete_recall has notes param
   completeRecall: (recallId: string, notes?: string | null) =>
     invoke<void>("complete_recall", { recall_id: recallId, notes: notes ?? null }),
   ```

5. **Append `// ─── Documentation commands ───` section** (16 wrappers) and **`// ─── Labs & Documents commands ───` section** (10 wrappers). Key corrected signatures:
   ```typescript
   createEncounter: (input: EncounterInput) =>
     invoke<EncounterRecord>("create_encounter", { input }),
   getEncounter: (encounterId: string) =>
     invoke<EncounterRecord>("get_encounter", { encounter_id: encounterId }),
   // NOTE: list_encounters has optional date/type filters
   listEncounters: (patientId: string, startDate?: string | null, endDate?: string | null, encounterType?: string | null) =>
     invoke<EncounterRecord[]>("list_encounters", { patient_id: patientId, start_date: startDate ?? null, end_date: endDate ?? null, encounter_type: encounterType ?? null }),
   updateEncounter: (encounterId: string, input: UpdateEncounterInput) =>
     invoke<EncounterRecord>("update_encounter", { encounter_id: encounterId, input }),
   recordVitals: (input: VitalsInput) =>
     invoke<VitalsRecord>("record_vitals", { input }),
   // NOTE: list_vitals has optional encounter_id param
   listVitals: (patientId: string, encounterId?: string | null) =>
     invoke<VitalsRecord[]>("list_vitals", { patient_id: patientId, encounter_id: encounterId ?? null }),
   saveRos: (input: ReviewOfSystemsInput) =>
     invoke<RosRecord>("save_ros", { input }),
   // NOTE: get_ros requires BOTH encounter_id AND patient_id
   getRos: (encounterId: string, patientId: string) =>
     invoke<RosRecord | null>("get_ros", { encounter_id: encounterId, patient_id: patientId }),
   savePhysicalExam: (input: PhysicalExamInput) =>
     invoke<PhysicalExamRecord>("save_physical_exam", { input }),
   // NOTE: get_physical_exam requires BOTH encounter_id AND patient_id
   getPhysicalExam: (encounterId: string, patientId: string) =>
     invoke<PhysicalExamRecord | null>("get_physical_exam", { encounter_id: encounterId, patient_id: patientId }),
   listTemplates: (specialty?: string | null) =>
     invoke<TemplateRecord[]>("list_templates", { specialty: specialty ?? null }),
   getTemplate: (templateId: string) =>
     invoke<TemplateRecord>("get_template", { template_id: templateId }),
   requestCosign: (input: CosignRequestInput) =>
     invoke<CosignRecord>("request_cosign", { input }),
   approveCosign: (cosignId: string) =>
     invoke<CosignRecord>("approve_cosign", { cosign_id: cosignId }),
   // NOTE: list_pending_cosigns takes optional supervising_provider_id
   listPendingCosigns: (supervisingProviderId?: string | null) =>
     invoke<CosignRecord[]>("list_pending_cosigns", { supervising_provider_id: supervisingProviderId ?? null }),
   checkDrugAllergyAlerts: (patientId: string) =>
     invoke<DrugAllergyAlert[]>("check_drug_allergy_alerts", { patient_id: patientId }),
   // Labs & Documents:
   addLabCatalogueEntry: (input: LabCatalogueInput) =>
     invoke<LabCatalogueRecord>("add_lab_catalogue_entry", { input }),
   // NOTE: list_lab_catalogue Rust param is category_filter (not category)
   listLabCatalogue: (categoryFilter?: string | null) =>
     invoke<LabCatalogueRecord[]>("list_lab_catalogue", { category_filter: categoryFilter ?? null }),
   createLabOrder: (input: LabOrderInput) =>
     invoke<LabOrderRecord>("create_lab_order", { input }),
   // NOTE: list_lab_orders Rust param is status_filter (not status)
   listLabOrders: (patientId: string, statusFilter?: string | null) =>
     invoke<LabOrderRecord[]>("list_lab_orders", { patient_id: patientId, status_filter: statusFilter ?? null }),
   enterLabResult: (input: LabResultInput) =>
     invoke<LabResultRecord>("enter_lab_result", { input }),
   listLabResults: (patientId: string, statusFilter?: string | null, abnormalOnly?: boolean | null) =>
     invoke<LabResultRecord[]>("list_lab_results", { patient_id: patientId, status_filter: statusFilter ?? null, abnormal_only: abnormalOnly ?? null }),
   signLabResult: (input: SignLabResultInput) =>
     invoke<LabResultRecord>("sign_lab_result", { input }),
   uploadDocument: (input: DocumentUploadInput) =>
     invoke<DocumentRecord>("upload_document", { input }),
   listDocuments: (patientId: string, categoryFilter?: string | null, titleSearch?: string | null) =>
     invoke<DocumentRecord[]>("list_documents", { patient_id: patientId, category_filter: categoryFilter ?? null, title_search: titleSearch ?? null }),
   verifyDocumentIntegrity: (documentId: string, contentBase64: string) =>
     invoke<IntegrityCheckResult>("verify_document_integrity", { document_id: documentId, content_base64: contentBase64 }),
   ```

## Must-Haves

- [ ] All 60 new wrappers are inside the existing `commands` object (flat, not namespaced)
- [ ] All existing wrappers (health, FHIR, auth, session, MFA, break-glass, audit) are unchanged
- [ ] All new type imports added at top of file — no unresolved type references
- [ ] Invoke parameter names are snake_case matching Rust function parameter names exactly (not camelCase)
- [ ] Optional parameters are passed as `param ?? null` to invoke (not `undefined` — Tauri expects explicit null)
- [ ] `createAppointment` return type is `AppointmentRecord[]` (not `AppointmentRecord` — recurring creates multiple)
- [ ] `cancelAppointment` includes `reason: reason ?? null` in invoke params and returns `AppointmentRecord` (not `void`)
- [ ] `listLabCatalogue` invoke uses `category_filter` (not `category`)
- [ ] `listLabOrders` invoke uses `status_filter` (not `status`)
- [ ] `getRos` invoke passes both `encounter_id` AND `patient_id`
- [ ] `getPhysicalExam` invoke passes both `encounter_id` AND `patient_id`
- [ ] `listVitals` invoke includes optional `encounter_id: encounterId ?? null`
- [ ] `listEncounters` invoke includes optional `start_date`, `end_date`, `encounter_type`
- [ ] `listRecalls` invoke uses `provider_id`, `overdue_only`, `status` (no `patient_id`)
- [ ] `listWaitlist` invoke uses `provider_id`, `appt_type`, `status` (no `patient_id`)
- [ ] `listPendingCosigns` invoke passes `supervising_provider_id: supervisingProviderId ?? null`
- [ ] No backup command wrappers (backup commands are not in lib.rs)
- [ ] `tsc --noEmit` exits 0 after all wrappers are added

## Verification

- `npx tsc --noEmit 2>&1` — expect zero errors
- `grep "patient_id\|provider_id\|encounter_id" src/lib/tauri.ts | grep "invoke" | head -10` — confirms snake_case in invoke keys
- `grep -c "^  [a-zA-Z]" src/lib/tauri.ts` — count wrappers; expect 88+ (60 new + ~28 existing)
- `grep "patientId\|providerId" src/lib/tauri.ts | grep "invoke" | head -5` — should return nothing (camelCase must NOT appear as invoke object keys)
- `grep "category_filter\|status_filter\|encounter_id.*patient_id" src/lib/tauri.ts` — confirms corrected param names

## Observability Impact

- Signals added/changed: None at runtime — compile-time wrappers only; runtime IPC errors surface as rejected Promises with Tauri's standard error format
- How a future agent inspects this: If a wrapper passes `patientId` (camelCase) to invoke instead of `patient_id` (snake_case), Tauri silently passes `null` to Rust — caught by verifying invoke key names against Rust fn signatures; `tsc --noEmit` does NOT catch snake_case mistakes (both are valid JS keys), so manual grep is required; missing optional params (e.g. `encounter_id` in `get_ros`) will cause the Rust handler to error with a missing field message at runtime
- Failure state exposed: Type mismatches between wrapper return types and actual Rust output surface as runtime type errors in page components — caught at compile time if TypeScript types are accurate mirrors of Rust structs

## Inputs

- `src/types/patient.ts`, `src/types/scheduling.ts`, `src/types/documentation.ts`, `src/types/labs.ts` — from T01 (required; must exist before this task)
- `src-tauri/src/commands/patient.rs`, `clinical.rs`, `scheduling.rs`, `documentation.rs`, `labs.rs` — Rust source to confirm exact function parameter names (snake_case); must be consulted to verify every invoke param key
- `src/lib/tauri.ts` — existing file to extend (append only); reference existing wrappers for style consistency
- `.gsd/DECISIONS.md` S01 entry: "Passed resource_type as snake_case in invoke() params" and "Flat (Option A) structure for commands object"

## Expected Output

- `src/lib/tauri.ts` — extended with 60 new wrappers in 5 new sections, all existing wrappers intact, all 4 new type import groups added, `tsc --noEmit` exits 0
- `grep -c "^  [a-zA-Z]" src/lib/tauri.ts` returns 88 or higher
