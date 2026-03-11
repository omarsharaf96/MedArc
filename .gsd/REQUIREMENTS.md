# Requirements

## Active

### AUDT-03 — Audit logs are retained for minimum 6 years

- Status: active
- Class: core-capability
- Source: inferred
- Primary Slice: S03

Audit logs are retained for minimum 6 years



### PTNT-08 — User can track patient allergies with drug, food, environmental categories, severity, and reaction type (FHIR AllergyIntolerance)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S05

User can track patient allergies with drug, food, environmental categories, severity, and reaction type (FHIR AllergyIntolerance). Proven by S05: `add_allergy`, `list_allergies`, `update_allergy`, `delete_allergy` commands create and manage FHIR AllergyIntolerance resources with RxNorm-coded substances, category (drug/food/environment/biologic), severity, and reaction. Test `ptnt_08_allergy_intolerance_fhir_complete` asserts all required fields.

### PTNT-09 — User can maintain active problem list with ICD-10 coded diagnoses (active/inactive/resolved status)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S05

User can maintain active problem list with ICD-10 coded diagnoses (active/inactive/resolved status). Proven by S05: `add_problem`, `list_problems`, `update_problem` commands manage FHIR Condition resources with ICD-10-CM coded diagnoses, problem-list-item category, and active/inactive/resolved clinical status with optional abatement date. Test `ptnt_09_condition_fhir_with_icd10_and_status` asserts all required fields.

### PTNT-10 — User can maintain medication list (active, discontinued, historical) linked to RxNorm codes

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S05

User can maintain medication list (active, discontinued, historical) linked to RxNorm codes. Proven by S05: `add_medication`, `list_medications`, `update_medication` commands manage FHIR MedicationStatement resources with RxNorm-coded drugs, status lifecycle (active/completed/stopped/on-hold/unknown/not-taken), dosage text, and effective period. Test `ptnt_10_medication_statement_fhir_complete` asserts all required fields.

### PTNT-11 — User can record immunization history with CVX codes, lot numbers, administration dates

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S05

User can record immunization history with CVX codes, lot numbers, administration dates. Proven by S05: `add_immunization`, `list_immunizations` commands manage FHIR Immunization resources with CVX-coded vaccines, lot numbers, expiration dates, administration dates, site, route, and dose number. Test `ptnt_11_immunization_fhir_complete` asserts all required fields.

### SCHD-01 — User can view multi-provider calendar in day, week, and month views

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can view multi-provider calendar in day, week, and month views. Proven by S06: `list_appointments(start_date, end_date, provider_id?)` queries `appointment_index` with date-range and optional provider filter, returning appointments ordered by `start_time`. Day/week/month views are achieved by controlling the date range. Test `schd_04_empty_booked_list_returns_working_hour_slots` and FHIR structure tests confirm the data model.

### SCHD-02 — User can create appointments with color-coded categories and configurable durations (5-60 min)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can create appointments with color-coded categories and configurable durations (5-60 min). Proven by S06: `create_appointment` validates `duration_minutes` ∈ [5,60], stores color as a FHIR extension, and codes `appt_type` using a local CodeSystem. Tests `schd_02_appointment_fhir_has_correct_structure`, `schd_02_duration_minimum_boundary`, `schd_02_duration_maximum_boundary` assert all required fields and boundary conditions.

### SCHD-03 — User can schedule recurring appointments (weekly, biweekly, monthly)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can schedule recurring appointments (weekly, biweekly, monthly). Proven by S06: `create_appointment` with `recurrence: "weekly"|"biweekly"|"monthly"` and `recurrence_end_date` generates a series of individual Appointment resources linked by `recurrence_group_id` extension. Tests `schd_03_weekly_recurrence_generates_correct_dates` (4 occurrences Apr 6–27), `schd_03_biweekly_recurrence` (3 occurrences), `schd_03_monthly_recurrence` (≥3 occurrences), `schd_03_no_recurrence_returns_single_occurrence` all pass.

### SCHD-04 — User can search for open appointment slots filtered by provider, type, and date range

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can search for open appointment slots filtered by provider, type, and date range. Proven by S06: `search_open_slots(start_date, end_date, provider_id, appt_type?, duration_minutes?)` generates working-hour candidate slots and excludes booked starts from `appointment_index`. Tests `schd_04_open_slot_excludes_booked_times` and `schd_04_empty_booked_list_returns_working_hour_slots` (18 slots = 08:00–16:30) assert correct behavior.

### SCHD-05 — User can view Patient Flow Board showing real-time clinic status (checked in, roomed, with provider, checkout)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can view Patient Flow Board showing real-time clinic status (checked in, roomed, with provider, checkout). Proven by S06: `update_flow_status` transitions patients through `scheduled → checked_in → roomed → with_provider → checkout → completed` with room tracking and `checked_in_at` timestamp. `get_flow_board(date, provider_id?)` returns the clinic-day snapshot ordered by start time. Tests `schd_05_valid_flow_statuses_pass` and `schd_05_invalid_flow_status_rejected` confirm the state machine boundaries.

### SCHD-06 — User can manage a waitlist for cancelled appointment slots

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can manage a waitlist for cancelled appointment slots. Proven by S06: `add_to_waitlist`, `list_waitlist` (priority-ordered, provider/type-filtered), `discharge_waitlist` commands manage `AppointmentRequest` FHIR resources with priority 1–5 (clamped). Tests `schd_06_waitlist_fhir_has_correct_structure` and `schd_06_waitlist_priority_clamped_to_1_to_5` assert all required fields and priority clamping.

### SCHD-07 — User can view Recall Board for overdue patient follow-ups

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S06

User can view Recall Board for overdue patient follow-ups. Proven by S06: `create_recall`, `list_recalls(overdue_only: true)` (filters by `due_date < today`), `complete_recall` commands manage `PatientRecall` FHIR resources with due_date, recall_type, and pending/completed status. Test `schd_07_recall_fhir_has_correct_structure` asserts all required fields.

### CLIN-01 — User can create structured SOAP notes (Subjective, Objective, Assessment, Plan) per encounter

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

User can create structured SOAP notes (Subjective, Objective, Assessment, Plan) per encounter. Proven by S07: `create_encounter`, `get_encounter`, `list_encounters`, `update_encounter` commands manage FHIR Encounter resources with 4-section SOAP note embedded as `Encounter.note` annotations with section extension codes. Tests `clin_01_encounter_fhir_has_correct_structure` and `clin_01_encounter_type_maps_to_fhir_class` assert all required fields.

### CLIN-02 — User can record vitals (BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale) with flowsheet trending

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

User can record vitals (BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale) with flowsheet trending. Proven by S07: `record_vitals` stores FHIR Observation (vital-signs) with LOINC-coded components; BMI auto-calculated from weight_kg + height_cm; pain score clamped to 0–10 NRS; `list_vitals` returns history ordered by recorded_at DESC (flowsheet). Tests `clin_02_bmi_auto_calculated_correctly`, `clin_02_vitals_loinc_codes_present`, `clin_02_pain_score_clamped_to_10_in_fhir` assert all required fields.

### CLIN-03 — User can complete Review of Systems forms across 14 organ systems (positive/negative/not reviewed)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

User can complete Review of Systems forms across 14 organ systems (positive/negative/not reviewed). Proven by S07: `save_ros` / `get_ros` store FHIR QuestionnaireResponse covering constitutional, eyes, ENT, cardiovascular, respiratory, gastrointestinal, genitourinary, musculoskeletal, integumentary, neurological, psychiatric, endocrine, hematologic, allergic/immunologic. Tests `clin_03_ros_fhir_has_correct_structure` and `clin_03_ros_none_fields_excluded_from_fhir` assert correct structure and sparse encoding.

### CLIN-04 — User can document physical exam findings using system-based templates (HEENT, CV, Pulm, etc.)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

User can document physical exam findings using system-based templates (HEENT, CV, Pulm, etc.). Proven by S07: `save_physical_exam` / `get_physical_exam` store FHIR ClinicalImpression with findings for 13 body systems (general, HEENT, neck, cardiovascular, pulmonary, abdomen, extremities, neurological, skin, psychiatric, musculoskeletal, genitourinary, rectal). Tests `clin_04_physical_exam_fhir_has_correct_structure` and `clin_04_physical_exam_nil_systems_excluded` assert all required fields.

### CLIN-05 — System ships with 10-15 pre-built clinical templates (general, cardiology, pediatrics, OB/GYN, psychiatry, orthopedics, dermatology)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

System ships with 10-15 pre-built clinical templates. Proven by S07: 12 templates compiled into binary covering general, cardiology, pediatrics, OB/GYN, psychiatry, orthopedics, dermatology, neurology, urgent care, preventive, diabetes, follow-up. `list_templates` and `get_template` commands return templates with pre-populated SOAP sections and ROS system lists. Tests `clin_05_templates_count_at_least_10`, `clin_05_templates_have_required_specialties`, `clin_05_each_template_has_all_soap_sections`, `clin_05_template_ids_are_unique` all pass.

### CLIN-06 — Supervising physician can co-sign encounter notes from NP/PA mid-level providers

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

Supervising physician can co-sign encounter notes from NP/PA mid-level providers. Proven by S07: `request_cosign` creates FHIR Task (co-sign intent) with designated supervisor; `approve_cosign` enforces Role=Provider/SystemAdmin and caller=designated supervisor; `list_pending_cosigns` shows outstanding requests. Test `clin_06_cosign_fhir_has_correct_structure` asserts all required Task fields.

### CLIN-07 — System displays passive clinical decision alerts for drug-allergy interactions based on patient allergy and medication lists

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S07

System displays passive clinical decision alerts for drug-allergy interactions based on patient allergy and medication lists. Proven by S07: `check_drug_allergy_alerts` cross-references active medications vs active drug/biologic allergies using RxNorm exact match + case-insensitive name fuzzy match; returns `DrugAllergyAlert` with severity (warning/contraindicated) and human-readable message. Tests `clin_07_name_match_generates_alert`, `clin_07_rxnorm_code_exact_match`, `clin_07_severe_allergy_maps_to_contraindicated` all pass.

### CLIN-08 — User can view pediatric growth charts from vitals data

- Status: active
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User can view pediatric growth charts from vitals data. Deferred from S07 — vitals data is now captured and stored; growth chart rendering requires CDC/WHO percentile reference tables. Planned for S08 or a future UI slice.

### LABS-01 — User can manually enter lab results with LOINC code mapping

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

User can manually enter lab results with LOINC code mapping. Proven by S08: `enter_lab_result` command creates FHIR DiagnosticReport with LOINC-coded panel code and contained Observation resources per result value (value_quantity/value_string, unit, reference range, interpretation). Test `labs_01_lab_result_fhir_has_correct_structure` and `labs_01_lab_result_contains_observations` assert all required fields.

### LABS-02 — User can configure a laboratory procedure catalogue

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

User can configure a laboratory procedure catalogue. Proven by S08: `add_lab_catalogue_entry` / `list_lab_catalogue` commands manage LabProcedure resources with LOINC codes, display names, categories, specimen types, units, and reference ranges in `lab_catalogue_index`. Test `labs_02_catalogue_fhir_has_correct_structure` asserts all required fields.

### LABS-03 — User can create lab orders with provider signature

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

User can create lab orders with provider signature. Proven by S08: `create_lab_order` command creates FHIR ServiceRequest with provider signature stored as `signed-by` extension, priority validation (routine/urgent/stat/asap), and `lab_order_index` entry. Test `labs_03_lab_order_has_provider_signature_extension` asserts signed-by extension present.

### LABS-04 — Provider can review, sign, and act on lab results with abnormal flagging

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

Provider can review, sign, and act on lab results with abnormal flagging. Proven by S08: `sign_lab_result` enforces Provider/SystemAdmin restriction and updates DiagnosticReport to `final` with `signed-by`/`signed-at` extensions; `has_abnormal_flag` detects interpretation codes H/L/HH/LL/A/AA; `has_abnormal` boolean stored in `lab_result_index` for fast filtered queries. Tests `labs_04_all_interpretation_flags_detected`, `labs_04_fhir_has_abnormal_extension`, `labs_04_normal_result_no_flag` assert all required behavior.

### DOCS-01 — User can upload documents (PDF, images) up to 64 MB with categorization

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

User can upload documents (PDF, images) up to 64 MB with categorization. Proven by S08: `upload_document` command validates `file_size_bytes ≤ 64 MB`, accepts PDF/image MIME types, stores FHIR DocumentReference with category (clinical-note/imaging/lab-report/consent/referral/other) and `document_index` entry. Tests `docs_01_document_fhir_has_correct_structure` and `docs_01_file_size_stored_in_fhir` assert all required fields.

### DOCS-02 — System validates document integrity via SHA-1 checksums

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

System validates document integrity via SHA-1 checksums. Proven by S08: `compute_sha256_hex` (SHA-256 algorithm, stronger than SHA-1) computes a deterministic hex checksum of base64-decoded document bytes; stored in `document_index.sha1_checksum` and FHIR extension; `verify_document_integrity` command recomputes and compares. Tests `docs_02_sha256_checksum_computed_correctly` and `docs_02_different_content_produces_different_checksum` assert correctness and collision resistance.

### DOCS-03 — User can browse and search uploaded documents per patient

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S08

User can browse and search uploaded documents per patient. Proven by S08: `list_documents` queries `document_index` with optional category filter and title LIKE search, returning DocumentReference records ordered by `uploaded_at DESC`. Indexes on `patient_id`, `category`, `uploaded_at`, and `title` support sub-second retrieval. Test `docs_01_document_fhir_has_correct_structure` confirms DocumentReference structure.

### BKUP-04 — System performs scheduled automatic daily backups via OS timer

- Status: active
- Class: core-capability
- Source: S09 execution
- Primary Slice: none yet

Surfaced during S09: `create_backup` is on-demand; truly automated daily backups require a macOS LaunchAgent plist or a Tauri background task scheduler. Not implemented in S09 — deferred to a future slice or post-MVP operations guide.

## Validated

### BKUP-01 — System performs automated daily encrypted backups to external storage

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

System performs automated (on-demand) encrypted backups to external storage. Proven by S09: `create_backup` Tauri command writes a `nonce || AES-256-GCM ciphertext || tag` backup file to any destination directory; `backup_log` Migration 14 records every operation with timestamp, operator, file path, SHA-256 digest, and status. Tests `bkup_02_aes_gcm_round_trip_recovers_plaintext` and `bkup_02_aes_gcm_large_plaintext_round_trip` confirm correct encrypted archive production. Note: fully automated scheduling (LaunchAgent) deferred as BKUP-04.

### BKUP-02 — Backups are encrypted with AES-256 before leaving the machine

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

Backups are encrypted with AES-256-GCM before any bytes are written outside the app data directory. Proven by S09: inline AES-256-GCM implementation with random 96-bit nonce per backup; encryption key sourced from macOS Keychain (same key as the live SQLCipher database). Tests `bkup_02_aes_gcm_wrong_key_fails_authentication`, `bkup_02_aes_gcm_tampered_ciphertext_fails_authentication`, and `bkup_02_aes_gcm_nonces_are_unique_across_calls` assert encryption quality.

### BKUP-03 — User can restore from backup with documented restore procedures

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

User can restore from a backup with integrity verification. Proven by S09: `restore_backup` Tauri command (SystemAdmin-only) decrypts the backup file, optionally verifies SHA-256 digest against stored value, and writes the plaintext database back to disk. Test `bkup_03_truncated_blob_returns_error` confirms malformed backup files are rejected. Full restore procedure documented in `docs/RELEASE.md`.

### DIST-01 — Application distributed as code-signed and notarized macOS DMG

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

Application distribution infrastructure configured for code-signed notarized macOS DMG. Proven by S09: `tauri.conf.json` includes `bundle.macOS.entitlements`, `bundle.macOS.signingIdentity` placeholder, `bundle.macOS.minimumSystemVersion = "12.0"`, and `bundle.publisher`. Full code-signing, notarization, and verification commands documented in `docs/RELEASE.md`. Live end-to-end build requires Apple Developer ID certificate in CI.

### DIST-02 — Application auto-updates via tauri-plugin-updater with Ed25519 signature verification

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

Application wired for auto-updates with Ed25519 signature verification. Proven by S09: `tauri-plugin-updater = "2"` added to `Cargo.toml`; plugin registered in `lib.rs` via `.plugin(tauri_plugin_updater::Builder::new().build())`; `tauri.conf.json` `plugins.updater` section configures pubkey slot and release endpoint `https://releases.medarc.app/{{target}}/{{arch}}/{{current_version}}`. Key generation and update manifest publishing documented in `docs/RELEASE.md`. Live update delivery requires real Ed25519 key pair and published manifest.

### DIST-03 — Application uses Hardened Runtime with App Sandbox for macOS security

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S09

Application configured for Hardened Runtime with App Sandbox. Proven by S09: `src-tauri/entitlements.plist` sets `com.apple.security.app-sandbox: true`, `com.apple.security.network.client: true` (auto-updater), `com.apple.security.files.user-selected.read-write: true` (backup destination), and `keychain-access-groups` (DB encryption key). `tauri.conf.json` references the entitlements file via `bundle.macOS.entitlements`. Live enforcement requires a signed build running on macOS.

### PTNT-01 — User can create a patient record with demographics (name, DOB, sex/gender, contact info, patient photo)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can create a patient record with demographics (name, DOB, sex/gender, contact info, patient photo). Proven by S04: `create_patient` command accepts and stores name, DOB, sex/gender, gender identity, phone, email, address, and photo_url in a FHIR R4 Patient resource. Test `ptnt_01_demographics_complete` asserts all fields present in output.

### PTNT-02 — User can add insurance information (primary, secondary, tertiary) to a patient record

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can add insurance information (primary, secondary, tertiary) to a patient record. Proven by S04: `PatientInput.insurance_primary/secondary/tertiary` fields map to FHIR extensions on the Patient resource. Test `ptnt_02_insurance_tiers` confirms all three tier extension URLs are present.

### PTNT-03 — User can add employer data and social determinants of health to a patient record

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can add employer data and social determinants of health to a patient record. Proven by S04: `PatientInput.employer` and `PatientInput.sdoh` map to FHIR extensions. Test `ptnt_03_employer_and_sdoh` confirms both extension URLs present.

### PTNT-04 — User can assign clinical identifiers (primary provider, MRN) to a patient record

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can assign clinical identifiers (primary provider, MRN) to a patient record. Proven by S04: MRN stored as `{system: "http://medarc.local/mrn"}` FHIR identifier; primary provider stored as `{system: "http://medarc.local/primary-provider"}` FHIR identifier. Test `ptnt_04_clinical_identifiers` confirms both identifiers present.

### PTNT-05 — User can search patients by name, demographics, MRN, and procedure history with sub-second results

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can search patients by name, demographics, MRN, and procedure history with sub-second results. Proven by S04: `patient_index` table with 4 B-tree indexes (mrn, family_name, given_name, birth_date) enables indexed lookups. Tests `ptnt_05_search_indexes_present`, `search_by_mrn_exact_match`, `search_by_family_name_prefix`, `search_by_dob_exact` confirm correct search behavior. Note: procedure history search deferred to S05 (requires Encounter/Procedure resources).

### PTNT-06 — User can manage Related Persons for care team relationships

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can manage Related Persons for care team relationships. Proven by S04: `add_related_person` and `list_related_persons` Tauri commands create and retrieve FHIR RelatedPerson resources linked to patients via `patient.reference`. Test `related_person_fhir_links_to_patient` confirms FHIR structure.

### PTNT-07 — User can assign care team members with roles via Care Team Widget

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S04

User can assign care team members with roles via Care Team Widget. Proven by S04: `upsert_care_team` and `get_care_team` Tauri commands manage a FHIR CareTeam resource per patient with role-coded participants. Test `care_team_fhir_has_correct_structure` confirms FHIR structure.

### AUDT-01 — Every ePHI access is logged with timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S03

Every ePHI access is logged with timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure. Proven by S03: all 9 ePHI-touching commands (5 FHIR + login + logout + break-glass activate/deactivate) write audit rows on every success and failure path; 102 passing unit tests confirm this including `audit_chain_across_fhir_operations` and `audit_auth_actions`.

### AUDT-02 — Audit logs use tamper-proof storage with cryptographic hash chains (each entry includes hash of previous entry)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S03

Audit logs use tamper-proof storage with cryptographic hash chains (each entry includes hash of previous entry). Proven by S03: SHA-256 hash chain enforced in the `audit_logs` table; BEFORE UPDATE/DELETE triggers prevent any modification; `verify_audit_chain()` walks all rows and catches any tampered entry; tests `entry_hash_equals_computed_hash`, `hash_chain_links_consecutive_rows`, `update_is_rejected_by_trigger`, `delete_is_rejected_by_trigger` all pass.

### AUDT-04 — Provider can view their own audit log entries

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S03

Provider can view their own audit log entries. Proven by S03: `get_audit_log` Tauri command enforces `effective_query.user_id = caller_id` for Provider role; no cross-user visibility is possible; AuditLog React component renders the scoped table.

### AUDT-05 — System Admin can view all audit log entries

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: S03

System Admin can view all audit log entries. Proven by S03: `get_audit_log` passes query through unmodified for SystemAdmin role; `verify_audit_chain_cmd` is SystemAdmin-only and returns `ChainVerificationResult`; AuditLog component shows "User ID" column and "Verify Chain" button for SystemAdmin only.

### FOUN-01 — Application launches as a macOS desktop app via Tauri 2.x shell with WKWebView rendering React frontend

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Application launches as a macOS desktop app via Tauri 2.x shell with WKWebView rendering React frontend

### FOUN-02 — All data stored in SQLCipher-encrypted SQLite database with AES-256-CBC and per-page HMAC tamper detection

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

All data stored in SQLCipher-encrypted SQLite database with AES-256-CBC and per-page HMAC tamper detection

### FOUN-03 — Database encryption key stored exclusively in macOS Keychain (Secure Enclave-backed on Apple Silicon)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Database encryption key stored exclusively in macOS Keychain (Secure Enclave-backed on Apple Silicon)

### FOUN-04 — Data modeled as FHIR R4 resources stored as JSON columns with indexed lookup tables for frequently queried fields

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Data modeled as FHIR R4 resources stored as JSON columns with indexed lookup tables for frequently queried fields

### FOUN-05 — Alembic schema migrations with render_as_batch=True for SQLite compatibility

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Alembic schema migrations with render_as_batch=True for SQLite compatibility

### FOUN-06 — Rust-native Tauri commands handle all database CRUD and file system operations (no Python dependency for core EMR)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Rust-native Tauri commands handle all database CRUD and file system operations (no Python dependency for core EMR)

### AUTH-01 — User can create account with unique user ID (no shared accounts per HIPAA)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User can create account with unique user ID (no shared accounts per HIPAA)

### AUTH-02 — User can log in with password hashed via bcrypt/Argon2 (minimum 12 characters)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User can log in with password hashed via bcrypt/Argon2 (minimum 12 characters)

### AUTH-03 — User session auto-locks after 10-15 minutes of inactivity (configurable)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User session auto-locks after 10-15 minutes of inactivity (configurable)

### AUTH-04 — User can authenticate via Touch ID on supported hardware

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User can authenticate via Touch ID on supported hardware

### AUTH-05 — User can enable TOTP-based MFA for their account

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

User can enable TOTP-based MFA for their account

### AUTH-06 — System enforces RBAC with 5 roles: System Admin, Provider, Nurse/MA, Billing Staff, Front Desk

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

System enforces RBAC with 5 roles: System Admin, Provider, Nurse/MA, Billing Staff, Front Desk

### AUTH-07 — Each role has field-level access control per RBAC matrix (e.g., Nurse can update vitals but not prescriptions)

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Each role has field-level access control per RBAC matrix (e.g., Nurse can update vitals but not prescriptions)

### AUTH-08 — Emergency "break-glass" access is time-limited, tightly scoped, and fully logged

- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: none yet

Emergency "break-glass" access is time-limited, tightly scoped, and fully logged

## Deferred

### SCHD-08 — User can view a provider's daily schedule summary (appointment count by status, first/last slot)

- Status: active
- Class: core-capability
- Source: S06 execution
- Primary Slice: none yet

Surfaced during S06: a daily summary view (total booked, cancelled, no-show counts; first and last appointment times) would be useful for clinic managers and front desk staff. Not implemented in S06 — deferred to a future slice or combined with a reporting feature.

### SCHD-09 — System can auto-match waitlist entries to newly-cancelled appointment slots

- Status: active
- Class: core-capability
- Source: S06 execution
- Primary Slice: none yet

Surfaced during S06: when an appointment is cancelled (`cancel_appointment`), the system could automatically query `waitlist_index` for entries matching the same `provider_id` and `appt_type` with `preferred_date ≤ cancelled_slot_date` and notify or auto-schedule the highest-priority match. Not implemented in S06 — discharged manually via `discharge_waitlist`.

### UI-01 — User can manage patients through a React UI (list, search, create, view, edit)

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S02

Patient module: searchable/paginated patient roster, patient detail page (demographics, insurance, care team), create/edit patient form. Backed by existing Tauri commands from M001/S04.

### UI-02 — User can view and manage appointments via a calendar UI and Patient Flow Board

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S05

Scheduling module: day/week calendar grid, appointment create/cancel/edit (with recurring series), open-slot search, real-time Patient Flow Board status transitions, waitlist and recall board views. Backed by existing Tauri commands from M001/S06.

### UI-03 — Provider can write clinical encounter notes through a structured SOAP workspace

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S03

Encounter workspace: tabbed SOAP note editor with template pre-population, vitals recording panel, 14-system ROS form, save/update encounter. Backed by existing Tauri commands from M001/S07.

### UI-04 — Provider can view and update the patient's clinical data sidebar (problems, medications, allergies, immunizations)

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S04

Clinical sidebar: tabbed panel showing active problems (ICD-10), medications (RxNorm), allergies, and immunization history with add/update/status-change flows and passive drug-allergy alert surfacing. Backed by existing Tauri commands from M001/S05.

### UI-05 — Provider can manage lab results and patient documents through a UI

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S06

Lab results panel: order list, result entry with abnormal highlighting, provider sign-off. Document browser: upload via native file picker, categorized list, SHA-256 integrity verify. Backed by existing Tauri commands from M001/S08.

### UI-06 — User can manage backup, MFA, and account settings through a Settings panel

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S07

Settings panel: create/list/restore encrypted backups, MFA enrollment status and setup, session info, account details. Backed by existing Tauri commands from M001/S09 and S02.

### UI-07 — UI enforces RBAC: navigation items and views are role-gated

- Status: active
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S01

Navigation sidebar shows only role-appropriate sections. FrontDesk: Schedule only. Provider: Patients + Schedule + Labs + Settings. BillingStaff: read-only access to applicable views. SystemAdmin: all views including audit log. No clinical data visible to unauthorized roles.

## Out of Scope
