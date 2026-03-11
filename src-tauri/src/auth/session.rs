use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::db::models::user::SessionInfo;
use crate::error::AppError;

/// Represents the current state of a user session.
#[derive(Debug, Clone)]
pub enum SessionState {
    Unauthenticated,
    Active {
        user_id: String,
        role: String,
        last_activity: DateTime<Utc>,
        session_id: String,
    },
    Locked {
        user_id: String,
        role: String,
        locked_at: DateTime<Utc>,
        session_id: String,
    },
    BreakGlass {
        user_id: String,
        original_role: String,
        elevated_permissions: Vec<String>,
        expires_at: DateTime<Utc>,
        session_id: String,
    },
}

/// Manages session state with configurable timeout.
pub struct SessionManager {
    pub state: Mutex<SessionState>,
    pub timeout_minutes: Mutex<u32>,
}

impl SessionManager {
    /// Create a new SessionManager in Unauthenticated state.
    pub fn new(_timeout: u32) -> Self {
        // Stub: will fail tests
        panic!("not implemented")
    }

    /// Transition to Active state on successful login. Returns a new session ID.
    pub fn login(&self, _user_id: &str, _role: &str) -> Result<String, AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }

    /// Transition to Unauthenticated state.
    pub fn logout(&self) -> Result<(), AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }

    /// Transition from Active to Locked state.
    pub fn lock(&self) -> Result<(), AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }

    /// Transition from Locked back to Active state.
    pub fn unlock(&self, _user_id: &str) -> Result<(), AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }

    /// Refresh the last activity timestamp (keeps session alive).
    pub fn refresh_activity(&self) -> Result<(), AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }

    /// Check if the session has timed out. Returns true if timed out.
    pub fn check_timeout(&self) -> bool {
        // Stub: will fail test (returns true meaning always timed out)
        true
    }

    /// Get current session info for the frontend.
    pub fn get_state(&self) -> SessionInfo {
        SessionInfo {
            session_id: None,
            user_id: None,
            role: None,
            state: "unauthenticated".to_string(),
            last_activity: None,
        }
    }

    /// Get the current user ID and role. Fails if not in Active or Locked state.
    pub fn get_current_user(&self) -> Result<(String, String), AppError> {
        Err(AppError::Authentication("not implemented".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_is_unauthenticated() {
        let manager = SessionManager::new(15);
        let info = manager.get_state();
        assert_eq!(info.state, "unauthenticated");
    }

    #[test]
    fn login_transitions_to_active() {
        let manager = SessionManager::new(15);
        let session_id = manager.login("user-1", "Physician").unwrap();
        assert!(!session_id.is_empty(), "session_id should not be empty");

        let info = manager.get_state();
        assert_eq!(info.state, "active");
        assert_eq!(info.user_id.as_deref(), Some("user-1"));
        assert_eq!(info.role.as_deref(), Some("Physician"));
    }

    #[test]
    fn check_timeout_false_within_window() {
        let manager = SessionManager::new(15);
        manager.login("user-1", "Physician").unwrap();
        // Just logged in, should not be timed out
        assert!(!manager.check_timeout(), "Session should not be timed out immediately after login");
    }

    #[test]
    fn lock_transitions_active_to_locked() {
        let manager = SessionManager::new(15);
        manager.login("user-1", "Physician").unwrap();
        manager.lock().unwrap();

        let info = manager.get_state();
        assert_eq!(info.state, "locked");
        assert_eq!(info.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn unlock_transitions_locked_to_active() {
        let manager = SessionManager::new(15);
        manager.login("user-1", "Physician").unwrap();
        manager.lock().unwrap();
        manager.unlock("user-1").unwrap();

        let info = manager.get_state();
        assert_eq!(info.state, "active");
    }

    #[test]
    fn logout_transitions_to_unauthenticated() {
        let manager = SessionManager::new(15);
        manager.login("user-1", "Physician").unwrap();
        manager.logout().unwrap();

        let info = manager.get_state();
        assert_eq!(info.state, "unauthenticated");
    }

    #[test]
    fn get_current_user_when_active() {
        let manager = SessionManager::new(15);
        manager.login("user-1", "Physician").unwrap();
        let (user_id, role) = manager.get_current_user().unwrap();
        assert_eq!(user_id, "user-1");
        assert_eq!(role, "Physician");
    }

    #[test]
    fn get_current_user_when_unauthenticated_fails() {
        let manager = SessionManager::new(15);
        assert!(manager.get_current_user().is_err());
    }
}
