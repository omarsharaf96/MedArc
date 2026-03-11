use crate::auth::session::{SessionManager, SessionState};
use crate::error::AppError;
use crate::rbac::roles::{has_permission, Action, Resource, Role};

use chrono::Utc;

/// Check if the current session has permission for the given resource and action.
///
/// Returns `Ok((user_id, role))` if the permission check passes.
/// Returns `Err(AppError::Unauthorized)` if:
///   - The session is unauthenticated or locked
///   - The role doesn't have the required permission
///   - A break-glass session has expired or the action is outside its scope
pub fn check_permission(
    session: &SessionManager,
    resource: Resource,
    action: Action,
) -> Result<(String, Role), AppError> {
    let state = session
        .state
        .lock()
        .map_err(|e| AppError::Unauthorized(format!("Failed to read session state: {}", e)))?;

    match &*state {
        SessionState::Unauthenticated => {
            Err(AppError::Unauthorized("Not authenticated".to_string()))
        }
        SessionState::Locked { .. } => Err(AppError::Unauthorized("Session is locked".to_string())),
        SessionState::Active { user_id, role, .. } => {
            let parsed_role = Role::from_str(role)?;
            if has_permission(parsed_role, resource, action) {
                Ok((user_id.clone(), parsed_role))
            } else {
                Err(AppError::Unauthorized(format!(
                    "Role '{}' does not have {:?} permission on {:?}",
                    role, action, resource
                )))
            }
        }
        SessionState::BreakGlass {
            user_id,
            original_role,
            elevated_permissions,
            expires_at,
            ..
        } => {
            // Check if break-glass session has expired
            if Utc::now() > *expires_at {
                return Err(AppError::Unauthorized(
                    "Break-glass session has expired".to_string(),
                ));
            }

            // Check if the action is within the elevated permission scope.
            // Elevated permissions use format "resource:action" (e.g., "clinicalrecords:read").
            let permission_key = format!("{:?}:{:?}", resource, action).to_lowercase();
            if elevated_permissions
                .iter()
                .any(|p| permission_key.starts_with(p))
            {
                let parsed_role = Role::from_str(original_role)?;
                return Ok((user_id.clone(), parsed_role));
            }

            // Fall back to normal role permissions
            let parsed_role = Role::from_str(original_role)?;
            if has_permission(parsed_role, resource, action) {
                Ok((user_id.clone(), parsed_role))
            } else {
                Err(AppError::Unauthorized(
                    "Action not permitted during break-glass session".to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::roles::{Action, Resource};

    #[test]
    fn check_permission_active_provider_clinical_read() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "provider").unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_ok());
        let (uid, role) = result.unwrap();
        assert_eq!(uid, "user-1");
        assert_eq!(role, Role::Provider);
    }

    #[test]
    fn check_permission_active_provider_prescription_create() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "provider").unwrap();
        let result = check_permission(&sm, Resource::Prescriptions, Action::Create);
        assert!(result.is_ok());
    }

    #[test]
    fn check_permission_unauthenticated_rejected() {
        let sm = SessionManager::new(15);
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_locked_rejected() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "provider").unwrap();
        sm.lock().unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_nurse_no_create_clinical() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "nurse_ma").unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Create);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_nurse_can_read_clinical() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "nurse_ma").unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_ok());
    }

    #[test]
    fn check_permission_billing_no_prescriptions() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "billing_staff").unwrap();
        let result = check_permission(&sm, Resource::Prescriptions, Action::Read);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_front_desk_full_scheduling() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "front_desk").unwrap();
        for action in [Action::Create, Action::Read, Action::Update, Action::Delete] {
            let result = check_permission(&sm, Resource::Scheduling, action);
            assert!(
                result.is_ok(),
                "FrontDesk should have {:?} on Scheduling",
                action
            );
        }
    }

    #[test]
    fn check_permission_system_admin_no_prescriptions() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "system_admin").unwrap();
        let result = check_permission(&sm, Resource::Prescriptions, Action::Create);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_break_glass_expired() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "front_desk").unwrap();
        // Activate break-glass with already-expired time
        sm.activate_break_glass(
            "user-1".to_string(),
            "front_desk".to_string(),
            vec!["clinicalrecords:read".to_string()],
            Utc::now() - chrono::Duration::minutes(1),
        )
        .unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_err());
    }

    #[test]
    fn check_permission_break_glass_within_scope() {
        let sm = SessionManager::new(15);
        sm.login("user-1", "front_desk").unwrap();
        sm.activate_break_glass(
            "user-1".to_string(),
            "front_desk".to_string(),
            vec!["clinicalrecords:read".to_string()],
            Utc::now() + chrono::Duration::minutes(30),
        )
        .unwrap();
        let result = check_permission(&sm, Resource::ClinicalRecords, Action::Read);
        assert!(result.is_ok());
    }
}
