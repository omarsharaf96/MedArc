/**
 * Type-safe wrappers around Tauri invoke() calls.
 *
 * Each function maps to a Rust #[tauri::command] in the backend.
 * Parameter names match the Rust function parameter names exactly.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  DbStatus,
  AppInfo,
  FhirResource,
  FhirResourceList,
  CreateFhirResource,
  UpdateFhirResource,
} from "../types/fhir";

import type {
  UserResponse,
  LoginInput,
  LoginResponse,
  RegisterInput,
  SessionInfo,
  TotpSetup,
  BiometricStatus,
  BreakGlassResponse,
} from "../types/auth";

import type {
  AuditLogPage,
  AuditQuery,
  ChainVerificationResult,
} from "../types/audit";

import type {
  PatientInput,
  PatientRecord,
  PatientSummary,
  PatientSearchQuery,
  CareTeamMemberInput,
  CareTeamRecord,
  RelatedPersonInput,
  RelatedPersonRecord,
  AllergyInput,
  AllergyRecord,
  ProblemInput,
  ProblemRecord,
  MedicationInput,
  MedicationRecord,
  ImmunizationInput,
  ImmunizationRecord,
} from "../types/patient";

import type {
  AppointmentInput,
  AppointmentRecord,
  UpdateAppointmentInput,
  WaitlistInput,
  WaitlistRecord,
  RecallInput,
  RecallRecord,
  UpdateFlowStatusInput,
  FlowBoardEntry,
} from "../types/scheduling";

import type {
  EncounterInput,
  EncounterRecord,
  UpdateEncounterInput,
  VitalsInput,
  VitalsRecord,
  ReviewOfSystemsInput,
  RosRecord,
  PhysicalExamInput,
  PhysicalExamRecord,
  TemplateRecord,
  CosignRequestInput,
  CosignRecord,
  DrugAllergyAlert,
} from "../types/documentation";

import type {
  LabCatalogueInput,
  LabCatalogueRecord,
  LabOrderInput,
  LabOrderRecord,
  LabResultInput,
  LabResultRecord,
  SignLabResultInput,
  DocumentUploadInput,
  DocumentRecord,
  IntegrityCheckResult,
} from "../types/labs";

import type { BackupResult, RestoreResult, BackupLogEntry } from "../types/backup";

import type { PtNoteInput, PtNoteRecord, PtNoteType } from "../types/pt";

export const commands = {
  /** Check database encryption health status. */
  checkDb: () => invoke<DbStatus>("check_db"),

  /** Get application version and database path. */
  getAppInfo: () => invoke<AppInfo>("get_app_info"),

  /** Create a new FHIR resource. */
  createResource: (input: CreateFhirResource) =>
    invoke<FhirResource>("create_resource", { input }),

  /** Retrieve a single FHIR resource by ID. */
  getResource: (id: string) => invoke<FhirResource>("get_resource", { id }),

  /** List FHIR resources, optionally filtered by resource type. */
  listResources: (resourceType?: string) =>
    invoke<FhirResourceList>("list_resources", {
      resourceType: resourceType ?? null,
    }),

  /** Update an existing FHIR resource's JSON content. */
  updateResource: (input: UpdateFhirResource) =>
    invoke<FhirResource>("update_resource", { input }),

  /** Delete a FHIR resource by ID. */
  deleteResource: (id: string) => invoke<void>("delete_resource", { id }),

  // ─── Auth commands ───────────────────────────────────────────────

  /** Register a new user account. */
  registerUser: (input: RegisterInput) =>
    invoke<UserResponse>("register_user", {
      username: input.username,
      password: input.password,
      displayName: input.displayName,
      role: input.role,
    }),

  /** Log in with username and password. */
  login: (input: LoginInput) =>
    invoke<LoginResponse>("login", {
      username: input.username,
      password: input.password,
    }),

  /** Log out the current user. */
  logout: () => invoke<void>("logout"),

  /** Complete login after MFA verification (password was already checked). */
  completeLogin: (userId: string, totpCode: string) =>
    invoke<LoginResponse>("complete_login", {
      userId: userId,
      totpCode: totpCode,
    }),

  /** Check if this is the first run (no users exist). */
  checkFirstRun: () => invoke<boolean>("check_first_run"),

  // ─── Session commands ────────────────────────────────────────────

  /** Lock the current active session. */
  lockSession: () => invoke<void>("lock_session"),

  /** Unlock a locked session by re-entering password. */
  unlockSession: (password: string) =>
    invoke<void>("unlock_session", { password }),

  /** Refresh the session activity timestamp. */
  refreshSession: () => invoke<void>("refresh_session"),

  /** Get the current session state for the frontend. */
  getSessionState: () => invoke<SessionInfo>("get_session_state"),

  /** Get the session timeout value in minutes. */
  getSessionTimeout: () => invoke<number>("get_session_timeout"),

  // ─── MFA commands ────────────────────────────────────────────────

  /** Begin TOTP setup -- returns QR code and secret. */
  setupTotp: () => invoke<TotpSetup>("setup_totp"),

  /** Verify a TOTP code during setup to finalize enrollment. */
  verifyTotpSetup: (secretBase32: string, code: string) =>
    invoke<string>("verify_totp_setup", { secretBase32: secretBase32, code }),

  /** Disable TOTP (requires password confirmation). */
  disableTotp: (password: string) =>
    invoke<void>("disable_totp", { password }),

  /** Check a TOTP code during login (requires user_id since session may not exist yet). */
  checkTotp: (userId: string, code: string) =>
    invoke<boolean>("check_totp", { userId: userId, code }),

  /** Check biometric (Touch ID) availability and enablement. */
  checkBiometric: () => invoke<BiometricStatus>("check_biometric"),

  /** Enable Touch ID (requires password confirmation). */
  enableTouchId: (password: string) =>
    invoke<void>("enable_touch_id", { password }),

  /** Disable Touch ID. */
  disableTouchId: () => invoke<void>("disable_touch_id"),

  /** Authenticate using biometrics (Touch ID). Throws on failure or cancellation. */
  biometricAuthenticate: () => invoke<void>("biometric_authenticate", {}),

  // ─── Break-glass commands ────────────────────────────────────────

  /** Activate emergency break-glass access. */
  activateBreakGlass: (reason: string, password: string, patientId?: string) =>
    invoke<BreakGlassResponse>("activate_break_glass", {
      reason,
      password,
      patientId: patientId ?? null,
    }),

  /** Deactivate break-glass and restore original role. */
  deactivateBreakGlass: () => invoke<void>("deactivate_break_glass"),

  // ─── Audit log commands ──────────────────────────────────────────

  /**
   * Retrieve a paginated, role-scoped page of audit log entries.
   *
   * Provider: only their own entries are returned (user_id enforced backend-side).
   * SystemAdmin: all entries, with optional filters.
   * Other roles: Unauthorized error.
   */
  getAuditLog: (query?: AuditQuery) =>
    invoke<AuditLogPage>("get_audit_log", { query: query ?? null }),

  /**
   * Verify the cryptographic hash chain integrity of the full audit log.
   * SystemAdmin only. Returns { valid, rowsChecked, error }.
   */
  verifyAuditChain: () =>
    invoke<ChainVerificationResult>("verify_audit_chain_cmd"),

  // ─── Patient commands ────────────────────────────────────────────

  /** Create a new patient record. */
  createPatient: (input: PatientInput) =>
    invoke<PatientRecord>("create_patient", { input }),

  /** Retrieve a single patient by ID. */
  getPatient: (patientId: string) =>
    invoke<PatientRecord>("get_patient", { patientId }),

  /** Update an existing patient record. */
  updatePatient: (patientId: string, input: PatientInput) =>
    invoke<PatientRecord>("update_patient", { patientId, input }),

  /** Search patients by name, MRN, or date of birth. */
  searchPatients: (query: PatientSearchQuery) =>
    invoke<PatientSummary[]>("search_patients", { query }),

  /** Delete a patient record by ID. */
  deletePatient: (patientId: string) =>
    invoke<void>("delete_patient", { patientId }),

  /** Create or update a care team for a patient. */
  upsertCareTeam: (input: CareTeamMemberInput) =>
    invoke<CareTeamRecord>("upsert_care_team", { input }),

  /** Get the care team for a patient. */
  getCareTeam: (patientId: string) =>
    invoke<CareTeamRecord | null>("get_care_team", { patientId }),

  /** Add a related person (next of kin, emergency contact, guarantor) to a patient. */
  addRelatedPerson: (input: RelatedPersonInput) =>
    invoke<RelatedPersonRecord>("add_related_person", { input }),

  /** List all related persons for a patient. */
  listRelatedPersons: (patientId: string) =>
    invoke<RelatedPersonRecord[]>("list_related_persons", { patientId }),

  // ─── Clinical commands ───────────────────────────────────────────

  /** Add an allergy/intolerance record for a patient. */
  addAllergy: (input: AllergyInput) =>
    invoke<AllergyRecord>("add_allergy", { input }),

  /** List all allergies for a patient. */
  listAllergies: (patientId: string) =>
    invoke<AllergyRecord[]>("list_allergies", { patientId }),

  /** Update an existing allergy record. */
  updateAllergy: (allergyId: string, input: AllergyInput) =>
    invoke<AllergyRecord>("update_allergy", { allergyId, input }),

  /** Delete an allergy record. */
  deleteAllergy: (allergyId: string, patientId: string) =>
    invoke<void>("delete_allergy", { allergyId, patientId }),

  /** Add a problem (condition/diagnosis) to a patient's problem list. */
  addProblem: (input: ProblemInput) =>
    invoke<ProblemRecord>("add_problem", { input }),

  /** List problems for a patient, optionally filtered by clinical status. */
  listProblems: (patientId: string, statusFilter?: string | null) =>
    invoke<ProblemRecord[]>("list_problems", { patientId, statusFilter: statusFilter ?? null }),

  /** Update an existing problem record. */
  updateProblem: (problemId: string, input: ProblemInput) =>
    invoke<ProblemRecord>("update_problem", { problemId, input }),

  /** Add a medication statement for a patient. */
  addMedication: (input: MedicationInput) =>
    invoke<MedicationRecord>("add_medication", { input }),

  /** List medications for a patient, optionally filtered by status. */
  listMedications: (patientId: string, statusFilter?: string | null) =>
    invoke<MedicationRecord[]>("list_medications", { patientId, statusFilter: statusFilter ?? null }),

  /** Update an existing medication record. */
  updateMedication: (medicationId: string, input: MedicationInput) =>
    invoke<MedicationRecord>("update_medication", { medicationId, input }),

  /** Add an immunization record for a patient. */
  addImmunization: (input: ImmunizationInput) =>
    invoke<ImmunizationRecord>("add_immunization", { input }),

  /** List all immunizations for a patient. */
  listImmunizations: (patientId: string) =>
    invoke<ImmunizationRecord[]>("list_immunizations", { patientId }),

  // ─── Scheduling commands ─────────────────────────────────────────

  /**
   * Create a new appointment. Returns an array because recurring appointments
   * generate multiple records (one per occurrence).
   */
  createAppointment: (input: AppointmentInput) =>
    invoke<AppointmentRecord[]>("create_appointment", { input }),

  /** List appointments within a date range, optionally filtered by patient or provider. */
  listAppointments: (startDate: string, endDate: string, patientId?: string | null, providerId?: string | null) =>
    invoke<AppointmentRecord[]>("list_appointments", { startDate, endDate, patientId: patientId ?? null, providerId: providerId ?? null }),

  /** Update appointment details (time, status, duration, provider, etc.). */
  updateAppointment: (appointmentId: string, input: UpdateAppointmentInput) =>
    invoke<AppointmentRecord>("update_appointment", { appointmentId, input }),

  /** Cancel an appointment, optionally recording a reason. */
  cancelAppointment: (appointmentId: string, reason?: string | null) =>
    invoke<AppointmentRecord>("cancel_appointment", { appointmentId, reason: reason ?? null }),

  /** Search for open appointment slots within a date range for a provider. */
  searchOpenSlots: (startDate: string, endDate: string, providerId: string, apptType?: string | null, durationMinutes?: number | null) =>
    invoke<Record<string, unknown>[]>("search_open_slots", { startDate, endDate, providerId, apptType: apptType ?? null, durationMinutes: durationMinutes ?? null }),

  /** Update a patient's flow board status (check-in, roomed, with provider, etc.). */
  updateFlowStatus: (input: UpdateFlowStatusInput) =>
    invoke<FlowBoardEntry>("update_flow_status", { input }),

  /** Get the patient flow board for a date, optionally filtered by provider. */
  getFlowBoard: (date: string, providerId?: string | null) =>
    invoke<FlowBoardEntry[]>("get_flow_board", { date, providerId: providerId ?? null }),

  /** Add a patient to the appointment waitlist. */
  addToWaitlist: (input: WaitlistInput) =>
    invoke<WaitlistRecord>("add_to_waitlist", { input }),

  /** List waitlist entries, optionally filtered by provider, appointment type, or status. */
  listWaitlist: (providerId?: string | null, apptType?: string | null, status?: string | null) =>
    invoke<WaitlistRecord[]>("list_waitlist", { providerId: providerId ?? null, apptType: apptType ?? null, status: status ?? null }),

  /** Discharge (remove) a patient from the waitlist, optionally with a reason. */
  dischargeWaitlist: (waitlistId: string, reason?: string | null) =>
    invoke<void>("discharge_waitlist", { waitlistId, reason: reason ?? null }),

  /** Create a recall entry for a patient follow-up. */
  createRecall: (input: RecallInput) =>
    invoke<RecallRecord>("create_recall", { input }),

  /** List recalls, optionally filtered by provider, overdue status, or recall status. */
  listRecalls: (providerId?: string | null, overdueOnly?: boolean | null, status?: string | null) =>
    invoke<RecallRecord[]>("list_recalls", { providerId: providerId ?? null, overdueOnly: overdueOnly ?? null, status: status ?? null }),

  /** Mark a recall as completed, optionally with notes. */
  completeRecall: (recallId: string, notes?: string | null) =>
    invoke<void>("complete_recall", { recallId, notes: notes ?? null }),

  // ─── Documentation commands ──────────────────────────────────────

  /** Create a new clinical encounter. */
  createEncounter: (input: EncounterInput) =>
    invoke<EncounterRecord>("create_encounter", { input }),

  /** Retrieve a single encounter by ID. */
  getEncounter: (encounterId: string) =>
    invoke<EncounterRecord>("get_encounter", { encounterId }),

  /** List encounters for a patient, optionally filtered by date range and encounter type. */
  listEncounters: (patientId: string, startDate?: string | null, endDate?: string | null, encounterType?: string | null) =>
    invoke<EncounterRecord[]>("list_encounters", { patientId, startDate: startDate ?? null, endDate: endDate ?? null, encounterType: encounterType ?? null }),

  /** Update an existing encounter (status, SOAP note, chief complaint). */
  updateEncounter: (encounterId: string, input: UpdateEncounterInput) =>
    invoke<EncounterRecord>("update_encounter", { encounterId, input }),

  /** Record a vitals observation set for a patient encounter. */
  recordVitals: (input: VitalsInput) =>
    invoke<VitalsRecord>("record_vitals", { input }),

  /** List vitals for a patient, optionally scoped to a specific encounter. */
  listVitals: (patientId: string, encounterId?: string | null) =>
    invoke<VitalsRecord[]>("list_vitals", { patientId, encounterId: encounterId ?? null }),

  /** Save (create or update) a Review of Systems for an encounter. */
  saveRos: (input: ReviewOfSystemsInput) =>
    invoke<RosRecord>("save_ros", { input }),

  /** Get the Review of Systems for a specific encounter and patient. */
  getRos: (encounterId: string, patientId: string) =>
    invoke<RosRecord | null>("get_ros", { encounterId, patientId }),

  /** Save (create or update) a Physical Exam for an encounter. */
  savePhysicalExam: (input: PhysicalExamInput) =>
    invoke<PhysicalExamRecord>("save_physical_exam", { input }),

  /** Get the Physical Exam for a specific encounter and patient. */
  getPhysicalExam: (encounterId: string, patientId: string) =>
    invoke<PhysicalExamRecord | null>("get_physical_exam", { encounterId, patientId }),

  /** List available note templates, optionally filtered by specialty. */
  listTemplates: (specialty?: string | null) =>
    invoke<TemplateRecord[]>("list_templates", { specialty: specialty ?? null }),

  /** Retrieve a single note template by ID. */
  getTemplate: (templateId: string) =>
    invoke<TemplateRecord>("get_template", { templateId }),

  /** Request a co-sign from a supervising provider for an encounter. */
  requestCosign: (input: CosignRequestInput) =>
    invoke<CosignRecord>("request_cosign", { input }),

  /** Approve (sign) a pending co-sign request. */
  approveCosign: (cosignId: string) =>
    invoke<CosignRecord>("approve_cosign", { cosignId }),

  /** List pending co-sign requests, optionally filtered by supervising provider. */
  listPendingCosigns: (supervisingProviderId?: string | null) =>
    invoke<CosignRecord[]>("list_pending_cosigns", { supervisingProviderId: supervisingProviderId ?? null }),

  /** Check for drug-allergy CDS alerts for a patient's active medications. */
  checkDrugAllergyAlerts: (patientId: string) =>
    invoke<DrugAllergyAlert[]>("check_drug_allergy_alerts", { patientId }),

  // ─── Labs & Documents commands ───────────────────────────────────

  /** Add a procedure entry to the lab catalogue. */
  addLabCatalogueEntry: (input: LabCatalogueInput) =>
    invoke<LabCatalogueRecord>("add_lab_catalogue_entry", { input }),

  /** List lab catalogue entries, optionally filtered by category. */
  listLabCatalogue: (categoryFilter?: string | null) =>
    invoke<LabCatalogueRecord[]>("list_lab_catalogue", { categoryFilter: categoryFilter ?? null }),

  /** Create a lab order (ServiceRequest) for a patient. */
  createLabOrder: (input: LabOrderInput) =>
    invoke<LabOrderRecord>("create_lab_order", { input }),

  /** List lab orders for a patient, optionally filtered by order status. */
  listLabOrders: (patientId: string, statusFilter?: string | null) =>
    invoke<LabOrderRecord[]>("list_lab_orders", { patientId, statusFilter: statusFilter ?? null }),

  /** Enter lab results (DiagnosticReport) for a patient. */
  enterLabResult: (input: LabResultInput) =>
    invoke<LabResultRecord>("enter_lab_result", { input }),

  /** List lab results for a patient, optionally filtered by status or abnormal flag. */
  listLabResults: (patientId: string, statusFilter?: string | null, abnormalOnly?: boolean | null) =>
    invoke<LabResultRecord[]>("list_lab_results", { patientId, statusFilter: statusFilter ?? null, abnormalOnly: abnormalOnly ?? null }),

  /** Provider sign-off on a lab result. */
  signLabResult: (input: SignLabResultInput) =>
    invoke<LabResultRecord>("sign_lab_result", { input }),

  /** Upload a patient document (stores base64 content with SHA-1 checksum). */
  uploadDocument: (input: DocumentUploadInput) =>
    invoke<DocumentRecord>("upload_document", { input }),

  /** List documents for a patient, optionally filtered by category or title search. */
  listDocuments: (patientId: string, categoryFilter?: string | null, titleSearch?: string | null) =>
    invoke<DocumentRecord[]>("list_documents", { patientId, categoryFilter: categoryFilter ?? null, titleSearch: titleSearch ?? null }),

  /** Verify the SHA-1 integrity of a stored document against provided content. */
  verifyDocumentIntegrity: (documentId: string, contentBase64: string) =>
    invoke<IntegrityCheckResult>("verify_document_integrity", { documentId, contentBase64 }),

  // ─── Backup commands ───────────────────────────────────────────────────────

  /** Create an encrypted backup of the database at the given destination directory. */
  createBackup: (destinationPath: string) =>
    invoke<BackupResult>("create_backup", { destinationPath }),

  /** Restore a backup from the given source path (SystemAdmin only). */
  restoreBackup: (sourcePath: string, expectedSha256?: string | null) =>
    invoke<RestoreResult>("restore_backup", {
      sourcePath,
      expectedSha256: expectedSha256 ?? null,
    }),

  /** List all backup log entries (most recent first, limit 100). */
  listBackups: () => invoke<BackupLogEntry[]>("list_backups"),

  // ─── PT Note commands ────────────────────────────────────────────

  /** Create a new PT note (draft). Returns the created PtNoteRecord. */
  createPtNote: (input: PtNoteInput) =>
    invoke<PtNoteRecord>("create_pt_note", { input }),

  /** Retrieve a single PT note by ID. */
  getPtNote: (ptNoteId: string) =>
    invoke<PtNoteRecord>("get_pt_note", { ptNoteId }),

  /** List PT notes for a patient, optionally filtered by note type. */
  listPtNotes: (patientId: string, noteType?: PtNoteType | null) =>
    invoke<PtNoteRecord[]>("list_pt_notes", {
      patientId,
      noteType: noteType ?? null,
    }),

  /** Update a PT note's fields (draft only; locked notes are rejected). */
  updatePtNote: (ptNoteId: string, input: PtNoteInput) =>
    invoke<PtNoteRecord>("update_pt_note", { ptNoteId, input }),

  /** Co-sign a PT note, transitioning it from draft → signed. */
  cosignPtNote: (ptNoteId: string) =>
    invoke<PtNoteRecord>("cosign_pt_note", { ptNoteId }),

  /** Lock a signed PT note, transitioning it from signed → locked. */
  lockPtNote: (ptNoteId: string) =>
    invoke<PtNoteRecord>("lock_pt_note", { ptNoteId }),
};
