/// audit/entry.rs — Write path for HIPAA audit log entries.
///
/// Every ePHI access must call `write_audit_entry()` with the 9 required
/// HIPAA fields. The function computes a SHA-256 hash chain: each row's
/// `entry_hash` covers its own content + `previous_hash`, so any tampering
/// with a historical row breaks every subsequent link.
///
/// The very first row uses `previous_hash = "GENESIS"` as the chain origin.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Input required to create one audit log entry.
///
/// `patient_id`, `resource_id`, and `details` are optional because some
/// actions (e.g. login) don't reference a specific patient or resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryInput {
    /// The authenticated user performing the action.
    pub user_id: String,
    /// A stable action name, e.g. "fhir.create", "auth.login", "fhir.get_failed".
    pub action: String,
    /// FHIR resource type affected, e.g. "Patient", "Observation", or "auth".
    pub resource_type: String,
    /// ID of the specific FHIR resource, if applicable.
    pub resource_id: Option<String>,
    /// Patient whose ePHI was touched, if applicable.
    pub patient_id: Option<String>,
    /// Workstation / hardware identifier from machine-uid.
    pub device_id: String,
    /// `true` if the operation succeeded; `false` if it was rejected/failed.
    pub success: bool,
    /// Optional free-text detail (error message, break-glass reason, etc.).
    /// Never include raw PHI here.
    pub details: Option<String>,
}

/// A fully-materialised audit log row as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub patient_id: Option<String>,
    pub device_id: String,
    pub success: bool,
    pub details: Option<String>,
    pub previous_hash: String,
    pub entry_hash: String,
}

/// Compute the SHA-256 entry hash for an audit row.
///
/// The canonical input string is:
///   `{previous_hash}|{id}|{timestamp}|{user_id}|{action}|{resource_type}|{resource_id}|{patient_id}|{device_id}|{success}|{details}`
///
/// Missing optional fields are represented as the empty string.
/// The separator `|` is chosen because it cannot appear in UUIDs or
/// RFC-3339 timestamps, making the pre-image unambiguous.
pub fn compute_hash(
    previous_hash: &str,
    id: &str,
    timestamp: &str,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    patient_id: Option<&str>,
    device_id: &str,
    success: bool,
    details: Option<&str>,
) -> String {
    let preimage = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        previous_hash,
        id,
        timestamp,
        user_id,
        action,
        resource_type,
        resource_id.unwrap_or(""),
        patient_id.unwrap_or(""),
        device_id,
        if success { "1" } else { "0" },
        details.unwrap_or(""),
    );
    let digest = Sha256::digest(preimage.as_bytes());
    hex::encode(digest)
}

/// Write a single audit entry to the database.
///
/// Automatically resolves the hash chain by reading the most recent
/// `entry_hash` in the table (or using `"GENESIS"` if the table is empty).
///
/// This function must be called while holding the database mutex lock.
/// The caller is responsible for acquiring `db.conn.lock()` — this avoids
/// re-entrant lock attempts when called from within other commands that
/// already hold the lock.
///
/// # Errors
/// Returns `AppError::Database` on any SQLite failure.
pub fn write_audit_entry(conn: &Connection, input: AuditEntryInput) -> Result<AuditEntry, AppError> {
    // Resolve the previous hash (tip of the chain).
    let previous_hash: String = conn
        .query_row(
            "SELECT entry_hash FROM audit_logs ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "GENESIS".to_string());

    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();

    let entry_hash = compute_hash(
        &previous_hash,
        &id,
        &timestamp,
        &input.user_id,
        &input.action,
        &input.resource_type,
        input.resource_id.as_deref(),
        input.patient_id.as_deref(),
        &input.device_id,
        input.success,
        input.details.as_deref(),
    );

    conn.execute(
        "INSERT INTO audit_logs
            (id, timestamp, user_id, action, resource_type, resource_id,
             patient_id, device_id, success, details, previous_hash, entry_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            timestamp,
            input.user_id,
            input.action,
            input.resource_type,
            input.resource_id,
            input.patient_id,
            input.device_id,
            input.success as i32,
            input.details,
            previous_hash,
            entry_hash,
        ],
    )?;

    Ok(AuditEntry {
        id,
        timestamp,
        user_id: input.user_id,
        action: input.action,
        resource_type: input.resource_type,
        resource_id: input.resource_id,
        patient_id: input.patient_id,
        device_id: input.device_id,
        success: input.success,
        details: input.details,
        previous_hash,
        entry_hash,
    })
}

// ─────────────────────────────────────────────────────────────
// Tests — TDD: these define the contract before verification.
// Run them with:  cargo test -p medarc audit::entry
// ─────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Open an in-memory SQLite database and apply the audit_logs schema
    /// (migration 8) so tests can run without the full app startup path.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        // Matches the DDL from migration 8 exactly.
        conn.execute_batch(
            "CREATE TABLE audit_logs (
                id            TEXT PRIMARY KEY NOT NULL,
                timestamp     TEXT NOT NULL,
                user_id       TEXT NOT NULL,
                action        TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id   TEXT,
                patient_id    TEXT,
                device_id     TEXT NOT NULL,
                success       INTEGER NOT NULL CHECK (success IN (0, 1)),
                details       TEXT,
                previous_hash TEXT NOT NULL,
                entry_hash    TEXT NOT NULL UNIQUE
            );
            CREATE TRIGGER audit_logs_no_update
            BEFORE UPDATE ON audit_logs
            BEGIN
                SELECT RAISE(ABORT, 'audit_logs rows are immutable: UPDATE is not allowed');
            END;
            CREATE TRIGGER audit_logs_no_delete
            BEFORE DELETE ON audit_logs
            BEGIN
                SELECT RAISE(ABORT, 'audit_logs rows are immutable: DELETE is not allowed');
            END;",
        )
        .unwrap();
        conn
    }

    fn sample_input() -> AuditEntryInput {
        AuditEntryInput {
            user_id: "user-123".to_string(),
            action: "fhir.create".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some("pat-456".to_string()),
            patient_id: Some("pat-456".to_string()),
            device_id: "device-abc".to_string(),
            success: true,
            details: None,
        }
    }

    // ── Must-Have 1: 9 required HIPAA fields are persisted ───────────────

    #[test]
    fn write_persists_all_nine_hipaa_fields() {
        let conn = test_db();
        let entry = write_audit_entry(&conn, sample_input()).unwrap();

        // Read back from DB and assert all 9 mandatory fields are present.
        let (timestamp, user_id, action, resource_type, resource_id,
             patient_id, device_id, success, details): (
            String,          // timestamp
            String,          // user_id
            String,          // action
            String,          // resource_type
            String,          // resource_id  (NOT NULL in sample)
            Option<String>,  // patient_id
            String,          // device_id
            i32,             // success
            Option<String>,  // details
        ) = conn
            .query_row(
                "SELECT timestamp, user_id, action, resource_type, resource_id,
                         patient_id, device_id, success, details
                  FROM audit_logs WHERE id = ?1",
                rusqlite::params![entry.id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .unwrap();

        assert!(!timestamp.is_empty(), "timestamp missing");
        assert_eq!(user_id,    "user-123",   "user_id mismatch");
        assert_eq!(action,     "fhir.create", "action mismatch");
        assert_eq!(resource_type, "Patient", "resource_type mismatch");
        assert_eq!(resource_id,   "pat-456", "resource_id mismatch");
        assert_eq!(patient_id.as_deref(), Some("pat-456"), "patient_id mismatch");
        assert_eq!(device_id,  "device-abc", "device_id mismatch");
        assert_eq!(success,    1i32,          "success should be 1");
        assert!(details.is_none(),            "details should be None");
    }

    // ── Must-Have 2: Hash chain integrity ────────────────────────────────

    #[test]
    fn first_row_has_genesis_previous_hash() {
        let conn = test_db();
        let entry = write_audit_entry(&conn, sample_input()).unwrap();
        assert_eq!(entry.previous_hash, "GENESIS");
    }

    #[test]
    fn hash_chain_links_consecutive_rows() {
        let conn = test_db();
        let e1 = write_audit_entry(&conn, sample_input()).unwrap();
        let e2 = write_audit_entry(&conn, sample_input()).unwrap();
        let e3 = write_audit_entry(&conn, sample_input()).unwrap();

        // Row N's previous_hash == row N-1's entry_hash
        assert_eq!(e2.previous_hash, e1.entry_hash, "chain broken between e1 and e2");
        assert_eq!(e3.previous_hash, e2.entry_hash, "chain broken between e2 and e3");
    }

    #[test]
    fn entry_hash_equals_computed_hash() {
        let conn = test_db();
        let e = write_audit_entry(&conn, sample_input()).unwrap();

        let expected = compute_hash(
            &e.previous_hash,
            &e.id,
            &e.timestamp,
            &e.user_id,
            &e.action,
            &e.resource_type,
            e.resource_id.as_deref(),
            e.patient_id.as_deref(),
            &e.device_id,
            e.success,
            e.details.as_deref(),
        );
        assert_eq!(e.entry_hash, expected);
    }

    // ── Must-Have 3: Immutability triggers ───────────────────────────────

    #[test]
    fn update_is_rejected_by_trigger() {
        let conn = test_db();
        let entry = write_audit_entry(&conn, sample_input()).unwrap();

        let result = conn.execute(
            "UPDATE audit_logs SET action = 'tampered' WHERE id = ?1",
            rusqlite::params![entry.id],
        );
        assert!(result.is_err(), "UPDATE should be rejected by trigger");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("immutable") || err_msg.contains("not allowed"),
            "unexpected error message: {err_msg}"
        );
    }

    #[test]
    fn delete_is_rejected_by_trigger() {
        let conn = test_db();
        let entry = write_audit_entry(&conn, sample_input()).unwrap();

        let result = conn.execute(
            "DELETE FROM audit_logs WHERE id = ?1",
            rusqlite::params![entry.id],
        );
        assert!(result.is_err(), "DELETE should be rejected by trigger");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("immutable") || err_msg.contains("not allowed"),
            "unexpected error message: {err_msg}"
        );
    }

    // ── Must-Have 4: First row has GENESIS ───────────────────────────────
    // (covered by first_row_has_genesis_previous_hash above)

    // ── Must-Have 5: Failed access is recorded with success = false ──────

    #[test]
    fn failed_access_records_success_false() {
        let conn = test_db();
        let input = AuditEntryInput {
            success: false,
            details: Some("Permission denied: insufficient role".to_string()),
            ..sample_input()
        };
        let entry = write_audit_entry(&conn, input).unwrap();
        assert!(!entry.success);

        let success_db: i32 = conn
            .query_row(
                "SELECT success FROM audit_logs WHERE id = ?1",
                rusqlite::params![entry.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(success_db, 0, "success flag should be 0 in DB");

        let details: Option<String> = conn
            .query_row(
                "SELECT details FROM audit_logs WHERE id = ?1",
                rusqlite::params![entry.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(details.is_some(), "details should be present for failed entry");
    }

    // ── compute_hash determinism ──────────────────────────────────────────

    #[test]
    fn compute_hash_is_deterministic() {
        let h1 = compute_hash("GENESIS", "id1", "2026-01-01T00:00:00Z",
            "u1", "fhir.get", "Patient", Some("p1"), Some("p1"), "dev1", true, None);
        let h2 = compute_hash("GENESIS", "id1", "2026-01-01T00:00:00Z",
            "u1", "fhir.get", "Patient", Some("p1"), Some("p1"), "dev1", true, None);
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_hash_changes_on_any_field_mutation() {
        let base = compute_hash("GENESIS", "id1", "t", "u", "a", "r", None, None, "d", true, None);
        assert_ne!(base, compute_hash("OTHER", "id1", "t", "u", "a", "r", None, None, "d", true, None));
        assert_ne!(base, compute_hash("GENESIS", "id2", "t", "u", "a", "r", None, None, "d", true, None));
        assert_ne!(base, compute_hash("GENESIS", "id1", "t2", "u", "a", "r", None, None, "d", true, None));
        assert_ne!(base, compute_hash("GENESIS", "id1", "t", "u", "a", "r", None, None, "d", false, None));
    }
}
