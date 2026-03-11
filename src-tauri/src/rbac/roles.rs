use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// The five roles defined in the MedArc RBAC matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    SystemAdmin,
    Provider,
    NurseMa,
    BillingStaff,
    FrontDesk,
}

impl Role {
    /// Parse a role from its database string representation.
    pub fn from_str(s: &str) -> Result<Role, AppError> {
        match s {
            "system_admin" => Ok(Role::SystemAdmin),
            "provider" => Ok(Role::Provider),
            "nurse_ma" => Ok(Role::NurseMa),
            "billing_staff" => Ok(Role::BillingStaff),
            "front_desk" => Ok(Role::FrontDesk),
            _ => Err(AppError::Validation(format!("Unknown role: {}", s))),
        }
    }

    /// Convert role to its database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::SystemAdmin => "system_admin",
            Role::Provider => "provider",
            Role::NurseMa => "nurse_ma",
            Role::BillingStaff => "billing_staff",
            Role::FrontDesk => "front_desk",
        }
    }
}

/// Resources protected by RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    ClinicalRecords,
    Scheduling,
    Billing,
    Prescriptions,
    AuditLogs,
    UserManagement,
}

/// Actions that can be performed on resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Read,
    Update,
    Delete,
}

/// Check if a role has permission for an action on a resource.
///
/// Implements the full RBAC matrix per the Day0 requirements.
/// Default deny for any unmatched combination.
pub fn has_permission(role: Role, resource: Resource, action: Action) -> bool {
    use Role::*;
    use Resource::*;
    use Action::*;

    match (role, resource, action) {
        // SystemAdmin: Full access to everything EXCEPT prescriptions.
        // SystemAdmin manages the system but cannot prescribe.
        (SystemAdmin, Prescriptions, _) => false,
        (SystemAdmin, _, _) => true,

        // Provider: Full CRUD clinical, CRU scheduling (no delete),
        // Read billing, Full CRUD prescriptions, Read own audit logs
        (Provider, ClinicalRecords, _) => true,
        (Provider, Scheduling, Create | Read | Update) => true,
        (Provider, Scheduling, Delete) => false,
        (Provider, Billing, Read) => true,
        (Provider, Billing, _) => false,
        (Provider, Prescriptions, _) => true,
        (Provider, AuditLogs, Read) => true,
        (Provider, AuditLogs, _) => false,
        (Provider, UserManagement, _) => false,

        // NurseMa: Read+Update clinical, CRU scheduling,
        // No billing, Read-only prescriptions, No audit
        (NurseMa, ClinicalRecords, Read | Update) => true,
        (NurseMa, ClinicalRecords, _) => false,
        (NurseMa, Scheduling, Create | Read | Update) => true,
        (NurseMa, Scheduling, Delete) => false,
        (NurseMa, Billing, _) => false,
        (NurseMa, Prescriptions, Read) => true,
        (NurseMa, Prescriptions, _) => false,
        (NurseMa, AuditLogs, _) => false,
        (NurseMa, UserManagement, _) => false,

        // BillingStaff: Read clinical (demographics only -- enforced at field level),
        // Read scheduling, Full CRUD billing, No prescriptions, No audit
        (BillingStaff, ClinicalRecords, Read) => true,
        (BillingStaff, ClinicalRecords, _) => false,
        (BillingStaff, Scheduling, Read) => true,
        (BillingStaff, Scheduling, _) => false,
        (BillingStaff, Billing, _) => true,
        (BillingStaff, Prescriptions, _) => false,
        (BillingStaff, AuditLogs, _) => false,
        (BillingStaff, UserManagement, _) => false,

        // FrontDesk: Read clinical (demographics only -- field level),
        // Full CRUD scheduling, Read billing, No prescriptions, No audit
        (FrontDesk, ClinicalRecords, Read) => true,
        (FrontDesk, ClinicalRecords, _) => false,
        (FrontDesk, Scheduling, _) => true,
        (FrontDesk, Billing, Read) => true,
        (FrontDesk, Billing, _) => false,
        (FrontDesk, Prescriptions, _) => false,
        (FrontDesk, AuditLogs, _) => false,
        (FrontDesk, UserManagement, _) => false,
    }
}

/// Define which FHIR resource JSON fields are visible per role.
///
/// Used for field-level access control (AUTH-07).
/// Returns vec!["*"] for full access, or a specific list of allowed field names.
pub fn visible_fields(role: Role, resource_type: &str) -> Vec<&'static str> {
    match (role, resource_type) {
        // BillingStaff sees demographics + billing codes only on Patient
        (Role::BillingStaff, "Patient") => vec![
            "id", "name", "birthDate", "gender", "address",
            "telecom", "identifier",
        ],
        // BillingStaff sees limited Encounter fields
        (Role::BillingStaff, "Encounter") => vec![
            "id", "status", "class", "type", "subject", "period",
        ],

        // FrontDesk sees demographics + contact on Patient
        (Role::FrontDesk, "Patient") => vec![
            "id", "name", "birthDate", "gender", "address",
            "telecom", "identifier", "contact",
        ],

        // Provider, SystemAdmin, NurseMa see all fields on all resources
        _ => vec!["*"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use Resource::*;
    use Role::*;

    // --- from_str parsing tests ---

    #[test]
    fn from_str_valid_roles() {
        assert_eq!(Role::from_str("system_admin").unwrap(), SystemAdmin);
        assert_eq!(Role::from_str("provider").unwrap(), Provider);
        assert_eq!(Role::from_str("nurse_ma").unwrap(), NurseMa);
        assert_eq!(Role::from_str("billing_staff").unwrap(), BillingStaff);
        assert_eq!(Role::from_str("front_desk").unwrap(), FrontDesk);
    }

    #[test]
    fn from_str_invalid_role() {
        assert!(Role::from_str("unknown").is_err());
        assert!(Role::from_str("").is_err());
        assert!(Role::from_str("admin").is_err());
    }

    #[test]
    fn as_str_roundtrip() {
        for role in [SystemAdmin, Provider, NurseMa, BillingStaff, FrontDesk] {
            assert_eq!(Role::from_str(role.as_str()).unwrap(), role);
        }
    }

    // --- SystemAdmin permission tests ---

    #[test]
    fn system_admin_full_clinical() {
        assert!(has_permission(SystemAdmin, ClinicalRecords, Create));
        assert!(has_permission(SystemAdmin, ClinicalRecords, Read));
        assert!(has_permission(SystemAdmin, ClinicalRecords, Update));
        assert!(has_permission(SystemAdmin, ClinicalRecords, Delete));
    }

    #[test]
    fn system_admin_full_scheduling() {
        assert!(has_permission(SystemAdmin, Scheduling, Create));
        assert!(has_permission(SystemAdmin, Scheduling, Read));
        assert!(has_permission(SystemAdmin, Scheduling, Update));
        assert!(has_permission(SystemAdmin, Scheduling, Delete));
    }

    #[test]
    fn system_admin_full_billing() {
        assert!(has_permission(SystemAdmin, Billing, Create));
        assert!(has_permission(SystemAdmin, Billing, Read));
        assert!(has_permission(SystemAdmin, Billing, Update));
        assert!(has_permission(SystemAdmin, Billing, Delete));
    }

    #[test]
    fn system_admin_no_prescriptions() {
        assert!(!has_permission(SystemAdmin, Prescriptions, Create));
        assert!(!has_permission(SystemAdmin, Prescriptions, Read));
        assert!(!has_permission(SystemAdmin, Prescriptions, Update));
        assert!(!has_permission(SystemAdmin, Prescriptions, Delete));
    }

    #[test]
    fn system_admin_full_audit() {
        assert!(has_permission(SystemAdmin, AuditLogs, Create));
        assert!(has_permission(SystemAdmin, AuditLogs, Read));
        assert!(has_permission(SystemAdmin, AuditLogs, Update));
        assert!(has_permission(SystemAdmin, AuditLogs, Delete));
    }

    #[test]
    fn system_admin_full_user_management() {
        assert!(has_permission(SystemAdmin, UserManagement, Create));
        assert!(has_permission(SystemAdmin, UserManagement, Read));
        assert!(has_permission(SystemAdmin, UserManagement, Update));
        assert!(has_permission(SystemAdmin, UserManagement, Delete));
    }

    // --- Provider permission tests ---

    #[test]
    fn provider_full_clinical() {
        assert!(has_permission(Provider, ClinicalRecords, Create));
        assert!(has_permission(Provider, ClinicalRecords, Read));
        assert!(has_permission(Provider, ClinicalRecords, Update));
        assert!(has_permission(Provider, ClinicalRecords, Delete));
    }

    #[test]
    fn provider_scheduling_no_delete() {
        assert!(has_permission(Provider, Scheduling, Create));
        assert!(has_permission(Provider, Scheduling, Read));
        assert!(has_permission(Provider, Scheduling, Update));
        assert!(!has_permission(Provider, Scheduling, Delete));
    }

    #[test]
    fn provider_billing_read_only() {
        assert!(!has_permission(Provider, Billing, Create));
        assert!(has_permission(Provider, Billing, Read));
        assert!(!has_permission(Provider, Billing, Update));
        assert!(!has_permission(Provider, Billing, Delete));
    }

    #[test]
    fn provider_full_prescriptions() {
        assert!(has_permission(Provider, Prescriptions, Create));
        assert!(has_permission(Provider, Prescriptions, Read));
        assert!(has_permission(Provider, Prescriptions, Update));
        assert!(has_permission(Provider, Prescriptions, Delete));
    }

    #[test]
    fn provider_audit_read_only() {
        assert!(!has_permission(Provider, AuditLogs, Create));
        assert!(has_permission(Provider, AuditLogs, Read));
        assert!(!has_permission(Provider, AuditLogs, Update));
        assert!(!has_permission(Provider, AuditLogs, Delete));
    }

    #[test]
    fn provider_no_user_management() {
        assert!(!has_permission(Provider, UserManagement, Create));
        assert!(!has_permission(Provider, UserManagement, Read));
        assert!(!has_permission(Provider, UserManagement, Update));
        assert!(!has_permission(Provider, UserManagement, Delete));
    }

    // --- NurseMa permission tests ---

    #[test]
    fn nurse_clinical_read_update_only() {
        assert!(!has_permission(NurseMa, ClinicalRecords, Create));
        assert!(has_permission(NurseMa, ClinicalRecords, Read));
        assert!(has_permission(NurseMa, ClinicalRecords, Update));
        assert!(!has_permission(NurseMa, ClinicalRecords, Delete));
    }

    #[test]
    fn nurse_scheduling_no_delete() {
        assert!(has_permission(NurseMa, Scheduling, Create));
        assert!(has_permission(NurseMa, Scheduling, Read));
        assert!(has_permission(NurseMa, Scheduling, Update));
        assert!(!has_permission(NurseMa, Scheduling, Delete));
    }

    #[test]
    fn nurse_no_billing() {
        assert!(!has_permission(NurseMa, Billing, Create));
        assert!(!has_permission(NurseMa, Billing, Read));
        assert!(!has_permission(NurseMa, Billing, Update));
        assert!(!has_permission(NurseMa, Billing, Delete));
    }

    #[test]
    fn nurse_prescriptions_read_only() {
        assert!(!has_permission(NurseMa, Prescriptions, Create));
        assert!(has_permission(NurseMa, Prescriptions, Read));
        assert!(!has_permission(NurseMa, Prescriptions, Update));
        assert!(!has_permission(NurseMa, Prescriptions, Delete));
    }

    #[test]
    fn nurse_no_audit() {
        assert!(!has_permission(NurseMa, AuditLogs, Create));
        assert!(!has_permission(NurseMa, AuditLogs, Read));
        assert!(!has_permission(NurseMa, AuditLogs, Update));
        assert!(!has_permission(NurseMa, AuditLogs, Delete));
    }

    #[test]
    fn nurse_no_user_management() {
        assert!(!has_permission(NurseMa, UserManagement, Create));
        assert!(!has_permission(NurseMa, UserManagement, Read));
        assert!(!has_permission(NurseMa, UserManagement, Update));
        assert!(!has_permission(NurseMa, UserManagement, Delete));
    }

    // --- BillingStaff permission tests ---

    #[test]
    fn billing_clinical_read_only() {
        assert!(!has_permission(BillingStaff, ClinicalRecords, Create));
        assert!(has_permission(BillingStaff, ClinicalRecords, Read));
        assert!(!has_permission(BillingStaff, ClinicalRecords, Update));
        assert!(!has_permission(BillingStaff, ClinicalRecords, Delete));
    }

    #[test]
    fn billing_scheduling_read_only() {
        assert!(!has_permission(BillingStaff, Scheduling, Create));
        assert!(has_permission(BillingStaff, Scheduling, Read));
        assert!(!has_permission(BillingStaff, Scheduling, Update));
        assert!(!has_permission(BillingStaff, Scheduling, Delete));
    }

    #[test]
    fn billing_full_billing() {
        assert!(has_permission(BillingStaff, Billing, Create));
        assert!(has_permission(BillingStaff, Billing, Read));
        assert!(has_permission(BillingStaff, Billing, Update));
        assert!(has_permission(BillingStaff, Billing, Delete));
    }

    #[test]
    fn billing_no_prescriptions() {
        assert!(!has_permission(BillingStaff, Prescriptions, Create));
        assert!(!has_permission(BillingStaff, Prescriptions, Read));
        assert!(!has_permission(BillingStaff, Prescriptions, Update));
        assert!(!has_permission(BillingStaff, Prescriptions, Delete));
    }

    #[test]
    fn billing_no_audit() {
        assert!(!has_permission(BillingStaff, AuditLogs, Create));
        assert!(!has_permission(BillingStaff, AuditLogs, Read));
        assert!(!has_permission(BillingStaff, AuditLogs, Update));
        assert!(!has_permission(BillingStaff, AuditLogs, Delete));
    }

    #[test]
    fn billing_no_user_management() {
        assert!(!has_permission(BillingStaff, UserManagement, Create));
        assert!(!has_permission(BillingStaff, UserManagement, Read));
        assert!(!has_permission(BillingStaff, UserManagement, Update));
        assert!(!has_permission(BillingStaff, UserManagement, Delete));
    }

    // --- FrontDesk permission tests ---

    #[test]
    fn front_desk_clinical_read_only() {
        assert!(!has_permission(FrontDesk, ClinicalRecords, Create));
        assert!(has_permission(FrontDesk, ClinicalRecords, Read));
        assert!(!has_permission(FrontDesk, ClinicalRecords, Update));
        assert!(!has_permission(FrontDesk, ClinicalRecords, Delete));
    }

    #[test]
    fn front_desk_full_scheduling() {
        assert!(has_permission(FrontDesk, Scheduling, Create));
        assert!(has_permission(FrontDesk, Scheduling, Read));
        assert!(has_permission(FrontDesk, Scheduling, Update));
        assert!(has_permission(FrontDesk, Scheduling, Delete));
    }

    #[test]
    fn front_desk_billing_read_only() {
        assert!(!has_permission(FrontDesk, Billing, Create));
        assert!(has_permission(FrontDesk, Billing, Read));
        assert!(!has_permission(FrontDesk, Billing, Update));
        assert!(!has_permission(FrontDesk, Billing, Delete));
    }

    #[test]
    fn front_desk_no_prescriptions() {
        assert!(!has_permission(FrontDesk, Prescriptions, Create));
        assert!(!has_permission(FrontDesk, Prescriptions, Read));
        assert!(!has_permission(FrontDesk, Prescriptions, Update));
        assert!(!has_permission(FrontDesk, Prescriptions, Delete));
    }

    #[test]
    fn front_desk_no_audit() {
        assert!(!has_permission(FrontDesk, AuditLogs, Create));
        assert!(!has_permission(FrontDesk, AuditLogs, Read));
        assert!(!has_permission(FrontDesk, AuditLogs, Update));
        assert!(!has_permission(FrontDesk, AuditLogs, Delete));
    }

    #[test]
    fn front_desk_no_user_management() {
        assert!(!has_permission(FrontDesk, UserManagement, Create));
        assert!(!has_permission(FrontDesk, UserManagement, Read));
        assert!(!has_permission(FrontDesk, UserManagement, Update));
        assert!(!has_permission(FrontDesk, UserManagement, Delete));
    }

    // --- visible_fields tests ---

    #[test]
    fn billing_staff_patient_demographics_only() {
        let fields = visible_fields(BillingStaff, "Patient");
        assert!(fields.contains(&"id"));
        assert!(fields.contains(&"name"));
        assert!(fields.contains(&"birthDate"));
        assert!(fields.contains(&"gender"));
        assert!(fields.contains(&"address"));
        assert!(fields.contains(&"telecom"));
        assert!(fields.contains(&"identifier"));
        // Should NOT include clinical fields
        assert!(!fields.contains(&"*"));
        assert!(!fields.contains(&"contact"));
    }

    #[test]
    fn front_desk_patient_demographics_plus_contact() {
        let fields = visible_fields(FrontDesk, "Patient");
        assert!(fields.contains(&"id"));
        assert!(fields.contains(&"name"));
        assert!(fields.contains(&"birthDate"));
        assert!(fields.contains(&"gender"));
        assert!(fields.contains(&"address"));
        assert!(fields.contains(&"telecom"));
        assert!(fields.contains(&"identifier"));
        assert!(fields.contains(&"contact"));
        assert!(!fields.contains(&"*"));
    }

    #[test]
    fn provider_all_patient_fields() {
        let fields = visible_fields(Provider, "Patient");
        assert_eq!(fields, vec!["*"]);
    }

    #[test]
    fn system_admin_all_patient_fields() {
        let fields = visible_fields(SystemAdmin, "Patient");
        assert_eq!(fields, vec!["*"]);
    }

    #[test]
    fn nurse_all_patient_fields() {
        let fields = visible_fields(NurseMa, "Patient");
        assert_eq!(fields, vec!["*"]);
    }

    #[test]
    fn billing_staff_encounter_limited() {
        let fields = visible_fields(BillingStaff, "Encounter");
        assert!(fields.contains(&"id"));
        assert!(fields.contains(&"status"));
        assert!(fields.contains(&"subject"));
        assert!(!fields.contains(&"*"));
    }

    #[test]
    fn provider_all_encounter_fields() {
        let fields = visible_fields(Provider, "Encounter");
        assert_eq!(fields, vec!["*"]);
    }
}
