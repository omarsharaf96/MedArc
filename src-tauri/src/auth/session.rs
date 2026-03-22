use std::sync::Mutex;

use chrono::{DateTime, Utc};

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
        #[allow(dead_code)]
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

/// Session info for frontend consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub role: Option<String>,
    pub state: String,
    pub last_activity: Option<String>,
}

/// Manages session state with configurable timeout.
pub struct SessionManager {
    pub state: Mutex<SessionState>,
    #[allow(dead_code)]
    pub timeout_minutes: Mutex<u32>,
}

impl SessionManager {
    /// Create a new SessionManager in Unauthenticated state.
    pub fn new(timeout: u32) -> Self {
        SessionManager {
            state: Mutex::new(SessionState::Unauthenticated),
            timeout_minutes: Mutex::new(timeout),
        }
    }

    /// Transition to Active state on successful login. Returns a new session ID.
    pub fn login(&self, user_id: &str, role: &str) -> Result<String, AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        let session_id = uuid::Uuid::new_v4().to_string();
        *state = SessionState::Active {
            user_id: user_id.to_string(),
            role: role.to_string(),
            last_activity: Utc::now(),
            session_id: session_id.clone(),
        };
        Ok(session_id)
    }

    /// Transition to Unauthenticated state.
    pub fn logout(&self) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        *state = SessionState::Unauthenticated;
        Ok(())
    }

    /// Transition from Active to Locked state.
    pub fn lock(&self) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        match state.clone() {
            SessionState::Active {
                user_id,
                role,
                session_id,
                ..
            } => {
                *state = SessionState::Locked {
                    user_id,
                    role,
                    locked_at: Utc::now(),
                    session_id,
                };
                Ok(())
            }
            _ => Err(AppError::Authentication(
                "Can only lock an active session".to_string(),
            )),
        }
    }

    /// Transition from Locked back to Active state.
    pub fn unlock(&self, requesting_user_id: &str) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        match state.clone() {
            SessionState::Locked {
                user_id,
                role,
                session_id,
                ..
            } => {
                if user_id != requesting_user_id {
                    return Err(AppError::Authentication(
                        "User ID does not match the locked session".to_string(),
                    ));
                }
                *state = SessionState::Active {
                    user_id,
                    role,
                    last_activity: Utc::now(),
                    session_id,
                };
                Ok(())
            }
            _ => Err(AppError::Authentication(
                "Can only unlock a locked session".to_string(),
            )),
        }
    }

    /// Refresh the last activity timestamp (keeps session alive).
    pub fn refresh_activity(&self) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        if let SessionState::Active { last_activity, .. } = &mut *state {
            *last_activity = Utc::now();
        }
        Ok(())
    }

    /// Check if the session has timed out. Returns true if timed out.
    #[allow(dead_code)]
    pub fn check_timeout(&self) -> Result<bool, AppError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AppError::Authentication("Session lock poisoned".to_string()))?;
        let timeout = *self
            .timeout_minutes
            .lock()
            .map_err(|_| AppError::Authentication("Session lock poisoned".to_string()))?;
        match &*state {
            SessionState::Active { last_activity, .. } => {
                Ok(Utc::now() - *last_activity > chrono::Duration::minutes(timeout as i64))
            }
            _ => Ok(false),
        }
    }

    /// Get current session info for the frontend.
    pub fn get_state(&self) -> SessionInfo {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => {
                return SessionInfo {
                    session_id: None,
                    user_id: None,
                    role: None,
                    state: "unauthenticated".to_string(),
                    last_activity: None,
                };
            }
        };
        match &*state {
            SessionState::Unauthenticated => SessionInfo {
                session_id: None,
                user_id: None,
                role: None,
                state: "unauthenticated".to_string(),
                last_activity: None,
            },
            SessionState::Active {
                user_id,
                role,
                last_activity,
                session_id,
            } => SessionInfo {
                session_id: Some(session_id.clone()),
                user_id: Some(user_id.clone()),
                role: Some(role.clone()),
                state: "active".to_string(),
                last_activity: Some(last_activity.to_rfc3339()),
            },
            SessionState::Locked {
                user_id,
                role,
                session_id,
                ..
            } => SessionInfo {
                session_id: Some(session_id.clone()),
                user_id: Some(user_id.clone()),
                role: Some(role.clone()),
                state: "locked".to_string(),
                last_activity: None,
            },
            SessionState::BreakGlass {
                user_id,
                original_role,
                session_id,
                expires_at,
                ..
            } => SessionInfo {
                session_id: Some(session_id.clone()),
                user_id: Some(user_id.clone()),
                role: Some(original_role.clone()),
                state: "break_glass".to_string(),
                last_activity: Some(expires_at.to_rfc3339()),
            },
        }
    }

    /// Get the current user ID and role. Fails if not in Active or BreakGlass state.
    pub fn get_current_user(&self) -> Result<(String, String), AppError> {
        let state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        match &*state {
            SessionState::Active { user_id, role, .. } => Ok((user_id.clone(), role.clone())),
            SessionState::Locked { user_id, role, .. } => Ok((user_id.clone(), role.clone())),
            SessionState::BreakGlass {
                user_id,
                original_role,
                ..
            } => Ok((user_id.clone(), original_role.clone())),
            _ => Err(AppError::Authentication("Not authenticated".to_string())),
        }
    }

    /// Transition the session to break-glass mode with elevated permissions.
    pub fn activate_break_glass(
        &self,
        user_id: String,
        original_role: String,
        permissions: Vec<String>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        let session_id = match &*state {
            SessionState::Active { session_id, .. } => session_id.clone(),
            _ => {
                return Err(AppError::Authentication(
                    "Must be in active session to activate break-glass".to_string(),
                ))
            }
        };
        *state = SessionState::BreakGlass {
            user_id,
            original_role,
            elevated_permissions: permissions,
            expires_at,
            session_id,
        };
        Ok(())
    }

    /// Deactivate break-glass and return to active session with original role.
    pub fn deactivate_break_glass(&self) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| AppError::Authentication(e.to_string()))?;
        match state.clone() {
            SessionState::BreakGlass {
                user_id,
                original_role,
                session_id,
                ..
            } => {
                *state = SessionState::Active {
                    user_id,
                    role: original_role,
                    last_activity: Utc::now(),
                    session_id,
                };
                Ok(())
            }
            _ => Err(AppError::Authentication(
                "Not in break-glass session".to_string(),
            )),
        }
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
        assert!(
            !manager.check_timeout().unwrap(),
            "Session should not be timed out immediately after login"
        );
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
