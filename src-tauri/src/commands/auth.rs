use tauri::State;

use crate::auth::password;
use crate::auth::session::SessionManager;
use crate::auth::totp;
use crate::db::connection::Database;
use crate::db::models::user::UserResponse;
use crate::error::AppError;

/// Login response combining user info and session info.
/// When mfa_required is true, session is a partial placeholder and
/// the frontend must call complete_login with a valid TOTP code.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub user: UserResponse,
    pub session: crate::auth::session::SessionInfo,
    pub mfa_required: bool,
    /// Present when mfa_required is true; the frontend passes this to complete_login.
    pub pending_user_id: Option<String>,
}

/// Register a new user account.
///
/// Rules:
/// - First user can be created without auth (bootstrap)
/// - Subsequent users require current user to be SystemAdmin
/// - Password must be >= 12 characters
/// - Username must be unique
#[tauri::command]
pub fn register_user(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    username: String,
    password: String,
    display_name: String,
    role: String,
) -> Result<UserResponse, AppError> {
    // Validate password length
    if password.len() < 12 {
        return Err(AppError::Validation(
            "Password must be at least 12 characters".to_string(),
        ));
    }

    // Validate role
    let valid_roles = [
        "SystemAdmin",
        "Physician",
        "Nurse",
        "MedicalAssistant",
        "FrontDesk",
    ];
    if !valid_roles.contains(&role.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid role: {}. Must be one of: {}",
            role,
            valid_roles.join(", ")
        )));
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Check authorization: first-run bootstrap or SystemAdmin only
    let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;

    if user_count > 0 {
        // Not first-run, require SystemAdmin
        let (_, current_role) = session.get_current_user()?;
        if current_role != "SystemAdmin" {
            return Err(AppError::Unauthorized(
                "Only SystemAdmin can create new users".to_string(),
            ));
        }
    }

    // Hash the password
    let password_hash = password::hash_password(&password)?;

    // Generate user ID
    let user_id = uuid::Uuid::new_v4().to_string();

    // Insert user into database
    conn.execute(
        "INSERT INTO users (id, username, password_hash, display_name, role) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![user_id, username, password_hash, display_name, role],
    ).map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed: users.username") {
            AppError::Validation(format!("Username '{}' is already taken", username))
        } else {
            AppError::Database(e.to_string())
        }
    })?;

    Ok(UserResponse {
        id: user_id,
        username,
        display_name,
        role,
    })
}

/// Log in with username and password.
///
/// Returns user info and session info on success.
/// Increments failed_login_attempts on failure; locks account after max attempts.
#[tauri::command]
pub fn login(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    username: String,
    password: String,
) -> Result<LoginResponse, AppError> {
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Look up user by username
    let user_row = conn.query_row(
        "SELECT id, username, password_hash, display_name, role, is_active, failed_login_attempts, locked_until FROM users WHERE username = ?1",
        rusqlite::params![username],
        |row| {
            Ok((
                row.get::<_, String>(0)?,  // id
                row.get::<_, String>(1)?,  // username
                row.get::<_, String>(2)?,  // password_hash
                row.get::<_, String>(3)?,  // display_name
                row.get::<_, String>(4)?,  // role
                row.get::<_, bool>(5)?,    // is_active
                row.get::<_, i32>(6)?,     // failed_login_attempts
                row.get::<_, Option<String>>(7)?, // locked_until
            ))
        },
    ).map_err(|_| AppError::Authentication("Invalid credentials".to_string()))?;

    let (user_id, _username, password_hash, display_name, role, is_active, failed_attempts, locked_until) = user_row;

    // Check if account is active
    if !is_active {
        return Err(AppError::Authentication("Invalid credentials".to_string()));
    }

    // Check if account is locked
    if let Some(ref lock_time) = locked_until {
        if let Ok(lock_dt) = chrono::NaiveDateTime::parse_from_str(lock_time, "%Y-%m-%d %H:%M:%S") {
            let lock_utc = lock_dt.and_utc();
            if chrono::Utc::now() < lock_utc {
                return Err(AppError::Authentication(
                    "Account is temporarily locked due to too many failed login attempts".to_string(),
                ));
            }
            // Lock has expired, reset
            conn.execute(
                "UPDATE users SET locked_until = NULL, failed_login_attempts = 0 WHERE id = ?1",
                rusqlite::params![user_id],
            )?;
        }
    }

    // Verify password
    if password::verify(&password, &password_hash).is_err() {
        // Increment failed attempts
        let new_attempts = failed_attempts + 1;

        // Read max_failed_logins from app_settings
        let max_failed: i32 = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'max_failed_logins'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);

        if new_attempts >= max_failed {
            // Read lockout duration
            let lockout_minutes: i64 = conn
                .query_row(
                    "SELECT value FROM app_settings WHERE key = 'lockout_duration_minutes'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30);

            let lock_until = chrono::Utc::now() + chrono::Duration::minutes(lockout_minutes);
            conn.execute(
                "UPDATE users SET failed_login_attempts = ?1, locked_until = ?2 WHERE id = ?3",
                rusqlite::params![
                    new_attempts,
                    lock_until.format("%Y-%m-%d %H:%M:%S").to_string(),
                    user_id
                ],
            )?;
        } else {
            conn.execute(
                "UPDATE users SET failed_login_attempts = ?1 WHERE id = ?2",
                rusqlite::params![new_attempts, user_id],
            )?;
        }

        return Err(AppError::Authentication("Invalid credentials".to_string()));
    }

    // Reset failed attempts on success
    conn.execute(
        "UPDATE users SET failed_login_attempts = 0, locked_until = NULL, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id],
    )?;

    // Check if user has TOTP MFA enabled
    let totp_enabled: bool = conn
        .query_row(
            "SELECT totp_enabled FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if totp_enabled {
        // MFA is required -- do NOT create a full session yet.
        // Return a partial response so the frontend can prompt for TOTP code.
        let placeholder_session = crate::auth::session::SessionInfo {
            session_id: None,
            user_id: Some(user_id.clone()),
            role: Some(role.clone()),
            state: "unauthenticated".to_string(),
            last_activity: None,
        };

        return Ok(LoginResponse {
            user: UserResponse {
                id: user_id.clone(),
                username,
                display_name,
                role,
            },
            session: placeholder_session,
            mfa_required: true,
            pending_user_id: Some(user_id),
        });
    }

    // No MFA -- create full session immediately
    let session_id = session.login(&user_id, &role)?;

    // Insert session row into sessions table
    conn.execute(
        "INSERT INTO sessions (id, user_id, state, last_activity) VALUES (?1, ?2, 'active', datetime('now'))",
        rusqlite::params![session_id, user_id],
    )?;

    let session_info = session.get_state();

    Ok(LoginResponse {
        user: UserResponse {
            id: user_id,
            username,
            display_name,
            role,
        },
        session: session_info,
        mfa_required: false,
        pending_user_id: None,
    })
}

/// Log out the current user, ending their session.
#[tauri::command]
pub fn logout(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<(), AppError> {
    // Get session info before logout to update DB
    let session_info = session.get_state();

    session.logout()?;

    // Update session row in database
    if let Some(session_id) = session_info.session_id {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET state = 'expired' WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
    }

    Ok(())
}

/// Complete login after MFA verification.
///
/// Called when the initial login returned mfa_required=true. The frontend
/// must provide the user_id from the pending login and a valid TOTP code.
/// Only after TOTP verification does this command create a full session.
#[tauri::command]
pub fn complete_login(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    user_id: String,
    totp_code: String,
) -> Result<LoginResponse, AppError> {
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch the user's TOTP secret and verify the code
    let (totp_secret, totp_enabled, username, display_name, role): (
        Option<String>,
        bool,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT totp_secret, totp_enabled, username, display_name, role FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|_| AppError::Authentication("Invalid credentials".to_string()))?;

    if !totp_enabled {
        return Err(AppError::Authentication(
            "MFA is not enabled for this user".to_string(),
        ));
    }

    let secret = totp_secret.ok_or_else(|| {
        AppError::Authentication("TOTP enabled but no secret configured".to_string())
    })?;

    // Verify the TOTP code
    let valid = totp::verify_totp(&secret, &totp_code)?;
    if !valid {
        return Err(AppError::Authentication("Invalid verification code".to_string()));
    }

    // TOTP verified -- now create the full session
    let session_id = session.login(&user_id, &role)?;

    conn.execute(
        "INSERT INTO sessions (id, user_id, state, last_activity) VALUES (?1, ?2, 'active', datetime('now'))",
        rusqlite::params![session_id, user_id],
    )?;

    let session_info = session.get_state();

    Ok(LoginResponse {
        user: UserResponse {
            id: user_id,
            username,
            display_name,
            role,
        },
        session: session_info,
        mfa_required: false,
        pending_user_id: None,
    })
}

/// Check if this is the first run (no users exist in the database).
///
/// Returns true if no users have been created yet, indicating the frontend
/// should show the first-run registration form with SystemAdmin role locked.
#[tauri::command]
pub fn check_first_run(db: State<'_, Database>) -> Result<bool, AppError> {
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(count == 0)
}
