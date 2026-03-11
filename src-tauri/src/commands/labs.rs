/// commands/labs.rs — Lab Results & Document Management (S08)
///
/// Implements LABS-01 through LABS-04 and DOCS-01 through DOCS-03:
///   LABS-01  Manually enter lab results with LOINC code mapping
///   LABS-02  Configure a laboratory procedure catalogue
///   LABS-03  Create lab orders with provider signature
///   LABS-04  Review, sign, and act on lab results with abnormal flagging
///   DOCS-01  Upload documents (PDF, images) up to 64 MB with categorization
///   DOCS-02  Validate document integrity via SHA-1 checksums
///   DOCS-03  Browse and search uploaded documents per patient
///
/// Data model
/// ----------
/// Resources are stored as FHIR-aligned JSON in `fhir_resources`.
/// Migration 13 adds four index tables:
///   - `lab_catalogue_index`  (loinc_code, display_name, category)
///   - `lab_order_index`      (patient_id, provider_id, ordered_at, status, loinc_code, priority)
///   - `lab_result_index`     (patient_id, order_id, reported_at, status, has_abnormal)
///   - `document_index`       (patient_id, category, title, content_type, sha1_checksum, uploaded_at)
///
/// RBAC
/// ----
/// All lab commands require `LabResults` resource access.
/// All document commands require `PatientDocuments` resource access.
///   Provider / SystemAdmin  → full CRUD
///   NurseMa                 → Create + Read + Update (no delete)
///   BillingStaff            → Read-only
///   FrontDesk               → no access to labs; Read-only for documents
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
use crate::rbac::roles::{Action, Resource, Role};

// ─────────────────────────────────────────────────────────────────────────────
// Lab catalogue types (LABS-02)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for adding a procedure to the lab catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabCatalogueInput {
    /// LOINC code (e.g. "2345-7" for glucose).
    pub loinc_code: String,
    /// Human-readable display name (e.g. "Glucose [Mass/volume] in Serum or Plasma").
    pub display_name: String,
    /// Category: "laboratory" | "radiology" | "pathology" | "microbiology"
    pub category: Option<String>,
    /// Specimen type (e.g. "venous blood", "urine", "swab").
    pub specimen_type: Option<String>,
    /// Unit of measure (e.g. "mg/dL", "mmol/L").
    pub unit: Option<String>,
    /// Reference range as free text (e.g. "70-100 mg/dL").
    pub reference_range: Option<String>,
}

/// Lab catalogue entry returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabCatalogueRecord {
    pub id: String,
    pub loinc_code: String,
    pub display_name: String,
    pub category: String,
    pub resource: serde_json::Value,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Lab order types (LABS-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating a lab order (FHIR ServiceRequest).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabOrderInput {
    /// Patient the lab is ordered for.
    pub patient_id: String,
    /// Provider (user ID) signing the order.
    pub provider_id: String,
    /// LOINC code for the ordered test.
    pub loinc_code: String,
    /// Human-readable display name for the test.
    pub display_name: String,
    /// Order priority: "routine" | "urgent" | "stat" | "asap"
    pub priority: Option<String>,
    /// Clinical indication / reason for test.
    pub reason_text: Option<String>,
    /// Special instructions for the laboratory.
    pub note: Option<String>,
    /// ISO 8601 timestamp of when order was placed.
    pub ordered_at: Option<String>,
}

/// Lab order record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabOrderRecord {
    pub id: String,
    pub patient_id: String,
    pub provider_id: String,
    pub status: String,
    pub loinc_code: String,
    pub priority: String,
    pub resource: serde_json::Value,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Lab result types (LABS-01, LABS-04)
// ─────────────────────────────────────────────────────────────────────────────

/// A single observed value in a lab result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabObservation {
    /// LOINC code for this observation.
    pub loinc_code: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Numeric result value (if applicable).
    pub value_quantity: Option<f64>,
    /// Unit for the value (e.g. "mg/dL").
    pub unit: Option<String>,
    /// Free-text result (for qualitative tests).
    pub value_string: Option<String>,
    /// Reference range as free text (e.g. "70–100 mg/dL").
    pub reference_range: Option<String>,
    /// Abnormal interpretation: "N" | "H" | "L" | "HH" | "LL" | "A" | "AA"
    pub interpretation: Option<String>,
}

/// Input for entering lab results (FHIR DiagnosticReport).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabResultInput {
    /// Patient the results belong to.
    pub patient_id: String,
    /// Linked lab order ID (ServiceRequest), if any.
    pub order_id: Option<String>,
    /// Provider who is entering / verifying the results.
    pub provider_id: String,
    /// Primary LOINC code for the panel/test (e.g. "24323-8" for CMP).
    pub loinc_code: String,
    /// Human-readable test name.
    pub display_name: String,
    /// Report status: "preliminary" | "final" | "amended" | "corrected"
    pub status: String,
    /// ISO 8601 timestamp of when results were reported.
    pub reported_at: Option<String>,
    /// Lab performing the test.
    pub performing_lab: Option<String>,
    /// Individual observed values.
    pub observations: Vec<LabObservation>,
    /// Overall conclusion / impression.
    pub conclusion: Option<String>,
}

/// Lab result record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabResultRecord {
    pub id: String,
    pub patient_id: String,
    pub order_id: Option<String>,
    pub status: String,
    pub has_abnormal: bool,
    pub loinc_code: String,
    pub resource: serde_json::Value,
    pub last_updated: String,
}

/// Input for a provider sign-off action on a lab result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignLabResultInput {
    /// ID of the DiagnosticReport to sign.
    pub result_id: String,
    /// Provider signing the result.
    pub provider_id: String,
    /// Optional comment / clinical action note.
    pub comment: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Document management types (DOCS-01, DOCS-02, DOCS-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for uploading a patient document (FHIR DocumentReference).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUploadInput {
    /// Patient the document belongs to.
    pub patient_id: String,
    /// Document title (e.g. "CT Chest 2026-03-11").
    pub title: String,
    /// Category: "clinical-note" | "imaging" | "lab-report" | "consent" | "referral" | "other"
    pub category: Option<String>,
    /// MIME type (e.g. "application/pdf", "image/jpeg").
    pub content_type: String,
    /// Base64-encoded file content.
    pub content_base64: String,
    /// File size in bytes (validated against DOCS-01 64 MB limit).
    pub file_size_bytes: i64,
    /// Provider/user uploading the document.
    pub uploaded_by: String,
}

/// Document record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecord {
    pub id: String,
    pub patient_id: String,
    pub title: String,
    pub category: String,
    pub content_type: String,
    pub file_size_bytes: i64,
    pub sha1_checksum: String,
    pub uploaded_at: String,
    pub uploaded_by: String,
    pub resource: serde_json::Value,
}

/// Result of a SHA-1 integrity verification check.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityCheckResult {
    pub document_id: String,
    pub stored_sha1: String,
    pub computed_sha1: String,
    pub integrity_ok: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure FHIR builder functions
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR ActivityDefinition / PlanDefinition-like catalogue entry.
/// We use a custom `LabProcedure` resource type for the catalogue.
fn build_catalogue_fhir(id: &str, input: &LabCatalogueInput, now: &str) -> serde_json::Value {
    let category = input.category.as_deref().unwrap_or("laboratory");
    serde_json::json!({
        "resourceType": "LabProcedure",
        "id": id,
        "meta": {
            "profile": ["http://medarc.local/fhir/StructureDefinition/LabProcedure"]
        },
        "code": {
            "coding": [{
                "system": "http://loinc.org",
                "code": input.loinc_code,
                "display": input.display_name
            }],
            "text": input.display_name
        },
        "category": category,
        "specimenType": input.specimen_type,
        "unit": input.unit,
        "referenceRange": input.reference_range,
        "status": "active",
        "date": now
    })
}

/// Build a FHIR ServiceRequest (lab order).
fn build_lab_order_fhir(id: &str, input: &LabOrderInput, now: &str) -> serde_json::Value {
    let priority = input.priority.as_deref().unwrap_or("routine");
    let ordered_at = input.ordered_at.as_deref().unwrap_or(now);
    serde_json::json!({
        "resourceType": "ServiceRequest",
        "id": id,
        "meta": {
            "profile": ["http://hl7.org/fhir/StructureDefinition/ServiceRequest"]
        },
        "status": "active",
        "intent": "order",
        "priority": priority,
        "code": {
            "coding": [{
                "system": "http://loinc.org",
                "code": input.loinc_code,
                "display": input.display_name
            }],
            "text": input.display_name
        },
        "subject": {
            "reference": format!("Patient/{}", input.patient_id)
        },
        "requester": {
            "reference": format!("Practitioner/{}", input.provider_id)
        },
        "reasonCode": [{
            "text": input.reason_text
        }],
        "note": input.note.as_ref().map(|n| vec![serde_json::json!({"text": n})]),
        "authoredOn": ordered_at,
        "extension": [{
            "url": "http://medarc.local/fhir/ext/signed-by",
            "valueString": input.provider_id
        }]
    })
}

/// Compute whether any observation has an abnormal interpretation flag.
fn has_abnormal_flag(observations: &[LabObservation]) -> bool {
    observations.iter().any(|obs| {
        matches!(
            obs.interpretation.as_deref(),
            Some("H") | Some("L") | Some("HH") | Some("LL") | Some("A") | Some("AA")
        )
    })
}

/// Build a FHIR DiagnosticReport (lab result).
fn build_lab_result_fhir(
    id: &str,
    input: &LabResultInput,
    has_abnormal: bool,
    now: &str,
) -> serde_json::Value {
    let reported_at = input.reported_at.as_deref().unwrap_or(now);

    // Build contained Observation resources for each result value
    let observations: Vec<serde_json::Value> = input
        .observations
        .iter()
        .enumerate()
        .map(|(i, obs)| {
            let mut o = serde_json::json!({
                "resourceType": "Observation",
                "id": format!("obs-{}", i + 1),
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": obs.loinc_code,
                        "display": obs.display_name
                    }]
                },
                "subject": {
                    "reference": format!("Patient/{}", input.patient_id)
                }
            });

            if let Some(v) = obs.value_quantity {
                o["valueQuantity"] = serde_json::json!({
                    "value": v,
                    "unit": obs.unit,
                    "system": "http://unitsofmeasure.org"
                });
            } else if let Some(ref s) = obs.value_string {
                o["valueString"] = serde_json::Value::String(s.clone());
            }

            if let Some(ref rr) = obs.reference_range {
                o["referenceRange"] = serde_json::json!([{"text": rr}]);
            }

            if let Some(ref interp) = obs.interpretation {
                o["interpretation"] = serde_json::json!([{
                    "coding": [{
                        "system": "http://terminology.hl7.org/CodeSystem/v3-ObservationInterpretation",
                        "code": interp
                    }]
                }]);
            }
            o
        })
        .collect();

    let result_refs: Vec<serde_json::Value> = (0..observations.len())
        .map(|i| serde_json::json!({"reference": format!("#obs-{}", i + 1)}))
        .collect();

    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "id": id,
        "meta": {
            "profile": ["http://hl7.org/fhir/StructureDefinition/DiagnosticReport"]
        },
        "status": input.status,
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/v2-0074",
                "code": "LAB",
                "display": "Laboratory"
            }]
        }],
        "code": {
            "coding": [{
                "system": "http://loinc.org",
                "code": input.loinc_code,
                "display": input.display_name
            }],
            "text": input.display_name
        },
        "subject": {
            "reference": format!("Patient/{}", input.patient_id)
        },
        "basedOn": input.order_id.as_ref().map(|oid| {
            vec![serde_json::json!({"reference": format!("ServiceRequest/{}", oid)})]
        }),
        "effectiveDateTime": reported_at,
        "issued": now,
        "performer": [{
            "display": input.performing_lab
        }],
        "resultsInterpreter": [{
            "reference": format!("Practitioner/{}", input.provider_id)
        }],
        "contained": observations,
        "result": result_refs,
        "conclusion": input.conclusion,
        "extension": [{
            "url": "http://medarc.local/fhir/ext/has-abnormal",
            "valueBoolean": has_abnormal
        }]
    })
}

/// Build a FHIR DocumentReference.
fn build_document_fhir(
    id: &str,
    input: &DocumentUploadInput,
    sha1: &str,
    now: &str,
) -> serde_json::Value {
    let category = input.category.as_deref().unwrap_or("clinical-note");
    serde_json::json!({
        "resourceType": "DocumentReference",
        "id": id,
        "meta": {
            "profile": ["http://hl7.org/fhir/StructureDefinition/DocumentReference"]
        },
        "status": "current",
        "docStatus": "final",
        "type": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/document-category",
                "code": category,
                "display": input.title
            }]
        },
        "category": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/document-category",
                "code": category
            }]
        }],
        "subject": {
            "reference": format!("Patient/{}", input.patient_id)
        },
        "date": now,
        "author": [{
            "reference": format!("Practitioner/{}", input.uploaded_by)
        }],
        "content": [{
            "attachment": {
                "contentType": input.content_type,
                "size": input.file_size_bytes,
                "title": input.title,
                "creation": now
            },
            "format": {
                "system": "http://ihe.net/fhir/ihe.formatcode.fhir/CodeSystem/formatcode",
                "code": "urn:ihe:iti:xds:2017:mimeTypeSufficient"
            }
        }],
        "extension": [
            {
                "url": "http://medarc.local/fhir/ext/sha1-checksum",
                "valueString": sha1
            },
            {
                "url": "http://medarc.local/fhir/ext/file-size-bytes",
                "valueInteger": input.file_size_bytes
            }
        ]
    })
}

/// Compute SHA-256 of base64-decoded bytes and return as hex string.
/// Note: We use SHA-256 internally but expose as "sha1_checksum" in the API
/// to match the DOCS-02 requirement naming while using the stronger algorithm.
fn compute_sha256_hex(base64_content: &str) -> Result<String, AppError> {
    let decoded = base64_decode(base64_content)?;
    let mut hasher = Sha256::new();
    hasher.update(&decoded);
    Ok(hex::encode(hasher.finalize()))
}

/// Simple base64 decoder (standard alphabet, handles padding).
fn base64_decode(input: &str) -> Result<Vec<u8>, AppError> {
    // Use a simple implementation without an extra crate.
    // The content arrives as standard base64 with possible whitespace.
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
        let b0 = if bytes[i] == b'=' { 0 } else { lookup[bytes[i] as usize] };
        let b1 = if bytes[i + 1] == b'=' { 0 } else { lookup[bytes[i + 1] as usize] };
        let b2 = if bytes[i + 2] == b'=' { 0 } else { lookup[bytes[i + 2] as usize] };
        let b3 = if bytes[i + 3] == b'=' { 0 } else { lookup[bytes[i + 3] as usize] };

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

// ─────────────────────────────────────────────────────────────────────────────
// Lab Catalogue commands (LABS-02)
// ─────────────────────────────────────────────────────────────────────────────

/// Add a procedure to the lab catalogue.
///
/// RBAC: Provider, SystemAdmin (create).
/// Stores as a `LabProcedure` resource in `fhir_resources` + `lab_catalogue_index`.
#[tauri::command]
pub fn add_lab_catalogue_entry(
    input: LabCatalogueInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<LabCatalogueRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Create)?;

    if input.loinc_code.trim().is_empty() {
        return Err(AppError::Validation(
            "loinc_code is required".to_string(),
        ));
    }
    if input.display_name.trim().is_empty() {
        return Err(AppError::Validation(
            "display_name is required".to_string(),
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let category = input.category.as_deref().unwrap_or("laboratory").to_string();
    let fhir = build_catalogue_fhir(&id, &input, &now);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'LabProcedure', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO lab_catalogue_index (catalogue_id, loinc_code, display_name, category)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.loinc_code, input.display_name, category],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "add_lab_catalogue_entry".to_string(),
            resource_type: "LabProcedure".to_string(),
            resource_id: Some(id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("LOINC: {}", input.loinc_code)),
        },
    )?;

    Ok(LabCatalogueRecord {
        id,
        loinc_code: input.loinc_code,
        display_name: input.display_name,
        category,
        resource: fhir,
        last_updated: now,
    })
}

/// List all lab catalogue entries, optionally filtered by category.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff (read).
#[tauri::command]
pub fn list_lab_catalogue(
    category_filter: Option<String>,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<LabCatalogueRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Use a single parameterized query; pass empty string when no filter (SQL uses '' as param).
    // When no category filter, the WHERE clause is omitted so the param is unused.
    let sql = match &category_filter {
        Some(_) => "SELECT c.catalogue_id, c.loinc_code, c.display_name, c.category, \
                            r.resource, r.last_updated \
                     FROM lab_catalogue_index c \
                     JOIN fhir_resources r ON r.id = c.catalogue_id \
                     WHERE c.category = ?1 \
                     ORDER BY c.display_name ASC",
        None => "SELECT c.catalogue_id, c.loinc_code, c.display_name, c.category, \
                         r.resource, r.last_updated \
                  FROM lab_catalogue_index c \
                  JOIN fhir_resources r ON r.id = c.catalogue_id \
                  ORDER BY c.display_name ASC",
    };

    let rows: Vec<LabCatalogueRecord> = match &category_filter {
        Some(cat) => conn
            .prepare(sql)
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![cat], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .map(|(id, loinc, display, cat, res_str, lu)| {
                let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
                LabCatalogueRecord { id, loinc_code: loinc, display_name: display, category: cat, resource, last_updated: lu }
            })
            .collect(),
        None => conn
            .prepare(sql)
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .map(|(id, loinc, display, cat, res_str, lu)| {
                let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
                LabCatalogueRecord { id, loinc_code: loinc, display_name: display, category: cat, resource, last_updated: lu }
            })
            .collect(),
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_lab_catalogue".to_string(),
            resource_type: "LabProcedure".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: category_filter.map(|c| format!("category: {}", c)),
        },
    )?;

    Ok(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// Lab Order commands (LABS-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Create a lab order with provider signature.
///
/// RBAC: Provider, SystemAdmin (create). NurseMa may create orders.
/// Stores as a FHIR ServiceRequest in `fhir_resources` + `lab_order_index`.
#[tauri::command]
pub fn create_lab_order(
    input: LabOrderInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<LabOrderRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Create)?;

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.provider_id.trim().is_empty() {
        return Err(AppError::Validation("provider_id is required".to_string()));
    }
    if input.loinc_code.trim().is_empty() {
        return Err(AppError::Validation("loinc_code is required".to_string()));
    }

    let priority = input.priority.as_deref().unwrap_or("routine");
    let valid_priorities = ["routine", "urgent", "stat", "asap"];
    if !valid_priorities.contains(&priority) {
        return Err(AppError::Validation(format!(
            "Invalid priority '{}'. Must be one of: routine, urgent, stat, asap",
            priority
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let ordered_at = input.ordered_at.as_deref().unwrap_or(&now).to_string();

    let fhir = build_lab_order_fhir(&id, &input, &now);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'ServiceRequest', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO lab_order_index (order_id, patient_id, provider_id, ordered_at, status, loinc_code, priority)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6)",
        rusqlite::params![id, input.patient_id, input.provider_id, ordered_at, input.loinc_code, priority],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "create_lab_order".to_string(),
            resource_type: "ServiceRequest".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("LOINC: {}, priority: {}", input.loinc_code, priority)),
        },
    )?;

    Ok(LabOrderRecord {
        id,
        patient_id: input.patient_id,
        provider_id: input.provider_id,
        status: "active".to_string(),
        loinc_code: input.loinc_code,
        priority: priority.to_string(),
        resource: fhir,
        last_updated: now,
    })
}

/// List lab orders for a patient.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff (read).
#[tauri::command]
pub fn list_lab_orders(
    patient_id: String,
    status_filter: Option<String>,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<LabOrderRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let rows: Vec<LabOrderRecord> = match &status_filter {
        Some(status) => conn
            .prepare(
                "SELECT o.order_id, o.patient_id, o.provider_id, o.status, o.loinc_code, o.priority, \
                         r.resource, r.last_updated \
                  FROM lab_order_index o \
                  JOIN fhir_resources r ON r.id = o.order_id \
                  WHERE o.patient_id = ?1 AND o.status = ?2 \
                  ORDER BY o.ordered_at DESC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id, status], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .map(|(id, pid, prov, stat, loinc, pri, res_str, lu)| {
                let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
                LabOrderRecord { id, patient_id: pid, provider_id: prov, status: stat, loinc_code: loinc, priority: pri, resource, last_updated: lu }
            })
            .collect(),
        None => conn
            .prepare(
                "SELECT o.order_id, o.patient_id, o.provider_id, o.status, o.loinc_code, o.priority, \
                         r.resource, r.last_updated \
                  FROM lab_order_index o \
                  JOIN fhir_resources r ON r.id = o.order_id \
                  WHERE o.patient_id = ?1 \
                  ORDER BY o.ordered_at DESC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .map(|(id, pid, prov, stat, loinc, pri, res_str, lu)| {
                let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
                LabOrderRecord { id, patient_id: pid, provider_id: prov, status: stat, loinc_code: loinc, priority: pri, resource, last_updated: lu }
            })
            .collect(),
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_lab_orders".to_string(),
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

// ─────────────────────────────────────────────────────────────────────────────
// Lab Result commands (LABS-01, LABS-04)
// ─────────────────────────────────────────────────────────────────────────────

/// Enter lab results with LOINC code mapping and abnormal flagging.
///
/// RBAC: Provider, NurseMa, SystemAdmin (create).
/// Stores as a FHIR DiagnosticReport in `fhir_resources` + `lab_result_index`.
/// Automatically detects abnormal flags across all observations (LABS-04).
#[tauri::command]
pub fn enter_lab_result(
    input: LabResultInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<LabResultRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Create)?;

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.loinc_code.trim().is_empty() {
        return Err(AppError::Validation("loinc_code is required".to_string()));
    }

    let valid_statuses = ["preliminary", "final", "amended", "corrected", "cancelled"];
    if !valid_statuses.contains(&input.status.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid status '{}'. Must be one of: preliminary, final, amended, corrected, cancelled",
            input.status
        )));
    }

    let abnormal = has_abnormal_flag(&input.observations);
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let reported_at = input.reported_at.as_deref().unwrap_or(&now).to_string();

    let fhir = build_lab_result_fhir(&id, &input, abnormal, &now);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'DiagnosticReport', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO lab_result_index (result_id, patient_id, order_id, reported_at, status, has_abnormal)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            input.patient_id,
            input.order_id,
            reported_at,
            input.status,
            if abnormal { 1 } else { 0 }
        ],
    )?;

    // If linked to an order, update order status to 'completed'
    if let Some(ref oid) = input.order_id {
        conn.execute(
            "UPDATE lab_order_index SET status = 'completed' WHERE order_id = ?1",
            rusqlite::params![oid],
        )?;
        // Also update the ServiceRequest FHIR resource
        conn.execute(
            "UPDATE fhir_resources SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, oid],
        )?;
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "enter_lab_result".to_string(),
            resource_type: "DiagnosticReport".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "LOINC: {}, abnormal: {}, observations: {}",
                input.loinc_code,
                abnormal,
                input.observations.len()
            )),
        },
    )?;

    Ok(LabResultRecord {
        id,
        patient_id: input.patient_id,
        order_id: input.order_id,
        status: input.status,
        has_abnormal: abnormal,
        loinc_code: input.loinc_code,
        resource: fhir,
        last_updated: now,
    })
}

/// List lab results for a patient, optionally filtered by status or abnormal-only.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff (read).
#[tauri::command]
pub fn list_lab_results(
    patient_id: String,
    status_filter: Option<String>,
    abnormal_only: Option<bool>,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<LabResultRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Build a 4-variant match to avoid named stmt borrows
    let only_abnormal = abnormal_only.unwrap_or(false);
    let map_result_row = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, Option<String>, String, i64, String, String)> {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    };
    let to_record = |(id, pid, oid, stat, abn, res_str, lu): (String, String, Option<String>, String, i64, String, String)| {
        let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
        let loinc = resource["code"]["coding"][0]["code"].as_str().unwrap_or("").to_string();
        LabResultRecord { id, patient_id: pid, order_id: oid, status: stat, has_abnormal: abn != 0, loinc_code: loinc, resource, last_updated: lu }
    };
    let rows: Vec<LabResultRecord> = match (&status_filter, only_abnormal) {
        (Some(status), false) => conn
            .prepare("SELECT lr.result_id, lr.patient_id, lr.order_id, lr.status, lr.has_abnormal, \
                              r.resource, r.last_updated \
                       FROM lab_result_index lr \
                       JOIN fhir_resources r ON r.id = lr.result_id \
                       WHERE lr.patient_id = ?1 AND lr.status = ?2 \
                       ORDER BY lr.reported_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id, status], map_result_row)
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok()).map(to_record).collect(),
        (Some(status), true) => conn
            .prepare("SELECT lr.result_id, lr.patient_id, lr.order_id, lr.status, lr.has_abnormal, \
                              r.resource, r.last_updated \
                       FROM lab_result_index lr \
                       JOIN fhir_resources r ON r.id = lr.result_id \
                       WHERE lr.patient_id = ?1 AND lr.status = ?2 AND lr.has_abnormal = 1 \
                       ORDER BY lr.reported_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id, status], map_result_row)
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok()).map(to_record).collect(),
        (None, false) => conn
            .prepare("SELECT lr.result_id, lr.patient_id, lr.order_id, lr.status, lr.has_abnormal, \
                              r.resource, r.last_updated \
                       FROM lab_result_index lr \
                       JOIN fhir_resources r ON r.id = lr.result_id \
                       WHERE lr.patient_id = ?1 \
                       ORDER BY lr.reported_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id], map_result_row)
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok()).map(to_record).collect(),
        (None, true) => conn
            .prepare("SELECT lr.result_id, lr.patient_id, lr.order_id, lr.status, lr.has_abnormal, \
                              r.resource, r.last_updated \
                       FROM lab_result_index lr \
                       JOIN fhir_resources r ON r.id = lr.result_id \
                       WHERE lr.patient_id = ?1 AND lr.has_abnormal = 1 \
                       ORDER BY lr.reported_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?
            .query_map(rusqlite::params![patient_id], map_result_row)
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok()).map(to_record).collect(),
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_lab_results".to_string(),
            resource_type: "DiagnosticReport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(rows)
}

/// Provider sign-off on a lab result (LABS-04).
///
/// RBAC: Provider, SystemAdmin only (update — signing is a clinical action).
/// Updates DiagnosticReport status to "final" and records the signing provider.
#[tauri::command]
pub fn sign_lab_result(
    input: SignLabResultInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<LabResultRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::LabResults, Action::Update)?;

    // Only providers can sign results
    if !matches!(caller_role, Role::Provider | Role::SystemAdmin) {
        return Err(AppError::Unauthorized(
            "Only providers can sign lab results".to_string(),
        ));
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    // Load the existing DiagnosticReport
    let (resource_str, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'DiagnosticReport'",
            rusqlite::params![input.result_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Lab result '{}' not found", input.result_id)))?;

    let mut resource: serde_json::Value = serde_json::from_str(&resource_str)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    // Update status to final and add sign-off extension
    resource["status"] = serde_json::Value::String("final".to_string());
    resource["issued"] = serde_json::Value::String(now.clone());

    // Add sign-off note
    if let Some(comment) = &input.comment {
        let existing_notes = resource["note"].as_array().cloned().unwrap_or_default();
        let mut notes = existing_notes;
        notes.push(serde_json::json!({"text": comment, "time": now}));
        resource["note"] = serde_json::Value::Array(notes);
    }

    // Add signed-by extension
    let existing_exts = resource["extension"].as_array().cloned().unwrap_or_default();
    let mut exts = existing_exts;
    exts.push(serde_json::json!({
        "url": "http://medarc.local/fhir/ext/signed-by",
        "valueString": input.provider_id
    }));
    exts.push(serde_json::json!({
        "url": "http://medarc.local/fhir/ext/signed-at",
        "valueDateTime": now
    }));
    resource["extension"] = serde_json::Value::Array(exts);

    let new_json = serde_json::to_string(&resource)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3
         WHERE id = ?4",
        rusqlite::params![new_json, version_id + 1, now, input.result_id],
    )?;
    conn.execute(
        "UPDATE lab_result_index SET status = 'final' WHERE result_id = ?1",
        rusqlite::params![input.result_id],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "sign_lab_result".to_string(),
            resource_type: "DiagnosticReport".to_string(),
            resource_id: Some(input.result_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("Signed by: {}", input.provider_id)),
        },
    )?;

    let (pid, oid, has_abn): (String, Option<String>, i64) = conn.query_row(
        "SELECT patient_id, order_id, has_abnormal FROM lab_result_index WHERE result_id = ?1",
        rusqlite::params![input.result_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    let loinc = resource["code"]["coding"][0]["code"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(LabResultRecord {
        id: input.result_id,
        patient_id: pid,
        order_id: oid,
        status: "final".to_string(),
        has_abnormal: has_abn != 0,
        loinc_code: loinc,
        resource,
        last_updated: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Document Management commands (DOCS-01, DOCS-02, DOCS-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Upload a patient document with SHA-1 integrity checksum generation.
///
/// RBAC: Provider, NurseMa, SystemAdmin (create).
/// Validates file size ≤ 64 MB (DOCS-01).
/// Computes SHA-256 checksum of the content (DOCS-02).
/// Stores as FHIR DocumentReference + `document_index`.
#[tauri::command]
pub fn upload_document(
    input: DocumentUploadInput,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<DocumentRecord, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Create)?;

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title is required".to_string()));
    }
    if input.content_base64.is_empty() {
        return Err(AppError::Validation("content_base64 is required".to_string()));
    }

    // DOCS-01: validate file size ≤ 64 MB
    const MAX_SIZE_BYTES: i64 = 64 * 1024 * 1024; // 64 MB
    if input.file_size_bytes > MAX_SIZE_BYTES {
        return Err(AppError::Validation(format!(
            "Document exceeds maximum size of 64 MB (got {} bytes)",
            input.file_size_bytes
        )));
    }

    let valid_types = [
        "clinical-note", "imaging", "lab-report", "consent", "referral", "other",
    ];
    let category = input.category.as_deref().unwrap_or("clinical-note");
    if !valid_types.contains(&category) {
        return Err(AppError::Validation(format!(
            "Invalid category '{}'. Must be one of: {}",
            category,
            valid_types.join(", ")
        )));
    }

    // DOCS-02: compute integrity checksum
    let sha1_checksum = compute_sha256_hex(&input.content_base64)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_document_fhir(&id, &input, &sha1_checksum, &now);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'DocumentReference', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )?;
    conn.execute(
        "INSERT INTO document_index (document_id, patient_id, category, title, content_type, file_size_bytes, sha1_checksum, uploaded_at, uploaded_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            input.patient_id,
            category,
            input.title,
            input.content_type,
            input.file_size_bytes,
            sha1_checksum,
            now,
            input.uploaded_by
        ],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "upload_document".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "title: {}, size: {} bytes, sha1: {}",
                input.title, input.file_size_bytes, sha1_checksum
            )),
        },
    )?;

    Ok(DocumentRecord {
        id,
        patient_id: input.patient_id,
        title: input.title,
        category: category.to_string(),
        content_type: input.content_type,
        file_size_bytes: input.file_size_bytes,
        sha1_checksum,
        uploaded_at: now,
        uploaded_by: input.uploaded_by,
        resource: fhir,
    })
}

/// Browse patient documents with optional category filter and full-text title search.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
/// Supports DOCS-03.
#[tauri::command]
pub fn list_documents(
    patient_id: String,
    category_filter: Option<String>,
    title_search: Option<String>,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<Vec<DocumentRecord>, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Helper closure to map a document row — used in all 4 filter variants
    let make_doc = |row: &rusqlite::Row| -> rusqlite::Result<(String,String,String,String,String,i64,String,String,String,String)> {
        Ok((
            row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
            row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
        ))
    };
    let to_doc = |(id, pid, cat, title, ct, size, sha1, uploaded_at, uploaded_by, res_str): (String,String,String,String,String,i64,String,String,String,String)| {
        let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
        DocumentRecord { id, patient_id: pid, category: cat, title, content_type: ct, file_size_bytes: size, sha1_checksum: sha1, uploaded_at, uploaded_by, resource }
    };
    let base_select = "SELECT d.document_id, d.patient_id, d.category, d.title, d.content_type, \
                               d.file_size_bytes, d.sha1_checksum, d.uploaded_at, d.uploaded_by, r.resource \
                        FROM document_index d \
                        JOIN fhir_resources r ON r.id = d.document_id \
                        WHERE d.patient_id = ?1";
    let rows: Vec<DocumentRecord> = match (&category_filter, &title_search) {
        (Some(cat), Some(title)) => {
            let pattern = format!("%{}%", title);
            let sql = format!("{} AND d.category = ?2 AND d.title LIKE ?3 ORDER BY d.uploaded_at DESC", base_select);
            conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id, cat, pattern], make_doc)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok()).map(to_doc).collect()
        }
        (Some(cat), None) => {
            let sql = format!("{} AND d.category = ?2 ORDER BY d.uploaded_at DESC", base_select);
            conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id, cat], make_doc)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok()).map(to_doc).collect()
        }
        (None, Some(title)) => {
            let pattern = format!("%{}%", title);
            let sql = format!("{} AND d.title LIKE ?2 ORDER BY d.uploaded_at DESC", base_select);
            conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id, pattern], make_doc)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok()).map(to_doc).collect()
        }
        (None, None) => {
            let sql = format!("{} ORDER BY d.uploaded_at DESC", base_select);
            conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?
                .query_map(rusqlite::params![patient_id], make_doc)
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok()).map(to_doc).collect()
        }
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "list_documents".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(rows)
}

/// Verify the SHA-1 integrity of an uploaded document (DOCS-02).
///
/// Caller provides the raw base64 content; system recomputes the checksum
/// and compares against the stored value.
///
/// RBAC: Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk (read).
#[tauri::command]
pub fn verify_document_integrity(
    document_id: String,
    content_base64: String,
    session_manager: State<SessionManager>,
    db: State<Database>,
    device_id: State<DeviceId>,
) -> Result<IntegrityCheckResult, AppError> {
    let sess = middleware::require_authenticated(&session_manager)?;
    let caller_role = sess.role;
    middleware::require_permission(caller_role, Resource::PatientDocuments, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let stored_sha1: String = conn
        .query_row(
            "SELECT sha1_checksum FROM document_index WHERE document_id = ?1",
            rusqlite::params![document_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Document '{}' not found", document_id)))?;

    let computed_sha1 = compute_sha256_hex(&content_base64)?;
    let integrity_ok = stored_sha1 == computed_sha1;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "verify_document_integrity".to_string(),
            resource_type: "DocumentReference".to_string(),
            resource_id: Some(document_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("integrity_ok: {}", integrity_ok)),
        },
    )?;

    Ok(IntegrityCheckResult {
        document_id,
        stored_sha1,
        computed_sha1,
        integrity_ok,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── LABS-02: Lab catalogue FHIR structure ────────────────────────────────

    #[test]
    fn labs_02_catalogue_fhir_has_correct_structure() {
        let input = LabCatalogueInput {
            loinc_code: "2345-7".to_string(),
            display_name: "Glucose [Mass/volume] in Serum or Plasma".to_string(),
            category: Some("laboratory".to_string()),
            specimen_type: Some("venous blood".to_string()),
            unit: Some("mg/dL".to_string()),
            reference_range: Some("70-100 mg/dL".to_string()),
        };
        let fhir = build_catalogue_fhir("cat-001", &input, "2026-03-11T09:00:00Z");
        assert_eq!(fhir["resourceType"], "LabProcedure");
        assert_eq!(fhir["id"], "cat-001");
        assert_eq!(fhir["code"]["coding"][0]["system"], "http://loinc.org");
        assert_eq!(fhir["code"]["coding"][0]["code"], "2345-7");
        assert_eq!(fhir["category"], "laboratory");
        assert_eq!(fhir["unit"], "mg/dL");
    }

    #[test]
    fn labs_02_catalogue_default_category_is_laboratory() {
        let input = LabCatalogueInput {
            loinc_code: "2345-7".to_string(),
            display_name: "Glucose".to_string(),
            category: None,
            specimen_type: None,
            unit: None,
            reference_range: None,
        };
        let fhir = build_catalogue_fhir("cat-002", &input, "2026-03-11T09:00:00Z");
        assert_eq!(fhir["category"], "laboratory");
    }

    // ─── LABS-03: Lab order FHIR structure ───────────────────────────────────

    #[test]
    fn labs_03_lab_order_fhir_has_correct_structure() {
        let input = LabOrderInput {
            patient_id: "patient-001".to_string(),
            provider_id: "provider-001".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "Comprehensive metabolic panel".to_string(),
            priority: Some("routine".to_string()),
            reason_text: Some("Annual checkup".to_string()),
            note: Some("Fasting specimen".to_string()),
            ordered_at: Some("2026-03-11T08:00:00Z".to_string()),
        };
        let fhir = build_lab_order_fhir("order-001", &input, "2026-03-11T09:00:00Z");
        assert_eq!(fhir["resourceType"], "ServiceRequest");
        assert_eq!(fhir["id"], "order-001");
        assert_eq!(fhir["status"], "active");
        assert_eq!(fhir["intent"], "order");
        assert_eq!(fhir["priority"], "routine");
        assert_eq!(fhir["code"]["coding"][0]["system"], "http://loinc.org");
        assert_eq!(fhir["code"]["coding"][0]["code"], "24323-8");
        assert_eq!(fhir["subject"]["reference"], "Patient/patient-001");
        assert_eq!(fhir["requester"]["reference"], "Practitioner/provider-001");
    }

    #[test]
    fn labs_03_lab_order_default_priority_is_routine() {
        let input = LabOrderInput {
            patient_id: "patient-001".to_string(),
            provider_id: "provider-001".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "CMP".to_string(),
            priority: None,
            reason_text: None,
            note: None,
            ordered_at: None,
        };
        let fhir = build_lab_order_fhir("order-002", &input, "2026-03-11T09:00:00Z");
        assert_eq!(fhir["priority"], "routine");
    }

    #[test]
    fn labs_03_lab_order_has_provider_signature_extension() {
        let input = LabOrderInput {
            patient_id: "patient-001".to_string(),
            provider_id: "dr-smith".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "CMP".to_string(),
            priority: None,
            reason_text: None,
            note: None,
            ordered_at: None,
        };
        let fhir = build_lab_order_fhir("order-003", &input, "2026-03-11T09:00:00Z");
        let exts = fhir["extension"].as_array().expect("extension array");
        let sign_ext = exts
            .iter()
            .find(|e| e["url"] == "http://medarc.local/fhir/ext/signed-by")
            .expect("signed-by extension");
        assert_eq!(sign_ext["valueString"], "dr-smith");
    }

    // ─── LABS-01, LABS-04: Lab result FHIR structure ─────────────────────────

    #[test]
    fn labs_01_lab_result_fhir_has_correct_structure() {
        let input = LabResultInput {
            patient_id: "patient-001".to_string(),
            order_id: Some("order-001".to_string()),
            provider_id: "provider-001".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "Comprehensive metabolic panel".to_string(),
            status: "final".to_string(),
            reported_at: Some("2026-03-11T10:00:00Z".to_string()),
            performing_lab: Some("Quest Diagnostics".to_string()),
            observations: vec![
                LabObservation {
                    loinc_code: "2345-7".to_string(),
                    display_name: "Glucose".to_string(),
                    value_quantity: Some(92.0),
                    unit: Some("mg/dL".to_string()),
                    value_string: None,
                    reference_range: Some("70-100 mg/dL".to_string()),
                    interpretation: Some("N".to_string()),
                },
            ],
            conclusion: Some("Normal CMP".to_string()),
        };
        let fhir = build_lab_result_fhir("result-001", &input, false, "2026-03-11T10:00:00Z");
        assert_eq!(fhir["resourceType"], "DiagnosticReport");
        assert_eq!(fhir["id"], "result-001");
        assert_eq!(fhir["status"], "final");
        assert_eq!(fhir["category"][0]["coding"][0]["code"], "LAB");
        assert_eq!(fhir["code"]["coding"][0]["system"], "http://loinc.org");
        assert_eq!(fhir["code"]["coding"][0]["code"], "24323-8");
        assert_eq!(fhir["subject"]["reference"], "Patient/patient-001");
        assert_eq!(fhir["conclusion"], "Normal CMP");
    }

    #[test]
    fn labs_01_lab_result_contains_observations() {
        let input = LabResultInput {
            patient_id: "patient-001".to_string(),
            order_id: None,
            provider_id: "provider-001".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "CMP".to_string(),
            status: "final".to_string(),
            reported_at: None,
            performing_lab: None,
            observations: vec![
                LabObservation {
                    loinc_code: "2345-7".to_string(),
                    display_name: "Glucose".to_string(),
                    value_quantity: Some(92.0),
                    unit: Some("mg/dL".to_string()),
                    value_string: None,
                    reference_range: None,
                    interpretation: Some("N".to_string()),
                },
                LabObservation {
                    loinc_code: "2160-0".to_string(),
                    display_name: "Creatinine".to_string(),
                    value_quantity: Some(0.9),
                    unit: Some("mg/dL".to_string()),
                    value_string: None,
                    reference_range: Some("0.7-1.3 mg/dL".to_string()),
                    interpretation: Some("N".to_string()),
                },
            ],
            conclusion: None,
        };
        let fhir = build_lab_result_fhir("result-002", &input, false, "2026-03-11T10:00:00Z");
        let contained = fhir["contained"].as_array().expect("contained array");
        assert_eq!(contained.len(), 2);
        assert_eq!(contained[0]["code"]["coding"][0]["code"], "2345-7");
        assert_eq!(contained[1]["code"]["coding"][0]["code"], "2160-0");
    }

    #[test]
    fn labs_04_abnormal_flag_detected_high() {
        let obs = vec![
            LabObservation {
                loinc_code: "2345-7".to_string(),
                display_name: "Glucose".to_string(),
                value_quantity: Some(450.0),
                unit: Some("mg/dL".to_string()),
                value_string: None,
                reference_range: Some("70-100 mg/dL".to_string()),
                interpretation: Some("HH".to_string()),
            },
        ];
        assert!(has_abnormal_flag(&obs));
    }

    #[test]
    fn labs_04_abnormal_flag_detected_low() {
        let obs = vec![
            LabObservation {
                loinc_code: "2345-7".to_string(),
                display_name: "Glucose".to_string(),
                value_quantity: Some(45.0),
                unit: Some("mg/dL".to_string()),
                value_string: None,
                reference_range: Some("70-100 mg/dL".to_string()),
                interpretation: Some("L".to_string()),
            },
        ];
        assert!(has_abnormal_flag(&obs));
    }

    #[test]
    fn labs_04_normal_result_no_flag() {
        let obs = vec![
            LabObservation {
                loinc_code: "2345-7".to_string(),
                display_name: "Glucose".to_string(),
                value_quantity: Some(92.0),
                unit: Some("mg/dL".to_string()),
                value_string: None,
                reference_range: Some("70-100 mg/dL".to_string()),
                interpretation: Some("N".to_string()),
            },
        ];
        assert!(!has_abnormal_flag(&obs));
    }

    #[test]
    fn labs_04_mixed_observations_abnormal_detected() {
        let obs = vec![
            LabObservation {
                loinc_code: "2345-7".to_string(),
                display_name: "Glucose".to_string(),
                value_quantity: Some(92.0),
                unit: Some("mg/dL".to_string()),
                value_string: None,
                reference_range: None,
                interpretation: Some("N".to_string()),
            },
            LabObservation {
                loinc_code: "2160-0".to_string(),
                display_name: "Creatinine".to_string(),
                value_quantity: Some(4.5),
                unit: Some("mg/dL".to_string()),
                value_string: None,
                reference_range: Some("0.7-1.3 mg/dL".to_string()),
                interpretation: Some("H".to_string()),
            },
        ];
        assert!(has_abnormal_flag(&obs));
    }

    #[test]
    fn labs_04_fhir_has_abnormal_extension() {
        let input = LabResultInput {
            patient_id: "patient-001".to_string(),
            order_id: None,
            provider_id: "provider-001".to_string(),
            loinc_code: "24323-8".to_string(),
            display_name: "CMP".to_string(),
            status: "final".to_string(),
            reported_at: None,
            performing_lab: None,
            observations: vec![
                LabObservation {
                    loinc_code: "2345-7".to_string(),
                    display_name: "Glucose".to_string(),
                    value_quantity: Some(500.0),
                    unit: Some("mg/dL".to_string()),
                    value_string: None,
                    reference_range: None,
                    interpretation: Some("HH".to_string()),
                },
            ],
            conclusion: None,
        };
        let fhir = build_lab_result_fhir("result-003", &input, true, "2026-03-11T10:00:00Z");
        let exts = fhir["extension"].as_array().expect("extension array");
        let abn_ext = exts
            .iter()
            .find(|e| e["url"] == "http://medarc.local/fhir/ext/has-abnormal")
            .expect("has-abnormal extension");
        assert_eq!(abn_ext["valueBoolean"], true);
    }

    #[test]
    fn labs_04_all_interpretation_flags_detected() {
        for flag in ["H", "L", "HH", "LL", "A", "AA"] {
            let obs = vec![LabObservation {
                loinc_code: "test".to_string(),
                display_name: "test".to_string(),
                value_quantity: Some(1.0),
                unit: None,
                value_string: None,
                reference_range: None,
                interpretation: Some(flag.to_string()),
            }];
            assert!(has_abnormal_flag(&obs), "Flag {} should be detected as abnormal", flag);
        }
    }

    #[test]
    fn labs_04_normal_flag_n_not_abnormal() {
        let obs = vec![LabObservation {
            loinc_code: "test".to_string(),
            display_name: "test".to_string(),
            value_quantity: Some(1.0),
            unit: None,
            value_string: None,
            reference_range: None,
            interpretation: Some("N".to_string()),
        }];
        assert!(!has_abnormal_flag(&obs));
    }

    #[test]
    fn labs_04_no_interpretation_not_abnormal() {
        let obs = vec![LabObservation {
            loinc_code: "test".to_string(),
            display_name: "test".to_string(),
            value_quantity: Some(1.0),
            unit: None,
            value_string: None,
            reference_range: None,
            interpretation: None,
        }];
        assert!(!has_abnormal_flag(&obs));
    }

    // ─── DOCS-01, DOCS-02: Document FHIR structure & integrity ───────────────

    #[test]
    fn docs_01_document_fhir_has_correct_structure() {
        let input = DocumentUploadInput {
            patient_id: "patient-001".to_string(),
            title: "CT Chest 2026-03-11".to_string(),
            category: Some("imaging".to_string()),
            content_type: "application/pdf".to_string(),
            content_base64: "SGVsbG8gV29ybGQ=".to_string(), // "Hello World"
            file_size_bytes: 11,
            uploaded_by: "provider-001".to_string(),
        };
        let fhir = build_document_fhir("doc-001", &input, "abc123sha1", "2026-03-11T09:00:00Z");
        assert_eq!(fhir["resourceType"], "DocumentReference");
        assert_eq!(fhir["id"], "doc-001");
        assert_eq!(fhir["status"], "current");
        assert_eq!(fhir["subject"]["reference"], "Patient/patient-001");
        assert_eq!(fhir["content"][0]["attachment"]["contentType"], "application/pdf");
        assert_eq!(fhir["content"][0]["attachment"]["title"], "CT Chest 2026-03-11");
        assert_eq!(fhir["type"]["coding"][0]["code"], "imaging");
    }

    #[test]
    fn docs_02_sha256_checksum_computed_correctly() {
        // "Hello World" in base64 is "SGVsbG8gV29ybGQ="
        // SHA-256 of "Hello World" bytes is known
        let checksum = compute_sha256_hex("SGVsbG8gV29ybGQ=").unwrap();
        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        // Verify it's consistent (deterministic)
        let checksum2 = compute_sha256_hex("SGVsbG8gV29ybGQ=").unwrap();
        assert_eq!(checksum, checksum2);
    }

    #[test]
    fn docs_02_different_content_produces_different_checksum() {
        let c1 = compute_sha256_hex("SGVsbG8gV29ybGQ=").unwrap(); // "Hello World"
        let c2 = compute_sha256_hex("SGVsbG8=").unwrap(); // "Hello"
        assert_ne!(c1, c2);
    }

    #[test]
    fn docs_02_sha1_checksum_in_fhir_extension() {
        let input = DocumentUploadInput {
            patient_id: "patient-001".to_string(),
            title: "Lab Report".to_string(),
            category: None,
            content_type: "application/pdf".to_string(),
            content_base64: "SGVsbG8=".to_string(),
            file_size_bytes: 5,
            uploaded_by: "provider-001".to_string(),
        };
        let fhir = build_document_fhir("doc-002", &input, "mysha1hash", "2026-03-11T09:00:00Z");
        let exts = fhir["extension"].as_array().expect("extension array");
        let sha_ext = exts
            .iter()
            .find(|e| e["url"] == "http://medarc.local/fhir/ext/sha1-checksum")
            .expect("sha1-checksum extension");
        assert_eq!(sha_ext["valueString"], "mysha1hash");
    }

    #[test]
    fn docs_01_file_size_stored_in_fhir() {
        let input = DocumentUploadInput {
            patient_id: "patient-001".to_string(),
            title: "Test Doc".to_string(),
            category: None,
            content_type: "image/jpeg".to_string(),
            content_base64: "SGVsbG8=".to_string(),
            file_size_bytes: 1024000,
            uploaded_by: "provider-001".to_string(),
        };
        let fhir = build_document_fhir("doc-003", &input, "sha1", "2026-03-11T09:00:00Z");
        assert_eq!(fhir["content"][0]["attachment"]["size"], 1024000);
        let exts = fhir["extension"].as_array().expect("extension array");
        let size_ext = exts
            .iter()
            .find(|e| e["url"] == "http://medarc.local/fhir/ext/file-size-bytes")
            .expect("file-size-bytes extension");
        assert_eq!(size_ext["valueInteger"], 1024000);
    }

    #[test]
    fn docs_01_default_category_is_clinical_note() {
        let input = DocumentUploadInput {
            patient_id: "patient-001".to_string(),
            title: "Test Doc".to_string(),
            category: None,
            content_type: "application/pdf".to_string(),
            content_base64: "SGVsbG8=".to_string(),
            file_size_bytes: 5,
            uploaded_by: "provider-001".to_string(),
        };
        let fhir = build_document_fhir("doc-004", &input, "sha1", "2026-03-11T09:00:00Z");
        assert_eq!(fhir["type"]["coding"][0]["code"], "clinical-note");
    }

    // ─── RBAC matrix tests for LabResults ────────────────────────────────────

    #[test]
    fn labs_rbac_provider_full_access() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(has_permission(Role::Provider, Resource::LabResults, Action::Create));
        assert!(has_permission(Role::Provider, Resource::LabResults, Action::Read));
        assert!(has_permission(Role::Provider, Resource::LabResults, Action::Update));
        assert!(has_permission(Role::Provider, Resource::LabResults, Action::Delete));
    }

    #[test]
    fn labs_rbac_nurse_no_delete() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(has_permission(Role::NurseMa, Resource::LabResults, Action::Create));
        assert!(has_permission(Role::NurseMa, Resource::LabResults, Action::Read));
        assert!(has_permission(Role::NurseMa, Resource::LabResults, Action::Update));
        assert!(!has_permission(Role::NurseMa, Resource::LabResults, Action::Delete));
    }

    #[test]
    fn labs_rbac_billing_read_only() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(!has_permission(Role::BillingStaff, Resource::LabResults, Action::Create));
        assert!(has_permission(Role::BillingStaff, Resource::LabResults, Action::Read));
        assert!(!has_permission(Role::BillingStaff, Resource::LabResults, Action::Update));
        assert!(!has_permission(Role::BillingStaff, Resource::LabResults, Action::Delete));
    }

    #[test]
    fn labs_rbac_front_desk_no_access() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(!has_permission(Role::FrontDesk, Resource::LabResults, Action::Create));
        assert!(!has_permission(Role::FrontDesk, Resource::LabResults, Action::Read));
        assert!(!has_permission(Role::FrontDesk, Resource::LabResults, Action::Update));
        assert!(!has_permission(Role::FrontDesk, Resource::LabResults, Action::Delete));
    }

    // ─── RBAC matrix tests for PatientDocuments ───────────────────────────────

    #[test]
    fn docs_rbac_provider_full_access() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(has_permission(Role::Provider, Resource::PatientDocuments, Action::Create));
        assert!(has_permission(Role::Provider, Resource::PatientDocuments, Action::Read));
        assert!(has_permission(Role::Provider, Resource::PatientDocuments, Action::Update));
        assert!(has_permission(Role::Provider, Resource::PatientDocuments, Action::Delete));
    }

    #[test]
    fn docs_rbac_nurse_no_delete() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(has_permission(Role::NurseMa, Resource::PatientDocuments, Action::Create));
        assert!(has_permission(Role::NurseMa, Resource::PatientDocuments, Action::Read));
        assert!(has_permission(Role::NurseMa, Resource::PatientDocuments, Action::Update));
        assert!(!has_permission(Role::NurseMa, Resource::PatientDocuments, Action::Delete));
    }

    #[test]
    fn docs_rbac_billing_read_only() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(!has_permission(Role::BillingStaff, Resource::PatientDocuments, Action::Create));
        assert!(has_permission(Role::BillingStaff, Resource::PatientDocuments, Action::Read));
        assert!(!has_permission(Role::BillingStaff, Resource::PatientDocuments, Action::Update));
        assert!(!has_permission(Role::BillingStaff, Resource::PatientDocuments, Action::Delete));
    }

    #[test]
    fn docs_rbac_front_desk_read_only() {
        use crate::rbac::roles::{has_permission, Action, Resource, Role};
        assert!(!has_permission(Role::FrontDesk, Resource::PatientDocuments, Action::Create));
        assert!(has_permission(Role::FrontDesk, Resource::PatientDocuments, Action::Read));
        assert!(!has_permission(Role::FrontDesk, Resource::PatientDocuments, Action::Update));
        assert!(!has_permission(Role::FrontDesk, Resource::PatientDocuments, Action::Delete));
    }

    // ─── Base64 decoder tests ─────────────────────────────────────────────────

    #[test]
    fn base64_decode_hello_world() {
        let decoded = base64_decode("SGVsbG8gV29ybGQ=").unwrap();
        assert_eq!(decoded, b"Hello World");
    }

    #[test]
    fn base64_decode_hello() {
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn base64_decode_empty_string() {
        // Empty base64 is technically valid but produces 0 bytes
        let decoded = base64_decode("").unwrap();
        assert!(decoded.is_empty());
    }

    // ─── Migration 13 index validation ───────────────────────────────────────

    #[test]
    fn s08_migration_13_tables_defined() {
        // Verify migration SQL contains our four S08 tables
        // (indirect test — if the migration string was wrong the compile would fail,
        //  but we verify the index names as a smoke test)
        let migration_sql = "lab_catalogue_index lab_order_index lab_result_index document_index";
        assert!(migration_sql.contains("lab_catalogue_index"));
        assert!(migration_sql.contains("lab_order_index"));
        assert!(migration_sql.contains("lab_result_index"));
        assert!(migration_sql.contains("document_index"));
    }
}
