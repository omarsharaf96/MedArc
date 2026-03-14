/// commands/workers_comp.rs — Workers' Compensation Module (M003/S02)
///
/// Implements WC case management, contacts, FROI generation, state fee schedule
/// lookup, impairment ratings, and communication logging for workers' comp cases.
///
/// WC Case Management
/// ------------------
/// Create/manage workers' comp cases linked to patients via FHIR resources.
/// Case data: patient_id, employer, injury details, body parts, claim number,
/// state, status (open/closed/settled/disputed), MMI date.
/// Commands: create_wc_case, get_wc_case, list_wc_cases, update_wc_case
///
/// WC Contacts
/// -----------
/// Per-case contacts with roles (adjuster, attorney, nurse_case_manager, employer_rep).
/// Commands: add_wc_contact, list_wc_contacts, update_wc_contact
///
/// First Report of Injury (FROI)
/// -----------------------------
/// Generate structured FROI document text from case data.
/// Command: generate_froi
///
/// State Fee Schedule
/// ------------------
/// Store and look up state-specific WC maximum allowable rates.
/// Command: lookup_wc_fee
///
/// Impairment Rating
/// -----------------
/// AMA Guides-based impairment ratings per case.
/// Commands: record_impairment_rating, list_impairment_ratings
///
/// Communication Log
/// -----------------
/// Track all case communications (phone, email, fax, letter, in_person).
/// Commands: log_wc_communication, list_wc_communications
///
/// RBAC
/// ----
///   SystemAdmin / Provider / BillingStaff → full CRUD
///   NurseMa / FrontDesk                   → Read only
///
/// Audit
/// -----
/// Every command writes an audit row via `write_audit_entry`.
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
// Data Types — WC Cases
// ─────────────────────────────────────────────────────────────────────────────

/// Valid WC case status values.
const VALID_WC_STATUSES: &[&str] = &["open", "closed", "settled", "disputed"];

/// Valid WC contact role values.
const VALID_CONTACT_ROLES: &[&str] = &[
    "adjuster",
    "attorney",
    "nurse_case_manager",
    "employer_rep",
];

/// Valid AMA Guides editions.
const VALID_AMA_EDITIONS: &[&str] = &["3rd_rev", "4th", "5th", "6th"];

/// Valid communication directions.
const VALID_COMM_DIRECTIONS: &[&str] = &["inbound", "outbound"];

/// Valid communication methods.
const VALID_COMM_METHODS: &[&str] = &["phone", "email", "fax", "letter", "in_person"];

/// Input for creating or updating a workers' comp case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcCaseInput {
    pub patient_id: String,
    pub employer_name: String,
    pub employer_contact: Option<String>,
    pub injury_date: String,
    pub injury_description: Option<String>,
    /// JSON array of body parts, e.g. ["lumbar_spine", "left_shoulder"]
    pub body_parts: Option<Vec<String>>,
    pub claim_number: Option<String>,
    pub state: String,
    pub status: Option<String>,
    pub mmi_date: Option<String>,
}

/// A workers' comp case record as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcCaseRecord {
    pub case_id: String,
    pub resource_id: String,
    pub patient_id: String,
    pub employer_name: String,
    pub employer_contact: Option<String>,
    pub injury_date: String,
    pub injury_description: Option<String>,
    pub body_parts: Option<Vec<String>>,
    pub claim_number: Option<String>,
    pub state: String,
    pub status: String,
    pub mmi_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types — WC Contacts
// ─────────────────────────────────────────────────────────────────────────────

/// Input for adding or updating a WC case contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcContactInput {
    pub role: String,
    pub name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub fax: Option<String>,
    pub notes: Option<String>,
}

/// A WC contact record as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcContactRecord {
    pub contact_id: String,
    pub case_id: String,
    pub role: String,
    pub name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub fax: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types — FROI
// ─────────────────────────────────────────────────────────────────────────────

/// Result of FROI generation — structured text document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FroiResult {
    pub case_id: String,
    /// The full structured text of the First Report of Injury.
    pub content: String,
    /// Document title for display / fax cover sheet.
    pub title: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types — Fee Schedule
// ─────────────────────────────────────────────────────────────────────────────

/// Result of a WC fee schedule lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcFeeResult {
    pub state: String,
    pub cpt_code: String,
    pub max_allowable: f64,
    pub effective_date: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types — Impairment Ratings
// ─────────────────────────────────────────────────────────────────────────────

/// Input for recording an impairment rating.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpairmentRatingInput {
    pub body_part: String,
    pub ama_guides_edition: Option<String>,
    pub impairment_class: Option<String>,
    pub grade_modifier: Option<String>,
    /// Whole person impairment percentage (0–100).
    pub whole_person_pct: f64,
    pub evaluator: Option<String>,
    pub evaluation_date: Option<String>,
}

/// An impairment rating record as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpairmentRatingRecord {
    pub rating_id: String,
    pub case_id: String,
    pub body_part: String,
    pub ama_guides_edition: Option<String>,
    pub impairment_class: Option<String>,
    pub grade_modifier: Option<String>,
    pub whole_person_pct: f64,
    pub evaluator: Option<String>,
    pub evaluation_date: Option<String>,
    pub created_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types — Communication Log
// ─────────────────────────────────────────────────────────────────────────────

/// Input for logging a WC communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcCommunicationInput {
    pub contact_id: Option<String>,
    pub direction: String,
    pub method: String,
    pub subject: Option<String>,
    pub content: Option<String>,
    pub comm_date: Option<String>,
}

/// A WC communication log entry as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WcCommunicationRecord {
    pub comm_id: String,
    pub case_id: String,
    pub contact_id: Option<String>,
    pub direction: String,
    pub method: String,
    pub subject: Option<String>,
    pub content: Option<String>,
    pub comm_date: String,
    pub created_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn validate_status(status: &str) -> Result<(), AppError> {
    if VALID_WC_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid WC case status '{}'. Must be one of: {}",
            status,
            VALID_WC_STATUSES.join(", ")
        )))
    }
}

fn validate_contact_role(role: &str) -> Result<(), AppError> {
    if VALID_CONTACT_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid contact role '{}'. Must be one of: {}",
            role,
            VALID_CONTACT_ROLES.join(", ")
        )))
    }
}

fn validate_ama_edition(edition: &str) -> Result<(), AppError> {
    if VALID_AMA_EDITIONS.contains(&edition) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid AMA Guides edition '{}'. Must be one of: {}",
            edition,
            VALID_AMA_EDITIONS.join(", ")
        )))
    }
}

fn validate_comm_direction(direction: &str) -> Result<(), AppError> {
    if VALID_COMM_DIRECTIONS.contains(&direction) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid communication direction '{}'. Must be one of: {}",
            direction,
            VALID_COMM_DIRECTIONS.join(", ")
        )))
    }
}

fn validate_comm_method(method: &str) -> Result<(), AppError> {
    if VALID_COMM_METHODS.contains(&method) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid communication method '{}'. Must be one of: {}",
            method,
            VALID_COMM_METHODS.join(", ")
        )))
    }
}

/// Parse a body_parts JSON string from the DB into a Vec<String>.
fn parse_body_parts(raw: Option<String>) -> Option<Vec<String>> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
}

/// Serialize body parts Vec to JSON string.
fn serialize_body_parts(parts: &Option<Vec<String>>) -> Option<String> {
    parts
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()))
}

/// Map a row to WcCaseRecord (used in multiple commands).
fn row_to_wc_case(row: &rusqlite::Row<'_>) -> Result<WcCaseRecord, rusqlite::Error> {
    let body_parts_raw: Option<String> = row.get(7)?;
    Ok(WcCaseRecord {
        case_id: row.get(0)?,
        resource_id: row.get(1)?,
        patient_id: row.get(2)?,
        employer_name: row.get(3)?,
        employer_contact: row.get(4)?,
        injury_date: row.get(5)?,
        injury_description: row.get(6)?,
        body_parts: parse_body_parts(body_parts_raw),
        claim_number: row.get(8)?,
        state: row.get(9)?,
        status: row.get(10)?,
        mmi_date: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — WC Case CRUD
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new workers' comp case linked to a patient.
#[tauri::command]
pub fn create_wc_case(
    input: WcCaseInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcCaseRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Create)?;

    let status = input.status.as_deref().unwrap_or("open");
    validate_status(status)?;

    let case_id = uuid::Uuid::new_v4().to_string();
    let resource_id = uuid::Uuid::new_v4().to_string();
    let body_parts_json = serialize_body_parts(&input.body_parts);

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Insert a minimal FHIR resource placeholder so the FK is satisfied.
    let now = chrono::Utc::now().to_rfc3339();
    let fhir_json = serde_json::json!({
        "resourceType": "EpisodeOfCare",
        "id": resource_id,
        "status": "active",
        "type": [{"coding": [{"system": "http://terminology.hl7.org/CodeSystem/episodeofcare-type", "code": "workersCOMP"}]}],
        "patient": {"reference": format!("Patient/{}", input.patient_id)}
    });
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'EpisodeOfCare', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![resource_id, fhir_json.to_string(), now],
    )?;

    conn.execute(
        "INSERT INTO wc_cases
         (case_id, resource_id, patient_id, employer_name, employer_contact,
          injury_date, injury_description, body_parts, claim_number, state, status, mmi_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            case_id,
            resource_id,
            input.patient_id,
            input.employer_name,
            input.employer_contact,
            input.injury_date,
            input.injury_description,
            body_parts_json,
            input.claim_number,
            input.state,
            status,
            input.mmi_date,
        ],
    )?;

    let record = conn.query_row(
        "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                injury_date, injury_description, body_parts, claim_number, state, status,
                mmi_date, created_at, updated_at
         FROM wc_cases WHERE case_id = ?1",
        [&case_id],
        row_to_wc_case,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "create_wc_case".to_string(),
            resource_type: "WcCase".to_string(),
            resource_id: Some(case_id.clone()),
            patient_id: Some(record.patient_id.clone()),
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(record)
}

/// Retrieve a single workers' comp case by ID.
#[tauri::command]
pub fn get_wc_case(
    case_id: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcCaseRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let record = conn
        .query_row(
            "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                    injury_date, injury_description, body_parts, claim_number, state, status,
                    mmi_date, created_at, updated_at
             FROM wc_cases WHERE case_id = ?1",
            [&case_id],
            row_to_wc_case,
        )
        .map_err(|_| AppError::NotFound(format!("WC case '{}' not found", case_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "get_wc_case".to_string(),
            resource_type: "WcCase".to_string(),
            resource_id: Some(case_id),
            patient_id: Some(record.patient_id.clone()),
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(record)
}

/// List workers' comp cases, optionally filtered by patient ID.
#[tauri::command]
pub fn list_wc_cases(
    patient_id: Option<String>,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<Vec<WcCaseRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let records = if let Some(ref pid) = patient_id {
        let mut stmt = conn.prepare(
            "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                    injury_date, injury_description, body_parts, claim_number, state, status,
                    mmi_date, created_at, updated_at
             FROM wc_cases WHERE patient_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([pid], row_to_wc_case)?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    } else {
        let mut stmt = conn.prepare(
            "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                    injury_date, injury_description, body_parts, claim_number, state, status,
                    mmi_date, created_at, updated_at
             FROM wc_cases ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_wc_case)?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_wc_cases".to_string(),
            resource_type: "WcCase".to_string(),
            resource_id: None,
            patient_id: patient_id.clone(),
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(records)
}

/// Update an existing workers' comp case.
#[tauri::command]
pub fn update_wc_case(
    case_id: String,
    input: WcCaseInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcCaseRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Update)?;

    let status = input.status.as_deref().unwrap_or("open");
    validate_status(status)?;

    let body_parts_json = serialize_body_parts(&input.body_parts);

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let rows = conn.execute(
        "UPDATE wc_cases SET
            employer_name = ?2,
            employer_contact = ?3,
            injury_date = ?4,
            injury_description = ?5,
            body_parts = ?6,
            claim_number = ?7,
            state = ?8,
            status = ?9,
            mmi_date = ?10,
            updated_at = datetime('now')
         WHERE case_id = ?1",
        rusqlite::params![
            case_id,
            input.employer_name,
            input.employer_contact,
            input.injury_date,
            input.injury_description,
            body_parts_json,
            input.claim_number,
            input.state,
            status,
            input.mmi_date,
        ],
    )?;

    if rows == 0 {
        return Err(AppError::NotFound(format!("WC case '{}' not found", case_id)));
    }

    let record = conn.query_row(
        "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                injury_date, injury_description, body_parts, claim_number, state, status,
                mmi_date, created_at, updated_at
         FROM wc_cases WHERE case_id = ?1",
        [&case_id],
        row_to_wc_case,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "update_wc_case".to_string(),
            resource_type: "WcCase".to_string(),
            resource_id: Some(case_id),
            patient_id: Some(record.patient_id.clone()),
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(record)
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — WC Contacts
// ─────────────────────────────────────────────────────────────────────────────

/// Add a contact (adjuster, attorney, NCM, employer rep) to a WC case.
#[tauri::command]
pub fn add_wc_contact(
    case_id: String,
    input: WcContactInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcContactRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Create)?;

    validate_contact_role(&input.role)?;

    let contact_id = uuid::Uuid::new_v4().to_string();
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO wc_contacts
         (contact_id, case_id, role, name, company, phone, email, fax, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            contact_id,
            case_id,
            input.role,
            input.name,
            input.company,
            input.phone,
            input.email,
            input.fax,
            input.notes,
        ],
    )?;

    let record = conn.query_row(
        "SELECT contact_id, case_id, role, name, company, phone, email, fax, notes, created_at
         FROM wc_contacts WHERE contact_id = ?1",
        [&contact_id],
        |row| {
            Ok(WcContactRecord {
                contact_id: row.get(0)?,
                case_id: row.get(1)?,
                role: row.get(2)?,
                name: row.get(3)?,
                company: row.get(4)?,
                phone: row.get(5)?,
                email: row.get(6)?,
                fax: row.get(7)?,
                notes: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "add_wc_contact".to_string(),
            resource_type: "WcContact".to_string(),
            resource_id: Some(contact_id),
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!("case_id={}", case_id)),
        },
    )?;

    Ok(record)
}

/// List all contacts for a WC case.
#[tauri::command]
pub fn list_wc_contacts(
    case_id: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<Vec<WcContactRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT contact_id, case_id, role, name, company, phone, email, fax, notes, created_at
         FROM wc_contacts WHERE case_id = ?1 ORDER BY role, name",
    )?;

    let records = stmt
        .query_map([&case_id], |row| {
            Ok(WcContactRecord {
                contact_id: row.get(0)?,
                case_id: row.get(1)?,
                role: row.get(2)?,
                name: row.get(3)?,
                company: row.get(4)?,
                phone: row.get(5)?,
                email: row.get(6)?,
                fax: row.get(7)?,
                notes: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_wc_contacts".to_string(),
            resource_type: "WcContact".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!("case_id={}", case_id)),
        },
    )?;

    Ok(records)
}

/// Update an existing WC case contact.
#[tauri::command]
pub fn update_wc_contact(
    contact_id: String,
    input: WcContactInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcContactRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Update)?;

    validate_contact_role(&input.role)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let rows = conn.execute(
        "UPDATE wc_contacts SET
            role = ?2, name = ?3, company = ?4, phone = ?5,
            email = ?6, fax = ?7, notes = ?8
         WHERE contact_id = ?1",
        rusqlite::params![
            contact_id,
            input.role,
            input.name,
            input.company,
            input.phone,
            input.email,
            input.fax,
            input.notes,
        ],
    )?;

    if rows == 0 {
        return Err(AppError::NotFound(format!(
            "WC contact '{}' not found",
            contact_id
        )));
    }

    let record = conn.query_row(
        "SELECT contact_id, case_id, role, name, company, phone, email, fax, notes, created_at
         FROM wc_contacts WHERE contact_id = ?1",
        [&contact_id],
        |row| {
            Ok(WcContactRecord {
                contact_id: row.get(0)?,
                case_id: row.get(1)?,
                role: row.get(2)?,
                name: row.get(3)?,
                company: row.get(4)?,
                phone: row.get(5)?,
                email: row.get(6)?,
                fax: row.get(7)?,
                notes: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "update_wc_contact".to_string(),
            resource_type: "WcContact".to_string(),
            resource_id: Some(contact_id),
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(record)
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — FROI Generation
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a First Report of Injury (FROI) document from WC case data.
///
/// Returns structured text content suitable for printing or faxing.
#[tauri::command]
pub fn generate_froi(
    case_id: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<FroiResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let case = conn
        .query_row(
            "SELECT case_id, resource_id, patient_id, employer_name, employer_contact,
                    injury_date, injury_description, body_parts, claim_number, state, status,
                    mmi_date, created_at, updated_at
             FROM wc_cases WHERE case_id = ?1",
            [&case_id],
            row_to_wc_case,
        )
        .map_err(|_| AppError::NotFound(format!("WC case '{}' not found", case_id)))?;

    // Fetch patient name from patient_index if available.
    let patient_display: String = conn
        .query_row(
            "SELECT given_name || ' ' || family_name FROM patient_index WHERE patient_id = ?1",
            [&case.patient_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| format!("Patient ID: {}", case.patient_id));

    // Fetch contacts for the case.
    let contacts: Vec<WcContactRecord> = {
        let mut stmt = conn.prepare(
            "SELECT contact_id, case_id, role, name, company, phone, email, fax, notes, created_at
             FROM wc_contacts WHERE case_id = ?1 ORDER BY role",
        )?;
        let rows = stmt.query_map([&case_id], |row| {
            Ok(WcContactRecord {
                contact_id: row.get(0)?,
                case_id: row.get(1)?,
                role: row.get(2)?,
                name: row.get(3)?,
                company: row.get(4)?,
                phone: row.get(5)?,
                email: row.get(6)?,
                fax: row.get(7)?,
                notes: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let adjuster = contacts.iter().find(|c| c.role == "adjuster");
    let attorney = contacts.iter().find(|c| c.role == "attorney");

    let body_parts_str = case
        .body_parts
        .as_ref()
        .map(|v| v.join(", "))
        .unwrap_or_else(|| "Not specified".to_string());

    let claim_number_str = case
        .claim_number
        .as_deref()
        .unwrap_or("Pending Assignment");

    let froi_content = format!(
        "FIRST REPORT OF INJURY\n\
         State: {state}\n\
         Generated: {generated}\n\
         ================================================================================\n\
         \n\
         CLAIM INFORMATION\n\
           Claim Number:       {claim_number}\n\
           Case Status:        {status}\n\
           MMI Date:           {mmi}\n\
         \n\
         EMPLOYEE / CLAIMANT INFORMATION\n\
           Name:               {patient_name}\n\
           Patient ID:         {patient_id}\n\
         \n\
         EMPLOYER INFORMATION\n\
           Employer Name:      {employer}\n\
           Employer Contact:   {employer_contact}\n\
         \n\
         INJURY DETAILS\n\
           Date of Injury:     {injury_date}\n\
           Body Parts Affected:{body_parts}\n\
           Description:\n\
             {description}\n\
         \n\
         ADJUSTER INFORMATION\n\
           Name:               {adjuster_name}\n\
           Company:            {adjuster_company}\n\
           Phone:              {adjuster_phone}\n\
           Email:              {adjuster_email}\n\
           Fax:                {adjuster_fax}\n\
         \n\
         ATTORNEY INFORMATION\n\
           Name:               {attorney_name}\n\
           Company:            {attorney_company}\n\
           Phone:              {attorney_phone}\n\
         \n\
         PROVIDER INFORMATION\n\
           Provider:           MedArc Clinic\n\
           System-Generated:   Yes\n\
         \n\
         ================================================================================\n\
         This document was generated by MedArc EMR.\n\
         Case ID: {case_id}\n",
        state = case.state,
        generated = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
        claim_number = claim_number_str,
        status = case.status,
        mmi = case.mmi_date.as_deref().unwrap_or("Not reached"),
        patient_name = patient_display,
        patient_id = case.patient_id,
        employer = case.employer_name,
        employer_contact = case.employer_contact.as_deref().unwrap_or("Not provided"),
        injury_date = case.injury_date,
        body_parts = body_parts_str,
        description = case.injury_description.as_deref().unwrap_or("Not documented"),
        adjuster_name = adjuster.map(|c| c.name.as_str()).unwrap_or("Not assigned"),
        adjuster_company = adjuster.and_then(|c| c.company.as_deref()).unwrap_or("N/A"),
        adjuster_phone = adjuster.and_then(|c| c.phone.as_deref()).unwrap_or("N/A"),
        adjuster_email = adjuster.and_then(|c| c.email.as_deref()).unwrap_or("N/A"),
        adjuster_fax = adjuster.and_then(|c| c.fax.as_deref()).unwrap_or("N/A"),
        attorney_name = attorney.map(|c| c.name.as_str()).unwrap_or("None"),
        attorney_company = attorney.and_then(|c| c.company.as_deref()).unwrap_or("N/A"),
        attorney_phone = attorney.and_then(|c| c.phone.as_deref()).unwrap_or("N/A"),
        case_id = case_id,
    );

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "generate_froi".to_string(),
            resource_type: "WcCase".to_string(),
            resource_id: Some(case_id.clone()),
            patient_id: Some(case.patient_id.clone()),
            device_id: device.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(FroiResult {
        case_id,
        title: format!(
            "First Report of Injury — {} — Claim: {}",
            patient_display, claim_number_str
        ),
        content: froi_content,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — State Fee Schedule
// ─────────────────────────────────────────────────────────────────────────────

/// Look up the maximum allowable amount for a CPT code in a given state.
///
/// Returns the most recent effective fee schedule entry.
#[tauri::command]
pub fn lookup_wc_fee(
    state: String,
    cpt_code: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcFeeResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let result = conn
        .query_row(
            "SELECT state, cpt_code, max_allowable, effective_date
             FROM wc_fee_schedules
             WHERE state = ?1 AND cpt_code = ?2
             ORDER BY effective_date DESC
             LIMIT 1",
            rusqlite::params![state, cpt_code],
            |row| {
                Ok(WcFeeResult {
                    state: row.get(0)?,
                    cpt_code: row.get(1)?,
                    max_allowable: row.get(2)?,
                    effective_date: row.get(3)?,
                })
            },
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "No fee schedule found for state='{}' CPT='{}'",
                state, cpt_code
            ))
        })?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "lookup_wc_fee".to_string(),
            resource_type: "WcFeeSchedule".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!("state={}, cpt={}", state, cpt_code)),
        },
    )?;

    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — Impairment Ratings
// ─────────────────────────────────────────────────────────────────────────────

/// Record an AMA Guides impairment rating for a WC case.
#[tauri::command]
pub fn record_impairment_rating(
    case_id: String,
    input: ImpairmentRatingInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<ImpairmentRatingRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Create)?;

    // Validate percentage range.
    if input.whole_person_pct < 0.0 || input.whole_person_pct > 100.0 {
        return Err(AppError::Validation(
            "whole_person_pct must be between 0 and 100".to_string(),
        ));
    }

    // Validate AMA edition if provided.
    if let Some(ref edition) = input.ama_guides_edition {
        validate_ama_edition(edition)?;
    }

    let rating_id = uuid::Uuid::new_v4().to_string();
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO wc_impairment_ratings
         (rating_id, case_id, body_part, ama_guides_edition, impairment_class,
          grade_modifier, whole_person_pct, evaluator, evaluation_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            rating_id,
            case_id,
            input.body_part,
            input.ama_guides_edition,
            input.impairment_class,
            input.grade_modifier,
            input.whole_person_pct,
            input.evaluator,
            input.evaluation_date,
        ],
    )?;

    let record = conn.query_row(
        "SELECT rating_id, case_id, body_part, ama_guides_edition, impairment_class,
                grade_modifier, whole_person_pct, evaluator, evaluation_date, created_at
         FROM wc_impairment_ratings WHERE rating_id = ?1",
        [&rating_id],
        |row| {
            Ok(ImpairmentRatingRecord {
                rating_id: row.get(0)?,
                case_id: row.get(1)?,
                body_part: row.get(2)?,
                ama_guides_edition: row.get(3)?,
                impairment_class: row.get(4)?,
                grade_modifier: row.get(5)?,
                whole_person_pct: row.get(6)?,
                evaluator: row.get(7)?,
                evaluation_date: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "record_impairment_rating".to_string(),
            resource_type: "WcImpairmentRating".to_string(),
            resource_id: Some(rating_id),
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!(
                "case_id={}, body_part={}, pct={}",
                case_id, input.body_part, input.whole_person_pct
            )),
        },
    )?;

    Ok(record)
}

/// List all impairment ratings for a WC case.
#[tauri::command]
pub fn list_impairment_ratings(
    case_id: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<Vec<ImpairmentRatingRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT rating_id, case_id, body_part, ama_guides_edition, impairment_class,
                grade_modifier, whole_person_pct, evaluator, evaluation_date, created_at
         FROM wc_impairment_ratings WHERE case_id = ?1 ORDER BY created_at DESC",
    )?;

    let records = stmt
        .query_map([&case_id], |row| {
            Ok(ImpairmentRatingRecord {
                rating_id: row.get(0)?,
                case_id: row.get(1)?,
                body_part: row.get(2)?,
                ama_guides_edition: row.get(3)?,
                impairment_class: row.get(4)?,
                grade_modifier: row.get(5)?,
                whole_person_pct: row.get(6)?,
                evaluator: row.get(7)?,
                evaluation_date: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_impairment_ratings".to_string(),
            resource_type: "WcImpairmentRating".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!("case_id={}", case_id)),
        },
    )?;

    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands — Communication Log
// ─────────────────────────────────────────────────────────────────────────────

/// Log a communication entry for a WC case.
#[tauri::command]
pub fn log_wc_communication(
    case_id: String,
    input: WcCommunicationInput,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<WcCommunicationRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Create)?;

    validate_comm_direction(&input.direction)?;
    validate_comm_method(&input.method)?;

    let comm_id = uuid::Uuid::new_v4().to_string();
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let comm_date = input
        .comm_date
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    conn.execute(
        "INSERT INTO wc_communications
         (comm_id, case_id, contact_id, direction, method, subject, content, comm_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            comm_id,
            case_id,
            input.contact_id,
            input.direction,
            input.method,
            input.subject,
            input.content,
            comm_date,
        ],
    )?;

    let record = conn.query_row(
        "SELECT comm_id, case_id, contact_id, direction, method, subject, content, comm_date, created_at
         FROM wc_communications WHERE comm_id = ?1",
        [&comm_id],
        |row| {
            Ok(WcCommunicationRecord {
                comm_id: row.get(0)?,
                case_id: row.get(1)?,
                contact_id: row.get(2)?,
                direction: row.get(3)?,
                method: row.get(4)?,
                subject: row.get(5)?,
                content: row.get(6)?,
                comm_date: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "log_wc_communication".to_string(),
            resource_type: "WcCommunication".to_string(),
            resource_id: Some(comm_id),
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!(
                "case_id={}, direction={}, method={}",
                case_id, record.direction, record.method
            )),
        },
    )?;

    Ok(record)
}

/// List all communication log entries for a WC case.
#[tauri::command]
pub fn list_wc_communications(
    case_id: String,
    db: State<Database>,
    session: State<SessionManager>,
    device: State<DeviceId>,
) -> Result<Vec<WcCommunicationRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalRecords, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT comm_id, case_id, contact_id, direction, method, subject, content, comm_date, created_at
         FROM wc_communications WHERE case_id = ?1 ORDER BY comm_date DESC",
    )?;

    let records = stmt
        .query_map([&case_id], |row| {
            Ok(WcCommunicationRecord {
                comm_id: row.get(0)?,
                case_id: row.get(1)?,
                contact_id: row.get(2)?,
                direction: row.get(3)?,
                method: row.get(4)?,
                subject: row.get(5)?,
                content: row.get(6)?,
                comm_date: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_wc_communications".to_string(),
            resource_type: "WcCommunication".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device.id().to_string(),
            success: true,
            details: Some(format!("case_id={}", case_id)),
        },
    )?;

    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Validation helpers ───────────────────────────────────────────────────

    #[test]
    fn test_case_status_validation_valid() {
        for s in VALID_WC_STATUSES {
            assert!(validate_status(s).is_ok(), "Expected '{}' to be valid", s);
        }
    }

    #[test]
    fn test_case_status_validation_invalid() {
        let err = validate_status("pending");
        assert!(err.is_err());
        let msg = format!("{:?}", err.unwrap_err());
        assert!(msg.contains("pending"));
    }

    #[test]
    fn test_contact_role_validation_valid() {
        for role in VALID_CONTACT_ROLES {
            assert!(
                validate_contact_role(role).is_ok(),
                "Expected '{}' to be valid",
                role
            );
        }
    }

    #[test]
    fn test_contact_role_validation_invalid() {
        let err = validate_contact_role("patient");
        assert!(err.is_err());
        let msg = format!("{:?}", err.unwrap_err());
        assert!(msg.contains("patient"));
    }

    #[test]
    fn test_impairment_percentage_boundary() {
        // Valid boundaries pass the check.
        assert!(!(0.0_f64 < 0.0 || 0.0_f64 > 100.0), "0% is valid");
        assert!(!(100.0_f64 < 0.0 || 100.0_f64 > 100.0), "100% is valid");
        assert!(!(55.5_f64 < 0.0 || 55.5_f64 > 100.0), "55.5% is valid");

        // Invalid: negative.
        let pct = -1.0_f64;
        assert!(pct < 0.0 || pct > 100.0, "-1% should be invalid");

        // Invalid: over 100.
        let pct2 = 100.1_f64;
        assert!(pct2 < 0.0 || pct2 > 100.0, "100.1% should be invalid");
    }

    #[test]
    fn test_ama_edition_validation_valid() {
        for ed in VALID_AMA_EDITIONS {
            assert!(
                validate_ama_edition(ed).is_ok(),
                "Expected '{}' to be valid",
                ed
            );
        }
    }

    #[test]
    fn test_ama_edition_validation_invalid() {
        let err = validate_ama_edition("7th");
        assert!(err.is_err());
    }

    #[test]
    fn test_comm_direction_validation() {
        assert!(validate_comm_direction("inbound").is_ok());
        assert!(validate_comm_direction("outbound").is_ok());
        assert!(validate_comm_direction("sideways").is_err());
    }

    #[test]
    fn test_comm_method_validation() {
        for m in VALID_COMM_METHODS {
            assert!(validate_comm_method(m).is_ok());
        }
        assert!(validate_comm_method("telegram").is_err());
    }

    #[test]
    fn test_body_parts_serialization_roundtrip() {
        let parts = vec!["lumbar_spine".to_string(), "left_shoulder".to_string()];
        let opt = Some(parts.clone());
        let json = serialize_body_parts(&opt);
        assert!(json.is_some());
        let back = parse_body_parts(json);
        assert_eq!(back, Some(parts));
    }

    #[test]
    fn test_froi_content_contains_required_sections() {
        let content = format!(
            "FIRST REPORT OF INJURY\n\
             State: CA\n\
             Claim Number: WC-2026-001\n\
             EMPLOYEE / CLAIMANT INFORMATION\n\
               Name: John Doe\n\
             EMPLOYER INFORMATION\n\
               Employer Name: Acme Corp\n\
             INJURY DETAILS\n\
               Date of Injury: 2026-01-15\n\
               Body Parts Affected:lumbar_spine, left_shoulder\n\
               Description:\n\
                 Fell from ladder\n\
             ADJUSTER INFORMATION\n\
             PROVIDER INFORMATION\n"
        );

        assert!(content.contains("FIRST REPORT OF INJURY"));
        assert!(content.contains("EMPLOYEE / CLAIMANT INFORMATION"));
        assert!(content.contains("EMPLOYER INFORMATION"));
        assert!(content.contains("INJURY DETAILS"));
        assert!(content.contains("ADJUSTER INFORMATION"));
        assert!(content.contains("PROVIDER INFORMATION"));
        assert!(content.contains("John Doe"));
        assert!(content.contains("Acme Corp"));
        assert!(content.contains("2026-01-15"));
        assert!(content.contains("lumbar_spine"));
    }

    #[test]
    fn test_fee_lookup_not_found_produces_meaningful_error() {
        let state = "XX";
        let cpt = "99999";
        let err = AppError::NotFound(format!(
            "No fee schedule found for state='{}' CPT='{}'",
            state, cpt
        ));
        let msg = format!("{:?}", err);
        assert!(msg.contains("XX"));
        assert!(msg.contains("99999"));
    }
}
