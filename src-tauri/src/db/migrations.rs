use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

use crate::db::connection::Database;
use crate::error::AppError;

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
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
