use serde::Serialize;
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::biometric;
use crate::auth::password;
use crate::auth::session::SessionManager;
use crate::auth::totp;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;

/// Response from biometric availability check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricStatus {
    pub available: bool,
    pub enabled: bool,
}

/// Begin TOTP setup by generating a secret and QR code.
///
/// Returns the setup data (secret, otpauth URL, QR code) to the frontend
/// for display. The secret is NOT stored in the database yet -- the user
/// must verify with a valid code via `verify_totp_setup` to confirm enrollment.
#[tauri::command]
pub fn setup_totp(session: State<'_, SessionManager>) -> Result<totp::TotpSetup, AppError> {
    // Require active session
    let (_, _) = session.get_current_user()?;

    // Get username for the TOTP label
    let state = session.get_state();
    let user_id = state
        .user_id
        .ok_or_else(|| AppError::Authentication("No active user".to_string()))?;

    // Generate TOTP setup (uses user_id as account name in otpauth URL)
    totp::generate_totp_setup(&user_id)
}

/// Verify and finalize TOTP enrollment.
///
/// The user must provide the secret (returned from `setup_totp`) and a valid
/// code from their authenticator app. Only after successful verification is
/// the TOTP secret stored in the database and MFA enabled.
#[tauri::command]
pub fn verify_totp_setup(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    secret_base32: String,
    code: String,
) -> Result<String, AppError> {
    // Require active session
    let (user_id, _) = session.get_current_user()?;

    // Verify the code against the provided secret
    let valid = totp::verify_totp(&secret_base32, &code)?;
    if !valid {
        return Err(AppError::Authentication(
            "Invalid TOTP code. Please try again with a new code from your authenticator app."
                .to_string(),
        ));
    }

    // Code verified -- store the secret and enable TOTP
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE users SET totp_secret = ?1, totp_enabled = 1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![secret_base32, user_id],
    )?;

    Ok("TOTP enabled".to_string())
}

/// Disable TOTP for the current user.
///
/// Requires password re-entry as a security measure to prevent unauthorized
/// disabling of MFA.
#[tauri::command]
pub fn disable_totp(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    password: String,
) -> Result<(), AppError> {
    let (user_id, _) = session.get_current_user()?;

    // Verify password for security
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

    password::verify(&password, &password_hash)?;

    // Clear TOTP secret and disable
    conn.execute(
        "UPDATE users SET totp_secret = NULL, totp_enabled = 0, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id],
    )?;

    Ok(())
}

/// Verify a TOTP code during login flow.
///
/// Called when MFA is required (totp_enabled = true for the user).
/// Returns Ok(true) if the code is valid.
#[tauri::command]
pub fn check_totp(
    db: State<'_, Database>,
    user_id: String,
    code: String,
) -> Result<bool, AppError> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get user's TOTP configuration
    let (totp_secret, totp_enabled): (Option<String>, bool) = conn
        .query_row(
            "SELECT totp_secret, totp_enabled FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound("User not found".to_string()))?;

    if !totp_enabled {
        return Err(AppError::Authentication(
            "TOTP not enabled for this user".to_string(),
        ));
    }

    let secret = totp_secret.ok_or_else(|| {
        AppError::Authentication("TOTP enabled but no secret configured".to_string())
    })?;

    totp::verify_totp(&secret, &code)
}

/// Check biometric (Touch ID) availability and user enrollment status.
#[tauri::command]
pub fn check_biometric(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<BiometricStatus, AppError> {
    let available = biometric::check_biometric_available();

    // Check if user has Touch ID enabled
    let enabled = match session.get_current_user() {
        Ok((user_id, _)) => {
            let conn = db
                .conn
                .lock()
                .map_err(|e| AppError::Database(e.to_string()))?;
            conn.query_row(
                "SELECT touch_id_enabled FROM users WHERE id = ?1",
                rusqlite::params![user_id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false)
        }
        Err(_) => false,
    };

    Ok(BiometricStatus { available, enabled })
}

/// Enable Touch ID for the current user.
///
/// Requires password re-entry for security. Touch ID is only used for
/// session unlock, not initial login (per HIPAA best practices).
#[tauri::command]
pub fn enable_touch_id(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    password: String,
) -> Result<(), AppError> {
    let (user_id, _) = session.get_current_user()?;

    // Verify password for security
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

    password::verify(&password, &password_hash)?;

    // Enable Touch ID
    conn.execute(
        "UPDATE users SET touch_id_enabled = 1, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id],
    )?;

    Ok(())
}

/// Disable Touch ID for the current user.
#[tauri::command]
pub fn disable_touch_id(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let (user_id, _) = session.get_current_user()?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE users SET touch_id_enabled = 0, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id],
    )?;

    Ok(())
}

// ── biometric_authenticate — macOS implementation ──────────────────────────

/// Authenticate the locked session using Touch ID (LAContext).
///
/// The LAContext is created and destroyed on a dedicated OS thread to satisfy
/// the ObjC requirement that `LAContext` is used on the same thread throughout.
/// A synchronous mpsc channel bridges the async ObjC callback back to the
/// calling Rust thread. On success, the session is unlocked and an audit row
/// is written. Both success and failure paths produce audit entries.
///
/// # Errors
/// - `AppError::Authentication("Touch ID is not available on this device")` — no Touch ID hardware/enrollment
/// - `AppError::Authentication("No locked session to unlock")` — session not in locked state
/// - `AppError::Authentication(reason)` — LAContext evaluation failed (user cancelled, hardware error, etc.)
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn biometric_authenticate(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    // ── Pre-flight: check hardware availability ──────────────────────────
    if !biometric::check_biometric_available() {
        return Err(AppError::Authentication(
            "Touch ID is not available on this device".to_string(),
        ));
    }

    // ── Pre-flight: require a locked session ─────────────────────────────
    let state_info = session.get_state();
    let user_id = state_info
        .user_id
        .ok_or_else(|| AppError::Authentication("No locked session to unlock".to_string()))?;
    let session_id = state_info
        .session_id
        .ok_or_else(|| AppError::Authentication("No locked session to unlock".to_string()))?;

    if state_info.state != "locked" {
        return Err(AppError::Authentication(
            "No locked session to unlock".to_string(),
        ));
    }

    let reason_str = biometric::authenticate_biometric_reason();
    let device_id_str = device_id.get().to_string();
    let user_id_clone = user_id.clone();

    // ── Spawn a blocking thread so we don't block the async executor ──────
    // Inside the blocking task we spawn a dedicated OS thread that owns the
    // LAContext for its entire lifetime, satisfying the ObjC threading contract.
    let biometric_result: Result<(), String> =
        tauri::async_runtime::spawn_blocking(move || {
            use std::sync::mpsc;
            use std::thread;

            let (tx, rx) = mpsc::sync_channel::<Result<(), String>>(1);

            // Dedicated thread: LAContext is created, used, and dropped here.
            let handle = thread::spawn(move || {
                use block2::RcBlock;
                use objc2::rc::Retained;
                use objc2::runtime::Bool;
                use objc2_foundation::{NSError, NSString};
                use objc2_local_authentication::{LAContext, LAPolicy};

                // SAFETY: LAContext is created on this thread and never shared.
                // evaluatePolicy_localizedReason_reply calls the block on an
                // internal GCD queue; the block only captures the SyncSender
                // (which is Send), so cross-thread safety is maintained.
                unsafe {
                    let ctx: Retained<LAContext> = LAContext::new();
                    let ns_reason = NSString::from_str(&reason_str);

                    // Build the ObjC reply block. The tx sender is Send + Sync.
                    let block = RcBlock::new(move |success: Bool, err_ptr: *mut NSError| {
                        if success.as_bool() {
                            let _ = tx.send(Ok(()));
                        } else {
                            let msg = if err_ptr.is_null() {
                                "Touch ID authentication failed".to_string()
                            } else {
                                // SAFETY: err_ptr is non-null and owned by LAContext.
                                let err: &NSError = &*err_ptr;
                                err.localizedDescription().to_string()
                            };
                            let _ = tx.send(Err(msg));
                        }
                    });

                    // Kick off the asynchronous LAContext evaluation. The block
                    // fires on an internal queue when the user taps Touch ID.
                    ctx.evaluatePolicy_localizedReason_reply(
                        LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
                        &ns_reason,
                        &*block,
                    );

                    // Block this thread until the ObjC callback fires.
                    rx.recv()
                        .unwrap_or_else(|_| Err("Touch ID channel closed".to_string()))
                }
            });

            handle
                .join()
                .unwrap_or_else(|_| Err("Touch ID thread panicked".to_string()))
        })
        .await
        .map_err(|e| AppError::Authentication(format!("Touch ID task failed: {}", e)))?;

    // ── Audit: write entry for both success and failure paths ────────────
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    match biometric_result {
        Ok(()) => {
            // Unlock the session in-memory
            session.unlock(&user_id)?;

            // Update the sessions row
            conn.execute(
                "UPDATE sessions SET state = 'active', last_activity = datetime('now') WHERE id = ?1",
                rusqlite::params![session_id],
            )?;

            // Audit: success
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: user_id_clone,
                    action: "auth.biometric.unlock".to_string(),
                    resource_type: "auth".to_string(),
                    resource_id: None,
                    patient_id: None,
                    device_id: device_id_str,
                    success: true,
                    details: None,
                },
            )?;

            Ok(())
        }
        Err(err_msg) => {
            // Audit: failure
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: user_id_clone,
                    action: "auth.biometric.failed".to_string(),
                    resource_type: "auth".to_string(),
                    resource_id: None,
                    patient_id: None,
                    device_id: device_id_str,
                    success: false,
                    details: Some(err_msg.clone()),
                },
            )?;

            Err(AppError::Authentication(err_msg))
        }
    }
}

/// Biometric authenticate — non-macOS fallback (always returns unavailable).
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn biometric_authenticate(
    _db: State<'_, Database>,
    _session: State<'_, SessionManager>,
    _device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    Err(AppError::Authentication(
        "Biometric not available".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that check_biometric_available() is callable and returns a bool
    /// on any platform. The function must compile and execute without panicking.
    #[test]
    fn biometric_check_available_returns_bool() {
        let result: bool = biometric::check_biometric_available();
        // On CI/non-Touch-ID hardware this will be false; on enrolled hardware true.
        // We only verify the function is callable and returns the right type.
        let _ = result;
    }
}
