use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::password;
use crate::auth::session::SessionManager;
use crate::auth::totp;
use crate::db::connection::Database;
use crate::db::models::user::UserResponse;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

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
        "Provider",
        "NurseMa",
        "BillingStaff",
        "FrontDesk",
    ];
    if !valid_roles.contains(&role.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid role: {}. Must be one of: {}",
            role,
            valid_roles.join(", ")
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

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
/// Writes an audit row with action_type LOGIN on both success and failure paths.
#[tauri::command]
pub fn login(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    username: String,
    password: String,
) -> Result<LoginResponse, AppError> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

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

    let (
        user_id,
        _username,
        password_hash,
        display_name,
        role,
        is_active,
        failed_attempts,
        locked_until,
    ) = user_row;

    // Check if account is active
    if !is_active {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Account is inactive".to_string()),
            },
        );
        return Err(AppError::Authentication("Invalid credentials".to_string()));
    }

    // Check if account is locked
    if let Some(ref lock_time) = locked_until {
        if let Ok(lock_dt) = chrono::NaiveDateTime::parse_from_str(lock_time, "%Y-%m-%d %H:%M:%S") {
            let lock_utc = lock_dt.and_utc();
            if chrono::Utc::now() < lock_utc {
                write_audit_entry(
                    &conn,
                    AuditEntryInput {
                        user_id: user_id.clone(),
                        action: "auth.login".to_string(),
                        resource_type: "auth".to_string(),
                        resource_id: None,
                        patient_id: None,
                        device_id: device_id.get().to_string(),
                        success: false,
                        details: Some("Account temporarily locked".to_string()),
                    },
                );
                return Err(AppError::Authentication(
                    "Account is temporarily locked due to too many failed login attempts"
                        .to_string(),
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

        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Invalid password".to_string()),
            },
        );
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
        // Record as a pending login (password succeeded but MFA still required).
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("MFA challenge required; session not yet established".to_string()),
            },
        );

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

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: user_id.clone(),
            action: "auth.login".to_string(),
            resource_type: "auth".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

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
/// Writes an audit row with action_type LOGOUT.
#[tauri::command]
pub fn logout(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    // Capture user_id and session_id before the state transition.
    let session_info = session.get_state();
    let user_id_for_audit = session_info
        .user_id
        .clone()
        .unwrap_or_else(|| "UNAUTHENTICATED".to_string());

    session.logout()?;

    // Update session row in database and write audit entry.
    if let Some(session_id) = session_info.session_id {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET state = 'expired' WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id_for_audit,
                action: "auth.logout".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: true,
                details: None,
            },
        );
    }

    Ok(())
}

/// Complete login after MFA verification.
///
/// Called when the initial login returned mfa_required=true. The frontend
/// must provide the user_id from the pending login and a valid TOTP code.
/// Only after TOTP verification does this command create a full session.
/// Writes an audit row with action_type LOGIN on both success and failure paths.
#[tauri::command]
pub fn complete_login(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    user_id: String,
    totp_code: String,
) -> Result<LoginResponse, AppError> {
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

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
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("MFA not enabled for user".to_string()),
            },
        );
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
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Invalid MFA code".to_string()),
            },
        );
        return Err(AppError::Authentication(
            "Invalid verification code".to_string(),
        ));
    }

    // TOTP verified -- now create the full session
    let session_id = session.login(&user_id, &role)?;

    conn.execute(
        "INSERT INTO sessions (id, user_id, state, last_activity) VALUES (?1, ?2, 'active', datetime('now'))",
        rusqlite::params![session_id, user_id],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: user_id.clone(),
            action: "auth.login".to_string(),
            resource_type: "auth".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("MFA verified".to_string()),
        },
    );

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
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(count == 0)
}

// ─── Dev-only bypass ─────────────────────────────────────────────────────────
//
// This command is ONLY compiled into debug builds (`cargo build` / `tauri dev`).
// The `#[cfg(not(debug_assertions))]` branch ensures the bypass is completely
// absent from any release binary — it is not just disabled at runtime, it is
// not present in the compiled artefact at all.
//
// The dev user (username: "dev", role: SystemAdmin) is auto-created on first
// use and then logged in, exactly like a normal login, so session timeout and
// all other session management still applies.

/// Skip the login form and authenticate as a pre-configured dev user.
///
/// Available in debug builds only. Returns Err in release builds.
#[tauri::command]
pub fn dev_bypass_login(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<LoginResponse, AppError> {
    // Guard: completely reject the call in release builds.
    #[cfg(not(debug_assertions))]
    return Err(AppError::Validation(
        "Dev bypass not available in production".to_string(),
    ));

    #[cfg(debug_assertions)]
    {
        const DEV_USERNAME: &str = "dev";
        const DEV_DISPLAY_NAME: &str = "Dev Admin";
        const DEV_ROLE: &str = "SystemAdmin";
        const DEV_PASSWORD: &str = "devpassword123!";

        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Look up (or create) the dev user.
        let existing: Option<(String, String)> = conn
            .query_row(
                "SELECT id, role FROM users WHERE username = ?1",
                rusqlite::params![DEV_USERNAME],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (user_id, role) = if let Some((id, r)) = existing {
            (id, r)
        } else {
            // Auto-create the dev user so the database is never left in an
            // inconsistent state when running from a fresh install.
            let password_hash = password::hash_password(DEV_PASSWORD)?;
            let new_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO users (id, username, password_hash, display_name, role) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![new_id, DEV_USERNAME, password_hash, DEV_DISPLAY_NAME, DEV_ROLE],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
            (new_id, DEV_ROLE.to_string())
        };

        // Create a full session — identical path to a successful normal login.
        let session_id = session.login(&user_id, &role)?;

        conn.execute(
            "INSERT INTO sessions (id, user_id, state, last_activity) VALUES (?1, ?2, 'active', datetime('now'))",
            rusqlite::params![session_id, user_id],
        )?;

        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "auth.dev_bypass_login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: true,
                details: Some("Dev bypass login used".to_string()),
            },
        );

        let session_info = session.get_state();

        Ok(LoginResponse {
            user: UserResponse {
                id: user_id,
                username: DEV_USERNAME.to_string(),
                display_name: DEV_DISPLAY_NAME.to_string(),
                role,
            },
            session: session_info,
            mfa_required: false,
            pending_user_id: None,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Extended user response that includes is_active status, for user management.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserListEntry {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: String,
}

/// List all users in the system.
///
/// SystemAdmin only. Returns id, username, display_name, role, is_active, created_at.
/// Password hash and TOTP secrets are never returned.
#[tauri::command]
pub fn list_users(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<UserListEntry>, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::UserManagement, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT id, username, display_name, role, is_active, created_at
         FROM users
         ORDER BY created_at ASC",
    )?;

    let users: Vec<UserListEntry> = stmt
        .query_map([], |row| {
            Ok(UserListEntry {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
                is_active: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "auth.list_users".to_string(),
            resource_type: "users".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("count={}", users.len())),
        },
    );

    Ok(users)
}

/// Deactivate a user account (sets is_active = false).
///
/// SystemAdmin only. Cannot deactivate your own account.
#[tauri::command]
pub fn deactivate_user(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    user_id_to_deactivate: String,
) -> Result<(), AppError> {
    let (acting_user_id, _role) =
        middleware::check_permission(&session, Resource::UserManagement, Action::Update)?;

    // Prevent self-deactivation
    if acting_user_id == user_id_to_deactivate {
        return Err(AppError::Validation(
            "Cannot deactivate your own account".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let updated = conn.execute(
        "UPDATE users SET is_active = 0, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id_to_deactivate],
    )?;

    if updated == 0 {
        return Err(AppError::NotFound(format!(
            "User '{}' not found",
            user_id_to_deactivate
        )));
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: acting_user_id,
            action: "auth.deactivate_user".to_string(),
            resource_type: "users".to_string(),
            resource_id: Some(user_id_to_deactivate.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("deactivated user_id={}", user_id_to_deactivate)),
        },
    );

    Ok(())
}

/// Update the current user's display name and/or username.
///
/// Requires an active session. Updates are applied to the currently
/// authenticated user only.
#[tauri::command]
pub fn update_user_profile(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    display_name: String,
    username: String,
) -> Result<UserResponse, AppError> {
    let (user_id, role) = session.get_current_user()?;

    if display_name.trim().is_empty() {
        return Err(AppError::Validation(
            "Display name cannot be empty".to_string(),
        ));
    }
    if username.trim().is_empty() {
        return Err(AppError::Validation(
            "Username cannot be empty".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE users SET display_name = ?1, username = ?2, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![display_name.trim(), username.trim(), user_id],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed: users.username") {
            AppError::Validation(format!("Username '{}' is already taken", username.trim()))
        } else {
            AppError::Database(e.to_string())
        }
    })?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: user_id.clone(),
            action: "auth.update_user_profile".to_string(),
            resource_type: "users".to_string(),
            resource_id: Some(user_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "updated display_name='{}', username='{}'",
                display_name.trim(),
                username.trim()
            )),
        },
    );

    Ok(UserResponse {
        id: user_id,
        username: username.trim().to_string(),
        display_name: display_name.trim().to_string(),
        role,
    })
}

/// Change the current user's password.
///
/// Requires the current (old) password to be verified first.
/// New password must be at least 12 characters.
#[tauri::command]
pub fn change_password(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    current_password: String,
    new_password: String,
) -> Result<(), AppError> {
    let (user_id, _role) = session.get_current_user()?;

    // Validate new password length
    if new_password.len() < 12 {
        return Err(AppError::Validation(
            "New password must be at least 12 characters".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch current password hash
    let current_hash: String = conn
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound("User not found".to_string()))?;

    // Verify current password
    password::verify(&current_password, &current_hash)?;

    // Hash the new password and update
    let new_hash = password::hash_password(&new_password)?;

    conn.execute(
        "UPDATE users SET password_hash = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![new_hash, user_id],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: user_id.clone(),
            action: "auth.change_password".to_string(),
            resource_type: "users".to_string(),
            resource_id: Some(user_id),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Password changed successfully".to_string()),
        },
    );

    Ok(())
}
