use tauri::State;

use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::db::models::{CreateFhirResource, FhirResource, FhirResourceList, UpdateFhirResource};
use crate::error::AppError;
use crate::rbac::field_filter;
use crate::rbac::middleware;
use crate::rbac::roles::{self, Action, Resource};

/// Create a new FHIR resource in the encrypted database.
///
/// Requires authenticated session with ClinicalRecords:Create permission.
#[tauri::command]
pub fn create_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    input: CreateFhirResource,
) -> Result<FhirResource, AppError> {
    middleware::check_permission(&session, Resource::ClinicalRecords, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let resource_json = serde_json::to_string(&input.resource)
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
        rusqlite::params![id, input.resource_type, resource_json, now, now, now],
    )?;

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

/// Retrieve a single FHIR resource by ID with role-based field filtering.
#[tauri::command]
pub fn get_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    id: String,
) -> Result<FhirResource, AppError> {
    let (_user_id, role) =
        middleware::check_permission(&session, Resource::ClinicalRecords, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT id, resource_type, resource, version_id, last_updated, created_at, updated_at
         FROM fhir_resources WHERE id = ?1",
    )?;

    let mut resource = stmt
        .query_row(rusqlite::params![id], |row| {
            let resource_str: String = row.get(2)?;
            let resource: serde_json::Value = serde_json::from_str(&resource_str)
                .unwrap_or(serde_json::Value::Null);
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
        })?;

    // Apply field-level filtering based on role
    let allowed_fields = roles::visible_fields(role, &resource.resource_type);
    let field_refs: Vec<&str> = allowed_fields.iter().copied().collect();
    resource.resource = field_filter::filter_resource(&resource.resource, &field_refs);

    Ok(resource)
}

/// List FHIR resources with role-based field filtering.
#[tauri::command]
pub fn list_resources(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    resource_type: Option<String>,
) -> Result<FhirResourceList, AppError> {
    let (_user_id, role) =
        middleware::check_permission(&session, Resource::ClinicalRecords, Action::Read)?;

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

    let total: i64 = conn.query_row(
        count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(query)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();
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

    // Apply field-level filtering to each resource based on role
    let filtered_resources: Vec<FhirResource> = resources
        .into_iter()
        .map(|mut r| {
            let allowed_fields = roles::visible_fields(role, &r.resource_type);
            let field_refs: Vec<&str> = allowed_fields.iter().copied().collect();
            r.resource = field_filter::filter_resource(&r.resource, &field_refs);
            r
        })
        .collect();

    Ok(FhirResourceList {
        resources: filtered_resources,
        total,
    })
}

/// Update an existing FHIR resource's JSON content.
///
/// Requires ClinicalRecords:Update permission.
#[tauri::command]
pub fn update_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    input: UpdateFhirResource,
) -> Result<FhirResource, AppError> {
    middleware::check_permission(&session, Resource::ClinicalRecords, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let current_version: i64 = conn
        .query_row(
            "SELECT version_id FROM fhir_resources WHERE id = ?1",
            rusqlite::params![input.id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Resource not found: {}", input.id))
            }
            other => AppError::Database(other.to_string()),
        })?;

    let now = chrono::Utc::now().to_rfc3339();
    let new_version = current_version + 1;
    let resource_json = serde_json::to_string(&input.resource)
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, input.id],
    )?;

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

    Ok(resource)
}

/// Delete a FHIR resource by ID.
///
/// Requires ClinicalRecords:Delete permission.
#[tauri::command]
pub fn delete_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    id: String,
) -> Result<(), AppError> {
    middleware::check_permission(&session, Resource::ClinicalRecords, Action::Delete)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows_affected = conn.execute(
        "DELETE FROM fhir_resources WHERE id = ?1",
        rusqlite::params![id],
    )?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!("Resource not found: {}", id)));
    }

    Ok(())
}
