use serde::Serialize;
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::password;
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::roles::Role;

/// Response from break-glass activation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakGlassResponse {
    pub log_id: String,
    pub expires_at: String,
}

/// Activate emergency break-glass access.
///
/// HIPAA requires:
/// - Documented justification (reason is mandatory and non-empty)
/// - Password re-entry for identity verification
/// - Time-limited session (30 minutes)
/// - Full audit logging
///
/// Scoped to clinical_records:read only.
/// Writes an audit row with action BREAK_GLASS_ACTIVATE on both success and failure paths.
#[tauri::command]
pub fn activate_break_glass(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    reason: String,
    password: String,
    patient_id: Option<String>,
) -> Result<BreakGlassResponse, AppError> {
    // 1. Require active session
    let (user_id, role_raw) = session.get_current_user()?;
    let role = Role::from_str(&role_raw)?.as_str().to_string();

    // 2. Validate reason is non-empty (HIPAA requires documented justification)
    if reason.trim().is_empty() {
        // Write failure audit before acquiring the lock (we still need a conn).
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "break_glass.activate".to_string(),
                resource_type: "break_glass".to_string(),
                resource_id: None,
                patient_id: patient_id.clone(),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Break-glass reason is required (HIPAA mandate)".to_string()),
            },
        );
        return Err(AppError::Validation(
            "Break-glass reason is required (HIPAA mandate)".to_string(),
        ));
    }

    // 3. Re-verify password for security
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let password_hash: String = conn
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::Authentication("User not found".to_string()))?;

    if let Err(e) = password::verify(&password, &password_hash) {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "break_glass.activate".to_string(),
                resource_type: "break_glass".to_string(),
                resource_id: None,
                patient_id: patient_id.clone(),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Password verification failed".to_string()),
            },
        );
        return Err(e);
    }

    // 4. Create 30-minute expiry
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(30);

    // 5. Log break-glass activation in break_glass_log table
    let log_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO break_glass_log (id, user_id, reason, patient_id, activated_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            log_id,
            user_id.clone(),
            reason.trim(),
            patient_id.clone(),
            now,
            expires_at.to_rfc3339()
        ],
    )?;

    // 6. Audit the activation (success)
    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: user_id.clone(),
            action: "break_glass.activate".to_string(),
            resource_type: "break_glass".to_string(),
            resource_id: Some(log_id.clone()),
            patient_id: patient_id.clone(),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("reason: {}", reason.trim())),
        },
    );

    // 7. Elevate session permissions -- scoped to clinical read-only
    session.activate_break_glass(
        user_id,
        role,
        vec!["clinicalrecords:read".to_string()],
        expires_at,
    )?;

    Ok(BreakGlassResponse {
        log_id,
        expires_at: expires_at.to_rfc3339(),
    })
}

/// Deactivate break-glass access and restore original role.
///
/// Updates the break_glass_log with deactivation timestamp.
/// Writes an audit row with action BREAK_GLASS_DEACTIVATE.
#[tauri::command]
pub fn deactivate_break_glass(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    // Get current user before deactivating to find the log entry
    let (user_id, _role) = session.get_current_user()?;

    // Deactivate break-glass in session (transitions back to Active)
    session.deactivate_break_glass()?;

    // Update the break_glass_log -- mark the most recent active entry as deactivated
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE break_glass_log SET deactivated_at = ?1
         WHERE user_id = ?2 AND deactivated_at IS NULL",
        rusqlite::params![now, user_id.clone()],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "break_glass.deactivate".to_string(),
            resource_type: "break_glass".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(())
}
