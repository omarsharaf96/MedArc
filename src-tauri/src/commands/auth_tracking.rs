/// commands/auth_tracking.rs — Authorization & Visit Tracking (S07/T01)
///
/// Implements insurance authorization tracking with visit counters:
///   - CRUD for auth records (payer, auth number, authorized visits, date range, CPT codes)
///   - Visit counter auto-increment on cosign/lock
///   - Alert computation (expiring, expired, exhausted, low visits)
///   - Re-authorization letter generation
///
/// Data model
/// ----------
/// Auth records are stored as FHIR-aligned JSON in `fhir_resources`.
/// Migration 15 adds `auth_record_index` for fast patient/status lookups.
///
/// RBAC
/// ----
/// Uses ClinicalDocumentation resource permissions:
///   Provider / SystemAdmin → full CRUD
///   NurseMa               → Create + Read + Update (no delete)
///   BillingStaff           → Read only
///   FrontDesk              → No access
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating/updating an auth record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRecordInput {
    /// Patient the authorization belongs to.
    pub patient_id: String,
    /// Insurance payer name.
    pub payer_name: String,
    /// Payer phone number.
    pub payer_phone: Option<String>,
    /// Authorization number from the payer.
    pub auth_number: Option<String>,
    /// Total number of visits authorized.
    pub authorized_visits: i64,
    /// JSON array of authorized CPT codes (e.g. ["97110","97140"]).
    pub authorized_cpt_codes: Option<Vec<String>>,
    /// Authorization start date (ISO 8601 date, e.g. "2026-01-01").
    pub start_date: String,
    /// Authorization end date (ISO 8601 date, e.g. "2026-06-30").
    pub end_date: String,
    /// Free-text notes.
    pub notes: Option<String>,
}

/// Auth record returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRecord {
    pub auth_id: String,
    pub patient_id: String,
    pub payer_name: String,
    pub payer_phone: Option<String>,
    pub auth_number: Option<String>,
    pub authorized_visits: i64,
    pub visits_used: i64,
    pub authorized_cpt_codes: Option<Vec<String>>,
    pub start_date: String,
    pub end_date: String,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub resource: serde_json::Value,
}

/// Alert type for authorization issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthAlert {
    pub auth_id: String,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub payer_name: String,
    pub auth_number: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR builder
// ─────────────────────────────────────────────────────────────────────────────

fn build_auth_record_fhir(id: &str, input: &AuthRecordInput, visits_used: i64, status: &str) -> serde_json::Value {
    let mut resource = serde_json::json!({
        "resourceType": "Coverage",
        "id": id,
        "status": status,
        "beneficiary": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "payor": [{
            "display": input.payer_name
        }],
        "period": {
            "start": input.start_date,
            "end": input.end_date
        },
        "extension": [
            {
                "url": "http://medarc.local/fhir/StructureDefinition/auth-number",
                "valueString": input.auth_number.clone().unwrap_or_default()
            },
            {
                "url": "http://medarc.local/fhir/StructureDefinition/authorized-visits",
                "valueInteger": input.authorized_visits
            },
            {
                "url": "http://medarc.local/fhir/StructureDefinition/visits-used",
                "valueInteger": visits_used
            }
        ]
    });

    if let Some(ref phone) = input.payer_phone {
        resource["payor"][0]["telecom"] = serde_json::json!([{
            "system": "phone",
            "value": phone
        }]);
    }

    if let Some(ref codes) = input.authorized_cpt_codes {
        resource["class"] = serde_json::json!([{
            "type": {
                "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/coverage-class",
                    "code": "cpt",
                    "display": "CPT Codes"
                }]
            },
            "value": codes.join(",")
        }]);
    }

    if let Some(ref notes) = input.notes {
        resource["note"] = serde_json::json!([{
            "text": notes
        }]);
    }

    resource
}

// ─────────────────────────────────────────────────────────────────────────────
// Alert computation
// ─────────────────────────────────────────────────────────────────────────────

/// Compute alerts for a single auth record (pure function — testable).
pub fn compute_alerts(
    auth_id: &str,
    payer_name: &str,
    auth_number: Option<&str>,
    authorized_visits: i64,
    visits_used: i64,
    _start_date: &str,
    end_date: &str,
    status: &str,
    today: &str,
) -> Vec<AuthAlert> {
    let mut alerts: Vec<AuthAlert> = Vec::new();

    // Skip non-active records
    if status != "active" {
        return alerts;
    }

    let visits_remaining = authorized_visits - visits_used;

    // Check if expired
    if end_date < today {
        alerts.push(AuthAlert {
            auth_id: auth_id.to_string(),
            alert_type: "expired".to_string(),
            severity: "error".to_string(),
            message: format!("Authorization expired on {}", end_date),
            payer_name: payer_name.to_string(),
            auth_number: auth_number.map(|s| s.to_string()),
        });
        return alerts;
    }

    // Check if exhausted
    if visits_remaining <= 0 {
        alerts.push(AuthAlert {
            auth_id: auth_id.to_string(),
            alert_type: "exhausted".to_string(),
            severity: "error".to_string(),
            message: "Authorization exhausted — all authorized visits used".to_string(),
            payer_name: payer_name.to_string(),
            auth_number: auth_number.map(|s| s.to_string()),
        });
        return alerts;
    }

    // Check if expiring soon (within 7 days)
    // Simple date comparison — both are ISO 8601 date strings (YYYY-MM-DD)
    if let Some(days_until_expiry) = days_between(today, end_date) {
        if days_until_expiry <= 7 {
            alerts.push(AuthAlert {
                auth_id: auth_id.to_string(),
                alert_type: "expiring_soon".to_string(),
                severity: "warning".to_string(),
                message: format!("Authorization expiring in {} day(s)", days_until_expiry),
                payer_name: payer_name.to_string(),
                auth_number: auth_number.map(|s| s.to_string()),
            });
        }
    }

    // Check low visits (2 or fewer remaining)
    if visits_remaining <= 2 {
        alerts.push(AuthAlert {
            auth_id: auth_id.to_string(),
            alert_type: "low_visits".to_string(),
            severity: "warning".to_string(),
            message: format!("{} visit(s) remaining on authorization", visits_remaining),
            payer_name: payer_name.to_string(),
            auth_number: auth_number.map(|s| s.to_string()),
        });
    }

    alerts
}

/// Calculate days between two ISO 8601 date strings (YYYY-MM-DD).
/// Returns None if either date is invalid.
fn days_between(from: &str, to: &str) -> Option<i64> {
    let from_date = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?;
    let to_date = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d").ok()?;
    Some((to_date - from_date).num_days())
}

/// Compute the status of an auth record based on current date and visit counts.
pub fn compute_status(authorized_visits: i64, visits_used: i64, end_date: &str, today: &str) -> String {
    if end_date < today {
        "expired".to_string()
    } else if visits_used >= authorized_visits {
        "exhausted".to_string()
    } else {
        "active".to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Re-auth letter generation
// ─────────────────────────────────────────────────────────────────────────────

fn generate_reauth_letter_text(
    patient_name: &str,
    patient_dob: &str,
    payer_name: &str,
    payer_phone: &str,
    auth_number: &str,
    authorized_visits: i64,
    visits_used: i64,
    start_date: &str,
    end_date: &str,
    provider_name: &str,
    today: &str,
) -> String {
    format!(
        r#"RE-AUTHORIZATION REQUEST

Date: {}
To: {} {}
From: {}

RE: Re-Authorization Request for Continued Treatment

Patient Name: {}
Date of Birth: {}
Authorization #: {}

Dear Utilization Review Department,

I am writing to request re-authorization for continued treatment for the above-referenced patient.

Current Authorization Summary:
- Authorization Number: {}
- Authorization Period: {} to {}
- Authorized Visits: {}
- Visits Used: {}
- Visits Remaining: {}

The patient continues to require ongoing treatment and has demonstrated meaningful progress toward established goals. Discontinuation of treatment at this time would result in regression of functional gains.

I respectfully request re-authorization of additional visits to continue the current plan of care.

Please contact our office if additional clinical documentation is needed to process this request.

Sincerely,

{}
"#,
        today,
        payer_name,
        if payer_phone.is_empty() { String::new() } else { format!("({})", payer_phone) },
        provider_name,
        patient_name,
        patient_dob,
        auth_number,
        auth_number,
        start_date,
        end_date,
        authorized_visits,
        visits_used,
        authorized_visits - visits_used,
        provider_name,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new authorization record.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn create_auth_record(
    input: AuthRecordInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AuthRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let auth_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let status = compute_status(input.authorized_visits, 0, &input.end_date, &today);

    let fhir = build_auth_record_fhir(&auth_id, &input, 0, &status);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let cpt_json = input
        .authorized_cpt_codes
        .as_ref()
        .map(|codes| serde_json::to_string(codes).unwrap_or_else(|_| "[]".to_string()));

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert FHIR resource
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Coverage', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![auth_id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert index row
    conn.execute(
        "INSERT INTO auth_record_index
            (auth_id, resource_id, patient_id, payer_name, payer_phone, auth_number,
             authorized_visits, visits_used, authorized_cpt_codes, start_date, end_date,
             status, notes, created_at)
         VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            auth_id,
            input.patient_id,
            input.payer_name,
            input.payer_phone,
            input.auth_number,
            input.authorized_visits,
            cpt_json,
            input.start_date,
            input.end_date,
            status,
            input.notes,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "auth_tracking.create".to_string(),
            resource_type: "Coverage".to_string(),
            resource_id: Some(auth_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("payer={}", input.payer_name)),
        },
    )?;

    Ok(AuthRecord {
        auth_id,
        patient_id: input.patient_id,
        payer_name: input.payer_name,
        payer_phone: input.payer_phone,
        auth_number: input.auth_number,
        authorized_visits: input.authorized_visits,
        visits_used: 0,
        authorized_cpt_codes: input.authorized_cpt_codes,
        start_date: input.start_date,
        end_date: input.end_date,
        status,
        notes: input.notes,
        created_at: now,
        resource: fhir,
    })
}

/// Retrieve a single auth record by ID.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_auth_record(
    auth_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AuthRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let record = query_auth_record(&conn, &auth_id)?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "auth_tracking.read".to_string(),
            resource_type: "Coverage".to_string(),
            resource_id: Some(auth_id),
            patient_id: Some(record.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(record)
}

/// List all auth records for a patient.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_auth_records(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<AuthRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT ai.auth_id, ai.patient_id, ai.payer_name, ai.payer_phone, ai.auth_number,
                    ai.authorized_visits, ai.visits_used, ai.authorized_cpt_codes,
                    ai.start_date, ai.end_date, ai.status, ai.notes, ai.created_at,
                    fr.resource
             FROM auth_record_index ai
             JOIN fhir_resources fr ON fr.id = ai.auth_id
             WHERE ai.patient_id = ?1
             ORDER BY ai.created_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            let cpt_str: Option<String> = row.get(7)?;
            let resource_str: String = row.get(13)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                cpt_str,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                resource_str,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let results: Vec<AuthRecord> = records
        .into_iter()
        .map(
            |(
                auth_id,
                patient_id,
                payer_name,
                payer_phone,
                auth_number,
                authorized_visits,
                visits_used,
                cpt_str,
                start_date,
                end_date,
                status,
                notes,
                created_at,
                resource_str,
            )| {
                let cpt_codes: Option<Vec<String>> =
                    cpt_str.and_then(|s| serde_json::from_str(&s).ok());
                let resource: serde_json::Value =
                    serde_json::from_str(&resource_str).unwrap_or(serde_json::json!({}));
                AuthRecord {
                    auth_id,
                    patient_id,
                    payer_name,
                    payer_phone,
                    auth_number,
                    authorized_visits,
                    visits_used,
                    authorized_cpt_codes: cpt_codes,
                    start_date,
                    end_date,
                    status,
                    notes,
                    created_at,
                    resource,
                }
            },
        )
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "auth_tracking.list".to_string(),
            resource_type: "Coverage".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", results.len())),
        },
    )?;

    Ok(results)
}

/// Update an existing auth record.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn update_auth_record(
    auth_id: String,
    input: AuthRecordInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AuthRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get current visits_used
    let visits_used: i64 = conn
        .query_row(
            "SELECT visits_used FROM auth_record_index WHERE auth_id = ?1",
            rusqlite::params![auth_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Auth record {} not found", auth_id)))?;

    let status = compute_status(input.authorized_visits, visits_used, &input.end_date, &today);
    let fhir = build_auth_record_fhir(&auth_id, &input, visits_used, &status);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let cpt_json = input
        .authorized_cpt_codes
        .as_ref()
        .map(|codes| serde_json::to_string(codes).unwrap_or_else(|_| "[]".to_string()));

    // Update FHIR resource
    conn.execute(
        "UPDATE fhir_resources
         SET resource = ?1, version_id = version_id + 1, last_updated = ?2, updated_at = ?2
         WHERE id = ?3",
        rusqlite::params![fhir_json, now, auth_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Update index
    conn.execute(
        "UPDATE auth_record_index
         SET patient_id = ?1, payer_name = ?2, payer_phone = ?3, auth_number = ?4,
             authorized_visits = ?5, authorized_cpt_codes = ?6, start_date = ?7,
             end_date = ?8, status = ?9, notes = ?10
         WHERE auth_id = ?11",
        rusqlite::params![
            input.patient_id,
            input.payer_name,
            input.payer_phone,
            input.auth_number,
            input.authorized_visits,
            cpt_json,
            input.start_date,
            input.end_date,
            status,
            input.notes,
            auth_id,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM auth_record_index WHERE auth_id = ?1",
            rusqlite::params![auth_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| now.clone());

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "auth_tracking.update".to_string(),
            resource_type: "Coverage".to_string(),
            resource_id: Some(auth_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("payer={}", input.payer_name)),
        },
    )?;

    Ok(AuthRecord {
        auth_id,
        patient_id: input.patient_id,
        payer_name: input.payer_name,
        payer_phone: input.payer_phone,
        auth_number: input.auth_number,
        authorized_visits: input.authorized_visits,
        visits_used,
        authorized_cpt_codes: input.authorized_cpt_codes,
        start_date: input.start_date,
        end_date: input.end_date,
        status,
        notes: input.notes,
        created_at,
        resource: fhir,
    })
}

/// Increment the visit count for active auth records for a patient.
/// Called when a Daily Progress Note is co-signed and locked.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn increment_visit_count(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<AuthRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Find active auth records for this patient whose date range includes today
    let mut stmt = conn
        .prepare(
            "SELECT auth_id, authorized_visits, visits_used, end_date
             FROM auth_record_index
             WHERE patient_id = ?1 AND status = 'active'
               AND start_date <= ?2 AND end_date >= ?2",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let active_auths: Vec<(String, i64, i64, String)> = stmt
        .query_map(rusqlite::params![patient_id, today], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut updated_records: Vec<AuthRecord> = Vec::new();

    for (auth_id, authorized_visits, visits_used, end_date) in active_auths {
        let new_visits_used = visits_used + 1;
        let new_status = compute_status(authorized_visits, new_visits_used, &end_date, &today);

        // Update index
        conn.execute(
            "UPDATE auth_record_index SET visits_used = ?1, status = ?2 WHERE auth_id = ?3",
            rusqlite::params![new_visits_used, new_status, auth_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Update FHIR resource visit count extension
        conn.execute(
            "UPDATE fhir_resources
             SET resource = json_set(resource,
                '$.extension[2].valueInteger', ?1,
                '$.status', ?2
             ),
             version_id = version_id + 1, last_updated = ?3, updated_at = ?3
             WHERE id = ?4",
            rusqlite::params![new_visits_used, new_status, now, auth_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "auth_tracking.increment_visit".to_string(),
                resource_type: "Coverage".to_string(),
                resource_id: Some(auth_id.clone()),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!(
                    "visits={}/{} status={}",
                    new_visits_used, authorized_visits, new_status
                )),
            },
        )?;

        // Re-query for the full record to return
        if let Ok(record) = query_auth_record(&conn, &auth_id) {
            updated_records.push(record);
        }
    }

    Ok(updated_records)
}

/// Get active alerts for a patient's auth records.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_auth_alerts(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    _device_id: State<'_, DeviceId>,
) -> Result<Vec<AuthAlert>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = sess; // consumed above

    let mut stmt = conn
        .prepare(
            "SELECT auth_id, payer_name, auth_number, authorized_visits, visits_used,
                    start_date, end_date, status
             FROM auth_record_index
             WHERE patient_id = ?1 AND status = 'active'",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows: Vec<(String, String, Option<String>, i64, i64, String, String, String)> = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut all_alerts: Vec<AuthAlert> = Vec::new();

    for (auth_id, payer_name, auth_number, authorized_visits, visits_used, start_date, end_date, status) in rows {
        let alerts = compute_alerts(
            &auth_id,
            &payer_name,
            auth_number.as_deref(),
            authorized_visits,
            visits_used,
            &start_date,
            &end_date,
            &status,
            &today,
        );
        all_alerts.extend(alerts);
    }

    Ok(all_alerts)
}

/// Generate a pre-filled re-authorization request letter.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn generate_reauth_letter(
    auth_id: String,
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<String, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get auth record
    let (payer_name, payer_phone, auth_number, authorized_visits, visits_used, start_date, end_date): (
        String, Option<String>, Option<String>, i64, i64, String, String,
    ) = conn
        .query_row(
            "SELECT payer_name, payer_phone, auth_number, authorized_visits, visits_used,
                    start_date, end_date
             FROM auth_record_index WHERE auth_id = ?1",
            rusqlite::params![auth_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("Auth record {} not found", auth_id)))?;

    // Get patient name and DOB from patient_index
    let (patient_name, patient_dob): (String, String) = conn
        .query_row(
            "SELECT COALESCE(given_name || ' ', '') || family_name, COALESCE(birth_date, 'Unknown')
             FROM patient_index WHERE patient_id = ?1",
            rusqlite::params![patient_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_else(|_| ("Unknown Patient".to_string(), "Unknown".to_string()));

    // Get provider name from session user
    let provider_name: String = conn
        .query_row(
            "SELECT display_name FROM users WHERE id = ?1",
            rusqlite::params![sess.user_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "Provider".to_string());

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let letter = generate_reauth_letter_text(
        &patient_name,
        &patient_dob,
        &payer_name,
        &payer_phone.unwrap_or_default(),
        &auth_number.unwrap_or_else(|| "N/A".to_string()),
        authorized_visits,
        visits_used,
        &start_date,
        &end_date,
        &provider_name,
        &today,
    );

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "auth_tracking.generate_reauth_letter".to_string(),
            resource_type: "Coverage".to_string(),
            resource_id: Some(auth_id),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(letter)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn query_auth_record(
    conn: &rusqlite::Connection,
    auth_id: &str,
) -> Result<AuthRecord, AppError> {
    let (
        patient_id,
        payer_name,
        payer_phone,
        auth_number,
        authorized_visits,
        visits_used,
        cpt_str,
        start_date,
        end_date,
        status,
        notes,
        created_at,
        resource_str,
    ): (
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        i64,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT ai.patient_id, ai.payer_name, ai.payer_phone, ai.auth_number,
                    ai.authorized_visits, ai.visits_used, ai.authorized_cpt_codes,
                    ai.start_date, ai.end_date, ai.status, ai.notes, ai.created_at,
                    fr.resource
             FROM auth_record_index ai
             JOIN fhir_resources fr ON fr.id = ai.auth_id
             WHERE ai.auth_id = ?1",
            rusqlite::params![auth_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("Auth record {} not found", auth_id)))?;

    let cpt_codes: Option<Vec<String>> = cpt_str.and_then(|s| serde_json::from_str(&s).ok());
    let resource: serde_json::Value =
        serde_json::from_str(&resource_str).unwrap_or(serde_json::json!({}));

    Ok(AuthRecord {
        auth_id: auth_id.to_string(),
        patient_id,
        payer_name,
        payer_phone,
        auth_number,
        authorized_visits,
        visits_used,
        authorized_cpt_codes: cpt_codes,
        start_date,
        end_date,
        status,
        notes,
        created_at,
        resource,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_status_active() {
        let status = compute_status(10, 5, "2026-12-31", "2026-06-15");
        assert_eq!(status, "active");
    }

    #[test]
    fn compute_status_expired() {
        let status = compute_status(10, 5, "2026-01-01", "2026-06-15");
        assert_eq!(status, "expired");
    }

    #[test]
    fn compute_status_exhausted() {
        let status = compute_status(10, 10, "2026-12-31", "2026-06-15");
        assert_eq!(status, "exhausted");
    }

    #[test]
    fn compute_status_exhausted_over() {
        let status = compute_status(5, 7, "2026-12-31", "2026-06-15");
        assert_eq!(status, "exhausted");
    }

    #[test]
    fn alerts_expired() {
        let alerts = compute_alerts(
            "auth-1",
            "BlueCross",
            Some("ABC123"),
            10,
            5,
            "2026-01-01",
            "2026-01-31",
            "active",
            "2026-02-15",
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "expired");
        assert_eq!(alerts[0].severity, "error");
    }

    #[test]
    fn alerts_exhausted() {
        let alerts = compute_alerts(
            "auth-1",
            "Aetna",
            Some("XYZ789"),
            10,
            10,
            "2026-01-01",
            "2026-12-31",
            "active",
            "2026-06-15",
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "exhausted");
        assert_eq!(alerts[0].severity, "error");
    }

    #[test]
    fn alerts_expiring_soon() {
        let alerts = compute_alerts(
            "auth-1",
            "United",
            None,
            20,
            5,
            "2026-01-01",
            "2026-06-20",
            "active",
            "2026-06-15",
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "expiring_soon");
        assert_eq!(alerts[0].severity, "warning");
    }

    #[test]
    fn alerts_low_visits() {
        let alerts = compute_alerts(
            "auth-1",
            "Cigna",
            Some("DEF456"),
            10,
            8,
            "2026-01-01",
            "2026-12-31",
            "active",
            "2026-06-15",
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "low_visits");
        assert_eq!(alerts[0].severity, "warning");
        assert!(alerts[0].message.contains("2 visit(s) remaining"));
    }

    #[test]
    fn alerts_low_visits_and_expiring_soon() {
        // Both conditions: 1 visit remaining AND expiring in 3 days
        let alerts = compute_alerts(
            "auth-1",
            "Humana",
            Some("GHI012"),
            10,
            9,
            "2026-01-01",
            "2026-06-18",
            "active",
            "2026-06-15",
        );
        assert_eq!(alerts.len(), 2);
        let types: Vec<&str> = alerts.iter().map(|a| a.alert_type.as_str()).collect();
        assert!(types.contains(&"expiring_soon"));
        assert!(types.contains(&"low_visits"));
    }

    #[test]
    fn alerts_no_alerts_healthy() {
        let alerts = compute_alerts(
            "auth-1",
            "BlueCross",
            Some("JKL345"),
            20,
            5,
            "2026-01-01",
            "2026-12-31",
            "active",
            "2026-06-15",
        );
        assert!(alerts.is_empty());
    }

    #[test]
    fn alerts_skip_non_active() {
        let alerts = compute_alerts(
            "auth-1",
            "BlueCross",
            Some("MNO678"),
            10,
            10,
            "2026-01-01",
            "2026-12-31",
            "exhausted",
            "2026-06-15",
        );
        assert!(alerts.is_empty());
    }

    #[test]
    fn days_between_basic() {
        assert_eq!(days_between("2026-01-01", "2026-01-08"), Some(7));
        assert_eq!(days_between("2026-06-15", "2026-06-20"), Some(5));
        assert_eq!(days_between("2026-06-15", "2026-06-15"), Some(0));
        assert_eq!(days_between("2026-06-15", "2026-06-10"), Some(-5));
    }

    #[test]
    fn days_between_invalid() {
        assert_eq!(days_between("not-a-date", "2026-06-15"), None);
        assert_eq!(days_between("2026-06-15", "invalid"), None);
    }

    #[test]
    fn reauth_letter_contains_key_info() {
        let letter = generate_reauth_letter_text(
            "John Smith",
            "1990-05-15",
            "BlueCross BlueShield",
            "800-555-1234",
            "AUTH-12345",
            20,
            18,
            "2026-01-01",
            "2026-06-30",
            "Dr. Jane Doe",
            "2026-06-15",
        );
        assert!(letter.contains("John Smith"));
        assert!(letter.contains("1990-05-15"));
        assert!(letter.contains("BlueCross BlueShield"));
        assert!(letter.contains("AUTH-12345"));
        assert!(letter.contains("20"));
        assert!(letter.contains("18"));
        assert!(letter.contains("Dr. Jane Doe"));
        assert!(letter.contains("RE-AUTHORIZATION REQUEST"));
    }
}
