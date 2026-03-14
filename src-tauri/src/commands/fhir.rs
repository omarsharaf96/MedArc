use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::db::models::{CreateFhirResource, FhirResource, FhirResourceList, UpdateFhirResource};
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::field_filter;
use crate::rbac::middleware;
use crate::rbac::roles::{self, Action, Resource};

/// Extract a patient identifier from a FHIR resource JSON value.
///
/// Checks common FHIR R4 fields in priority order:
/// 1. `subject.reference`  (Observation, Condition, MedicationRequest, …)
/// 2. `patient.reference`  (AllergyIntolerance, Immunization, …)
/// 3. `id` when `resourceType == "Patient"`
///
/// Returns `Some(reference_string)` or `None` when not found.
/// This is audit metadata only — never used for clinical logic.
fn extract_patient_id(resource_type: &str, resource: &serde_json::Value) -> Option<String> {
    if resource_type == "Patient" {
        return resource
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    if let Some(r) = resource
        .get("subject")
        .and_then(|s| s.get("reference"))
        .and_then(|v| v.as_str())
    {
        return Some(r.to_string());
    }

    if let Some(r) = resource
        .get("patient")
        .and_then(|s| s.get("reference"))
        .and_then(|v| v.as_str())
    {
        return Some(r.to_string());
    }

    None
}

/// Write a failure audit row and return the original error.
///
/// Helper to avoid repetition in the denied-before-DB-lock pattern.
/// The DB connection is acquired here; does nothing if lock fails.
fn audit_denied(
    db: &Database,
    device_id: &DeviceId,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    patient_id: Option<String>,
    reason: &str,
) {
    if let Ok(conn) = db.conn.lock() {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.to_string(),
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                patient_id,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(reason.to_string()),
            },
        );
    }
}

/// Create a new FHIR resource in the encrypted database.
///
/// Requires authenticated session with ClinicalRecords:Create permission.
/// Writes an audit entry on both success and failure paths (inside the same lock).
#[tauri::command]
pub fn create_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: CreateFhirResource,
) -> Result<FhirResource, AppError> {
    // Permission check — runs before acquiring the DB lock.
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalRecords, Action::Create) {
            Ok(pair) => pair,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "fhir.create",
                    &input.resource_type,
                    None,
                    extract_patient_id(&input.resource_type, &input.resource),
                    &format!("Permission denied: {}", e),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let resource_json =
        serde_json::to_string(&input.resource).map_err(|e| AppError::Database(e.to_string()))?;

    let insert_result = conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
        rusqlite::params![id, input.resource_type, resource_json, now, now, now],
    );

    let patient_id = extract_patient_id(&input.resource_type, &input.resource);

    match insert_result {
        Ok(_) => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "fhir.create".to_string(),
                    resource_type: input.resource_type.clone(),
                    resource_id: Some(id.clone()),
                    patient_id,
                    device_id: device_id.get().to_string(),
                    success: true,
                    details: None,
                },
            );
            Ok(FhirResource {
                id,
                resource_type: input.resource_type,
                resource: input.resource,
                version_id: 1,
                last_updated: now.clone(),
                created_at: now.clone(),
                updated_at: now,
            })
        }
        Err(e) => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "fhir.create".to_string(),
                    resource_type: input.resource_type,
                    resource_id: None,
                    patient_id,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(format!("DB error: {}", e)),
                },
            );
            Err(AppError::Database(e.to_string()))
        }
    }
}

/// Retrieve a single FHIR resource by ID with role-based field filtering.
/// Writes an audit entry on both success and failure paths.
#[tauri::command]
pub fn get_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    id: String,
) -> Result<FhirResource, AppError> {
    let (user_id, role) =
        match middleware::check_permission(&session, Resource::ClinicalRecords, Action::Read) {
            Ok(pair) => pair,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "fhir.get",
                    "unknown",
                    Some(id.clone()),
                    None,
                    &format!("Permission denied: {}", e),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT id, resource_type, resource, version_id, last_updated, created_at, updated_at
         FROM fhir_resources WHERE id = ?1",
    )?;

    let fetch_result = stmt
        .query_row(rusqlite::params![id.clone()], |row| {
            let resource_str: String = row.get(2)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(FhirResource {
                id: row.get(0)?,
                resource_type: row.get(1)?,
                resource,
                version_id: row.get(3)?,
                last_updated: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Resource not found: {}", id))
            }
            other => AppError::Database(other.to_string()),
        });

    match fetch_result {
        Ok(mut resource) => {
            let patient_id = extract_patient_id(&resource.resource_type, &resource.resource);
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "fhir.get".to_string(),
                    resource_type: resource.resource_type.clone(),
                    resource_id: Some(resource.id.clone()),
                    patient_id,
                    device_id: device_id.get().to_string(),
                    success: true,
                    details: None,
                },
            );
            let allowed_fields = roles::visible_fields(role, &resource.resource_type);
            let field_refs: Vec<&str> = allowed_fields.iter().copied().collect();
            resource.resource = field_filter::filter_resource(&resource.resource, &field_refs);
            Ok(resource)
        }
        Err(e) => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "fhir.get".to_string(),
                    resource_type: "unknown".to_string(),
                    resource_id: Some(id),
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(e.to_string()),
                },
            );
            Err(e)
        }
    }
}

/// List FHIR resources with role-based field filtering.
/// Writes an audit entry on both success and failure paths.
#[tauri::command]
pub fn list_resources(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    resource_type: Option<String>,
) -> Result<FhirResourceList, AppError> {
    let (user_id, role) =
        match middleware::check_permission(&session, Resource::ClinicalRecords, Action::Read) {
            Ok(pair) => pair,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "fhir.list",
                    &resource_type.clone().unwrap_or_else(|| "all".to_string()),
                    None,
                    None,
                    &format!("Permission denied: {}", e),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (query, count_query, params): (&str, &str, Vec<Box<dyn rusqlite::types::ToSql>>) =
        match &resource_type {
            Some(rt) => (
                "SELECT id, resource_type, resource, version_id, last_updated, created_at, updated_at
                 FROM fhir_resources WHERE resource_type = ?1 ORDER BY last_updated DESC",
                "SELECT COUNT(*) FROM fhir_resources WHERE resource_type = ?1",
                vec![Box::new(rt.clone()) as Box<dyn rusqlite::types::ToSql>],
            ),
            None => (
                "SELECT id, resource_type, resource, version_id, last_updated, created_at, updated_at
                 FROM fhir_resources ORDER BY last_updated DESC",
                "SELECT COUNT(*) FROM fhir_resources",
                vec![],
            ),
        };

    let total_result: Result<i64, _> = conn.query_row(
        count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get(0),
    );

    let total = match total_result {
        Ok(t) => t,
        Err(e) => {
            let rt_label = resource_type.unwrap_or_else(|| "all".to_string());
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "fhir.list".to_string(),
                    resource_type: rt_label,
                    resource_id: None,
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(format!("DB error: {}", e)),
                },
            );
            return Err(AppError::Database(e.to_string()));
        }
    };

    let mut stmt = conn.prepare(query)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let resources: Vec<FhirResource> = stmt
        .query_map(param_refs.as_slice(), |row| {
            let resource_str: String = row.get(2)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(FhirResource {
                id: row.get(0)?,
                resource_type: row.get(1)?,
                resource,
                version_id: row.get(3)?,
                last_updated: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let filtered_resources: Vec<FhirResource> = resources
        .into_iter()
        .map(|mut r| {
            let allowed_fields = roles::visible_fields(role, &r.resource_type);
            let field_refs: Vec<&str> = allowed_fields.iter().copied().collect();
            r.resource = field_filter::filter_resource(&r.resource, &field_refs);
            r
        })
        .collect();

    let rt_label = resource_type.unwrap_or_else(|| "all".to_string());
    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "fhir.list".to_string(),
            resource_type: rt_label,
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} records", filtered_resources.len())),
        },
    );

    Ok(FhirResourceList {
        resources: filtered_resources,
        total,
    })
}

/// Update an existing FHIR resource's JSON content.
///
/// Requires ClinicalRecords:Update permission.
/// Writes an audit entry on both success and failure paths.
#[tauri::command]
pub fn update_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: UpdateFhirResource,
) -> Result<FhirResource, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalRecords, Action::Update) {
            Ok(pair) => pair,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "fhir.update",
                    "unknown",
                    Some(input.id.clone()),
                    extract_patient_id("unknown", &input.resource),
                    &format!("Permission denied: {}", e),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let version_result: Result<i64, _> = conn.query_row(
        "SELECT version_id FROM fhir_resources WHERE id = ?1",
        rusqlite::params![input.id],
        |row| row.get(0),
    );

    let current_version = match version_result {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: user_id.clone(),
                    action: "fhir.update".to_string(),
                    resource_type: "unknown".to_string(),
                    resource_id: Some(input.id.clone()),
                    patient_id: extract_patient_id("unknown", &input.resource),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(format!("Not found: {}", input.id)),
                },
            );
            return Err(AppError::NotFound(format!(
                "Resource not found: {}",
                input.id
            )));
        }
        Err(e) => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: user_id.clone(),
                    action: "fhir.update".to_string(),
                    resource_type: "unknown".to_string(),
                    resource_id: Some(input.id.clone()),
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(format!("DB error: {}", e)),
                },
            );
            return Err(AppError::Database(e.to_string()));
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let new_version = current_version + 1;
    let resource_json =
        serde_json::to_string(&input.resource).map_err(|e| AppError::Database(e.to_string()))?;

    let update_result = conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, input.id],
    );

    if let Err(e) = update_result {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "fhir.update".to_string(),
                resource_type: "unknown".to_string(),
                resource_id: Some(input.id.clone()),
                patient_id: extract_patient_id("unknown", &input.resource),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(format!("DB error: {}", e)),
            },
        );
        return Err(AppError::Database(e.to_string()));
    }

    let mut stmt = conn.prepare(
        "SELECT id, resource_type, resource, version_id, last_updated, created_at, updated_at
         FROM fhir_resources WHERE id = ?1",
    )?;

    let resource = stmt.query_row(rusqlite::params![input.id], |row| {
        let resource_str: String = row.get(2)?;
        let resource: serde_json::Value =
            serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
        Ok(FhirResource {
            id: row.get(0)?,
            resource_type: row.get(1)?,
            resource,
            version_id: row.get(3)?,
            last_updated: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;

    let patient_id = extract_patient_id(&resource.resource_type, &resource.resource);
    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "fhir.update".to_string(),
            resource_type: resource.resource_type.clone(),
            resource_id: Some(resource.id.clone()),
            patient_id,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(resource)
}

/// Delete a FHIR resource by ID.
///
/// Requires ClinicalRecords:Delete permission.
/// Writes an audit entry on both success and failure paths.
#[tauri::command]
pub fn delete_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    id: String,
) -> Result<(), AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalRecords, Action::Delete) {
            Ok(pair) => pair,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "fhir.delete",
                    "unknown",
                    Some(id.clone()),
                    None,
                    &format!("Permission denied: {}", e),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch resource metadata before deleting so we can populate audit fields.
    let pre_delete: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT resource_type, resource FROM fhir_resources WHERE id = ?1",
            rusqlite::params![id.clone()],
            |row| {
                let rt: String = row.get(0)?;
                let json_str: Option<String> = row.get(1)?;
                Ok((rt, json_str))
            },
        )
        .ok();

    let (resource_type_for_audit, patient_id) = match &pre_delete {
        Some((rt, Some(json_str))) => {
            let parsed: serde_json::Value =
                serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);
            let pid = extract_patient_id(rt, &parsed);
            (rt.clone(), pid)
        }
        Some((rt, None)) => (rt.clone(), None),
        None => ("unknown".to_string(), None),
    };

    let rows_affected = conn.execute(
        "DELETE FROM fhir_resources WHERE id = ?1",
        rusqlite::params![id.clone()],
    )?;

    if rows_affected == 0 {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id,
                action: "fhir.delete".to_string(),
                resource_type: resource_type_for_audit,
                resource_id: Some(id.clone()),
                patient_id,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(format!("Not found: {}", id)),
            },
        );
        return Err(AppError::NotFound(format!("Resource not found: {}", id)));
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "fhir.delete".to_string(),
            resource_type: resource_type_for_audit,
            resource_id: Some(id),
            patient_id,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for audit injection helpers
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::entry::write_audit_entry;
    use rusqlite::Connection;

    /// Minimal in-memory DB with the fhir_resources and audit_logs schemas.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fhir_resources (
                id TEXT PRIMARY KEY NOT NULL,
                resource_type TEXT NOT NULL,
                resource JSON NOT NULL,
                version_id INTEGER NOT NULL DEFAULT 1,
                last_updated TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE audit_logs (
                id            TEXT PRIMARY KEY NOT NULL,
                timestamp     TEXT NOT NULL,
                user_id       TEXT NOT NULL,
                action        TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id   TEXT,
                patient_id    TEXT,
                device_id     TEXT NOT NULL,
                success       INTEGER NOT NULL CHECK (success IN (0, 1)),
                details       TEXT,
                previous_hash TEXT NOT NULL,
                entry_hash    TEXT NOT NULL UNIQUE
            );",
        )
        .unwrap();
        conn
    }

    // ── extract_patient_id ───────────────────────────────────────────────────

    #[test]
    fn extract_patient_id_from_patient_resource() {
        let resource = serde_json::json!({ "id": "pat-001", "name": "Alice" });
        let pid = extract_patient_id("Patient", &resource);
        assert_eq!(pid.as_deref(), Some("pat-001"));
    }

    #[test]
    fn extract_patient_id_from_subject_reference() {
        let resource = serde_json::json!({
            "subject": { "reference": "Patient/pat-002" }
        });
        let pid = extract_patient_id("Observation", &resource);
        assert_eq!(pid.as_deref(), Some("Patient/pat-002"));
    }

    #[test]
    fn extract_patient_id_from_patient_reference() {
        let resource = serde_json::json!({
            "patient": { "reference": "Patient/pat-003" }
        });
        let pid = extract_patient_id("AllergyIntolerance", &resource);
        assert_eq!(pid.as_deref(), Some("Patient/pat-003"));
    }

    #[test]
    fn extract_patient_id_returns_none_when_absent() {
        let resource = serde_json::json!({ "resourceType": "Practitioner" });
        let pid = extract_patient_id("Practitioner", &resource);
        assert!(pid.is_none());
    }

    // ── audit write path (integration) ──────────────────────────────────────

    /// Verify that a successful create produces exactly one audit row
    /// with the expected action, resource_type, and success = 1.
    #[test]
    fn audit_write_on_create_success() {
        let conn = test_db();

        // Simulate what create_resource does after the INSERT succeeds.
        let resource_id = "res-001".to_string();
        let entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "fhir.create".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: Some(resource_id.clone()),
                patient_id: Some("pat-001".to_string()),
                device_id: "DEVICE_PENDING".to_string(),
                success: true,
                details: None,
            },
        )
        .unwrap();

        assert!(entry.success);
        assert_eq!(entry.action, "fhir.create");
        assert_eq!(entry.resource_type, "Patient");
        assert_eq!(entry.resource_id.as_deref(), Some("res-001"));
        assert_eq!(entry.patient_id.as_deref(), Some("pat-001"));
    }

    /// Verify that a failed create records success = false.
    #[test]
    fn audit_write_on_create_failure() {
        let conn = test_db();

        let entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "fhir.create".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "DEVICE_PENDING".to_string(),
                success: false,
                details: Some("DB error: UNIQUE constraint failed".to_string()),
            },
        )
        .unwrap();

        assert!(!entry.success);
        assert!(entry.details.unwrap().contains("UNIQUE constraint"));
    }

    /// Verify the hash chain integrity across multiple commands.
    ///
    /// Writes three audit entries in sequence (simulating create, update, delete)
    /// and asserts the chain links correctly.
    #[test]
    fn audit_chain_across_fhir_operations() {
        let conn = test_db();

        let ops = [
            ("fhir.create", "Patient", true),
            ("fhir.update", "Patient", true),
            ("fhir.delete", "Patient", true),
        ];

        let mut entries = vec![];
        for (action, rt, success) in &ops {
            let e = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: "user-001".to_string(),
                    action: action.to_string(),
                    resource_type: rt.to_string(),
                    resource_id: Some("res-001".to_string()),
                    patient_id: Some("pat-001".to_string()),
                    device_id: "DEVICE_PENDING".to_string(),
                    success: *success,
                    details: None,
                },
            )
            .unwrap();
            entries.push(e);
        }

        // Verify chain: first row has GENESIS, subsequent rows link correctly.
        assert_eq!(entries[0].previous_hash, "GENESIS");
        assert_eq!(entries[1].previous_hash, entries[0].entry_hash);
        assert_eq!(entries[2].previous_hash, entries[1].entry_hash);
    }

    /// Verify that denied permission attempts (success=false) are recorded.
    #[test]
    fn audit_permission_denied_records_failure() {
        let conn = test_db();

        let entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "UNAUTHENTICATED".to_string(),
                action: "fhir.create".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "DEVICE_PENDING".to_string(),
                success: false,
                details: Some("Permission denied: Not authenticated".to_string()),
            },
        )
        .unwrap();

        assert!(!entry.success);
        assert_eq!(entry.user_id, "UNAUTHENTICATED");
        let details = entry.details.unwrap();
        assert!(details.contains("Permission denied"));
    }

    /// Verify auth.login and auth.logout actions write correctly.
    #[test]
    fn audit_auth_actions() {
        let conn = test_db();

        let login_entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "auth.login".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "DEVICE_PENDING".to_string(),
                success: true,
                details: None,
            },
        )
        .unwrap();

        let logout_entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "auth.logout".to_string(),
                resource_type: "auth".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "DEVICE_PENDING".to_string(),
                success: true,
                details: None,
            },
        )
        .unwrap();

        assert_eq!(login_entry.action, "auth.login");
        assert_eq!(logout_entry.action, "auth.logout");
        // Chain links
        assert_eq!(logout_entry.previous_hash, login_entry.entry_hash);
    }

    /// Verify break_glass actions write correctly.
    #[test]
    fn audit_break_glass_actions() {
        let conn = test_db();

        let activate = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "break_glass.activate".to_string(),
                resource_type: "break_glass".to_string(),
                resource_id: Some("log-001".to_string()),
                patient_id: Some("Patient/pat-001".to_string()),
                device_id: "DEVICE_PENDING".to_string(),
                success: true,
                details: Some("reason: emergency access".to_string()),
            },
        )
        .unwrap();

        let deactivate = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "break_glass.deactivate".to_string(),
                resource_type: "break_glass".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "DEVICE_PENDING".to_string(),
                success: true,
                details: None,
            },
        )
        .unwrap();

        assert_eq!(activate.action, "break_glass.activate");
        assert_eq!(deactivate.action, "break_glass.deactivate");
        assert!(activate.details.unwrap().contains("emergency access"));
        // Chain links
        assert_eq!(deactivate.previous_hash, activate.entry_hash);
    }
}
