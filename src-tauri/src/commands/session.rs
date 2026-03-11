use tauri::State;

use crate::auth::password;
use crate::auth::session::{SessionInfo, SessionManager};
use crate::db::connection::Database;
use crate::error::AppError;

/// Lock the current active session.
#[tauri::command]
pub fn lock_session(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let session_info = session.get_state();
    session.lock()?;

    if let Some(session_id) = session_info.session_id {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET state = 'locked' WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
    }

    Ok(())
}

/// Unlock a locked session by re-entering the password.
#[tauri::command]
pub fn unlock_session(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    password: String,
) -> Result<(), AppError> {
    // Get current user from locked session
    let state_info = session.get_state();
    let user_id = state_info
        .user_id
        .ok_or_else(|| AppError::Authentication("No active session to unlock".to_string()))?;

    if state_info.state != "locked" {
        return Err(AppError::Authentication(
            "Session is not locked".to_string(),
        ));
    }

    // Verify password against stored hash
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let password_hash: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |row| row.get(0),
    )?;

    password::verify(&password, &password_hash)?;

    // Unlock the session
    session.unlock(&user_id)?;

    // Update session row in database
    if let Some(session_id) = state_info.session_id {
        conn.execute(
            "UPDATE sessions SET state = 'active', last_activity = datetime('now') WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
    }

    Ok(())
}

/// Refresh the session activity timestamp to prevent timeout.
#[tauri::command]
pub fn refresh_session(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<(), AppError> {
    session.refresh_activity()?;

    let session_info = session.get_state();
    if let Some(session_id) = session_info.session_id {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET last_activity = datetime('now') WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
    }

    Ok(())
}

/// Get the current session state for the frontend.
#[tauri::command]
pub fn get_session_state(session: State<'_, SessionManager>) -> Result<SessionInfo, AppError> {
    Ok(session.get_state())
}

/// Get the session timeout value from app_settings.
#[tauri::command]
pub fn get_session_timeout(db: State<'_, Database>) -> Result<u32, AppError> {
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let timeout_str: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'session_timeout_minutes'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "15".to_string());

    timeout_str
        .parse::<u32>()
        .map_err(|e| AppError::Database(format!("Invalid timeout value: {}", e)))
}
