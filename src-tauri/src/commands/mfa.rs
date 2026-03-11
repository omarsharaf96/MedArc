use serde::Serialize;
use tauri::State;

use crate::auth::biometric;
use crate::auth::password;
use crate::auth::session::SessionManager;
use crate::auth::totp;
use crate::db::connection::Database;
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
