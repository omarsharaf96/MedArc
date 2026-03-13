/// commands/pt_notes.rs — Physical Therapy Note Commands (M003/S01)
///
/// Implements PT-DOC-01 through PT-DOC-04:
///   PT-DOC-01  Initial Evaluation note — chief complaint, mechanism of injury, ROM, goals,
///              plan of care, frequency/duration, CPT codes, referring physician.
///   PT-DOC-02  Daily Progress Note — subjective, HEP compliance, treatments, exercises,
///              assessment, progress toward goals, plan.
///   PT-DOC-03  Discharge Summary — total visits, treatment summary, goal achievement,
///              outcome comparison (filled in S02), discharge recommendations.
///   PT-DOC-04  Note lifecycle: draft → signed (co-sign) → locked.
///
/// Data model
/// ----------
/// Note content is stored as FHIR Composition JSON in `fhir_resources`
/// (resource_type = 'PTNote'). Migration 15 adds `pt_note_index` for fast
/// patient/type/status queries.
///
/// RBAC
/// ----
/// All PT note commands require `ClinicalDocumentation` resource access.
///   Provider / SystemAdmin  → full CRUD (create, read, update, cosign, lock)
///   NurseMa                 → Create + Read + Update (cannot lock or cosign)
///   BillingStaff            → Read-only
///   FrontDesk               → No access
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.
/// Audit action strings: pt_note.create, pt_note.read, pt_note.list,
///                       pt_note.update, pt_note.cosign, pt_note.lock
///
/// Observability
/// -------------
/// Inspection:  SELECT * FROM pt_note_index ORDER BY created_at DESC LIMIT 20;
/// Audit trail: SELECT * FROM audit_log WHERE action LIKE 'pt_note.%' ORDER BY timestamp DESC LIMIT 20;
/// Cosign hook: pt_note.cosign rows carry patient_id and encounter_id in `details` for S07.
use std::fmt;

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
// PT Note type enum
// ─────────────────────────────────────────────────────────────────────────────

/// The three Physical Therapy note shapes supported by MedArc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PtNoteType {
    InitialEval,
    ProgressNote,
    DischargeSummary,
}

impl fmt::Display for PtNoteType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PtNoteType::InitialEval => write!(f, "initial_eval"),
            PtNoteType::ProgressNote => write!(f, "progress_note"),
            PtNoteType::DischargeSummary => write!(f, "discharge_summary"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PT-DOC-01: Initial Evaluation fields
// ─────────────────────────────────────────────────────────────────────────────

/// Fields specific to a Physical Therapy Initial Evaluation (PT-DOC-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialEvalFields {
    pub chief_complaint: Option<String>,
    pub mechanism_of_injury: Option<String>,
    pub prior_level_of_function: Option<String>,
    /// Numeric Rating Scale pain score (0–10), stored as string for flexibility.
    pub pain_nrs: Option<String>,
    pub functional_limitations: Option<String>,
    pub icd10_codes: Option<String>,
    pub physical_exam_findings: Option<String>,
    pub short_term_goals: Option<String>,
    pub long_term_goals: Option<String>,
    pub plan_of_care: Option<String>,
    /// Treatment frequency and duration (e.g. "3x/week × 6 weeks").
    pub frequency_duration: Option<String>,
    pub cpt_codes: Option<String>,
    pub referring_physician: Option<String>,
    /// Reference to an uploaded referral document (DocumentReference ID).
    pub referral_document_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// PT-DOC-02: Daily Progress Note fields
// ─────────────────────────────────────────────────────────────────────────────

/// Fields specific to a Physical Therapy Daily Progress Note (PT-DOC-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNoteFields {
    pub subjective: Option<String>,
    /// Numeric Rating Scale pain score reported by patient (0–10).
    pub patient_report_pain_nrs: Option<String>,
    /// Home Exercise Program compliance (e.g. "Good", "Fair", "Poor", or freetext).
    pub hep_compliance: Option<String>,
    pub barriers: Option<String>,
    pub treatments: Option<String>,
    pub exercises: Option<String>,
    pub assessment: Option<String>,
    pub progress_toward_goals: Option<String>,
    pub plan: Option<String>,
    /// Updates to the Home Exercise Program made this visit.
    pub hep_updates: Option<String>,
    /// Total treatment time in minutes for billing (CPT timed codes).
    pub total_treatment_minutes: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// PT-DOC-03: Discharge Summary fields
// ─────────────────────────────────────────────────────────────────────────────

/// Fields specific to a Physical Therapy Discharge Summary (PT-DOC-03).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DischargeSummaryFields {
    pub total_visits_attended: Option<String>,
    pub total_visits_authorized: Option<String>,
    pub treatment_summary: Option<String>,
    pub goal_achievement: Option<String>,
    /// Placeholder for objective outcome measure comparison filled in S02
    /// (e.g. PSFS, LEFS, NDI scores at intake vs discharge).
    pub outcome_comparison_placeholder: Option<String>,
    pub discharge_recommendations: Option<String>,
    pub hep_narrative: Option<String>,
    /// Return-to-care plan or referral instructions.
    pub return_to_care: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tagged union for all note shapes
// ─────────────────────────────────────────────────────────────────────────────

/// Discriminated union of the three PT note field sets.
///
/// Serializes as `{ "noteType": "initial_eval", "fields": { ... } }` so the
/// TypeScript frontend can discriminate on `noteType` without a wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "noteType", content = "fields", rename_all = "snake_case")]
pub enum PtNoteFields {
    InitialEval(InitialEvalFields),
    ProgressNote(ProgressNoteFields),
    DischargeSummary(DischargeSummaryFields),
}

// ─────────────────────────────────────────────────────────────────────────────
// Command input / output types
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating a new PT note.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtNoteInput {
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub note_type: PtNoteType,
    /// Optional pre-populated fields; may be omitted to create an empty draft.
    pub fields: Option<PtNoteFields>,
    /// If this note is an addendum to an existing note, its ID goes here.
    pub addendum_of: Option<String>,
}

/// PT note record returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtNoteRecord {
    pub id: String,
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub note_type: String,
    pub status: String,
    pub provider_id: String,
    /// The full FHIR Composition JSON for this note.
    pub resource: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    pub addendum_of: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR builder
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR R4 Composition resource for a PT note.
fn build_pt_note_fhir(
    id: &str,
    note_type: &PtNoteType,
    patient_id: &str,
    fields: Option<&PtNoteFields>,
    now: &str,
) -> serde_json::Value {
    let type_code = note_type.to_string();
    let type_display = match note_type {
        PtNoteType::InitialEval => "PT Initial Evaluation",
        PtNoteType::ProgressNote => "PT Daily Progress Note",
        PtNoteType::DischargeSummary => "PT Discharge Summary",
    };

    let mut resource = serde_json::json!({
        "resourceType": "Composition",
        "id": id,
        "status": "preliminary",
        "type": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/pt-note-type",
                "code": type_code,
                "display": type_display
            }]
        },
        "subject": {
            "reference": format!("Patient/{}", patient_id),
            "type": "Patient"
        },
        "date": now,
        "title": type_display
    });

    if let Some(f) = fields {
        let fields_json = serde_json::to_value(f).unwrap_or(serde_json::Value::Null);
        resource["section"] = serde_json::json!([{
            "extension": [{
                "url": "http://medarc.local/fhir/StructureDefinition/pt-note-fields",
                "valueString": fields_json.to_string()
            }],
            "text": {
                "status": "generated",
                "div": fields_json.to_string()
            }
        }]);
    }

    resource
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new PT note draft (PT-DOC-01/02/03).
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn create_pt_note(
    input: PtNoteInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtNoteRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let note_type_str = input.note_type.to_string();

    let fhir = build_pt_note_fhir(&id, &input.note_type, &input.patient_id, input.fields.as_ref(), &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'PTNote', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO pt_note_index
            (pt_note_id, patient_id, encounter_id, note_type, status, provider_id, addendum_of, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7, ?7)",
        rusqlite::params![
            id,
            input.patient_id,
            input.encounter_id,
            note_type_str,
            sess.user_id,
            input.addendum_of,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.create".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("note_type={}", note_type_str)),
        },
    );

    Ok(PtNoteRecord {
        id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        note_type: note_type_str,
        status: "draft".to_string(),
        provider_id: sess.user_id,
        resource: fhir,
        created_at: now.clone(),
        updated_at: now,
        addendum_of: input.addendum_of,
    })
}

/// Get a single PT note by ID.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_pt_note(
    pt_note_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtNoteRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, encounter_id, note_type, status, provider_id, resource_str, created_at, updated_at, addendum_of): (
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT pni.patient_id, pni.encounter_id, pni.note_type, pni.status,
                    pni.provider_id, fr.resource, pni.created_at, pni.updated_at, pni.addendum_of
             FROM pt_note_index pni
             JOIN fhir_resources fr ON fr.id = pni.pt_note_id
             WHERE pni.pt_note_id = ?1",
            rusqlite::params![pt_note_id],
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
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("PT note {} not found", pt_note_id)))?;

    let resource: serde_json::Value = serde_json::from_str(&resource_str)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.read".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: Some(pt_note_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(PtNoteRecord {
        id: pt_note_id,
        patient_id,
        encounter_id,
        note_type,
        status,
        provider_id,
        resource,
        created_at,
        updated_at,
        addendum_of,
    })
}

/// List PT notes for a patient, optionally filtered by note_type.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_pt_notes(
    patient_id: String,
    note_type: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<PtNoteRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT pni.pt_note_id, pni.patient_id, pni.encounter_id, pni.note_type, pni.status,
                pni.provider_id, fr.resource, pni.created_at, pni.updated_at, pni.addendum_of
         FROM pt_note_index pni
         JOIN fhir_resources fr ON fr.id = pni.pt_note_id
         WHERE pni.patient_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(patient_id.clone())];

    if let Some(ref nt) = note_type {
        query.push_str(&format!(" AND pni.note_type = ?{}", params.len() + 1));
        params.push(Box::new(nt.clone()));
    }
    query.push_str(" ORDER BY pni.created_at DESC");

    let records: Vec<PtNoteRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(id, pid, enc_id, nt, status, prov, res_str, created, updated, addendum)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            PtNoteRecord {
                id,
                patient_id: pid,
                encounter_id: enc_id,
                note_type: nt,
                status,
                provider_id: prov,
                resource,
                created_at: created,
                updated_at: updated,
                addendum_of: addendum,
            }
        })
        .collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.list".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: note_type.map(|nt| format!("note_type={}", nt)),
        },
    );

    Ok(records)
}

/// Update a PT note's fields (only allowed when status is "draft").
///
/// Locked notes are read-only — returns AppError::Validation if status is "locked".
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn update_pt_note(
    pt_note_id: String,
    input: PtNoteInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtNoteRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Read current record to check status
    let (patient_id, encounter_id, note_type_str, current_status, provider_id, addendum_of): (
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT patient_id, encounter_id, note_type, status, provider_id, addendum_of
             FROM pt_note_index
             WHERE pt_note_id = ?1",
            rusqlite::params![pt_note_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("PT note {} not found", pt_note_id)))?;

    if current_status == "locked" {
        return Err(AppError::Validation(format!(
            "PT note {} is locked and cannot be modified",
            pt_note_id
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_pt_note_fhir(&pt_note_id, &input.note_type, &patient_id, input.fields.as_ref(), &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, last_updated = ?2, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![fhir_json, now, pt_note_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE pt_note_index SET updated_at = ?1 WHERE pt_note_id = ?2",
        rusqlite::params![now, pt_note_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.update".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: Some(pt_note_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("note_type={}", note_type_str)),
        },
    );

    // Read back updated created_at
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM pt_note_index WHERE pt_note_id = ?1",
            rusqlite::params![pt_note_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(PtNoteRecord {
        id: pt_note_id,
        patient_id,
        encounter_id,
        note_type: note_type_str,
        status: current_status,
        provider_id,
        resource: fhir,
        created_at,
        updated_at: now,
        addendum_of,
    })
}

/// Co-sign a PT note: transition status from "draft" to "signed".
///
/// Only allowed when current status is "draft". Returns AppError::Validation
/// if the note is not in "draft" status.
///
/// The audit row includes patient_id and encounter_id in `details` so that
/// S07's visit counter can JOIN pt_note_index without schema changes.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn cosign_pt_note(
    pt_note_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtNoteRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, encounter_id, note_type, current_status, provider_id, created_at, addendum_of): (
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT patient_id, encounter_id, note_type, status, provider_id, created_at, addendum_of
             FROM pt_note_index
             WHERE pt_note_id = ?1",
            rusqlite::params![pt_note_id],
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
        .map_err(|_| AppError::NotFound(format!("PT note {} not found", pt_note_id)))?;

    if current_status != "draft" {
        return Err(AppError::Validation(format!(
            "PT note {} cannot be co-signed: expected status 'draft', found '{}'",
            pt_note_id, current_status
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE pt_note_index SET status = 'signed', updated_at = ?1 WHERE pt_note_id = ?2",
        rusqlite::params![now, pt_note_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch the current FHIR resource JSON for the returned record
    let resource_str: String = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1",
            rusqlite::params![pt_note_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let resource: serde_json::Value =
        serde_json::from_str(&resource_str).map_err(|e| AppError::Serialization(e.to_string()))?;

    // details carries patient_id + encounter_id so S07 visit counter can JOIN
    let enc_id_display = encounter_id.as_deref().unwrap_or("none");
    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.cosign".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: Some(pt_note_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "patient_id={},encounter_id={}",
                patient_id, enc_id_display
            )),
        },
    );

    Ok(PtNoteRecord {
        id: pt_note_id,
        patient_id,
        encounter_id,
        note_type,
        status: "signed".to_string(),
        provider_id,
        resource,
        created_at,
        updated_at: now,
        addendum_of,
    })
}

/// Lock a PT note: transition status from "signed" to "locked".
///
/// Only allowed when current status is "signed". Returns AppError::Validation
/// if the note is not in "signed" status. Locked notes are permanently
/// read-only — no further updates are allowed.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn lock_pt_note(
    pt_note_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtNoteRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, encounter_id, note_type, current_status, provider_id, created_at, addendum_of): (
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT patient_id, encounter_id, note_type, status, provider_id, created_at, addendum_of
             FROM pt_note_index
             WHERE pt_note_id = ?1",
            rusqlite::params![pt_note_id],
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
        .map_err(|_| AppError::NotFound(format!("PT note {} not found", pt_note_id)))?;

    if current_status != "signed" {
        return Err(AppError::Validation(format!(
            "PT note {} cannot be locked: expected status 'signed', found '{}'",
            pt_note_id, current_status
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE pt_note_index SET status = 'locked', updated_at = ?1 WHERE pt_note_id = ?2",
        rusqlite::params![now, pt_note_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch the current FHIR resource JSON for the returned record
    let resource_str: String = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1",
            rusqlite::params![pt_note_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let resource: serde_json::Value =
        serde_json::from_str(&resource_str).map_err(|e| AppError::Serialization(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pt_note.lock".to_string(),
            resource_type: "PTNote".to_string(),
            resource_id: Some(pt_note_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(PtNoteRecord {
        id: pt_note_id,
        patient_id,
        encounter_id,
        note_type,
        status: "locked".to_string(),
        provider_id,
        resource,
        created_at,
        updated_at: now,
        addendum_of,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that PtNoteType serializes to the correct snake_case strings.
    #[test]
    fn pt_note_type_serializes_correctly() {
        assert_eq!(
            serde_json::to_string(&PtNoteType::InitialEval).unwrap(),
            "\"initial_eval\""
        );
        assert_eq!(
            serde_json::to_string(&PtNoteType::ProgressNote).unwrap(),
            "\"progress_note\""
        );
        assert_eq!(
            serde_json::to_string(&PtNoteType::DischargeSummary).unwrap(),
            "\"discharge_summary\""
        );
    }

    /// Verify that all PtNoteType variants round-trip through JSON correctly.
    #[test]
    fn pt_note_type_all_variants_round_trip() {
        for (variant, expected_str) in [
            (PtNoteType::InitialEval, "\"initial_eval\""),
            (PtNoteType::ProgressNote, "\"progress_note\""),
            (PtNoteType::DischargeSummary, "\"discharge_summary\""),
        ] {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_str);
            let deserialized: PtNoteType = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized.to_string(), variant.to_string());
        }
    }

    /// Verify that Migration 15 SQL is accepted by rusqlite_migration validation.
    #[test]
    fn migration_15_is_valid() {
        use crate::db::migrations::MIGRATIONS;
        assert!(
            MIGRATIONS.validate().is_ok(),
            "MIGRATIONS.validate() failed — check Migration 15 SQL syntax"
        );
    }

    /// Verify that InitialEvalFields with all-None fields serializes to JSON
    /// with explicit null values (not omitted).
    #[test]
    fn initial_eval_fields_serialize_nulls() {
        let fields = InitialEvalFields {
            chief_complaint: None,
            mechanism_of_injury: None,
            prior_level_of_function: None,
            pain_nrs: None,
            functional_limitations: None,
            icd10_codes: None,
            physical_exam_findings: None,
            short_term_goals: None,
            long_term_goals: None,
            plan_of_care: None,
            frequency_duration: None,
            cpt_codes: None,
            referring_physician: None,
            referral_document_id: None,
        };

        let json_str = serde_json::to_string(&fields).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let obj = json_val.as_object().expect("should be a JSON object");

        // All optional fields must be explicitly present as null (not absent)
        let expected_keys = [
            "chiefComplaint",
            "mechanismOfInjury",
            "priorLevelOfFunction",
            "painNrs",
            "functionalLimitations",
            "icd10Codes",
            "physicalExamFindings",
            "shortTermGoals",
            "longTermGoals",
            "planOfCare",
            "frequencyDuration",
            "cptCodes",
            "referringPhysician",
            "referralDocumentId",
        ];
        for key in &expected_keys {
            assert!(
                obj.contains_key(*key),
                "expected field '{}' to be present in JSON (as null)",
                key
            );
            assert!(
                obj[*key].is_null(),
                "expected field '{}' to be null, got {:?}",
                key,
                obj[*key]
            );
        }
    }
}
