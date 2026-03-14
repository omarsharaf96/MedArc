use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

use crate::db::connection::Database;
use crate::error::AppError;

pub static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        // Migration 1: App metadata table
        M::up(
            "CREATE TABLE IF NOT EXISTS app_metadata (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('schema_version', '1');
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('created_at', datetime('now'));"
        ),
        // Migration 2: FHIR resources table with JSON column and indexes
        M::up(
            "CREATE TABLE IF NOT EXISTS fhir_resources (
                id TEXT PRIMARY KEY NOT NULL,
                resource_type TEXT NOT NULL,
                resource JSON NOT NULL,
                version_id INTEGER NOT NULL DEFAULT 1,
                last_updated TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_fhir_resources_type ON fhir_resources(resource_type);
            CREATE INDEX IF NOT EXISTS idx_fhir_resources_updated ON fhir_resources(last_updated);"
        ),
        // Migration 3: FHIR identifier lookup table for fast identifier-based queries
        M::up(
            "CREATE TABLE IF NOT EXISTS fhir_identifiers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                system TEXT,
                value TEXT NOT NULL,
                UNIQUE(system, value)
            );
            CREATE INDEX IF NOT EXISTS idx_fhir_identifiers_value ON fhir_identifiers(value);
            CREATE INDEX IF NOT EXISTS idx_fhir_identifiers_resource ON fhir_identifiers(resource_id);"
        ),
        // Migration 4: Users table for authentication
        M::up(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY NOT NULL,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL CHECK (role IN ('SystemAdmin', 'Physician', 'Nurse', 'MedicalAssistant', 'FrontDesk')),
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                touch_id_enabled INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                failed_login_attempts INTEGER NOT NULL DEFAULT 0,
                locked_until TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);"
        ),
        // Migration 5: Sessions table for session state tracking
        M::up(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                state TEXT NOT NULL CHECK (state IN ('active', 'locked', 'expired')),
                last_activity TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);"
        ),
        // Migration 6: Break glass log for emergency access audit trail
        M::up(
            "CREATE TABLE IF NOT EXISTS break_glass_log (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                reason TEXT NOT NULL,
                patient_id TEXT,
                activated_at TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT,
                deactivated_at TEXT,
                actions_taken TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_break_glass_user_id ON break_glass_log(user_id);
            CREATE INDEX IF NOT EXISTS idx_break_glass_activated ON break_glass_log(activated_at);"
        ),
        // Migration 7: App settings with default values
        M::up(
            "CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('session_timeout_minutes', '15');
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('max_failed_logins', '5');
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('lockout_duration_minutes', '30');"
        ),
        // Migration 8: Audit logs — HIPAA-required tamper-proof access log with hash chain
        //
        // HIPAA required fields (9): timestamp, user_id, action, resource_type, resource_id,
        // patient_id, device_id, success, details.
        // Additional chain fields: previous_hash, entry_hash.
        //
        // Immutability is enforced at the database level via triggers that abort
        // any UPDATE or DELETE on this table. Even a SystemAdmin cannot alter past records.
        M::up(
            "CREATE TABLE IF NOT EXISTS audit_logs (
                id          TEXT PRIMARY KEY NOT NULL,
                timestamp   TEXT NOT NULL,
                user_id     TEXT NOT NULL,
                action      TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT,
                patient_id  TEXT,
                device_id   TEXT NOT NULL,
                success     INTEGER NOT NULL CHECK (success IN (0, 1)),
                details     TEXT,
                previous_hash TEXT NOT NULL,
                entry_hash  TEXT NOT NULL UNIQUE
            );
            CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id   ON audit_logs(user_id);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_patient   ON audit_logs(patient_id);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_action    ON audit_logs(action);

            -- Prevent any UPDATE on audit_logs rows (tamper-proof).
            CREATE TRIGGER IF NOT EXISTS audit_logs_no_update
            BEFORE UPDATE ON audit_logs
            BEGIN
                SELECT RAISE(ABORT, 'audit_logs rows are immutable: UPDATE is not allowed');
            END;

            -- Prevent any DELETE on audit_logs rows (tamper-proof).
            CREATE TRIGGER IF NOT EXISTS audit_logs_no_delete
            BEFORE DELETE ON audit_logs
            BEGIN
                SELECT RAISE(ABORT, 'audit_logs rows are immutable: DELETE is not allowed');
            END;"
        ),
        // Migration 9: Patient index — denormalised lookup table for sub-second patient search
        //
        // Stores extracted demographic fields (MRN, family_name, given_name, birth_date, gender)
        // from the FHIR Patient JSON into indexed columns so searches avoid JSON extraction.
        //
        // CASCADE DELETE ensures that deleting a Patient from fhir_resources automatically
        // removes the corresponding patient_index row.
        M::up(
            "PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS patient_index (
                patient_id          TEXT PRIMARY KEY NOT NULL
                                    REFERENCES fhir_resources(id) ON DELETE CASCADE,
                mrn                 TEXT NOT NULL UNIQUE,
                family_name         TEXT NOT NULL,
                given_name          TEXT,
                birth_date          TEXT,
                gender              TEXT,
                primary_provider_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_patient_index_mrn    ON patient_index(mrn);
            CREATE INDEX IF NOT EXISTS idx_patient_index_family  ON patient_index(family_name);
            CREATE INDEX IF NOT EXISTS idx_patient_index_given   ON patient_index(given_name);
            CREATE INDEX IF NOT EXISTS idx_patient_index_dob     ON patient_index(birth_date);"
        ),
        // Migration 10: Clinical data index tables for S05
        //
        // Four index tables support the clinical data lists:
        //   - allergy_index:       maps AllergyIntolerance resources by patient / status / category
        //   - problem_index:       maps Condition resources by patient / status / ICD-10 code
        //   - medication_index:    maps MedicationStatement resources by patient / status / RxNorm
        //   - immunization_index:  maps Immunization resources by patient / CVX code / date
        //
        // All four use ON DELETE CASCADE from fhir_resources so that deleting the FHIR resource
        // automatically removes the corresponding index row.  This mirrors the patient_index
        // pattern established in Migration 9.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS allergy_index (
                allergy_id      TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                clinical_status TEXT NOT NULL DEFAULT 'active',
                category        TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_allergy_patient   ON allergy_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_allergy_status    ON allergy_index(clinical_status);
            CREATE INDEX IF NOT EXISTS idx_allergy_category  ON allergy_index(category);

            CREATE TABLE IF NOT EXISTS problem_index (
                problem_id      TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                clinical_status TEXT NOT NULL DEFAULT 'active',
                icd10_code      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_problem_patient   ON problem_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_problem_status    ON problem_index(clinical_status);
            CREATE INDEX IF NOT EXISTS idx_problem_icd10     ON problem_index(icd10_code);

            CREATE TABLE IF NOT EXISTS medication_index (
                medication_id   TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'active',
                rxnorm_code     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_medication_patient ON medication_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_medication_status  ON medication_index(status);
            CREATE INDEX IF NOT EXISTS idx_medication_rxnorm  ON medication_index(rxnorm_code);

            CREATE TABLE IF NOT EXISTS immunization_index (
                immunization_id TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                cvx_code        TEXT NOT NULL,
                administered_date TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_immunization_patient ON immunization_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_immunization_cvx     ON immunization_index(cvx_code);
            CREATE INDEX IF NOT EXISTS idx_immunization_date    ON immunization_index(administered_date);"
        ),
        // Migration 11: Scheduling index tables for S06
        //
        // Four index tables support the scheduling feature set:
        //   - appointment_index:  maps Appointment resources by patient, provider, start_time, status
        //   - waitlist_index:     maps AppointmentRequest resources by patient, provider, preferred_date
        //   - recall_index:       maps PatientRecall resources by patient, provider, due_date, status
        //   - flow_board_index:   maps real-time clinic flow status per appointment
        //
        // All appointment/waitlist/recall index rows reference fhir_resources via ON DELETE CASCADE.
        // flow_board_index references appointment_index for cascade deletion.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS appointment_index (
                appointment_id      TEXT PRIMARY KEY NOT NULL
                                    REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id          TEXT NOT NULL,
                provider_id         TEXT NOT NULL,
                start_time          TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'booked',
                appt_type           TEXT NOT NULL,
                color               TEXT,
                recurrence_group_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_appt_patient   ON appointment_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_appt_provider  ON appointment_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_appt_start     ON appointment_index(start_time);
            CREATE INDEX IF NOT EXISTS idx_appt_status    ON appointment_index(status);
            CREATE INDEX IF NOT EXISTS idx_appt_recurrence ON appointment_index(recurrence_group_id);

            CREATE TABLE IF NOT EXISTS waitlist_index (
                waitlist_id     TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                provider_id     TEXT,
                preferred_date  TEXT NOT NULL,
                appt_type       TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'active',
                priority        INTEGER NOT NULL DEFAULT 3
            );
            CREATE INDEX IF NOT EXISTS idx_waitlist_patient  ON waitlist_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_waitlist_provider ON waitlist_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_waitlist_date     ON waitlist_index(preferred_date);
            CREATE INDEX IF NOT EXISTS idx_waitlist_status   ON waitlist_index(status);
            CREATE INDEX IF NOT EXISTS idx_waitlist_priority ON waitlist_index(priority);

            CREATE TABLE IF NOT EXISTS recall_index (
                recall_id   TEXT PRIMARY KEY NOT NULL
                            REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id  TEXT NOT NULL,
                provider_id TEXT,
                due_date    TEXT NOT NULL,
                recall_type TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending'
            );
            CREATE INDEX IF NOT EXISTS idx_recall_patient  ON recall_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_recall_provider ON recall_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_recall_due_date ON recall_index(due_date);
            CREATE INDEX IF NOT EXISTS idx_recall_status   ON recall_index(status);

            CREATE TABLE IF NOT EXISTS flow_board_index (
                appointment_id TEXT PRIMARY KEY NOT NULL
                               REFERENCES appointment_index(appointment_id) ON DELETE CASCADE,
                patient_id     TEXT NOT NULL,
                provider_id    TEXT NOT NULL,
                flow_status    TEXT NOT NULL DEFAULT 'scheduled',
                start_time     TEXT NOT NULL,
                appt_type      TEXT NOT NULL,
                color          TEXT,
                room           TEXT,
                checked_in_at  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_flow_patient    ON flow_board_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_flow_provider   ON flow_board_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_flow_start_time ON flow_board_index(start_time);
            CREATE INDEX IF NOT EXISTS idx_flow_status     ON flow_board_index(flow_status);"
        ),
        // Migration 12: Clinical Documentation index tables for S07
        //
        // Three index tables support the clinical documentation feature set:
        //   - encounter_index:  maps Encounter resources by patient, provider, encounter_date, status, type
        //   - vitals_index:     maps Observation (vital-signs) resources by patient, encounter, recorded_at
        //   - cosign_index:     maps Task (co-sign) resources by encounter, requesting/supervising provider, status
        //
        // All index rows reference fhir_resources via ON DELETE CASCADE.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS encounter_index (
                encounter_id    TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                provider_id     TEXT NOT NULL,
                encounter_date  TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'in-progress',
                encounter_type  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_encounter_patient  ON encounter_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_encounter_provider ON encounter_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_encounter_date     ON encounter_index(encounter_date);
            CREATE INDEX IF NOT EXISTS idx_encounter_status   ON encounter_index(status);
            CREATE INDEX IF NOT EXISTS idx_encounter_type     ON encounter_index(encounter_type);

            CREATE TABLE IF NOT EXISTS vitals_index (
                vitals_id       TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                encounter_id    TEXT NOT NULL,
                recorded_at     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_vitals_patient     ON vitals_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_vitals_encounter   ON vitals_index(encounter_id);
            CREATE INDEX IF NOT EXISTS idx_vitals_recorded_at ON vitals_index(recorded_at);

            CREATE TABLE IF NOT EXISTS cosign_index (
                cosign_id               TEXT PRIMARY KEY NOT NULL
                                        REFERENCES fhir_resources(id) ON DELETE CASCADE,
                encounter_id            TEXT NOT NULL,
                requesting_provider_id  TEXT NOT NULL,
                supervising_provider_id TEXT NOT NULL,
                status                  TEXT NOT NULL DEFAULT 'requested',
                requested_at            TEXT NOT NULL,
                signed_at               TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_cosign_encounter  ON cosign_index(encounter_id);
            CREATE INDEX IF NOT EXISTS idx_cosign_supervisor ON cosign_index(supervising_provider_id);
            CREATE INDEX IF NOT EXISTS idx_cosign_status     ON cosign_index(status);"
        ),
        // Migration 13: Lab Results & Document Management index tables for S08
        //
        // Four index tables support the lab and document feature set:
        //   - lab_catalogue_index: maps LabProcedure catalogue entries by LOINC code / display
        //   - lab_order_index:     maps ServiceRequest (lab order) resources by patient, provider, ordered_at, status
        //   - lab_result_index:    maps DiagnosticReport resources by patient, order, reported_at, status, abnormal
        //   - document_index:      maps DocumentReference resources by patient, category, uploaded_at, sha1
        //
        // All index rows reference fhir_resources via ON DELETE CASCADE.
        M::up(
            "PRAGMA foreign_keys = ON;

            -- Lab catalogue: user-configurable procedure library (LABS-02)
            CREATE TABLE IF NOT EXISTS lab_catalogue_index (
                catalogue_id    TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                loinc_code      TEXT NOT NULL,
                display_name    TEXT NOT NULL,
                category        TEXT NOT NULL DEFAULT 'laboratory'
            );
            CREATE INDEX IF NOT EXISTS idx_lab_cat_loinc    ON lab_catalogue_index(loinc_code);
            CREATE INDEX IF NOT EXISTS idx_lab_cat_category ON lab_catalogue_index(category);
            CREATE INDEX IF NOT EXISTS idx_lab_cat_name     ON lab_catalogue_index(display_name);

            -- Lab orders: FHIR ServiceRequest (LABS-03)
            CREATE TABLE IF NOT EXISTS lab_order_index (
                order_id        TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                provider_id     TEXT NOT NULL,
                ordered_at      TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'active',
                loinc_code      TEXT,
                priority        TEXT NOT NULL DEFAULT 'routine'
            );
            CREATE INDEX IF NOT EXISTS idx_lab_order_patient    ON lab_order_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_lab_order_provider   ON lab_order_index(provider_id);
            CREATE INDEX IF NOT EXISTS idx_lab_order_status     ON lab_order_index(status);
            CREATE INDEX IF NOT EXISTS idx_lab_order_ordered_at ON lab_order_index(ordered_at);

            -- Lab results: FHIR DiagnosticReport (LABS-01, LABS-04)
            CREATE TABLE IF NOT EXISTS lab_result_index (
                result_id       TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                order_id        TEXT,
                reported_at     TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'preliminary',
                has_abnormal    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_lab_result_patient    ON lab_result_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_lab_result_order      ON lab_result_index(order_id);
            CREATE INDEX IF NOT EXISTS idx_lab_result_reported   ON lab_result_index(reported_at);
            CREATE INDEX IF NOT EXISTS idx_lab_result_status     ON lab_result_index(status);
            CREATE INDEX IF NOT EXISTS idx_lab_result_abnormal   ON lab_result_index(has_abnormal);

            -- Documents: FHIR DocumentReference (DOCS-01, DOCS-02, DOCS-03)
            CREATE TABLE IF NOT EXISTS document_index (
                document_id     TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                category        TEXT NOT NULL DEFAULT 'clinical-note',
                title           TEXT NOT NULL,
                content_type    TEXT NOT NULL,
                file_size_bytes INTEGER NOT NULL DEFAULT 0,
                sha1_checksum   TEXT NOT NULL,
                uploaded_at     TEXT NOT NULL,
                uploaded_by     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_document_patient    ON document_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_document_category   ON document_index(category);
            CREATE INDEX IF NOT EXISTS idx_document_uploaded   ON document_index(uploaded_at);
            CREATE INDEX IF NOT EXISTS idx_document_title      ON document_index(title);"
        ),
        // Migration 14: Backup log for BKUP-01 / BKUP-02 / BKUP-03
        //
        // Tracks every backup and restore operation:
        //   backup_log  — one row per backup or restore event with AES-256-GCM encrypted payload,
        //                 SHA-256 content digest, and outcome metadata.
        //
        // The actual encrypted archive is written to the filesystem by the backup command.
        // This table records the audit trail: when, who, what file, outcome, and the content
        // digest so integrity can be verified before restore (BKUP-03).
        M::up(
            "
            CREATE TABLE IF NOT EXISTS backup_log (
                id              TEXT PRIMARY KEY NOT NULL,
                operation       TEXT NOT NULL CHECK(operation IN ('backup','restore')),
                initiated_by    TEXT NOT NULL,
                started_at      TEXT NOT NULL,
                completed_at    TEXT,
                status          TEXT NOT NULL DEFAULT 'in_progress'
                                CHECK(status IN ('in_progress','completed','failed')),
                file_path       TEXT,
                file_size_bytes INTEGER,
                sha256_digest   TEXT,
                error_message   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_backup_log_started ON backup_log(started_at);
            CREATE INDEX IF NOT EXISTS idx_backup_log_op      ON backup_log(operation);"
        ),
        // Migration 15: PT Note index table for PT-DOC-01 through PT-DOC-04
        //
        // `pt_note_index` is the fast-query index for Physical Therapy notes.
        // The actual note content is stored as FHIR Composition JSON in `fhir_resources`
        // (resource_type = 'PTNote').
        //
        // Three note types are supported:
        //   initial_eval     — Initial Evaluation (PT-DOC-01)
        //   progress_note    — Daily Progress Note (PT-DOC-02)
        //   discharge_summary — Discharge Summary (PT-DOC-03)
        //
        // Status lifecycle: draft → signed → locked
        //   draft   — editable by the creating provider
        //   signed  — co-signed; triggers visit counter hooks in S07
        //   locked  — immutable; no further edits allowed
        //
        // `addendum_of` ships from day one so S07 addendum linkage requires no
        // breaking migration later.
        M::up(
            "PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS pt_note_index (
                pt_note_id   TEXT PRIMARY KEY NOT NULL,
                patient_id   TEXT NOT NULL,
                encounter_id TEXT,
                note_type    TEXT NOT NULL
                             CHECK(note_type IN ('initial_eval','progress_note','discharge_summary')),
                status       TEXT NOT NULL DEFAULT 'draft'
                             CHECK(status IN ('draft','signed','locked')),
                provider_id  TEXT NOT NULL,
                addendum_of  TEXT REFERENCES pt_note_index(pt_note_id),
                created_at   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pt_note_patient ON pt_note_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_pt_note_type    ON pt_note_index(note_type);
            CREATE INDEX IF NOT EXISTS idx_pt_note_status  ON pt_note_index(status);"
        ),
        // Migration 16: Outcome Score index table for M003/S02 — Objective Measures & Outcome Scores
        //
        // `outcome_score_index` stores scored outcome measures (LEFS, DASH, NDI, Oswestry,
        // PSFS, FABQ) with their computed scores, severity classifications, and episode phases.
        // The actual FHIR Observation JSON is stored in `fhir_resources`.
        //
        // `score_secondary` is used for measures with dual scores (e.g. FABQ Work subscale).
        // `episode_phase` tracks whether a score was taken at initial, mid, or discharge.
        // `loinc_code` maps each measure to its LOINC code for FHIR compliance.
        M::up(
            "CREATE TABLE IF NOT EXISTS outcome_score_index (
                score_id        TEXT PRIMARY KEY,
                resource_id     TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                encounter_id    TEXT,
                measure_type    TEXT NOT NULL CHECK(measure_type IN ('lefs','dash','ndi','oswestry','psfs','fabq')),
                score           REAL NOT NULL,
                score_secondary REAL,
                severity        TEXT,
                episode_phase   TEXT CHECK(episode_phase IN ('initial','mid','discharge')),
                loinc_code      TEXT,
                recorded_at     TEXT NOT NULL DEFAULT (datetime('now')),
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_outcome_patient  ON outcome_score_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_outcome_measure  ON outcome_score_index(measure_type);
            CREATE INDEX IF NOT EXISTS idx_outcome_recorded ON outcome_score_index(recorded_at);"
        ),
        // Migration 17: Document Center tables for M003/S04
        //
        // Four index tables support the Document Center feature set:
        //   - document_category_index:  PT-specific categorized document upload
        //   - survey_template_index:    intake survey templates (built-in + custom)
        //   - survey_response_index:    completed survey responses per patient
        //   - referral_index:           referring provider tracking per patient
        //
        // All index rows reference fhir_resources via ON DELETE CASCADE.
        M::up(
            "PRAGMA foreign_keys = ON;

            -- Document category index (upgrade existing documents with PT categories)
            CREATE TABLE IF NOT EXISTS document_category_index (
                document_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id TEXT NOT NULL,
                category TEXT NOT NULL CHECK(category IN ('referral_rx','imaging','consent_forms','intake_surveys','insurance','legal','home_exercise_program','other')),
                file_name TEXT NOT NULL,
                mime_type TEXT,
                file_size INTEGER,
                sha1_hash TEXT,
                uploaded_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_doc_cat_patient ON document_category_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_doc_cat_category ON document_category_index(category);

            -- Intake survey templates
            CREATE TABLE IF NOT EXISTS survey_template_index (
                template_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                field_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Intake survey responses
            CREATE TABLE IF NOT EXISTS survey_response_index (
                response_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                template_id TEXT NOT NULL,
                patient_id TEXT NOT NULL,
                completed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_survey_resp_patient ON survey_response_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_survey_resp_template ON survey_response_index(template_id);

            -- Referral tracking
            CREATE TABLE IF NOT EXISTS referral_index (
                referral_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id TEXT NOT NULL,
                referring_provider_name TEXT NOT NULL,
                referring_provider_npi TEXT,
                practice_name TEXT,
                phone TEXT,
                fax TEXT,
                referral_date TEXT,
                authorized_visit_count INTEGER,
                diagnosis_icd10 TEXT,
                linked_document_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_referral_patient ON referral_index(patient_id);"
        ),
        // Migration 18: Export log for M003/S05 — PDF Export & Report Generation
        M::up(
            "CREATE TABLE IF NOT EXISTS export_log (
                export_id    TEXT PRIMARY KEY,
                patient_id   TEXT NOT NULL,
                export_type  TEXT NOT NULL CHECK(export_type IN ('note_pdf','progress_report','insurance_narrative','legal_report','chart_export')),
                file_path    TEXT NOT NULL,
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                generated_by TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_export_patient ON export_log(patient_id);"
        ),
        // Migration 19: Fax integration tables for M003/S06
        M::up(
            "CREATE TABLE IF NOT EXISTS fax_log (
                fax_id TEXT PRIMARY KEY,
                phaxio_fax_id TEXT,
                direction TEXT NOT NULL CHECK(direction IN ('sent','received')),
                patient_id TEXT,
                recipient_name TEXT,
                recipient_fax TEXT,
                document_name TEXT,
                file_path TEXT,
                status TEXT NOT NULL CHECK(status IN ('queued','in_progress','success','failed')),
                sent_at TEXT NOT NULL DEFAULT (datetime('now')),
                delivered_at TEXT,
                pages INTEGER,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_fax_patient ON fax_log(patient_id);
            CREATE INDEX IF NOT EXISTS idx_fax_direction ON fax_log(direction);
            CREATE INDEX IF NOT EXISTS idx_fax_status ON fax_log(status);

            CREATE TABLE IF NOT EXISTS fax_contacts (
                contact_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                organization TEXT,
                fax_number TEXT NOT NULL,
                phone_number TEXT,
                contact_type TEXT NOT NULL CHECK(contact_type IN ('insurance','referring_md','attorney','other')),
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_fax_contact_type ON fax_contacts(contact_type);"
        ),
        // Migration 20: Authorization & Visit Tracking index table (M003/S07)
        //
        // Tracks insurance authorization records with visit counters:
        //   - auth_record_index: maps Coverage FHIR resources by patient, payer, status, date range
        //
        // visits_used increments each time a note is co-signed and locked.
        // status auto-transitions: active → exhausted (visits_used >= authorized_visits)
        //                          active → expired (end_date < today)
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS auth_record_index (
                auth_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id TEXT NOT NULL,
                payer_name TEXT NOT NULL,
                payer_phone TEXT,
                auth_number TEXT,
                authorized_visits INTEGER NOT NULL,
                visits_used INTEGER NOT NULL DEFAULT 0,
                authorized_cpt_codes TEXT,
                start_date TEXT NOT NULL,
                end_date TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','expired','exhausted')),
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_auth_patient ON auth_record_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_auth_status ON auth_record_index(status);"
        ),
        // Migration 21: Composite indexes for common multi-column query patterns
        M::up(
            "CREATE INDEX IF NOT EXISTS idx_auth_patient_status ON auth_record_index(patient_id, status);
            CREATE INDEX IF NOT EXISTS idx_doc_cat_patient_category ON document_category_index(patient_id, category);
            CREATE INDEX IF NOT EXISTS idx_outcome_patient_measure ON outcome_score_index(patient_id, measure_type);
            CREATE INDEX IF NOT EXISTS idx_fax_patient_direction ON fax_log(patient_id, direction);"
        ),
        // Migration 22: CPT Billing Engine tables (M004/S01)
        //
        // Three tables support the CPT billing feature set:
        //   - cpt_fee_schedule:    per-payer fee schedules; NULL payer_id = self-pay default
        //   - encounter_billing:   one billing header per encounter with totals and status
        //   - billing_line_items:  individual CPT line items linked to encounter_billing
        //
        // Status lifecycle for encounter_billing:
        //   draft → ready → submitted → paid
        //
        // billing_rule must be one of 'medicare' | 'ama' to enforce correct
        // 8-minute rule application server-side.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS cpt_fee_schedule (
                fee_id TEXT PRIMARY KEY,
                payer_id TEXT,
                cpt_code TEXT NOT NULL,
                description TEXT,
                allowed_amount REAL NOT NULL,
                is_timed INTEGER NOT NULL DEFAULT 1,
                effective_date TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_fee_payer ON cpt_fee_schedule(payer_id);
            CREATE INDEX IF NOT EXISTS idx_fee_cpt ON cpt_fee_schedule(cpt_code);

            CREATE TABLE IF NOT EXISTS encounter_billing (
                billing_id TEXT PRIMARY KEY,
                encounter_id TEXT NOT NULL,
                patient_id TEXT NOT NULL,
                payer_id TEXT,
                billing_rule TEXT NOT NULL CHECK(billing_rule IN ('medicare','ama')),
                total_charge REAL NOT NULL DEFAULT 0,
                total_units INTEGER NOT NULL DEFAULT 0,
                total_minutes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','ready','submitted','paid')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_billing_encounter ON encounter_billing(encounter_id);
            CREATE INDEX IF NOT EXISTS idx_billing_patient ON encounter_billing(patient_id);

            CREATE TABLE IF NOT EXISTS billing_line_items (
                line_id TEXT PRIMARY KEY,
                billing_id TEXT NOT NULL REFERENCES encounter_billing(billing_id) ON DELETE CASCADE,
                cpt_code TEXT NOT NULL,
                modifiers TEXT,
                minutes INTEGER NOT NULL DEFAULT 0,
                units INTEGER NOT NULL DEFAULT 0,
                charge REAL NOT NULL DEFAULT 0,
                dx_pointers TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_line_billing ON billing_line_items(billing_id);"
        ),
        // Migration 23: HEP (Home Exercise Program) tables (M003/S02)
        //
        // Three tables support the HEP Builder feature:
        //   - exercise_library:  built-in (~50) and custom exercises, organized by body region
        //                        and category; seeded at runtime via INSERT OR IGNORE.
        //   - hep_programs:      one row per program per patient encounter; exercises stored
        //                        as JSON (array of ExercisePrescription).
        //   - hep_templates:     reusable templates (built-in + user-created); exercises stored
        //                        as JSON with default prescription values.
        //
        // hep_programs.resource_id references fhir_resources (FHIR CarePlan) so the program
        // participates in the FHIR resource graph and cascades on patient delete.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS exercise_library (
                exercise_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                body_region TEXT NOT NULL CHECK(body_region IN ('cervical','thoracic','lumbar','shoulder','elbow','wrist','hip','knee','ankle','general')),
                category TEXT NOT NULL CHECK(category IN ('rom','strengthening','stretching','balance','functional','cardio')),
                description TEXT,
                instructions TEXT,
                equipment TEXT,
                difficulty TEXT CHECK(difficulty IN ('beginner','intermediate','advanced')),
                is_builtin INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_exercise_region ON exercise_library(body_region);
            CREATE INDEX IF NOT EXISTS idx_exercise_category ON exercise_library(category);

            CREATE TABLE IF NOT EXISTS hep_programs (
                program_id TEXT PRIMARY KEY,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id TEXT NOT NULL,
                encounter_id TEXT,
                created_by TEXT NOT NULL,
                exercises_json TEXT NOT NULL,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_hep_patient ON hep_programs(patient_id);

            CREATE TABLE IF NOT EXISTS hep_templates (
                template_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                body_region TEXT,
                condition_name TEXT,
                exercises_json TEXT NOT NULL,
                created_by TEXT NOT NULL,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );"
        ),
        // Migration 24: Therapy Cap & KX Modifier tables (M004/S02)
        //
        // Two tables support therapy cap monitoring:
        //   - therapy_cap_tracking: per-patient per-year cumulative Medicare charge totals,
        //     KX modifier application date, and Targeted Medical Review flag.
        //   - abn_records: Advance Beneficiary Notice (CMS-R-131) records tracking
        //     patient choice (option1_pay / option2_dont_pay / option3_dont_provide)
        //     and signature date.
        //
        // therapy_cap_tracking has a UNIQUE constraint on (patient_id, calendar_year, payer_type)
        // so upserts are safe via INSERT OR REPLACE / UPDATE patterns.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS therapy_cap_tracking (
              tracking_id TEXT PRIMARY KEY,
              patient_id TEXT NOT NULL,
              calendar_year INTEGER NOT NULL,
              payer_type TEXT NOT NULL DEFAULT 'medicare' CHECK(payer_type IN ('medicare','medicaid','commercial')),
              cumulative_charges REAL NOT NULL DEFAULT 0,
              threshold_amount REAL NOT NULL DEFAULT 2480,
              kx_applied_date TEXT,
              review_threshold_reached INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL DEFAULT (datetime('now')),
              updated_at TEXT NOT NULL DEFAULT (datetime('now')),
              UNIQUE(patient_id, calendar_year, payer_type)
            );
            CREATE INDEX IF NOT EXISTS idx_cap_patient ON therapy_cap_tracking(patient_id);

            CREATE TABLE IF NOT EXISTS abn_records (
              abn_id TEXT PRIMARY KEY,
              patient_id TEXT NOT NULL,
              reason TEXT NOT NULL CHECK(reason IN ('therapy_cap_approaching','auth_expired','non_covered_service','frequency_limit')),
              services_json TEXT NOT NULL,
              patient_choice TEXT CHECK(patient_choice IN ('option1_pay','option2_dont_pay','option3_dont_provide')),
              signed_date TEXT,
              created_by TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_abn_patient ON abn_records(patient_id);"
        ),
        // Migration 25: Electronic Claims Submission tables (M004/S02)
        //
        // Two tables support 837P EDI claim management:
        //   - payer_config:  payer configurations (EDI payer ID, clearinghouse, billing rule)
        //   - claims:        837P claim lifecycle (draft → validated → submitted → accepted → paid/denied/appealed)
        //
        // claims.status follows a strict lifecycle enforced by CHECK constraint.
        // edi_content stores the full 837P EDI text; edi_file_path stores the file system path.
        // control_number is the ISA13/GS06 interchange control number used for payer correlation.
        M::up(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS payer_config (
              payer_id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              edi_payer_id TEXT,
              clearinghouse TEXT CHECK(clearinghouse IN ('office_ally','availity','trizetto','manual')),
              billing_rule TEXT NOT NULL DEFAULT 'medicare' CHECK(billing_rule IN ('medicare','ama')),
              phone TEXT,
              address TEXT,
              created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS claims (
              claim_id TEXT PRIMARY KEY,
              encounter_billing_id TEXT NOT NULL REFERENCES encounter_billing(billing_id),
              payer_id TEXT NOT NULL REFERENCES payer_config(payer_id),
              patient_id TEXT NOT NULL,
              status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','validated','submitted','accepted','paid','denied','appealed')),
              edi_content TEXT,
              edi_file_path TEXT,
              control_number TEXT,
              submitted_at TEXT,
              response_at TEXT,
              paid_amount REAL,
              adjustment_amount REAL,
              denial_reason TEXT,
              notes TEXT,
              created_at TEXT NOT NULL DEFAULT (datetime('now')),
              updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_claims_patient ON claims(patient_id);
            CREATE INDEX IF NOT EXISTS idx_claims_status ON claims(status);
            CREATE INDEX IF NOT EXISTS idx_claims_payer ON claims(payer_id);"
        ),
    ])
});

pub fn run(db: &Database) -> Result<(), AppError> {
    let mut conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        assert!(MIGRATIONS.validate().is_ok());
    }
}
