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

import type {
  PtNoteInput,
  PtNoteRecord,
  PtNoteType,
  MeasureType,
  OutcomeScoreInput,
  OutcomeScoreRecord,
  ObjectiveMeasuresInput,
  ObjectiveMeasuresRecord,
  OutcomeComparison,
} from "../types/pt";

import type {
  AudioLevel,
  MicrophoneStatus,
  StartRecordingResult,
  StopRecordingResult,
  TranscriptionResult,
  WhisperModelInfo,
  WhisperModelSize,
  OllamaStatus,
  NoteDraftResult,
  CptSuggestion,
  ExtractedObjectiveData,
  PatientContext,
  LlmSettings,
  LlmSettingsInput,
} from "../types/ai";

import type { PdfExportResult } from "../types/export";

import type {
  FaxRecord,
  FaxContact,
  FaxContactInput,
  SendFaxInput,
  PhaxioConfigInput,
  PhaxioConfigRecord,
  FaxDirection,
  FaxStatus,
  FaxContactType,
} from "../types/fax";

import type {
  DocumentCategory,
  CategorizedDocumentInput,
  CategorizedDocument,
  SurveyTemplateInput,
  SurveyTemplate,
  SurveyResponseInput,
  SurveyResponse,
  ReferralInput,
  ReferralRecord,
} from "../types/documents";

import type {
  AuthRecordInput,
  AuthRecord,
  AuthAlert,
} from "../types/auth-tracking";

import type {
  Exercise,
  ExerciseRegion,
  ExerciseCategory,
  ExercisePrescription,
  HEPProgram,
  HEPTemplate,
  CreateHepProgramInput,
  CreateHepTemplateInput,
} from "../types/hep";

import type {
  CptCode,
  BillingRule,
  ServiceMinutes,
  UnitCalculationResult,
  FeeScheduleInput,
  FeeScheduleEntry,
  SaveEncounterBillingInput,
  EncounterBilling,
} from "../types/billing";

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
    invoke<BackupResult>("create_backup", { destination_path: destinationPath }),

  /** Restore a backup from the given source path (SystemAdmin only). */
  restoreBackup: (sourcePath: string, expectedSha256?: string | null) =>
    invoke<RestoreResult>("restore_backup", {
      source_path: sourcePath,
      expected_sha256: expectedSha256 ?? null,
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

  // ─── M003/S02 — Objective Measures & Outcome Scores ──────────────

  /** Record objective measures (ROM, MMT, ortho tests) for a patient encounter. */
  recordObjectiveMeasures: (input: ObjectiveMeasuresInput) =>
    invoke<ObjectiveMeasuresRecord>("record_objective_measures", { input }),

  /** Retrieve objective measures for a patient, optionally scoped to an encounter. */
  getObjectiveMeasures: (patientId: string, encounterId?: string | null) =>
    invoke<ObjectiveMeasuresRecord[]>("get_objective_measures", {
      patientId,
      encounterId: encounterId ?? null,
    }),

  /** Record and score an outcome measure for a patient. */
  recordOutcomeScore: (input: OutcomeScoreInput) =>
    invoke<OutcomeScoreRecord>("record_outcome_score", { input }),

  /** List outcome scores for a patient, optionally filtered by measure type and date range. */
  listOutcomeScores: (
    patientId: string,
    measureType?: MeasureType | null,
    startDate?: string | null,
    endDate?: string | null,
  ) =>
    invoke<OutcomeScoreRecord[]>("list_outcome_scores", {
      patientId,
      measureType: measureType ?? null,
      startDate: startDate ?? null,
      endDate: endDate ?? null,
    }),

  /** Get a single outcome score by ID. */
  getOutcomeScore: (scoreId: string) =>
    invoke<OutcomeScoreRecord>("get_outcome_score", { scoreId }),

  /** Get outcome comparison (earliest vs latest per measure type) for a patient. */
  getOutcomeComparison: (patientId: string) =>
    invoke<OutcomeComparison>("get_outcome_comparison", { patientId }),

  // ─── M003/S03 — Audio Capture & Transcription ────────────────────

  /** Start recording audio from the microphone. Returns a recording ID. */
  startAudioRecording: () =>
    invoke<StartRecordingResult>("start_audio_recording"),

  /** Stop an active recording. Returns the path to the WAV file. */
  stopAudioRecording: (recordingId: string) =>
    invoke<StopRecordingResult>("stop_audio_recording", { recordingId }),

  /** Get the current audio amplitude level (0.0–1.0) for the visualizer. */
  getAudioLevel: () =>
    invoke<AudioLevel>("get_audio_level"),

  /** Check whether a microphone is available on the system. */
  checkMicrophoneAvailable: () =>
    invoke<MicrophoneStatus>("check_microphone_available"),

  /** Transcribe a WAV audio file using Whisper. Deletes the WAV after success. */
  transcribeAudio: (wavPath: string, modelSize?: WhisperModelSize | null) =>
    invoke<TranscriptionResult>("transcribe_audio", {
      wavPath,
      modelSize: modelSize ?? null,
    }),

  /** Check if a Whisper model is downloaded and ready to use. */
  checkWhisperModel: (modelSize?: WhisperModelSize | null) =>
    invoke<WhisperModelInfo>("check_whisper_model", {
      modelSize: modelSize ?? null,
    }),

  /** Download a Whisper model to the app support directory. */
  downloadWhisperModel: (modelSize?: WhisperModelSize | null) =>
    invoke<WhisperModelInfo>("download_whisper_model", {
      modelSize: modelSize ?? null,
    }),

  // ─── M003/S03 — LLM Integration (Ollama + Bedrock) ────────────────

  /** Check if Ollama is running and list available models. */
  checkOllamaStatus: () =>
    invoke<OllamaStatus>("check_ollama_status"),

  /** Generate a PT note draft from a session transcript. */
  generateNoteDraft: (transcript: string, noteType: string, patientContext?: PatientContext | null) =>
    invoke<NoteDraftResult>("generate_note_draft", {
      transcript,
      noteType,
      patientContext: patientContext ?? null,
    }),

  /** Suggest CPT codes based on a note's text content. */
  suggestCptCodes: (noteText: string) =>
    invoke<CptSuggestion[]>("suggest_cpt_codes", { noteText }),

  /** Extract objective data (ROM, pain, MMT) from a session transcript. */
  extractObjectiveData: (transcript: string, patientId?: string | null) =>
    invoke<ExtractedObjectiveData>("extract_objective_data", {
      transcript,
      patientId: patientId ?? null,
    }),

  /** Configure LLM provider, model, and credentials. */
  configureLlmSettings: (input: LlmSettingsInput) =>
    invoke<LlmSettings>("configure_llm_settings", { input }),

  // ─── M003/S05 — PDF Export & Report Generation ───────────────────

  /** Generate a single note PDF. */
  generateNotePdf: (ptNoteId: string) =>
    invoke<PdfExportResult>("generate_note_pdf", { ptNoteId }),

  /** Generate a progress report PDF. */
  generateProgressReport: (patientId: string, startDate?: string | null, endDate?: string | null) =>
    invoke<PdfExportResult>("generate_progress_report", {
      patientId,
      startDate: startDate ?? null,
      endDate: endDate ?? null,
    }),

  /** Generate an insurance narrative PDF. */
  generateInsuranceNarrative: (patientId: string) =>
    invoke<PdfExportResult>("generate_insurance_narrative", { patientId }),

  /** Generate a legal/IME report PDF. */
  generateLegalReport: (patientId: string) =>
    invoke<PdfExportResult>("generate_legal_report", { patientId }),

  /** Generate a full chart export PDF. */
  generateChartExport: (patientId: string, startDate?: string | null, endDate?: string | null) =>
    invoke<PdfExportResult>("generate_chart_export", {
      patientId,
      startDate: startDate ?? null,
      endDate: endDate ?? null,
    }),

  // ─── M003/S04 — Document Center commands ──────────────────────────

  /** Upload a document with a PT-specific category. */
  uploadCategorizedDocument: (input: CategorizedDocumentInput) =>
    invoke<CategorizedDocument>("upload_categorized_document", { input }),

  /** List patient documents, optionally filtered by category and sorted. */
  listPatientDocuments: (patientId: string, category?: DocumentCategory | null, sortBy?: string | null) =>
    invoke<CategorizedDocument[]>("list_patient_documents", { patientId, category: category ?? null, sortBy: sortBy ?? null }),

  /** Retrieve a single document by ID with metadata. */
  getDocument: (documentId: string) =>
    invoke<CategorizedDocument>("get_document", { documentId }),

  /** Update the category of a document. */
  updateDocumentCategory: (documentId: string, category: DocumentCategory) =>
    invoke<CategorizedDocument>("update_document_category", { documentId, category }),

  /** Soft-delete a document (marks FHIR status as entered-in-error). */
  deleteDocument: (documentId: string) =>
    invoke<void>("delete_document", { documentId }),

  /** Create a custom survey template. */
  createSurveyTemplate: (input: SurveyTemplateInput) =>
    invoke<SurveyTemplate>("create_survey_template", { input }),

  /** List all survey templates including built-in ones. */
  listSurveyTemplates: () =>
    invoke<SurveyTemplate[]>("list_survey_templates"),

  /** Get a single survey template by ID. */
  getSurveyTemplate: (templateId: string) =>
    invoke<SurveyTemplate>("get_survey_template", { templateId }),

  /** Submit a survey response for a patient. */
  submitSurveyResponse: (input: SurveyResponseInput) =>
    invoke<SurveyResponse>("submit_survey_response", { input }),

  /** List all survey responses for a patient. */
  listSurveyResponses: (patientId: string) =>
    invoke<SurveyResponse[]>("list_survey_responses", { patientId }),

  /** Get a single survey response by ID. */
  getSurveyResponse: (responseId: string) =>
    invoke<SurveyResponse>("get_survey_response", { responseId }),

  /** Create a referral record for a patient. */
  createReferral: (input: ReferralInput) =>
    invoke<ReferralRecord>("create_referral", { input }),

  /** Get a single referral by ID. */
  getReferral: (referralId: string) =>
    invoke<ReferralRecord>("get_referral", { referralId }),

  /** List all referrals for a patient. */
  listReferrals: (patientId: string) =>
    invoke<ReferralRecord[]>("list_referrals", { patientId }),

  /** Update a referral record. */
  updateReferral: (referralId: string, input: ReferralInput) =>
    invoke<ReferralRecord>("update_referral", { referralId, input }),

  // ─── M003/S06 — Fax Integration (Phaxio) ─────────────────────────

  /** Configure Phaxio API credentials. */
  configurePhaxio: (input: PhaxioConfigInput) =>
    invoke<PhaxioConfigRecord>("configure_phaxio", { input }),

  /** Test Phaxio API connection. */
  testPhaxioConnection: () =>
    invoke<{ success: boolean; message: string }>("test_phaxio_connection"),

  /** Send a fax via Phaxio. */
  sendFax: (input: SendFaxInput) =>
    invoke<FaxRecord>("send_fax", { input }),

  /** Poll for received faxes from Phaxio. */
  pollReceivedFaxes: () =>
    invoke<FaxRecord[]>("poll_received_faxes"),

  /** Create a fax contact. */
  createFaxContact: (input: FaxContactInput) =>
    invoke<FaxContact>("create_fax_contact", { input }),

  /** List fax contacts, optionally filtered by type. */
  listFaxContacts: (contactType?: FaxContactType | null) =>
    invoke<FaxContact[]>("list_fax_contacts", { contactType: contactType ?? null }),

  /** Update a fax contact. */
  updateFaxContact: (contactId: string, input: FaxContactInput) =>
    invoke<FaxContact>("update_fax_contact", { contactId, input }),

  /** Delete a fax contact. */
  deleteFaxContact: (contactId: string) =>
    invoke<void>("delete_fax_contact", { contactId }),

  /** List fax log entries with optional filters. */
  listFaxLog: (patientId?: string | null, direction?: FaxDirection | null, status?: FaxStatus | null) =>
    invoke<FaxRecord[]>("list_fax_log", { patientId: patientId ?? null, direction: direction ?? null, status: status ?? null }),

  /** Get fax delivery status from Phaxio. */
  getFaxStatus: (faxId: string) =>
    invoke<FaxRecord>("get_fax_status", { faxId }),

  /** Retry a failed fax. */
  retryFax: (faxId: string) =>
    invoke<FaxRecord>("retry_fax", { faxId }),

  // ─── M003/S07 — Authorization & Visit Tracking ──────────────────

  /** Create a new authorization record for a patient. */
  createAuthRecord: (input: AuthRecordInput) =>
    invoke<AuthRecord>("create_auth_record", { input }),

  /** Retrieve a single authorization record by ID. */
  getAuthRecord: (authId: string) =>
    invoke<AuthRecord>("get_auth_record", { authId }),

  /** List all authorization records for a patient. */
  listAuthRecords: (patientId: string) =>
    invoke<AuthRecord[]>("list_auth_records", { patientId }),

  /** Update an existing authorization record. */
  updateAuthRecord: (authId: string, input: AuthRecordInput) =>
    invoke<AuthRecord>("update_auth_record", { authId, input }),

  /** Increment visit count for active auth records (called on note co-sign). */
  incrementVisitCount: (patientId: string) =>
    invoke<AuthRecord[]>("increment_visit_count", { patientId }),

  /** Get active alerts for a patient's auth records. */
  getAuthAlerts: (patientId: string) =>
    invoke<AuthAlert[]>("get_auth_alerts", { patientId }),

  /** Generate a pre-filled re-authorization request letter. */
  generateReauthLetter: (authId: string, patientId: string) =>
    invoke<string>("generate_reauth_letter", { authId, patientId }),

  // ─── M003/S02 — HEP Builder ──────────────────────────────────────

  /** List exercises from the library, optionally filtered by body region and category. */
  listExercises: (bodyRegion?: ExerciseRegion | null, category?: ExerciseCategory | null) =>
    invoke<Exercise[]>("list_exercises", {
      bodyRegion: bodyRegion ?? null,
      category: category ?? null,
    }),

  /** Search exercises by name or description (case-insensitive). */
  searchExercises: (query: string) =>
    invoke<Exercise[]>("search_exercises", { query }),

  /** Create a new HEP program for a patient. */
  createHepProgram: (input: CreateHepProgramInput) =>
    invoke<HEPProgram>("create_hep_program", { input }),

  /** Retrieve a single HEP program by ID. */
  getHepProgram: (programId: string) =>
    invoke<HEPProgram>("get_hep_program", { programId }),

  /** List all HEP programs for a patient. */
  listHepPrograms: (patientId: string) =>
    invoke<HEPProgram[]>("list_hep_programs", { patientId }),

  /** Update an existing HEP program's exercise list. */
  updateHepProgram: (programId: string, exercises: ExercisePrescription[], notes?: string | null) =>
    invoke<HEPProgram>("update_hep_program", {
      programId,
      exercises,
      notes: notes ?? null,
    }),

  /** Save a HEP program as a reusable template. */
  createHepTemplate: (input: CreateHepTemplateInput) =>
    invoke<HEPTemplate>("create_hep_template", { input }),

  /** List all HEP templates (built-in and user-created). */
  listHepTemplates: () =>
    invoke<HEPTemplate[]>("list_hep_templates"),

  /** Get a single HEP template by ID. */
  getHepTemplate: (templateId: string) =>
    invoke<HEPTemplate>("get_hep_template", { templateId }),

  // M004/S01 — CPT Billing Engine

  /**
   * List all PT CPT codes from the built-in library.
   * Optionally filter by category: "evaluation" | "timed" | "untimed"
   */
  listCptCodes: (category?: string) =>
    invoke<CptCode[]>("list_cpt_codes", { category: category ?? null }),

  /**
   * Calculate billing units for a set of timed services using the specified rule.
   * Untimed codes (97010, G0283, 97150) should be excluded from this call.
   */
  calculateBillingUnits: (services: ServiceMinutes[], ruleType: BillingRule) =>
    invoke<UnitCalculationResult[]>("calculate_billing_units", {
      services,
      ruleType,
    }),

  /**
   * Create a new fee schedule entry for a payer / CPT code combination.
   * Pass payerId = null for the self-pay default schedule.
   */
  createFeeScheduleEntry: (input: FeeScheduleInput) =>
    invoke<FeeScheduleEntry>("create_fee_schedule_entry", { input }),

  /**
   * List fee schedule entries, optionally filtered by payer ID.
   * Pass payerId = null to retrieve the self-pay / default schedule.
   */
  listFeeSchedule: (payerId?: string | null) =>
    invoke<FeeScheduleEntry[]>("list_fee_schedule", {
      payerId: payerId ?? null,
    }),

  /**
   * Get the complete billing summary (header + line items) for an encounter.
   * Returns an error if no billing record exists for the encounter.
   */
  getEncounterBillingSummary: (encounterId: string) =>
    invoke<EncounterBilling>("get_encounter_billing_summary", { encounterId }),

  /**
   * Save (create or replace) billing data for an encounter.
   * Idempotent: if a billing record already exists it is replaced.
   * Totals are computed server-side from the provided line items.
   */
  saveEncounterBilling: (input: SaveEncounterBillingInput) =>
    invoke<EncounterBilling>("save_encounter_billing", { input }),
};
