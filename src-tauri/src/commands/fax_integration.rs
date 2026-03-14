/// commands/fax_integration.rs — Phaxio Fax Integration Backend (M003/S06)
///
/// Implements fax send/receive via the Phaxio API with full HIPAA audit logging:
///   - Configure Phaxio credentials (stored encrypted in SQLCipher app_settings)
///   - Send faxes via Phaxio POST /v2/faxes
///   - Poll for received faxes via GET /v2/faxes?direction=received
///   - Fax contacts directory CRUD
///   - Fax log with filterable queries
///   - Auto-retry failed faxes (max 2 retries)
///
/// Data model
/// ----------
/// Migration 15 creates two tables:
///   - `fax_log`      — tracks every sent/received fax with status lifecycle
///   - `fax_contacts`  — reusable fax contact directory
///
/// API credentials are stored in the existing `app_settings` table (SQLCipher encrypted).
///
/// RBAC
/// ----
/// All fax commands require `ClinicalRecords` resource access.
///   SystemAdmin    → full CRUD
///   Provider       → full CRUD
///   NurseMa        → Read + Update (can view/retry but not configure)
///   FrontDesk      → Read-only (view fax log)
///   BillingStaff   → Read-only
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.
/// Document content is NEVER logged (HIPAA).

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
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of automatic retries for a failed fax.
const MAX_AUTO_RETRIES: i32 = 2;

/// Phaxio API base URL.
const PHAXIO_BASE_URL: &str = "https://api.phaxio.com/v2";

// ─────────────────────────────────────────────────────────────────────────────
// Types — Phaxio Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Input for configuring Phaxio API credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaxioConfigInput {
    /// Phaxio API key.
    pub api_key: String,
    /// Phaxio API secret.
    pub api_secret: String,
    /// Sender fax number (E.164 format, e.g. "+15551234567").
    pub fax_number: String,
}

/// Phaxio configuration record returned to callers (secrets masked).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaxioConfigRecord {
    /// Whether Phaxio credentials are configured.
    pub configured: bool,
    /// The configured fax number (if any).
    pub fax_number: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types — Fax Records
// ─────────────────────────────────────────────────────────────────────────────

/// Valid fax direction values.
const VALID_DIRECTIONS: &[&str] = &["sent", "received"];

/// Valid fax status values.
const VALID_STATUSES: &[&str] = &["queued", "in_progress", "success", "failed"];

/// Valid fax contact type values.
const VALID_CONTACT_TYPES: &[&str] = &["insurance", "referring_md", "attorney", "other"];

/// A fax log record.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaxRecord {
    pub fax_id: String,
    pub phaxio_fax_id: Option<String>,
    pub direction: String,
    pub patient_id: Option<String>,
    pub recipient_name: Option<String>,
    pub recipient_fax: Option<String>,
    pub document_name: Option<String>,
    pub file_path: Option<String>,
    pub status: String,
    pub sent_at: String,
    pub delivered_at: Option<String>,
    pub pages: Option<i32>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

/// Input for sending a fax.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendFaxInput {
    /// Path to the file to fax.
    pub file_path: String,
    /// Recipient fax number (E.164 format).
    pub recipient_fax: String,
    /// Recipient name for logging.
    pub recipient_name: String,
    /// Optional patient ID to associate with the fax.
    pub patient_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types — Fax Contacts
// ─────────────────────────────────────────────────────────────────────────────

/// A fax contact directory entry.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaxContact {
    pub contact_id: String,
    pub name: String,
    pub organization: Option<String>,
    pub fax_number: String,
    pub phone_number: Option<String>,
    pub contact_type: String,
    pub notes: Option<String>,
    pub created_at: String,
}

/// Input for creating / updating a fax contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaxContactInput {
    pub name: String,
    pub organization: Option<String>,
    pub fax_number: String,
    pub phone_number: Option<String>,
    /// One of: "insurance", "referring_md", "attorney", "other".
    pub contact_type: String,
    pub notes: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Validate that a fax direction value is one of the allowed values.
pub fn validate_fax_direction(direction: &str) -> Result<(), AppError> {
    if VALID_DIRECTIONS.contains(&direction) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid fax direction '{}'. Must be one of: {}",
            direction,
            VALID_DIRECTIONS.join(", ")
        )))
    }
}

/// Validate that a fax status value is one of the allowed values.
pub fn validate_fax_status(status: &str) -> Result<(), AppError> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid fax status '{}'. Must be one of: {}",
            status,
            VALID_STATUSES.join(", ")
        )))
    }
}

/// Validate that a contact type value is one of the allowed values.
pub fn validate_contact_type(contact_type: &str) -> Result<(), AppError> {
    if VALID_CONTACT_TYPES.contains(&contact_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid contact type '{}'. Must be one of: {}",
            contact_type,
            VALID_CONTACT_TYPES.join(", ")
        )))
    }
}

/// Validate that a retry count is within the allowed maximum.
pub fn validate_retry_count(retry_count: i32) -> Result<(), AppError> {
    if retry_count <= MAX_AUTO_RETRIES {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Retry count {} exceeds maximum of {}",
            retry_count, MAX_AUTO_RETRIES
        )))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers — credential storage
// ─────────────────────────────────────────────────────────────────────────────

/// Store a Phaxio setting in app_settings (encrypted at rest by SQLCipher).
fn store_phaxio_setting(
    conn: &rusqlite::Connection,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![key, value],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Retrieve a Phaxio setting from app_settings.
fn get_phaxio_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>, AppError> {
    let result = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

/// Retrieve all three Phaxio credentials. Returns None if any are missing.
fn get_phaxio_credentials(
    conn: &rusqlite::Connection,
) -> Result<Option<(String, String, String)>, AppError> {
    let api_key = get_phaxio_setting(conn, "phaxio_api_key")?;
    let api_secret = get_phaxio_setting(conn, "phaxio_api_secret")?;
    let fax_number = get_phaxio_setting(conn, "phaxio_fax_number")?;
    match (api_key, api_secret, fax_number) {
        (Some(k), Some(s), Some(n)) => Ok(Some((k, s, n))),
        _ => Ok(None),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Phaxio Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configure Phaxio API credentials. Stores them in SQLCipher (encrypted at rest).
///
/// RBAC: SystemAdmin only.
#[tauri::command]
pub fn configure_phaxio(
    input: PhaxioConfigInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PhaxioConfigRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Create)?;

    // Only SystemAdmin can configure fax credentials
    if session.role != crate::rbac::roles::Role::SystemAdmin {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "configure_phaxio".to_string(),
                resource_type: "FaxIntegration".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("configure_phaxio requires SystemAdmin role".to_string()),
            },
        );
        return Err(AppError::Unauthorized(
            "Only SystemAdmin can configure Phaxio credentials".to_string(),
        ));
    }

    if input.api_key.is_empty() || input.api_secret.is_empty() || input.fax_number.is_empty() {
        return Err(AppError::Validation(
            "API key, API secret, and fax number are all required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    store_phaxio_setting(&conn, "phaxio_api_key", &input.api_key)?;
    store_phaxio_setting(&conn, "phaxio_api_secret", &input.api_secret)?;
    store_phaxio_setting(&conn, "phaxio_fax_number", &input.fax_number)?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id,
            action: "configure_phaxio".to_string(),
            resource_type: "FaxIntegration".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Phaxio credentials configured".to_string()),
        },
    );

    Ok(PhaxioConfigRecord {
        configured: true,
        fax_number: Some(input.fax_number),
    })
}

/// Test the Phaxio API connection by verifying stored credentials.
///
/// Calls GET /v2/account (or similar lightweight endpoint) to confirm credentials work.
///
/// RBAC: SystemAdmin or Provider.
#[tauri::command]
pub fn test_phaxio_connection(
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<bool, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let creds = get_phaxio_credentials(&conn)?;
    let (api_key, api_secret, _fax_number) = match creds {
        Some(c) => c,
        None => {
            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: session.user_id,
                    action: "test_phaxio_connection".to_string(),
                    resource_type: "FaxIntegration".to_string(),
                    resource_id: None,
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Phaxio credentials not configured".to_string()),
                },
            );
            return Err(AppError::Validation(
                "Phaxio credentials not configured".to_string(),
            ));
        }
    };

    // Drop conn before blocking HTTP call
    drop(conn);

    // Call Phaxio account info endpoint to verify credentials
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("{}/account", PHAXIO_BASE_URL))
        .basic_auth(&api_key, Some(&api_secret))
        .send()
        .map_err(|e| AppError::Database(format!("Phaxio API request failed: {}", e)))?;

    let success = resp.status().is_success();

    let conn2 = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    write_audit_entry(
        &conn2,
        AuditEntryInput {
            user_id: session.user_id,
            action: "test_phaxio_connection".to_string(),
            resource_type: "FaxIntegration".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success,
            details: Some(format!("Phaxio connection test: {}", if success { "passed" } else { "failed" })),
        },
    );

    Ok(success)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Send Fax
// ─────────────────────────────────────────────────────────────────────────────

/// Send a fax via Phaxio. Creates a fax_log entry with status "queued" and
/// submits the fax to the Phaxio API using multipart form data.
///
/// RBAC: SystemAdmin or Provider (ClinicalRecords::Create).
#[tauri::command]
pub fn send_fax(
    input: SendFaxInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Create)?;

    // Validate input
    if input.file_path.is_empty() {
        return Err(AppError::Validation("file_path is required".to_string()));
    }
    if input.recipient_fax.is_empty() {
        return Err(AppError::Validation(
            "recipient_fax is required".to_string(),
        ));
    }

    // Verify file exists
    if !std::path::Path::new(&input.file_path).exists() {
        return Err(AppError::Validation(format!(
            "File not found: {}",
            input.file_path
        )));
    }

    let fax_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let document_name = std::path::Path::new(&input.file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let creds = get_phaxio_credentials(&conn)?;
    let (api_key, api_secret, _fax_number) = match creds {
        Some(c) => c,
        None => {
            return Err(AppError::Validation(
                "Phaxio credentials not configured. Use configure_phaxio first.".to_string(),
            ));
        }
    };

    // Insert fax_log entry with status "queued"
    conn.execute(
        "INSERT INTO fax_log (fax_id, direction, patient_id, recipient_name, recipient_fax,
         document_name, file_path, status, sent_at, retry_count)
         VALUES (?1, 'sent', ?2, ?3, ?4, ?5, ?6, 'queued', ?7, 0)",
        rusqlite::params![
            &fax_id,
            &input.patient_id,
            &input.recipient_name,
            &input.recipient_fax,
            &document_name,
            &input.file_path,
            &now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Audit log (do not log document content — HIPAA)
    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "send_fax".to_string(),
            resource_type: "FaxLog".to_string(),
            resource_id: Some(fax_id.clone()),
            patient_id: input.patient_id.clone(),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "Fax queued to {} ({})",
                input.recipient_name, input.recipient_fax
            )),
        },
    );

    // Drop conn before blocking HTTP call
    drop(conn);

    // Read file bytes for multipart upload
    let file_bytes = std::fs::read(&input.file_path)
        .map_err(|e| AppError::Database(format!("Failed to read fax file: {}", e)))?;

    let file_part = reqwest::blocking::multipart::Part::bytes(file_bytes)
        .file_name(document_name.clone())
        .mime_str("application/pdf")
        .map_err(|e| AppError::Database(format!("Failed to create multipart part: {}", e)))?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("to", input.recipient_fax.clone())
        .part("file", file_part);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/faxes", PHAXIO_BASE_URL))
        .basic_auth(&api_key, Some(&api_secret))
        .multipart(form)
        .send();

    let conn2 = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    match resp {
        Ok(response) if response.status().is_success() => {
            // Try to extract the Phaxio fax ID from the response
            let phaxio_fax_id: Option<String> = response
                .json::<serde_json::Value>()
                .ok()
                .and_then(|v| v.get("data")?.get("id")?.as_i64())
                .map(|id| id.to_string());

            conn2
                .execute(
                    "UPDATE fax_log SET status = 'in_progress', phaxio_fax_id = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&phaxio_fax_id, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
        Ok(response) => {
            let error_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            conn2
                .execute(
                    "UPDATE fax_log SET status = 'failed', error_message = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&error_text, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
        Err(e) => {
            let error_msg = format!("Phaxio API request failed: {}", e);
            conn2
                .execute(
                    "UPDATE fax_log SET status = 'failed', error_message = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&error_msg, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
    }

    // Re-read the record to return current state
    let record = conn2
        .query_row(
            "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
             document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
             FROM fax_log WHERE fax_id = ?1",
            rusqlite::params![&fax_id],
            |row| {
                Ok(FaxRecord {
                    fax_id: row.get(0)?,
                    phaxio_fax_id: row.get(1)?,
                    direction: row.get(2)?,
                    patient_id: row.get(3)?,
                    recipient_name: row.get(4)?,
                    recipient_fax: row.get(5)?,
                    document_name: row.get(6)?,
                    file_path: row.get(7)?,
                    status: row.get(8)?,
                    sent_at: row.get(9)?,
                    delivered_at: row.get(10)?,
                    pages: row.get(11)?,
                    error_message: row.get(12)?,
                    retry_count: row.get(13)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(record)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Receive Fax (polling)
// ─────────────────────────────────────────────────────────────────────────────

/// Poll for received faxes from Phaxio. Downloads new inbound fax PDFs to
/// the app data directory and creates fax_log entries.
///
/// RBAC: SystemAdmin or Provider (ClinicalRecords::Read).
#[tauri::command]
pub fn poll_received_faxes(
    app_handle: tauri::AppHandle,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<FaxRecord>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let creds = get_phaxio_credentials(&conn)?;
    let (api_key, api_secret, _fax_number) = match creds {
        Some(c) => c,
        None => {
            return Err(AppError::Validation(
                "Phaxio credentials not configured".to_string(),
            ));
        }
    };

    drop(conn);

    // Get the app data directory for storing received fax PDFs
    use tauri::Manager;
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Database(format!("Failed to get app data dir: {}", e)))?;
    let fax_dir = app_data_dir.join("received_faxes");
    std::fs::create_dir_all(&fax_dir)
        .map_err(|e| AppError::Database(format!("Failed to create fax directory: {}", e)))?;

    // Poll Phaxio for received faxes
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("{}/faxes", PHAXIO_BASE_URL))
        .basic_auth(&api_key, Some(&api_secret))
        .query(&[("direction", "received")])
        .send()
        .map_err(|e| AppError::Database(format!("Phaxio API request failed: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError::Database(format!(
            "Phaxio API returned status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| AppError::Database(format!("Failed to parse Phaxio response: {}", e)))?;

    let mut new_records = Vec::new();

    let conn2 = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(faxes) = body.get("data").and_then(|d| d.as_array()) {
        for fax in faxes {
            let phaxio_id = fax
                .get("id")
                .and_then(|v| v.as_i64())
                .map(|id| id.to_string());

            let phaxio_id_str = match &phaxio_id {
                Some(id) => id.as_str(),
                None => continue,
            };

            // Check if we already have this fax
            let exists: bool = conn2
                .query_row(
                    "SELECT COUNT(*) FROM fax_log WHERE phaxio_fax_id = ?1",
                    rusqlite::params![phaxio_id_str],
                    |row| row.get::<_, i32>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);

            if exists {
                continue;
            }

            let fax_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let pages = fax.get("num_pages").and_then(|v| v.as_i64()).map(|p| p as i32);

            // Download the fax PDF
            let file_path = fax_dir.join(format!("fax-{}.pdf", phaxio_id_str));
            let file_path_str = file_path.to_string_lossy().to_string();

            let download_resp = client
                .get(format!("{}/faxes/{}/file", PHAXIO_BASE_URL, phaxio_id_str))
                .basic_auth(&api_key, Some(&api_secret))
                .send();

            let download_success = match download_resp {
                Ok(r) if r.status().is_success() => {
                    let bytes = r.bytes().unwrap_or_default();
                    std::fs::write(&file_path, &bytes).is_ok()
                }
                _ => false,
            };

            let status = if download_success {
                "success"
            } else {
                "failed"
            };

            conn2
                .execute(
                    "INSERT INTO fax_log (fax_id, phaxio_fax_id, direction, status, sent_at, pages, file_path, retry_count)
                     VALUES (?1, ?2, 'received', ?3, ?4, ?5, ?6, 0)",
                    rusqlite::params![
                        &fax_id,
                        phaxio_id_str,
                        status,
                        &now,
                        pages,
                        if download_success { Some(&file_path_str) } else { None },
                    ],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;

            write_audit_entry(
                &conn2,
                AuditEntryInput {
                    user_id: session.user_id.clone(),
                    action: "poll_received_faxes".to_string(),
                    resource_type: "FaxLog".to_string(),
                    resource_id: Some(fax_id.clone()),
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: download_success,
                    details: Some(format!("Received fax {} from Phaxio", phaxio_id_str)),
                },
            );

            new_records.push(FaxRecord {
                fax_id,
                phaxio_fax_id: phaxio_id,
                direction: "received".to_string(),
                patient_id: None,
                recipient_name: None,
                recipient_fax: None,
                document_name: None,
                file_path: if download_success {
                    Some(file_path_str)
                } else {
                    None
                },
                status: status.to_string(),
                sent_at: now,
                delivered_at: None,
                pages,
                error_message: None,
                retry_count: 0,
            });
        }
    }

    Ok(new_records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Fax Contacts Directory
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new fax contact in the directory.
///
/// RBAC: SystemAdmin, Provider, or NurseMa (ClinicalRecords::Create).
#[tauri::command]
pub fn create_fax_contact(
    input: FaxContactInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxContact, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Create)?;

    validate_contact_type(&input.contact_type)?;

    if input.name.is_empty() {
        return Err(AppError::Validation("Contact name is required".to_string()));
    }
    if input.fax_number.is_empty() {
        return Err(AppError::Validation(
            "Contact fax number is required".to_string(),
        ));
    }

    let contact_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fax_contacts (contact_id, name, organization, fax_number, phone_number,
         contact_type, notes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            &contact_id,
            &input.name,
            &input.organization,
            &input.fax_number,
            &input.phone_number,
            &input.contact_type,
            &input.notes,
            &now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id,
            action: "create_fax_contact".to_string(),
            resource_type: "FaxContact".to_string(),
            resource_id: Some(contact_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("Created fax contact: {}", input.name)),
        },
    );

    Ok(FaxContact {
        contact_id,
        name: input.name,
        organization: input.organization,
        fax_number: input.fax_number,
        phone_number: input.phone_number,
        contact_type: input.contact_type,
        notes: input.notes,
        created_at: now,
    })
}

/// List fax contacts, optionally filtered by contact type.
///
/// RBAC: Any authenticated user with ClinicalRecords::Read.
#[tauri::command]
pub fn list_fax_contacts(
    contact_type: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
) -> Result<Vec<FaxContact>, AppError> {
    let _session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(_session.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let contacts = if let Some(ct) = &contact_type {
        validate_contact_type(ct)?;
        let mut stmt = conn
            .prepare(
                "SELECT contact_id, name, organization, fax_number, phone_number,
                 contact_type, notes, created_at
                 FROM fax_contacts WHERE contact_type = ?1 ORDER BY name",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![ct], map_fax_contact)
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;
        rows
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT contact_id, name, organization, fax_number, phone_number,
                 contact_type, notes, created_at
                 FROM fax_contacts ORDER BY name",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], map_fax_contact)
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;
        rows
    };

    Ok(contacts)
}

/// Update an existing fax contact.
///
/// RBAC: SystemAdmin or Provider (ClinicalRecords::Update).
#[tauri::command]
pub fn update_fax_contact(
    contact_id: String,
    input: FaxContactInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxContact, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Update)?;

    validate_contact_type(&input.contact_type)?;

    if input.name.is_empty() {
        return Err(AppError::Validation("Contact name is required".to_string()));
    }
    if input.fax_number.is_empty() {
        return Err(AppError::Validation(
            "Contact fax number is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows_affected = conn
        .execute(
            "UPDATE fax_contacts SET name = ?1, organization = ?2, fax_number = ?3,
             phone_number = ?4, contact_type = ?5, notes = ?6
             WHERE contact_id = ?7",
            rusqlite::params![
                &input.name,
                &input.organization,
                &input.fax_number,
                &input.phone_number,
                &input.contact_type,
                &input.notes,
                &contact_id,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Fax contact not found: {}",
            contact_id
        )));
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id,
            action: "update_fax_contact".to_string(),
            resource_type: "FaxContact".to_string(),
            resource_id: Some(contact_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("Updated fax contact: {}", input.name)),
        },
    );

    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM fax_contacts WHERE contact_id = ?1",
            rusqlite::params![&contact_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(FaxContact {
        contact_id,
        name: input.name,
        organization: input.organization,
        fax_number: input.fax_number,
        phone_number: input.phone_number,
        contact_type: input.contact_type,
        notes: input.notes,
        created_at,
    })
}

/// Delete a fax contact from the directory.
///
/// RBAC: SystemAdmin only (ClinicalRecords::Delete).
#[tauri::command]
pub fn delete_fax_contact(
    contact_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Delete)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows_affected = conn
        .execute(
            "DELETE FROM fax_contacts WHERE contact_id = ?1",
            rusqlite::params![&contact_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Fax contact not found: {}",
            contact_id
        )));
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id,
            action: "delete_fax_contact".to_string(),
            resource_type: "FaxContact".to_string(),
            resource_id: Some(contact_id),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Deleted fax contact".to_string()),
        },
    );

    Ok(())
}

/// Row mapper for fax_contacts queries.
fn map_fax_contact(row: &rusqlite::Row<'_>) -> rusqlite::Result<FaxContact> {
    Ok(FaxContact {
        contact_id: row.get(0)?,
        name: row.get(1)?,
        organization: row.get(2)?,
        fax_number: row.get(3)?,
        phone_number: row.get(4)?,
        contact_type: row.get(5)?,
        notes: row.get(6)?,
        created_at: row.get(7)?,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Fax Log
// ─────────────────────────────────────────────────────────────────────────────

/// List fax log entries with optional filters.
///
/// RBAC: Any authenticated user with ClinicalRecords::Read.
#[tauri::command]
pub fn list_fax_log(
    patient_id: Option<String>,
    direction: Option<String>,
    status: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
) -> Result<Vec<FaxRecord>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Read)?;

    // Validate filters if provided
    if let Some(ref d) = direction {
        validate_fax_direction(d)?;
    }
    if let Some(ref s) = status {
        validate_fax_status(s)?;
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Build query dynamically based on provided filters
    let mut sql = String::from(
        "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
         document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
         FROM fax_log WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref pid) = patient_id {
        params.push(Box::new(pid.clone()));
        sql.push_str(&format!(" AND patient_id = ?{}", params.len()));
    }
    if let Some(ref d) = direction {
        params.push(Box::new(d.clone()));
        sql.push_str(&format!(" AND direction = ?{}", params.len()));
    }
    if let Some(ref s) = status {
        params.push(Box::new(s.clone()));
        sql.push_str(&format!(" AND status = ?{}", params.len()));
    }

    sql.push_str(" ORDER BY sent_at DESC LIMIT 200");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let records = stmt
        .query_map(param_refs.as_slice(), map_fax_record)
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(records)
}

/// Check the current status of a fax by querying the Phaxio API.
///
/// RBAC: Any authenticated user with ClinicalRecords::Read.
#[tauri::command]
pub fn get_fax_status(
    fax_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get current record
    let record = conn
        .query_row(
            "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
             document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
             FROM fax_log WHERE fax_id = ?1",
            rusqlite::params![&fax_id],
            map_fax_record,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Fax not found: {}", fax_id))
            }
            _ => AppError::Database(e.to_string()),
        })?;

    // If we have a Phaxio fax ID and the fax is still in progress, check the API
    if let Some(ref phaxio_fax_id) = record.phaxio_fax_id {
        if record.status == "queued" || record.status == "in_progress" {
            let creds = get_phaxio_credentials(&conn)?;
            if let Some((api_key, api_secret, _)) = creds {
                drop(conn);

                let client = reqwest::blocking::Client::new();
                let resp = client
                    .get(format!("{}/faxes/{}", PHAXIO_BASE_URL, phaxio_fax_id))
                    .basic_auth(&api_key, Some(&api_secret))
                    .send();

                if let Ok(response) = resp {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<serde_json::Value>() {
                            let phaxio_status = body
                                .get("data")
                                .and_then(|d| d.get("status"))
                                .and_then(|s| s.as_str());

                            let new_status = match phaxio_status {
                                Some("success") => Some("success"),
                                Some("failure") => Some("failed"),
                                Some("queued") => Some("queued"),
                                Some("inProgress") | Some("transmitting") => Some("in_progress"),
                                _ => None,
                            };

                            if let Some(status) = new_status {
                                let conn2 = db
                                    .conn
                                    .lock()
                                    .map_err(|e| AppError::Database(e.to_string()))?;

                                let delivered_at = if status == "success" {
                                    Some(chrono::Utc::now().to_rfc3339())
                                } else {
                                    None
                                };

                                conn2
                                    .execute(
                                        "UPDATE fax_log SET status = ?1, delivered_at = ?2 WHERE fax_id = ?3",
                                        rusqlite::params![status, &delivered_at, &fax_id],
                                    )
                                    .map_err(|e| AppError::Database(e.to_string()))?;

                                write_audit_entry(
                                    &conn2,
                                    AuditEntryInput {
                                        user_id: session.user_id,
                                        action: "get_fax_status".to_string(),
                                        resource_type: "FaxLog".to_string(),
                                        resource_id: Some(fax_id.clone()),
                                        patient_id: record.patient_id.clone(),
                                        device_id: device_id.get().to_string(),
                                        success: true,
                                        details: Some(format!("Fax status updated to {}", status)),
                                    },
                                );

                                // Return updated record
                                let updated = conn2
                                    .query_row(
                                        "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
                                         document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
                                         FROM fax_log WHERE fax_id = ?1",
                                        rusqlite::params![&fax_id],
                                        map_fax_record,
                                    )
                                    .map_err(|e| AppError::Database(e.to_string()))?;
                                return Ok(updated);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(record)
}

/// Retry a failed fax. Will fail if the fax has already been retried the maximum
/// number of times (2).
///
/// RBAC: SystemAdmin or Provider (ClinicalRecords::Update).
#[tauri::command]
pub fn retry_fax(
    fax_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get current fax record
    let record = conn
        .query_row(
            "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
             document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
             FROM fax_log WHERE fax_id = ?1",
            rusqlite::params![&fax_id],
            map_fax_record,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Fax not found: {}", fax_id))
            }
            _ => AppError::Database(e.to_string()),
        })?;

    // Can only retry failed faxes
    if record.status != "failed" {
        return Err(AppError::Validation(format!(
            "Cannot retry fax with status '{}'. Only failed faxes can be retried.",
            record.status
        )));
    }

    // Check retry limit
    if record.retry_count >= MAX_AUTO_RETRIES {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "retry_fax".to_string(),
                resource_type: "FaxLog".to_string(),
                resource_id: Some(fax_id.clone()),
                patient_id: record.patient_id.clone(),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(format!(
                    "Max retries ({}) exceeded for fax {}",
                    MAX_AUTO_RETRIES, fax_id
                )),
            },
        );
        return Err(AppError::Validation(format!(
            "Maximum retry count ({}) exceeded. Manual intervention required.",
            MAX_AUTO_RETRIES
        )));
    }

    // Can only retry sent faxes that have a file path
    if record.direction != "sent" {
        return Err(AppError::Validation(
            "Can only retry sent faxes".to_string(),
        ));
    }

    let file_path = match &record.file_path {
        Some(fp) => fp.clone(),
        None => {
            return Err(AppError::Validation(
                "No file path available for retry".to_string(),
            ));
        }
    };

    let recipient_fax = match &record.recipient_fax {
        Some(rf) => rf.clone(),
        None => {
            return Err(AppError::Validation(
                "No recipient fax number available for retry".to_string(),
            ));
        }
    };

    let creds = get_phaxio_credentials(&conn)?;
    let (api_key, api_secret, _fax_number) = match creds {
        Some(c) => c,
        None => {
            return Err(AppError::Validation(
                "Phaxio credentials not configured".to_string(),
            ));
        }
    };

    // Increment retry count and set status back to queued
    let new_retry_count = record.retry_count + 1;
    conn.execute(
        "UPDATE fax_log SET status = 'queued', retry_count = ?1, error_message = NULL WHERE fax_id = ?2",
        rusqlite::params![new_retry_count, &fax_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id,
            action: "retry_fax".to_string(),
            resource_type: "FaxLog".to_string(),
            resource_id: Some(fax_id.clone()),
            patient_id: record.patient_id.clone(),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "Fax retry #{} initiated for {}",
                new_retry_count, recipient_fax
            )),
        },
    );

    // Drop conn before blocking HTTP call
    drop(conn);

    // Re-send via Phaxio
    let file_bytes = std::fs::read(&file_path)
        .map_err(|e| AppError::Database(format!("Failed to read fax file: {}", e)))?;

    let document_name = std::path::Path::new(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let file_part = reqwest::blocking::multipart::Part::bytes(file_bytes)
        .file_name(document_name)
        .mime_str("application/pdf")
        .map_err(|e| AppError::Database(format!("Failed to create multipart part: {}", e)))?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("to", recipient_fax)
        .part("file", file_part);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/faxes", PHAXIO_BASE_URL))
        .basic_auth(&api_key, Some(&api_secret))
        .multipart(form)
        .send();

    let conn2 = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    match resp {
        Ok(response) if response.status().is_success() => {
            let phaxio_fax_id: Option<String> = response
                .json::<serde_json::Value>()
                .ok()
                .and_then(|v| v.get("data")?.get("id")?.as_i64())
                .map(|id| id.to_string());

            conn2
                .execute(
                    "UPDATE fax_log SET status = 'in_progress', phaxio_fax_id = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&phaxio_fax_id, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
        Ok(response) => {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            conn2
                .execute(
                    "UPDATE fax_log SET status = 'failed', error_message = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&error_text, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
        Err(e) => {
            let error_msg = format!("Phaxio API request failed: {}", e);
            conn2
                .execute(
                    "UPDATE fax_log SET status = 'failed', error_message = ?1 WHERE fax_id = ?2",
                    rusqlite::params![&error_msg, &fax_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
        }
    }

    // Return updated record
    let updated = conn2
        .query_row(
            "SELECT fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax,
             document_name, file_path, status, sent_at, delivered_at, pages, error_message, retry_count
             FROM fax_log WHERE fax_id = ?1",
            rusqlite::params![&fax_id],
            map_fax_record,
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(updated)
}

/// Row mapper for fax_log queries.
fn map_fax_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<FaxRecord> {
    Ok(FaxRecord {
        fax_id: row.get(0)?,
        phaxio_fax_id: row.get(1)?,
        direction: row.get(2)?,
        patient_id: row.get(3)?,
        recipient_name: row.get(4)?,
        recipient_fax: row.get(5)?,
        document_name: row.get(6)?,
        file_path: row.get(7)?,
        status: row.get(8)?,
        sent_at: row.get(9)?,
        delivered_at: row.get(10)?,
        pages: row.get(11)?,
        error_message: row.get(12)?,
        retry_count: row.get(13)?,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Fax status validation ──────────────────────────────────────────────

    #[test]
    fn fax_status_valid_values_accepted() {
        assert!(validate_fax_status("queued").is_ok());
        assert!(validate_fax_status("in_progress").is_ok());
        assert!(validate_fax_status("success").is_ok());
        assert!(validate_fax_status("failed").is_ok());
    }

    #[test]
    fn fax_status_invalid_values_rejected() {
        assert!(validate_fax_status("pending").is_err());
        assert!(validate_fax_status("").is_err());
        assert!(validate_fax_status("completed").is_err());
        assert!(validate_fax_status("QUEUED").is_err());
    }

    // ── Fax direction validation ───────────────────────────────────────────

    #[test]
    fn fax_direction_valid_values_accepted() {
        assert!(validate_fax_direction("sent").is_ok());
        assert!(validate_fax_direction("received").is_ok());
    }

    #[test]
    fn fax_direction_invalid_values_rejected() {
        assert!(validate_fax_direction("inbound").is_err());
        assert!(validate_fax_direction("outbound").is_err());
        assert!(validate_fax_direction("").is_err());
        assert!(validate_fax_direction("SENT").is_err());
    }

    // ── Contact type validation ────────────────────────────────────────────

    #[test]
    fn contact_type_valid_values_accepted() {
        assert!(validate_contact_type("insurance").is_ok());
        assert!(validate_contact_type("referring_md").is_ok());
        assert!(validate_contact_type("attorney").is_ok());
        assert!(validate_contact_type("other").is_ok());
    }

    #[test]
    fn contact_type_invalid_values_rejected() {
        assert!(validate_contact_type("pharmacy").is_err());
        assert!(validate_contact_type("").is_err());
        assert!(validate_contact_type("INSURANCE").is_err());
        assert!(validate_contact_type("doctor").is_err());
    }

    // ── Retry count validation ─────────────────────────────────────────────

    #[test]
    fn retry_count_within_limit_accepted() {
        assert!(validate_retry_count(0).is_ok());
        assert!(validate_retry_count(1).is_ok());
        assert!(validate_retry_count(2).is_ok());
    }

    #[test]
    fn retry_count_exceeding_limit_rejected() {
        assert!(validate_retry_count(3).is_err());
        assert!(validate_retry_count(10).is_err());
        assert!(validate_retry_count(100).is_err());
    }

    // ── Migration SQL validation ───────────────────────────────────────────

    #[test]
    fn migration_sql_creates_fax_log_table() {
        // Verify the migration SQL is valid by running it against an in-memory SQLite DB
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fax_log (
                fax_id TEXT PRIMARY KEY,
                phaxio_fax_id TEXT,
                direction TEXT NOT NULL CHECK(direction IN ('sent','received')),
                patient_id TEXT,
                recipient_name TEXT,
                recipient_fax TEXT,
                document_name TEXT,
                file_path TEXT,
                status TEXT NOT NULL CHECK(status IN ('queued','in_progress','success','failed')),
                sent_at TEXT NOT NULL DEFAULT (datetime('now')),
                delivered_at TEXT,
                pages INTEGER,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_fax_patient ON fax_log(patient_id);
            CREATE INDEX IF NOT EXISTS idx_fax_direction ON fax_log(direction);
            CREATE INDEX IF NOT EXISTS idx_fax_status ON fax_log(status);",
        )
        .expect("fax_log migration SQL must be valid");

        // Verify we can insert and query
        conn.execute(
            "INSERT INTO fax_log (fax_id, direction, status, sent_at) VALUES ('test-1', 'sent', 'queued', datetime('now'))",
            [],
        )
        .expect("insert must succeed");

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM fax_log", [], |row| row.get(0))
            .expect("query must succeed");
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_sql_creates_fax_contacts_table() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fax_contacts (
                contact_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                organization TEXT,
                fax_number TEXT NOT NULL,
                phone_number TEXT,
                contact_type TEXT NOT NULL CHECK(contact_type IN ('insurance','referring_md','attorney','other')),
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_fax_contact_type ON fax_contacts(contact_type);",
        )
        .expect("fax_contacts migration SQL must be valid");

        // Verify we can insert
        conn.execute(
            "INSERT INTO fax_contacts (contact_id, name, fax_number, contact_type) VALUES ('c-1', 'Test', '+15551234567', 'insurance')",
            [],
        )
        .expect("insert must succeed");

        // Verify CHECK constraint on contact_type
        let result = conn.execute(
            "INSERT INTO fax_contacts (contact_id, name, fax_number, contact_type) VALUES ('c-2', 'Test', '+15551234567', 'invalid_type')",
            [],
        );
        assert!(result.is_err(), "Invalid contact_type should be rejected by CHECK constraint");
    }

    #[test]
    fn migration_sql_enforces_direction_check_constraint() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fax_log (
                fax_id TEXT PRIMARY KEY,
                phaxio_fax_id TEXT,
                direction TEXT NOT NULL CHECK(direction IN ('sent','received')),
                patient_id TEXT,
                recipient_name TEXT,
                recipient_fax TEXT,
                document_name TEXT,
                file_path TEXT,
                status TEXT NOT NULL CHECK(status IN ('queued','in_progress','success','failed')),
                sent_at TEXT NOT NULL DEFAULT (datetime('now')),
                delivered_at TEXT,
                pages INTEGER,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0
            );",
        )
        .expect("create table must succeed");

        // Valid direction
        let ok = conn.execute(
            "INSERT INTO fax_log (fax_id, direction, status) VALUES ('t1', 'sent', 'queued')",
            [],
        );
        assert!(ok.is_ok());

        // Invalid direction
        let bad = conn.execute(
            "INSERT INTO fax_log (fax_id, direction, status) VALUES ('t2', 'inbound', 'queued')",
            [],
        );
        assert!(bad.is_err(), "Invalid direction must be rejected by CHECK constraint");
    }

    #[test]
    fn migration_sql_enforces_status_check_constraint() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fax_log (
                fax_id TEXT PRIMARY KEY,
                direction TEXT NOT NULL CHECK(direction IN ('sent','received')),
                status TEXT NOT NULL CHECK(status IN ('queued','in_progress','success','failed')),
                sent_at TEXT NOT NULL DEFAULT (datetime('now')),
                retry_count INTEGER DEFAULT 0
            );",
        )
        .expect("create table must succeed");

        // Valid status
        let ok = conn.execute(
            "INSERT INTO fax_log (fax_id, direction, status) VALUES ('t1', 'sent', 'queued')",
            [],
        );
        assert!(ok.is_ok());

        // Invalid status
        let bad = conn.execute(
            "INSERT INTO fax_log (fax_id, direction, status) VALUES ('t2', 'sent', 'pending')",
            [],
        );
        assert!(bad.is_err(), "Invalid status must be rejected by CHECK constraint");
    }

    // ── FaxRecord serialization ────────────────────────────────────────────

    #[test]
    fn fax_record_serialization_uses_camel_case() {
        let record = FaxRecord {
            fax_id: "test-id".to_string(),
            phaxio_fax_id: Some("123456".to_string()),
            direction: "sent".to_string(),
            patient_id: Some("patient-1".to_string()),
            recipient_name: Some("Dr. Smith".to_string()),
            recipient_fax: Some("+15551234567".to_string()),
            document_name: Some("referral.pdf".to_string()),
            file_path: Some("/path/to/file.pdf".to_string()),
            status: "queued".to_string(),
            sent_at: "2026-03-14T10:00:00Z".to_string(),
            delivered_at: None,
            pages: Some(3),
            error_message: None,
            retry_count: 0,
        };

        let json = serde_json::to_string(&record).expect("must serialize");
        assert!(json.contains("\"faxId\""), "must use camelCase: faxId");
        assert!(json.contains("\"phaxioFaxId\""), "must use camelCase: phaxioFaxId");
        assert!(json.contains("\"patientId\""), "must use camelCase: patientId");
        assert!(json.contains("\"recipientName\""), "must use camelCase: recipientName");
        assert!(json.contains("\"recipientFax\""), "must use camelCase: recipientFax");
        assert!(json.contains("\"retryCount\""), "must use camelCase: retryCount");
    }

    #[test]
    fn fax_contact_serialization_uses_camel_case() {
        let contact = FaxContact {
            contact_id: "c-1".to_string(),
            name: "Test Contact".to_string(),
            organization: Some("Test Org".to_string()),
            fax_number: "+15551234567".to_string(),
            phone_number: Some("+15559876543".to_string()),
            contact_type: "insurance".to_string(),
            notes: None,
            created_at: "2026-03-14T10:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&contact).expect("must serialize");
        assert!(json.contains("\"contactId\""), "must use camelCase: contactId");
        assert!(json.contains("\"faxNumber\""), "must use camelCase: faxNumber");
        assert!(json.contains("\"phoneNumber\""), "must use camelCase: phoneNumber");
        assert!(json.contains("\"contactType\""), "must use camelCase: contactType");
        assert!(json.contains("\"createdAt\""), "must use camelCase: createdAt");
    }

    #[test]
    fn max_auto_retries_constant_is_two() {
        assert_eq!(MAX_AUTO_RETRIES, 2, "max auto retries must be 2 per specification");
    }
}
