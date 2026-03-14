/// commands/document_center.rs — Patient Document Center (M003/S04)
///
/// Implements the Document Center upgrade with three feature areas:
///   1. Document Categories — PT-specific categorized document upload, browse, re-categorize
///   2. Intake Survey Builder — survey templates with built-in PT intake forms, response storage
///   3. Referral Tracking — referring provider records linked to patients
///
/// Data model
/// ----------
/// Resources are stored as FHIR-aligned JSON in `fhir_resources`.
/// Migration 17 adds four index tables:
///   - `document_category_index`  (patient_id, category, file_name, mime_type, file_size, sha1_hash)
///   - `survey_template_index`    (name, is_builtin, field_count)
///   - `survey_response_index`    (template_id, patient_id, completed_at)
///   - `referral_index`           (patient_id, referring_provider_name, npi, referral_date, etc.)
///
/// RBAC
/// ----
/// All document commands require `PatientDocuments` resource access.
///   Provider / SystemAdmin  → full CRUD
///   NurseMa                 → Create + Read + Update (no delete)
///   BillingStaff            → Read-only
///   FrontDesk               → Read-only
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Document Category types
// ─────────────────────────────────────────────────────────────────────────────

/// PT-specific document categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCategory {
    ReferralRx,
    Imaging,
    ConsentForms,
    IntakeSurveys,
    Insurance,
    Legal,
    HomeExerciseProgram,
    Other,
}

impl DocumentCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentCategory::ReferralRx => "referral_rx",
            DocumentCategory::Imaging => "imaging",
            DocumentCategory::ConsentForms => "consent_forms",
            DocumentCategory::IntakeSurveys => "intake_surveys",
            DocumentCategory::Insurance => "insurance",
            DocumentCategory::Legal => "legal",
            DocumentCategory::HomeExerciseProgram => "home_exercise_program",
            DocumentCategory::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "referral_rx" => Ok(DocumentCategory::ReferralRx),
            "imaging" => Ok(DocumentCategory::Imaging),
            "consent_forms" => Ok(DocumentCategory::ConsentForms),
            "intake_surveys" => Ok(DocumentCategory::IntakeSurveys),
            "insurance" => Ok(DocumentCategory::Insurance),
            "legal" => Ok(DocumentCategory::Legal),
            "home_exercise_program" => Ok(DocumentCategory::HomeExerciseProgram),
            "other" => Ok(DocumentCategory::Other),
            _ => Err(AppError::Validation(format!(
                "Invalid document category '{}'. Must be one of: referral_rx, imaging, consent_forms, \
                 intake_surveys, insurance, legal, home_exercise_program, other",
                s
            ))),
        }
    }

    pub fn all_values() -> &'static [&'static str] {
        &[
            "referral_rx",
            "imaging",
            "consent_forms",
            "intake_surveys",
            "insurance",
            "legal",
            "home_exercise_program",
            "other",
        ]
    }
}

/// Input for uploading a categorized patient document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorizedDocumentInput {
    /// Patient the document belongs to.
    pub patient_id: String,
    /// PT-specific category.
    pub category: String,
    /// File name (e.g. "referral_dr_smith.pdf").
    pub file_name: String,
    /// Base64-encoded file content.
    pub file_data_base64: String,
    /// MIME type (e.g. "application/pdf", "image/jpeg").
    pub mime_type: String,
}

/// Categorized document record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorizedDocumentRecord {
    pub document_id: String,
    pub patient_id: String,
    pub category: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub sha1_hash: String,
    pub uploaded_at: String,
    pub resource: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Survey types
// ─────────────────────────────────────────────────────────────────────────────

/// Field types supported in intake surveys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SurveyFieldType {
    Text,
    Number,
    YesNo,
    PainScale,
    Date,
}

impl SurveyFieldType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SurveyFieldType::Text => "text",
            SurveyFieldType::Number => "number",
            SurveyFieldType::YesNo => "yes_no",
            SurveyFieldType::PainScale => "pain_scale",
            SurveyFieldType::Date => "date",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "text" => Ok(SurveyFieldType::Text),
            "number" => Ok(SurveyFieldType::Number),
            "yes_no" => Ok(SurveyFieldType::YesNo),
            "pain_scale" => Ok(SurveyFieldType::PainScale),
            "date" => Ok(SurveyFieldType::Date),
            _ => Err(AppError::Validation(format!(
                "Invalid survey field type '{}'. Must be one of: text, number, yes_no, pain_scale, date",
                s
            ))),
        }
    }
}

/// A single field in a survey template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurveyField {
    pub field_id: String,
    pub field_type: String,
    pub label: String,
    pub required: bool,
    pub order: i32,
}

/// Input for creating a custom survey template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurveyTemplateInput {
    pub name: String,
    pub fields: Vec<SurveyField>,
}

/// Survey template record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurveyTemplateRecord {
    pub template_id: String,
    pub name: String,
    pub is_builtin: bool,
    pub field_count: i32,
    pub fields: Vec<SurveyField>,
    pub created_at: String,
    pub updated_at: String,
    pub resource: serde_json::Value,
}

/// Input for submitting a survey response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurveyResponseInput {
    pub template_id: String,
    pub patient_id: String,
    pub responses: serde_json::Value,
}

/// Survey response record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurveyResponseRecord {
    pub response_id: String,
    pub template_id: String,
    pub patient_id: String,
    pub responses: serde_json::Value,
    pub completed_at: String,
    pub resource: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Referral types
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating a referral record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferralInput {
    pub patient_id: String,
    pub referring_provider_name: String,
    pub referring_provider_npi: Option<String>,
    pub practice_name: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
    pub referral_date: Option<String>,
    pub authorized_visit_count: Option<i32>,
    pub diagnosis_icd10: Option<String>,
    pub linked_document_id: Option<String>,
    pub notes: Option<String>,
}

/// Referral record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferralRecord {
    pub referral_id: String,
    pub patient_id: String,
    pub referring_provider_name: String,
    pub referring_provider_npi: Option<String>,
    pub practice_name: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
    pub referral_date: Option<String>,
    pub authorized_visit_count: Option<i32>,
    pub diagnosis_icd10: Option<String>,
    pub linked_document_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub resource: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR builder helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_categorized_document_fhir(
    id: &str,
    input: &CategorizedDocumentInput,
    sha1: &str,
    file_size: i64,
    now: &str,
) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "DocumentReference",
        "id": id,
        "meta": {
            "profile": ["http://medarc.local/fhir/StructureDefinition/CategorizedDocument"]
        },
        "status": "current",
        "docStatus": "final",
        "type": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/pt-document-category",
                "code": input.category,
                "display": input.file_name
            }]
        },
        "category": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/pt-document-category",
                "code": input.category
            }]
        }],
        "subject": {
            "reference": format!("Patient/{}", input.patient_id)
        },
        "date": now,
        "content": [{
            "attachment": {
                "contentType": input.mime_type,
                "size": file_size,
                "title": input.file_name,
                "creation": now
            }
        }],
        "extension": [
            {
                "url": "http://medarc.local/fhir/ext/sha1-checksum",
                "valueString": sha1
            },
            {
                "url": "http://medarc.local/fhir/ext/file-size-bytes",
                "valueInteger": file_size
            }
        ]
    })
}

fn build_survey_template_fhir(
    id: &str,
    name: &str,
    fields: &[SurveyField],
    is_builtin: bool,
    now: &str,
) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Questionnaire",
        "id": id,
        "meta": {
            "profile": ["http://medarc.local/fhir/StructureDefinition/IntakeSurvey"]
        },
        "status": "active",
        "name": name,
        "title": name,
        "date": now,
        "item": fields.iter().map(|f| {
            serde_json::json!({
                "linkId": f.field_id,
                "text": f.label,
                "type": f.field_type,
                "required": f.required,
                "extension": [{
                    "url": "http://medarc.local/fhir/ext/field-order",
                    "valueInteger": f.order
                }]
            })
        }).collect::<Vec<_>>(),
        "extension": [{
            "url": "http://medarc.local/fhir/ext/is-builtin",
            "valueBoolean": is_builtin
        }]
    })
}

fn build_survey_response_fhir(
    id: &str,
    template_id: &str,
    patient_id: &str,
    responses: &serde_json::Value,
    now: &str,
) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "QuestionnaireResponse",
        "id": id,
        "meta": {
            "profile": ["http://medarc.local/fhir/StructureDefinition/IntakeSurveyResponse"]
        },
        "status": "completed",
        "questionnaire": format!("Questionnaire/{}", template_id),
        "subject": {
            "reference": format!("Patient/{}", patient_id)
        },
        "authored": now,
        "item": responses
    })
}

fn build_referral_fhir(
    id: &str,
    input: &ReferralInput,
    now: &str,
) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "ServiceRequest",
        "id": id,
        "meta": {
            "profile": ["http://medarc.local/fhir/StructureDefinition/PTReferral"]
        },
        "status": "active",
        "intent": "order",
        "subject": {
            "reference": format!("Patient/{}", input.patient_id)
        },
        "requester": {
            "display": input.referring_provider_name,
            "identifier": input.referring_provider_npi.as_ref().map(|npi| {
                serde_json::json!({
                    "system": "http://hl7.org/fhir/sid/us-npi",
                    "value": npi
                })
            })
        },
        "authoredOn": input.referral_date.as_deref().unwrap_or(now),
        "note": input.notes.as_ref().map(|n| vec![serde_json::json!({"text": n})]),
        "extension": [
            {
                "url": "http://medarc.local/fhir/ext/practice-name",
                "valueString": input.practice_name
            },
            {
                "url": "http://medarc.local/fhir/ext/phone",
                "valueString": input.phone
            },
            {
                "url": "http://medarc.local/fhir/ext/fax",
                "valueString": input.fax
            },
            {
                "url": "http://medarc.local/fhir/ext/authorized-visit-count",
                "valueInteger": input.authorized_visit_count
            },
            {
                "url": "http://medarc.local/fhir/ext/diagnosis-icd10",
                "valueString": input.diagnosis_icd10
            },
            {
                "url": "http://medarc.local/fhir/ext/linked-document-id",
                "valueString": input.linked_document_id
            }
        ]
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Utility functions
// ─────────────────────────────────────────────────────────────────────────────

/// Compute SHA-256 of base64-decoded bytes and return as hex string.
fn compute_sha256_hex(base64_content: &str) -> Result<(String, i64), AppError> {
    let decoded = base64_decode(base64_content)?;
    let size = decoded.len() as i64;
    let mut hasher = Sha256::new();
    hasher.update(&decoded);
    Ok((hex::encode(hasher.finalize()), size))
}

/// Simple base64 decoder (standard alphabet, handles padding).
fn base64_decode(input: &str) -> Result<Vec<u8>, AppError> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in alphabet.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }

    let mut result = Vec::new();
    let bytes = cleaned.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 3 < len {
        let b0 = if bytes[i] == b'=' {
            0
        } else {
            lookup[bytes[i] as usize]
        };
        let b1 = if bytes[i + 1] == b'=' {
            0
        } else {
            lookup[bytes[i + 1] as usize]
        };
        let b2 = if bytes[i + 2] == b'=' {
            0
        } else {
            lookup[bytes[i + 2] as usize]
        };
        let b3 = if bytes[i + 3] == b'=' {
            0
        } else {
            lookup[bytes[i + 3] as usize]
        };

        if b0 == 255 || b1 == 255 {
            return Err(AppError::Validation("Invalid base64 content".to_string()));
        }

        result.push((b0 << 2) | (b1 >> 4));
        if bytes[i + 2] != b'=' {
            result.push((b1 << 4) | (b2 >> 2));
        }
        if bytes[i + 3] != b'=' {
            result.push((b2 << 6) | b3);
        }
        i += 4;
    }

    Ok(result)
}

/// Return the 3 built-in survey templates for PT intake.
pub fn builtin_survey_templates() -> Vec<(String, String, Vec<SurveyField>, bool)> {
    vec![
        (
            "builtin-pain-function-intake".to_string(),
            "Pain and Function Intake".to_string(),
            vec![
                SurveyField {
                    field_id: "pfi-name".to_string(),
                    field_type: "text".to_string(),
                    label: "Full Name".to_string(),
                    required: true,
                    order: 1,
                },
                SurveyField {
                    field_id: "pfi-dob".to_string(),
                    field_type: "date".to_string(),
                    label: "Date of Birth".to_string(),
                    required: true,
                    order: 2,
                },
                SurveyField {
                    field_id: "pfi-chief-complaint".to_string(),
                    field_type: "text".to_string(),
                    label: "Chief Complaint".to_string(),
                    required: true,
                    order: 3,
                },
                SurveyField {
                    field_id: "pfi-pain-location".to_string(),
                    field_type: "text".to_string(),
                    label: "Pain Location".to_string(),
                    required: true,
                    order: 4,
                },
                SurveyField {
                    field_id: "pfi-pain-nrs".to_string(),
                    field_type: "pain_scale".to_string(),
                    label: "Pain NRS (0-10)".to_string(),
                    required: true,
                    order: 5,
                },
                SurveyField {
                    field_id: "pfi-onset-date".to_string(),
                    field_type: "date".to_string(),
                    label: "Onset Date".to_string(),
                    required: false,
                    order: 6,
                },
                SurveyField {
                    field_id: "pfi-mechanism".to_string(),
                    field_type: "text".to_string(),
                    label: "Mechanism of Injury".to_string(),
                    required: false,
                    order: 7,
                },
                SurveyField {
                    field_id: "pfi-prior-treatment".to_string(),
                    field_type: "text".to_string(),
                    label: "Prior Treatment".to_string(),
                    required: false,
                    order: 8,
                },
            ],
            true,
        ),
        (
            "builtin-medical-history".to_string(),
            "Medical History".to_string(),
            vec![
                SurveyField {
                    field_id: "mh-conditions".to_string(),
                    field_type: "text".to_string(),
                    label: "Current Medical Conditions".to_string(),
                    required: true,
                    order: 1,
                },
                SurveyField {
                    field_id: "mh-medications".to_string(),
                    field_type: "text".to_string(),
                    label: "Current Medications".to_string(),
                    required: true,
                    order: 2,
                },
                SurveyField {
                    field_id: "mh-allergies".to_string(),
                    field_type: "text".to_string(),
                    label: "Known Allergies".to_string(),
                    required: true,
                    order: 3,
                },
                SurveyField {
                    field_id: "mh-surgeries".to_string(),
                    field_type: "text".to_string(),
                    label: "Previous Surgeries".to_string(),
                    required: false,
                    order: 4,
                },
                SurveyField {
                    field_id: "mh-family-history".to_string(),
                    field_type: "text".to_string(),
                    label: "Family History".to_string(),
                    required: false,
                    order: 5,
                },
            ],
            true,
        ),
        (
            "builtin-hipaa-acknowledgment".to_string(),
            "HIPAA Acknowledgment".to_string(),
            vec![
                SurveyField {
                    field_id: "hipaa-name".to_string(),
                    field_type: "text".to_string(),
                    label: "Patient Name".to_string(),
                    required: true,
                    order: 1,
                },
                SurveyField {
                    field_id: "hipaa-date".to_string(),
                    field_type: "date".to_string(),
                    label: "Date".to_string(),
                    required: true,
                    order: 2,
                },
                SurveyField {
                    field_id: "hipaa-signature".to_string(),
                    field_type: "yes_no".to_string(),
                    label: "Signature Confirmation".to_string(),
                    required: true,
                    order: 3,
                },
                SurveyField {
                    field_id: "hipaa-ack".to_string(),
                    field_type: "yes_no".to_string(),
                    label: "I acknowledge receipt of the Notice of Privacy Practices".to_string(),
                    required: true,
                    order: 4,
                },
            ],
            true,
        ),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Document Category commands
// ─────────────────────────────────────────────────────────────────────────────

/// Upload a document with a PT-specific category.
///
/// RBAC: Provider, NurseMa, SystemAdmin (create).
/// Computes SHA-256 checksum of the content. Validates category.
/// Stores as FHIR DocumentReference + `document_category_index`.
#[tauri::command]
pub fn upload_categorized_document(
    input: CategorizedDocumentInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<CategorizedDocumentRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Create)?;

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.file_name.trim().is_empty() {
        return Err(AppError::Validation("file_name is required".to_string()));
    }
    if input.file_data_base64.is_empty() {
        return Err(AppError::Validation(
            "file_data_base64 is required".to_string(),
        ));
    }

    // Validate category
    let _cat = DocumentCategory::from_str(&input.category)?;

    let (sha1_hash, file_size) = compute_sha256_hex(&input.file_data_base64)?;

    // 64 MB limit
    const MAX_SIZE_BYTES: i64 = 64 * 1024 * 1024;
    if file_size > MAX_SIZE_BYTES {
        return Err(AppError::Validation(format!(
            "Document exceeds maximum size of 64 MB (got {} bytes)",
            file_size
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_categorized_document_fhir(&id, &input, &sha1_hash, file_size, &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'DocumentReference', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO document_category_index (document_id, resource_id, patient_id, category, file_name, mime_type, file_size, sha1_hash, uploaded_at)
         VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id,
            input.patient_id,
            input.category,
            input.file_name,
            input.mime_type,
            file_size,
            sha1_hash,
            now
        ],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "upload_categorized_document".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "category: {}, file: {}",
                input.category, input.file_name
            )),
        },
    )?;

    Ok(CategorizedDocumentRecord {
        document_id: id,
        patient_id: input.patient_id,
        category: input.category,
        file_name: input.file_name,
        mime_type: input.mime_type,
        file_size,
        sha1_hash,
        uploaded_at: now,
        resource: fhir,
    })
}

/// List patient documents, optionally filtered by category and sorted.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn list_patient_documents(
    patient_id: String,
    category: Option<String>,
    sort_by: Option<String>,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<CategorizedDocumentRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    // Validate category if provided
    if let Some(ref cat) = category {
        let _ = DocumentCategory::from_str(cat)?;
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let order_clause = match sort_by.as_deref() {
        Some("name") => "d.file_name ASC",
        Some("category") => "d.category ASC, d.uploaded_at DESC",
        _ => "d.uploaded_at DESC",
    };

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, String, String, String, i64, String, String, String)> {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    };

    let to_record = |(doc_id, pid, cat, fname, mime, fsize, sha, uploaded, res_str): (String, String, String, String, String, i64, String, String, String)| {
        let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
        CategorizedDocumentRecord {
            document_id: doc_id,
            patient_id: pid,
            category: cat,
            file_name: fname,
            mime_type: mime,
            file_size: fsize,
            sha1_hash: sha,
            uploaded_at: uploaded,
            resource,
        }
    };

    let rows: Vec<CategorizedDocumentRecord> = match &category {
        Some(cat) => {
            let sql = format!(
                "SELECT d.document_id, d.patient_id, d.category, d.file_name, d.mime_type, \
                        d.file_size, d.sha1_hash, d.uploaded_at, r.resource \
                 FROM document_category_index d \
                 JOIN fhir_resources r ON r.id = d.document_id \
                 WHERE d.patient_id = ?1 AND d.category = ?2 \
                 ORDER BY {}",
                order_clause
            );
            conn.prepare(&sql)
                .map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id, cat], map_row)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .map(to_record)
                .collect()
        }
        None => {
            let sql = format!(
                "SELECT d.document_id, d.patient_id, d.category, d.file_name, d.mime_type, \
                        d.file_size, d.sha1_hash, d.uploaded_at, r.resource \
                 FROM document_category_index d \
                 JOIN fhir_resources r ON r.id = d.document_id \
                 WHERE d.patient_id = ?1 \
                 ORDER BY {}",
                order_clause
            );
            conn.prepare(&sql)
                .map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id], map_row)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .map(to_record)
                .collect()
        }
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_patient_documents".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: category.map(|c| format!("category: {}", c)),
        },
    )?;

    Ok(rows)
}

/// Retrieve a single document by ID with metadata.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn get_document(
    document_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<CategorizedDocumentRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (doc_id, pid, cat, fname, mime, fsize, sha, uploaded, res_str): (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT d.document_id, d.patient_id, d.category, d.file_name, d.mime_type, \
                    d.file_size, d.sha1_hash, d.uploaded_at, r.resource \
             FROM document_category_index d \
             JOIN fhir_resources r ON r.id = d.document_id \
             WHERE d.document_id = ?1",
            rusqlite::params![document_id],
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
        .map_err(|_| AppError::NotFound(format!("Document '{}' not found", document_id)))?;

    let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "get_document".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(doc_id.clone()),
            patient_id: Some(pid.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(CategorizedDocumentRecord {
        document_id: doc_id,
        patient_id: pid,
        category: cat,
        file_name: fname,
        mime_type: mime,
        file_size: fsize,
        sha1_hash: sha,
        uploaded_at: uploaded,
        resource,
    })
}

/// Update the category of a document.
///
/// RBAC: Provider, NurseMa, SystemAdmin (update).
#[tauri::command]
pub fn update_document_category(
    document_id: String,
    category: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<CategorizedDocumentRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Update)?;

    // Validate category
    let _cat = DocumentCategory::from_str(&category)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    // Update the index
    let rows_updated = conn.execute(
        "UPDATE document_category_index SET category = ?1 WHERE document_id = ?2",
        rusqlite::params![category, document_id],
    )?;
    if rows_updated == 0 {
        return Err(AppError::NotFound(format!(
            "Document '{}' not found",
            document_id
        )));
    }

    // Update the FHIR resource JSON
    let (resource_str, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1",
            rusqlite::params![document_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Document resource '{}' not found", document_id)))?;

    let mut resource: serde_json::Value =
        serde_json::from_str(&resource_str).map_err(|e| AppError::Serialization(e.to_string()))?;
    resource["category"] = serde_json::json!([{
        "coding": [{
            "system": "http://medarc.local/fhir/CodeSystem/pt-document-category",
            "code": category
        }]
    }]);
    resource["type"]["coding"][0]["code"] = serde_json::Value::String(category.clone());

    let new_json =
        serde_json::to_string(&resource).map_err(|e| AppError::Serialization(e.to_string()))?;
    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![new_json, version_id + 1, now, document_id],
    )?;

    // Fetch updated record
    let (doc_id, pid, cat, fname, mime, fsize, sha, uploaded): (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT document_id, patient_id, category, file_name, mime_type, file_size, sha1_hash, uploaded_at \
             FROM document_category_index WHERE document_id = ?1",
            rusqlite::params![document_id],
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
                ))
            },
        )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "update_document_category".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(doc_id.clone()),
            patient_id: Some(pid.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("new category: {}", cat)),
        },
    )?;

    Ok(CategorizedDocumentRecord {
        document_id: doc_id,
        patient_id: pid,
        category: cat,
        file_name: fname,
        mime_type: mime,
        file_size: fsize,
        sha1_hash: sha,
        uploaded_at: uploaded,
        resource,
    })
}

/// Soft-delete a document (marks FHIR status as entered-in-error).
///
/// RBAC: Provider, SystemAdmin (delete).
#[tauri::command]
pub fn delete_document(
    document_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Delete)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    // Fetch patient_id for audit before deleting
    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM document_category_index WHERE document_id = ?1",
            rusqlite::params![document_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Document '{}' not found", document_id)))?;

    // Update FHIR status to entered-in-error (soft delete)
    let (resource_str, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1",
            rusqlite::params![document_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Document resource '{}' not found", document_id)))?;

    let mut resource: serde_json::Value =
        serde_json::from_str(&resource_str).map_err(|e| AppError::Serialization(e.to_string()))?;
    resource["status"] = serde_json::Value::String("entered-in-error".to_string());

    let new_json =
        serde_json::to_string(&resource).map_err(|e| AppError::Serialization(e.to_string()))?;
    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![new_json, version_id + 1, now, document_id],
    )?;

    // Remove from category index
    conn.execute(
        "DELETE FROM document_category_index WHERE document_id = ?1",
        rusqlite::params![document_id],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "delete_document".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(document_id),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("soft delete — status set to entered-in-error".to_string()),
        },
    )?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Survey Template commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a custom survey template.
///
/// RBAC: Provider, SystemAdmin (create).
#[tauri::command]
pub fn create_survey_template(
    input: SurveyTemplateInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<SurveyTemplateRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Create)?;

    if input.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".to_string()));
    }
    if input.fields.is_empty() {
        return Err(AppError::Validation(
            "At least one field is required".to_string(),
        ));
    }

    // Validate field types
    for field in &input.fields {
        let _ = SurveyFieldType::from_str(&field.field_type)?;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let field_count = input.fields.len() as i32;

    let fhir = build_survey_template_fhir(&id, &input.name, &input.fields, false, &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Questionnaire', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO survey_template_index (template_id, resource_id, name, is_builtin, field_count, created_at, updated_at)
         VALUES (?1, ?1, ?2, 0, ?3, ?4, ?4)",
        rusqlite::params![id, input.name, field_count, now],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "create_survey_template".to_string(),
            resource_type: "Questionnaire".to_string(),
            resource_id: Some(id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("template: {}, fields: {}", input.name, field_count)),
        },
    )?;

    Ok(SurveyTemplateRecord {
        template_id: id,
        name: input.name,
        is_builtin: false,
        field_count,
        fields: input.fields,
        created_at: now.clone(),
        updated_at: now,
        resource: fhir,
    })
}

/// List all survey templates including built-in ones.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn list_survey_templates(
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<SurveyTemplateRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Ensure built-in templates exist
    ensure_builtin_templates(&conn)?;

    let rows: Vec<SurveyTemplateRecord> = conn
        .prepare(
            "SELECT t.template_id, t.name, t.is_builtin, t.field_count, t.created_at, t.updated_at, r.resource \
             FROM survey_template_index t \
             JOIN fhir_resources r ON r.id = t.template_id \
             ORDER BY t.is_builtin DESC, t.name ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(tid, name, builtin, fc, ca, ua, res_str)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            let fields = extract_fields_from_fhir(&resource);
            SurveyTemplateRecord {
                template_id: tid,
                name,
                is_builtin: builtin != 0,
                field_count: fc,
                fields,
                created_at: ca,
                updated_at: ua,
                resource,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_survey_templates".to_string(),
            resource_type: "Questionnaire".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("count: {}", rows.len())),
        },
    )?;

    Ok(rows)
}

/// Get a single survey template by ID.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn get_survey_template(
    template_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<SurveyTemplateRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Ensure built-in templates exist
    ensure_builtin_templates(&conn)?;

    let (tid, name, builtin, fc, ca, ua, res_str): (String, String, i64, i32, String, String, String) = conn
        .query_row(
            "SELECT t.template_id, t.name, t.is_builtin, t.field_count, t.created_at, t.updated_at, r.resource \
             FROM survey_template_index t \
             JOIN fhir_resources r ON r.id = t.template_id \
             WHERE t.template_id = ?1",
            rusqlite::params![template_id],
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
        .map_err(|_| AppError::NotFound(format!("Survey template '{}' not found", template_id)))?;

    let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
    let fields = extract_fields_from_fhir(&resource);

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "get_survey_template".to_string(),
            resource_type: "Questionnaire".to_string(),
            resource_id: Some(tid.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(SurveyTemplateRecord {
        template_id: tid,
        name,
        is_builtin: builtin != 0,
        field_count: fc,
        fields,
        created_at: ca,
        updated_at: ua,
        resource,
    })
}

/// Submit a survey response. Stores the response and generates a JSON snapshot.
///
/// RBAC: Provider, NurseMa, SystemAdmin (create).
#[tauri::command]
pub fn submit_survey_response(
    input: SurveyResponseInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<SurveyResponseRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Create)?;

    if input.template_id.trim().is_empty() {
        return Err(AppError::Validation(
            "template_id is required".to_string(),
        ));
    }
    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Ensure built-in templates exist so the template can be found
    ensure_builtin_templates(&conn)?;

    // Verify template exists
    let template_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM survey_template_index WHERE template_id = ?1",
            rusqlite::params![input.template_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !template_exists {
        return Err(AppError::NotFound(format!(
            "Survey template '{}' not found",
            input.template_id
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir =
        build_survey_response_fhir(&id, &input.template_id, &input.patient_id, &input.responses, &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'QuestionnaireResponse', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO survey_response_index (response_id, resource_id, template_id, patient_id, completed_at)
         VALUES (?1, ?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.template_id, input.patient_id, now],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "submit_survey_response".to_string(),
            resource_type: "QuestionnaireResponse".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("template: {}", input.template_id)),
        },
    )?;

    Ok(SurveyResponseRecord {
        response_id: id,
        template_id: input.template_id,
        patient_id: input.patient_id,
        responses: input.responses,
        completed_at: now,
        resource: fhir,
    })
}

/// List all survey responses for a patient.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn list_survey_responses(
    patient_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<SurveyResponseRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows: Vec<SurveyResponseRecord> = conn
        .prepare(
            "SELECT s.response_id, s.template_id, s.patient_id, s.completed_at, r.resource \
             FROM survey_response_index s \
             JOIN fhir_resources r ON r.id = s.response_id \
             WHERE s.patient_id = ?1 \
             ORDER BY s.completed_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(rid, tid, pid, ca, res_str)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            let responses = resource["item"].clone();
            SurveyResponseRecord {
                response_id: rid,
                template_id: tid,
                patient_id: pid,
                responses,
                completed_at: ca,
                resource,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_survey_responses".to_string(),
            resource_type: "QuestionnaireResponse".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(rows)
}

/// Get a single survey response by ID.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn get_survey_response(
    response_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<SurveyResponseRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (rid, tid, pid, ca, res_str): (String, String, String, String, String) = conn
        .query_row(
            "SELECT s.response_id, s.template_id, s.patient_id, s.completed_at, r.resource \
             FROM survey_response_index s \
             JOIN fhir_resources r ON r.id = s.response_id \
             WHERE s.response_id = ?1",
            rusqlite::params![response_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("Survey response '{}' not found", response_id)))?;

    let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
    let responses = resource["item"].clone();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "get_survey_response".to_string(),
            resource_type: "QuestionnaireResponse".to_string(),
            resource_id: Some(rid.clone()),
            patient_id: Some(pid.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(SurveyResponseRecord {
        response_id: rid,
        template_id: tid,
        patient_id: pid,
        responses,
        completed_at: ca,
        resource,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Referral Tracking commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a referral record for a patient.
///
/// RBAC: Provider, NurseMa, SystemAdmin (create).
#[tauri::command]
pub fn create_referral(
    input: ReferralInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<ReferralRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Create)?;

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.referring_provider_name.trim().is_empty() {
        return Err(AppError::Validation(
            "referring_provider_name is required".to_string(),
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_referral_fhir(&id, &input, &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'ServiceRequest', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO referral_index (referral_id, resource_id, patient_id, referring_provider_name, referring_provider_npi, \
         practice_name, phone, fax, referral_date, authorized_visit_count, diagnosis_icd10, linked_document_id, created_at)
         VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            input.patient_id,
            input.referring_provider_name,
            input.referring_provider_npi,
            input.practice_name,
            input.phone,
            input.fax,
            input.referral_date,
            input.authorized_visit_count,
            input.diagnosis_icd10,
            input.linked_document_id,
            now
        ],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "create_referral".to_string(),
            resource_type: "ServiceRequest".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("provider: {}", input.referring_provider_name)),
        },
    )?;

    Ok(ReferralRecord {
        referral_id: id,
        patient_id: input.patient_id,
        referring_provider_name: input.referring_provider_name,
        referring_provider_npi: input.referring_provider_npi,
        practice_name: input.practice_name,
        phone: input.phone,
        fax: input.fax,
        referral_date: input.referral_date,
        authorized_visit_count: input.authorized_visit_count,
        diagnosis_icd10: input.diagnosis_icd10,
        linked_document_id: input.linked_document_id,
        notes: input.notes,
        created_at: now,
        resource: fhir,
    })
}

/// Get a single referral by ID.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn get_referral(
    referral_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<ReferralRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (rid, pid, rpn, rpnpi, pn, phone, fax, rd, avc, diag, ldid, ca, res_str): (
        String, String, String, Option<String>, Option<String>, Option<String>,
        Option<String>, Option<String>, Option<i32>, Option<String>, Option<String>,
        String, String,
    ) = conn
        .query_row(
            "SELECT ref.referral_id, ref.patient_id, ref.referring_provider_name, ref.referring_provider_npi, \
                    ref.practice_name, ref.phone, ref.fax, ref.referral_date, ref.authorized_visit_count, \
                    ref.diagnosis_icd10, ref.linked_document_id, ref.created_at, r.resource \
             FROM referral_index ref \
             JOIN fhir_resources r ON r.id = ref.referral_id \
             WHERE ref.referral_id = ?1",
            rusqlite::params![referral_id],
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
        .map_err(|_| AppError::NotFound(format!("Referral '{}' not found", referral_id)))?;

    let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
    let notes = resource["note"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|n| n["text"].as_str())
        .map(|s| s.to_string());

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "get_referral".to_string(),
            resource_type: "ServiceRequest".to_string(),
            resource_id: Some(rid.clone()),
            patient_id: Some(pid.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(ReferralRecord {
        referral_id: rid,
        patient_id: pid,
        referring_provider_name: rpn,
        referring_provider_npi: rpnpi,
        practice_name: pn,
        phone,
        fax,
        referral_date: rd,
        authorized_visit_count: avc,
        diagnosis_icd10: diag,
        linked_document_id: ldid,
        notes,
        created_at: ca,
        resource,
    })
}

/// List all referrals for a patient.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn list_referrals(
    patient_id: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<ReferralRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows: Vec<ReferralRecord> = conn
        .prepare(
            "SELECT ref.referral_id, ref.patient_id, ref.referring_provider_name, ref.referring_provider_npi, \
                    ref.practice_name, ref.phone, ref.fax, ref.referral_date, ref.authorized_visit_count, \
                    ref.diagnosis_icd10, ref.linked_document_id, ref.created_at, r.resource \
             FROM referral_index ref \
             JOIN fhir_resources r ON r.id = ref.referral_id \
             WHERE ref.patient_id = ?1 \
             ORDER BY ref.created_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i32>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(rid, pid, rpn, rpnpi, pn, phone, fax, rd, avc, diag, ldid, ca, res_str)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            let notes = resource["note"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|n| n["text"].as_str())
                .map(|s| s.to_string());
            ReferralRecord {
                referral_id: rid,
                patient_id: pid,
                referring_provider_name: rpn,
                referring_provider_npi: rpnpi,
                practice_name: pn,
                phone,
                fax,
                referral_date: rd,
                authorized_visit_count: avc,
                diagnosis_icd10: diag,
                linked_document_id: ldid,
                notes,
                created_at: ca,
                resource,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_referrals".to_string(),
            resource_type: "ServiceRequest".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(rows)
}

/// Update a referral record.
///
/// RBAC: Provider, NurseMa, SystemAdmin (update).
#[tauri::command]
pub fn update_referral(
    referral_id: String,
    input: ReferralInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<ReferralRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Update)?;

    if input.referring_provider_name.trim().is_empty() {
        return Err(AppError::Validation(
            "referring_provider_name is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    // Verify exists
    let (resource_str, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1",
            rusqlite::params![referral_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Referral '{}' not found", referral_id)))?;

    // Update index
    conn.execute(
        "UPDATE referral_index SET \
         referring_provider_name = ?1, referring_provider_npi = ?2, practice_name = ?3, \
         phone = ?4, fax = ?5, referral_date = ?6, authorized_visit_count = ?7, \
         diagnosis_icd10 = ?8, linked_document_id = ?9 \
         WHERE referral_id = ?10",
        rusqlite::params![
            input.referring_provider_name,
            input.referring_provider_npi,
            input.practice_name,
            input.phone,
            input.fax,
            input.referral_date,
            input.authorized_visit_count,
            input.diagnosis_icd10,
            input.linked_document_id,
            referral_id
        ],
    )?;

    // Build updated FHIR resource
    let fhir = build_referral_fhir(&referral_id, &input, &now);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![fhir_json, version_id + 1, now, referral_id],
    )?;

    // Suppress unused-variable warning; we read resource_str to confirm existence above.
    let _ = resource_str;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "update_referral".to_string(),
            resource_type: "ServiceRequest".to_string(),
            resource_id: Some(referral_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("provider: {}", input.referring_provider_name)),
        },
    )?;

    Ok(ReferralRecord {
        referral_id,
        patient_id: input.patient_id,
        referring_provider_name: input.referring_provider_name,
        referring_provider_npi: input.referring_provider_npi,
        practice_name: input.practice_name,
        phone: input.phone,
        fax: input.fax,
        referral_date: input.referral_date,
        authorized_visit_count: input.authorized_visit_count,
        diagnosis_icd10: input.diagnosis_icd10,
        linked_document_id: input.linked_document_id,
        notes: input.notes,
        created_at: now,
        resource: fhir,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Ensure the 3 built-in survey templates exist in the database.
/// Inserts them if missing — idempotent.
fn ensure_builtin_templates(conn: &rusqlite::Connection) -> Result<(), AppError> {
    for (id, name, fields, is_builtin) in builtin_survey_templates() {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM survey_template_index WHERE template_id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !exists {
            let now = chrono::Utc::now().to_rfc3339();
            let field_count = fields.len() as i32;
            let fhir = build_survey_template_fhir(&id, &name, &fields, is_builtin, &now);
            let fhir_json = serde_json::to_string(&fhir)
                .map_err(|e| AppError::Serialization(e.to_string()))?;

            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
                 VALUES (?1, 'Questionnaire', ?2, 1, ?3, ?3, ?3)",
                rusqlite::params![id, fhir_json, now],
            )?;
            conn.execute(
                "INSERT INTO survey_template_index (template_id, resource_id, name, is_builtin, field_count, created_at, updated_at)
                 VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![
                    id,
                    name,
                    if is_builtin { 1 } else { 0 },
                    field_count,
                    now
                ],
            )?;
        }
    }
    Ok(())
}

/// Extract SurveyField vec from FHIR Questionnaire JSON.
fn extract_fields_from_fhir(resource: &serde_json::Value) -> Vec<SurveyField> {
    resource["item"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let order = item["extension"]
                        .as_array()
                        .and_then(|exts| {
                            exts.iter().find(|e| {
                                e["url"].as_str()
                                    == Some("http://medarc.local/fhir/ext/field-order")
                            })
                        })
                        .and_then(|e| e["valueInteger"].as_i64())
                        .unwrap_or(0) as i32;

                    SurveyField {
                        field_id: item["linkId"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        field_type: item["type"]
                            .as_str()
                            .unwrap_or("text")
                            .to_string(),
                        label: item["text"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        required: item["required"].as_bool().unwrap_or(false),
                        order,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_category_validation() {
        // Valid categories
        assert!(DocumentCategory::from_str("referral_rx").is_ok());
        assert!(DocumentCategory::from_str("imaging").is_ok());
        assert!(DocumentCategory::from_str("consent_forms").is_ok());
        assert!(DocumentCategory::from_str("intake_surveys").is_ok());
        assert!(DocumentCategory::from_str("insurance").is_ok());
        assert!(DocumentCategory::from_str("legal").is_ok());
        assert!(DocumentCategory::from_str("home_exercise_program").is_ok());
        assert!(DocumentCategory::from_str("other").is_ok());

        // Invalid categories
        assert!(DocumentCategory::from_str("invalid").is_err());
        assert!(DocumentCategory::from_str("").is_err());
        assert!(DocumentCategory::from_str("clinical-note").is_err());

        // Roundtrip
        for cat_str in DocumentCategory::all_values() {
            let cat = DocumentCategory::from_str(cat_str).unwrap();
            assert_eq!(cat.as_str(), *cat_str);
        }
    }

    #[test]
    fn survey_field_type_serialization() {
        // Valid types
        assert_eq!(
            SurveyFieldType::from_str("text").unwrap(),
            SurveyFieldType::Text
        );
        assert_eq!(
            SurveyFieldType::from_str("number").unwrap(),
            SurveyFieldType::Number
        );
        assert_eq!(
            SurveyFieldType::from_str("yes_no").unwrap(),
            SurveyFieldType::YesNo
        );
        assert_eq!(
            SurveyFieldType::from_str("pain_scale").unwrap(),
            SurveyFieldType::PainScale
        );
        assert_eq!(
            SurveyFieldType::from_str("date").unwrap(),
            SurveyFieldType::Date
        );

        // Invalid type
        assert!(SurveyFieldType::from_str("checkbox").is_err());
        assert!(SurveyFieldType::from_str("").is_err());

        // Roundtrip
        let types = [
            SurveyFieldType::Text,
            SurveyFieldType::Number,
            SurveyFieldType::YesNo,
            SurveyFieldType::PainScale,
            SurveyFieldType::Date,
        ];
        for t in &types {
            assert_eq!(SurveyFieldType::from_str(t.as_str()).unwrap(), *t);
        }

        // Serde serialization
        let field = SurveyField {
            field_id: "test-1".to_string(),
            field_type: "pain_scale".to_string(),
            label: "Pain Rating".to_string(),
            required: true,
            order: 1,
        };
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"fieldType\":\"pain_scale\""));
        assert!(json.contains("\"fieldId\":\"test-1\""));

        // Deserialization
        let deserialized: SurveyField = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.field_id, "test-1");
        assert_eq!(deserialized.field_type, "pain_scale");
    }

    #[test]
    fn referral_record_creation() {
        let input = ReferralInput {
            patient_id: "pat-001".to_string(),
            referring_provider_name: "Dr. Smith".to_string(),
            referring_provider_npi: Some("1234567890".to_string()),
            practice_name: Some("Smith Orthopedics".to_string()),
            phone: Some("555-0100".to_string()),
            fax: Some("555-0101".to_string()),
            referral_date: Some("2026-03-01".to_string()),
            authorized_visit_count: Some(12),
            diagnosis_icd10: Some("M54.5".to_string()),
            linked_document_id: None,
            notes: Some("Post-surgical rehab".to_string()),
        };

        let fhir = build_referral_fhir("ref-001", &input, "2026-03-01T12:00:00Z");

        assert_eq!(fhir["resourceType"], "ServiceRequest");
        assert_eq!(fhir["id"], "ref-001");
        assert_eq!(fhir["status"], "active");
        assert_eq!(
            fhir["subject"]["reference"],
            "Patient/pat-001"
        );
        assert_eq!(
            fhir["requester"]["display"],
            "Dr. Smith"
        );

        // Build the record
        let record = ReferralRecord {
            referral_id: "ref-001".to_string(),
            patient_id: input.patient_id.clone(),
            referring_provider_name: input.referring_provider_name.clone(),
            referring_provider_npi: input.referring_provider_npi.clone(),
            practice_name: input.practice_name.clone(),
            phone: input.phone.clone(),
            fax: input.fax.clone(),
            referral_date: input.referral_date.clone(),
            authorized_visit_count: input.authorized_visit_count,
            diagnosis_icd10: input.diagnosis_icd10.clone(),
            linked_document_id: input.linked_document_id.clone(),
            notes: input.notes.clone(),
            created_at: "2026-03-01T12:00:00Z".to_string(),
            resource: fhir,
        };

        // Verify serialization roundtrip
        let json_str = serde_json::to_string(&record).unwrap();
        let deserialized: ReferralRecord = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.referral_id, "ref-001");
        assert_eq!(deserialized.authorized_visit_count, Some(12));
        assert_eq!(deserialized.diagnosis_icd10, Some("M54.5".to_string()));
    }

    #[test]
    fn builtin_templates_loaded() {
        let templates = builtin_survey_templates();
        assert_eq!(templates.len(), 3);

        // Pain and Function Intake
        let (id, name, fields, is_builtin) = &templates[0];
        assert_eq!(id, "builtin-pain-function-intake");
        assert_eq!(name, "Pain and Function Intake");
        assert!(is_builtin);
        assert_eq!(fields.len(), 8);
        assert_eq!(fields[0].field_id, "pfi-name");
        assert_eq!(fields[0].field_type, "text");
        assert!(fields[0].required);
        assert_eq!(fields[4].field_type, "pain_scale");

        // Medical History
        let (id, name, fields, is_builtin) = &templates[1];
        assert_eq!(id, "builtin-medical-history");
        assert_eq!(name, "Medical History");
        assert!(is_builtin);
        assert_eq!(fields.len(), 5);

        // HIPAA Acknowledgment
        let (id, name, fields, is_builtin) = &templates[2];
        assert_eq!(id, "builtin-hipaa-acknowledgment");
        assert_eq!(name, "HIPAA Acknowledgment");
        assert!(is_builtin);
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[2].field_type, "yes_no");
        assert_eq!(fields[3].field_type, "yes_no");
        assert!(fields[3].required);

        // All builtin templates should produce valid FHIR
        for (id, name, fields, is_builtin) in &templates {
            let fhir = build_survey_template_fhir(id, name, fields, *is_builtin, "2026-01-01T00:00:00Z");
            assert_eq!(fhir["resourceType"], "Questionnaire");
            assert_eq!(fhir["status"], "active");
            assert_eq!(fhir["name"], name.as_str());
            let items = fhir["item"].as_array().unwrap();
            assert_eq!(items.len(), fields.len());
        }
    }

    #[test]
    fn document_fhir_builder() {
        let input = CategorizedDocumentInput {
            patient_id: "pat-001".to_string(),
            category: "referral_rx".to_string(),
            file_name: "referral.pdf".to_string(),
            file_data_base64: String::new(),
            mime_type: "application/pdf".to_string(),
        };

        let fhir = build_categorized_document_fhir(
            "doc-001",
            &input,
            "abc123",
            1024,
            "2026-03-01T12:00:00Z",
        );

        assert_eq!(fhir["resourceType"], "DocumentReference");
        assert_eq!(fhir["id"], "doc-001");
        assert_eq!(fhir["status"], "current");
        assert_eq!(fhir["subject"]["reference"], "Patient/pat-001");
        assert_eq!(
            fhir["category"][0]["coding"][0]["code"],
            "referral_rx"
        );
        assert_eq!(
            fhir["content"][0]["attachment"]["contentType"],
            "application/pdf"
        );
    }
}
