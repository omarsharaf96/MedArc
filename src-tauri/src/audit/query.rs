/// audit/query.rs — Read path and hash-chain verification for audit logs.
///
/// Provides:
/// - `query_audit_log()`: paginated, role-scoped query for the frontend viewer.
/// - `verify_audit_chain()`: cryptographic chain integrity check — walks every row
///   in insertion order and verifies each `entry_hash` matches its recomputed value
///   and that `previous_hash` equals the prior row's `entry_hash`.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::audit::entry::{compute_hash, AuditEntry};
use crate::error::AppError;

/// Filter parameters for querying the audit log.
///
/// All filters are optional and additive (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    /// Restrict to entries authored by this user (Provider viewing own logs).
    pub user_id: Option<String>,
    /// Restrict to entries touching this patient's ePHI.
    pub patient_id: Option<String>,
    /// Restrict to entries with this action string.
    pub action: Option<String>,
    /// ISO-8601 lower bound for `timestamp` (inclusive).
    pub from: Option<String>,
    /// ISO-8601 upper bound for `timestamp` (inclusive).
    pub to: Option<String>,
    /// Page size (default 50, max 200).
    pub limit: Option<u32>,
    /// Zero-based page offset.
    pub offset: Option<u32>,
}

/// Paginated result set from `query_audit_log`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogPage {
    pub entries: Vec<AuditEntry>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

/// Result of a chain verification run.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainVerificationResult {
    /// True if every row's hash recomputes correctly and links to the prior row.
    pub valid: bool,
    /// Total number of rows inspected.
    pub rows_checked: usize,
    /// If `valid` is false, describes the first broken link.
    pub error: Option<String>,
}

/// Query audit log entries with optional filters, sorted oldest-first.
///
/// Role scoping (Provider sees only own entries, SystemAdmin sees all) is
/// enforced by the caller passing an appropriate `user_id` filter; this
/// function does not enforce RBAC itself.
pub fn query_audit_log(conn: &Connection, query: AuditQuery) -> Result<AuditLogPage, AppError> {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    // Build WHERE clauses dynamically.
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(uid) = &query.user_id {
        params.push(Box::new(uid.clone()));
        conditions.push(format!("user_id = ?{}", params.len()));
    }
    if let Some(pid) = &query.patient_id {
        params.push(Box::new(pid.clone()));
        conditions.push(format!("patient_id = ?{}", params.len()));
    }
    if let Some(act) = &query.action {
        params.push(Box::new(act.clone()));
        conditions.push(format!("action = ?{}", params.len()));
    }
    if let Some(from) = &query.from {
        params.push(Box::new(from.clone()));
        conditions.push(format!("timestamp >= ?{}", params.len()));
    }
    if let Some(to) = &query.to {
        params.push(Box::new(to.clone()));
        conditions.push(format!("timestamp <= ?{}", params.len()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count matching rows.
    let count_sql = format!("SELECT COUNT(*) FROM audit_logs {}", where_clause);
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let total: i64 = conn.query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))?;

    // Fetch page.
    let select_sql = format!(
        "SELECT id, timestamp, user_id, action, resource_type, resource_id,
                patient_id, device_id, success, details, previous_hash, entry_hash
         FROM audit_logs
         {}
         ORDER BY rowid ASC
         LIMIT {} OFFSET {}",
        where_clause, limit, offset
    );

    let mut stmt = conn.prepare(&select_sql)?;
    let entries: Vec<AuditEntry> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                user_id: row.get(2)?,
                action: row.get(3)?,
                resource_type: row.get(4)?,
                resource_id: row.get(5)?,
                patient_id: row.get(6)?,
                device_id: row.get(7)?,
                success: row.get::<_, i32>(8)? != 0,
                details: row.get(9)?,
                previous_hash: row.get(10)?,
                entry_hash: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AuditLogPage {
        entries,
        total,
        limit,
        offset,
    })
}

/// Verify the integrity of the entire audit log hash chain.
///
/// Walks every row in insertion order (by rowid) and checks:
/// 1. The stored `previous_hash` equals the prior row's `entry_hash`
///    (or "GENESIS" for the first row).
/// 2. The stored `entry_hash` equals `compute_hash(...)` over this row's fields.
///
/// Returns immediately on the first broken link with a descriptive error.
pub fn verify_audit_chain(conn: &Connection) -> Result<ChainVerificationResult, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, user_id, action, resource_type, resource_id,
                patient_id, device_id, success, details, previous_hash, entry_hash
         FROM audit_logs
         ORDER BY rowid ASC",
    )?;

    let rows: Vec<AuditEntry> = stmt
        .query_map([], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                user_id: row.get(2)?,
                action: row.get(3)?,
                resource_type: row.get(4)?,
                resource_id: row.get(5)?,
                patient_id: row.get(6)?,
                device_id: row.get(7)?,
                success: row.get::<_, i32>(8)? != 0,
                details: row.get(9)?,
                previous_hash: row.get(10)?,
                entry_hash: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut expected_previous = "GENESIS".to_string();

    for (i, row) in rows.iter().enumerate() {
        // Check that previous_hash matches the chain expectation.
        if row.previous_hash != expected_previous {
            return Ok(ChainVerificationResult {
                valid: false,
                rows_checked: i,
                error: Some(format!(
                    "Row {} (id={}): previous_hash='{}' expected '{}'",
                    i, row.id, row.previous_hash, expected_previous
                )),
            });
        }

        // Recompute and compare entry_hash.
        let recomputed = compute_hash(
            &row.previous_hash,
            &row.id,
            &row.timestamp,
            &row.user_id,
            &row.action,
            &row.resource_type,
            row.resource_id.as_deref(),
            row.patient_id.as_deref(),
            &row.device_id,
            row.success,
            row.details.as_deref(),
        );

        if row.entry_hash != recomputed {
            return Ok(ChainVerificationResult {
                valid: false,
                rows_checked: i,
                error: Some(format!(
                    "Row {} (id={}): entry_hash mismatch — stored='{}' recomputed='{}'",
                    i, row.id, row.entry_hash, recomputed
                )),
            });
        }

        expected_previous = row.entry_hash.clone();
    }

    Ok(ChainVerificationResult {
        valid: true,
        rows_checked: rows.len(),
        error: None,
    })
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::entry::{write_audit_entry, AuditEntryInput};
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
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
            BEGIN SELECT RAISE(ABORT, 'audit_logs rows are immutable: UPDATE is not allowed'); END;
            CREATE TRIGGER audit_logs_no_delete
            BEFORE DELETE ON audit_logs
            BEGIN SELECT RAISE(ABORT, 'audit_logs rows are immutable: DELETE is not allowed'); END;",
        )
        .unwrap();
        conn
    }

    fn make_input(user: &str, patient: Option<&str>, success: bool) -> AuditEntryInput {
        AuditEntryInput {
            user_id: user.to_string(),
            action: "fhir.get".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: patient.map(str::to_string),
            patient_id: patient.map(str::to_string),
            device_id: "dev-1".to_string(),
            success,
            details: None,
        }
    }

    // ── query_audit_log tests ─────────────────────────────────────────────

    #[test]
    fn query_returns_all_rows_when_no_filters() {
        let conn = test_db();
        write_audit_entry(&conn, make_input("alice", Some("p1"), true)).unwrap();
        write_audit_entry(&conn, make_input("bob", Some("p2"), true)).unwrap();
        write_audit_entry(&conn, make_input("alice", Some("p3"), false)).unwrap();

        let page = query_audit_log(&conn, AuditQuery::default()).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.entries.len(), 3);
    }

    #[test]
    fn query_filters_by_user_id() {
        let conn = test_db();
        write_audit_entry(&conn, make_input("alice", None, true)).unwrap();
        write_audit_entry(&conn, make_input("bob", None, true)).unwrap();
        write_audit_entry(&conn, make_input("alice", None, true)).unwrap();

        let page = query_audit_log(
            &conn,
            AuditQuery {
                user_id: Some("alice".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.entries.iter().all(|e| e.user_id == "alice"));
    }

    #[test]
    fn query_filters_by_patient_id() {
        let conn = test_db();
        write_audit_entry(&conn, make_input("u1", Some("p1"), true)).unwrap();
        write_audit_entry(&conn, make_input("u2", Some("p2"), true)).unwrap();

        let page = query_audit_log(
            &conn,
            AuditQuery {
                patient_id: Some("p1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].patient_id.as_deref(), Some("p1"));
    }

    #[test]
    fn query_pagination_works() {
        let conn = test_db();
        for _ in 0..10 {
            write_audit_entry(&conn, make_input("u", None, true)).unwrap();
        }
        let page = query_audit_log(
            &conn,
            AuditQuery {
                limit: Some(3),
                offset: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 10);
        assert_eq!(page.entries.len(), 3);

        let page2 = query_audit_log(
            &conn,
            AuditQuery {
                limit: Some(3),
                offset: Some(9),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page2.entries.len(), 1); // last row
    }

    // ── verify_audit_chain tests ──────────────────────────────────────────

    #[test]
    fn empty_chain_is_valid() {
        let conn = test_db();
        let result = verify_audit_chain(&conn).unwrap();
        assert!(result.valid);
        assert_eq!(result.rows_checked, 0);
        assert!(result.error.is_none());
    }

    #[test]
    fn valid_chain_passes_verification() {
        let conn = test_db();
        write_audit_entry(&conn, make_input("u", None, true)).unwrap();
        write_audit_entry(&conn, make_input("u", None, true)).unwrap();
        write_audit_entry(&conn, make_input("u", None, false)).unwrap();

        let result = verify_audit_chain(&conn).unwrap();
        assert!(
            result.valid,
            "expected valid chain, got: {:?}",
            result.error
        );
        assert_eq!(result.rows_checked, 3);
    }

    #[test]
    fn tampered_entry_hash_fails_verification() {
        let conn = test_db();
        write_audit_entry(&conn, make_input("u", None, true)).unwrap();
        write_audit_entry(&conn, make_input("u", None, true)).unwrap();

        // We need to bypass the trigger to inject a tampered row directly.
        // We do this by inserting a new row with a fabricated (wrong) entry_hash.
        // This simulates a row where the hash was forged post-insertion.
        //
        // Note: we can't UPDATE (trigger blocks it), so instead we verify that
        // chain verification catches inconsistency introduced at insert time
        // by manually inserting a row with a bad hash via a raw SQL call that
        // doesn't go through write_audit_entry.
        conn.execute(
            "INSERT INTO audit_logs
                (id, timestamp, user_id, action, resource_type, resource_id,
                 patient_id, device_id, success, details, previous_hash, entry_hash)
             VALUES ('bad-id', '2026-01-01T00:00:00Z', 'u', 'fhir.get', 'Patient',
                     NULL, NULL, 'dev', 1, NULL, 'WRONG_PREVIOUS', 'BADHASH')",
            [],
        )
        .unwrap();

        let result = verify_audit_chain(&conn).unwrap();
        assert!(!result.valid, "expected invalid chain");
        assert!(result.error.is_some());
    }
}
