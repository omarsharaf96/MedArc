/// commands/audit.rs — Tauri commands for viewing and verifying the audit log.
///
/// Two commands are exposed:
/// - `get_audit_log`: Role-scoped paginated retrieval.
///   Provider sees only their own entries (user_id filter enforced here).
///   SystemAdmin sees all entries.
///   Any other role returns Unauthorized.
/// - `verify_audit_chain`: Chain integrity check (SystemAdmin only).
///   Returns a boolean `valid` field plus row count and optional error description.
use tauri::State;

use crate::audit::query::{query_audit_log, verify_audit_chain, AuditLogPage, AuditQuery, ChainVerificationResult};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::error::AppError;

/// Return a paginated, role-scoped page of audit log entries.
///
/// Role enforcement:
/// - Provider   → `user_id` filter is locked to their own ID (cannot see others' entries).
/// - SystemAdmin → no user_id filter is applied; all entries are visible.
/// - All other roles → `Unauthorized` error.
///
/// The `query` parameter accepts optional filters for `patientId`, `action`,
/// `from` / `to` date bounds, `limit`, and `offset`. For Providers the `userId`
/// field inside the query is ignored and overridden with the session user ID.
#[tauri::command]
pub fn get_audit_log(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    query: Option<AuditQuery>,
) -> Result<AuditLogPage, AppError> {
    // Resolve the caller's identity and role.
    let (caller_id, role) = session.get_current_user()?;

    // Build the effective query, enforcing role-based scope.
    let mut effective_query = query.unwrap_or_default();

    match role.as_str() {
        "Provider" => {
            // Override whatever user_id filter the frontend may have sent.
            effective_query.user_id = Some(caller_id.clone());
        }
        "SystemAdmin" => {
            // SystemAdmin may pass an optional user_id filter; do not override.
        }
        _ => {
            return Err(AppError::Unauthorized(format!(
                "Role '{}' is not permitted to access audit logs",
                role
            )));
        }
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    query_audit_log(&conn, effective_query)
}

/// Verify the cryptographic hash chain integrity of the entire audit log.
///
/// Restricted to SystemAdmin only. Walks every row in insertion order and
/// checks that each row's stored `entry_hash` matches the recomputed value
/// and that `previous_hash` equals the prior row's `entry_hash`.
///
/// Returns `{ valid: bool, rowsChecked: usize, error: Option<String> }`.
#[tauri::command]
pub fn verify_audit_chain_cmd(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<ChainVerificationResult, AppError> {
    let (_, role) = session.get_current_user()?;

    if role != "SystemAdmin" {
        return Err(AppError::Unauthorized(
            "Only SystemAdmin may verify the audit chain".to_string(),
        ));
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    verify_audit_chain(&conn)
}
