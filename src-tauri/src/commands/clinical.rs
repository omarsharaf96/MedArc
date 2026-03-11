/// commands/clinical.rs — Clinical Patient Data (S05)
///
/// Implements PTNT-08 through PTNT-11:
///   PTNT-08  Allergy list — drug / food / environmental, severity, reaction type (FHIR AllergyIntolerance)
///   PTNT-09  Problem list — ICD-10-coded diagnoses, active / inactive / resolved status (FHIR Condition)
///   PTNT-10  Medication list — active / discontinued / historical, RxNorm codes (FHIR MedicationStatement)
///   PTNT-11  Immunization history — CVX codes, lot numbers, administration dates (FHIR Immunization)
///
/// Data model
/// ----------
/// All clinical resources are stored as FHIR R4 JSON in `fhir_resources`.
/// Migration 10 adds four index tables:
///   - `allergy_index`       (patient_id, status, category)
///   - `problem_index`       (patient_id, status, icd10_code)
///   - `medication_index`    (patient_id, status, rxnorm_code)
///   - `immunization_index`  (patient_id, cvx_code, administered_date)
///
/// RBAC
/// ----
/// All clinical data commands require `ClinicalData` resource access.
///   Provider / SystemAdmin  → full CRUD
///   NurseMa                 → Create + Read + Update (no delete)
///   BillingStaff / FrontDesk → Read-only
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using the shared
/// `write_audit_entry` helper, keeping the audit chain intact.
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
// Allergy types (PTNT-08)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating or updating an AllergyIntolerance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllergyInput {
    /// The patient this allergy belongs to.
    pub patient_id: String,
    /// "drug" | "food" | "environment" | "biologic"
    pub category: String,
    /// Free-text or coded allergen name (e.g. "Penicillin", "Peanuts")
    pub substance: String,
    /// SNOMED or RxNorm code for the substance (optional)
    pub substance_code: Option<String>,
    /// Code system for substance_code (e.g. "http://www.nlm.nih.gov/research/umls/rxnorm")
    pub substance_system: Option<String>,
    /// "active" | "inactive" | "resolved"
    pub clinical_status: Option<String>,
    /// "allergy" | "intolerance"
    pub allergy_type: Option<String>,
    /// "mild" | "moderate" | "severe" | "life-threatening"
    pub severity: Option<String>,
    /// Free-text description of the reaction (e.g. "hives", "anaphylaxis")
    pub reaction: Option<String>,
    /// ISO 8601 date of onset (e.g. "2024-01-15")
    pub onset_date: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Stored allergy record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllergyRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem types (PTNT-09)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating or updating a Condition (problem list entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemInput {
    /// The patient this problem belongs to.
    pub patient_id: String,
    /// ICD-10 code (e.g. "J06.9", "I10")
    pub icd10_code: String,
    /// Human-readable display for the ICD-10 code
    pub display: String,
    /// "active" | "inactive" | "resolved"
    pub clinical_status: Option<String>,
    /// ISO 8601 date of onset (e.g. "2024-03-01")
    pub onset_date: Option<String>,
    /// ISO 8601 date resolved/abated (if applicable)
    pub abatement_date: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Stored problem record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Medication types (PTNT-10)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating or updating a MedicationStatement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MedicationInput {
    /// The patient this medication belongs to.
    pub patient_id: String,
    /// RxNorm code (e.g. "1049502")
    pub rxnorm_code: Option<String>,
    /// Drug name for display (e.g. "Amoxicillin 500 MG Oral Capsule")
    pub display: String,
    /// "active" | "completed" | "entered-in-error" | "intended" | "stopped" | "on-hold" | "unknown" | "not-taken"
    pub status: Option<String>,
    /// Dosage instructions (e.g. "500mg TID x 10 days")
    pub dosage: Option<String>,
    /// ISO 8601 effective start date
    pub effective_start: Option<String>,
    /// ISO 8601 effective end date (if stopped/completed)
    pub effective_end: Option<String>,
    /// Prescribing provider ID
    pub prescriber_id: Option<String>,
    /// Reason for medication (ICD-10 code or free text)
    pub reason: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Stored medication record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MedicationRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Immunization types (PTNT-11)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating or updating an Immunization record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmunizationInput {
    /// The patient this immunization belongs to.
    pub patient_id: String,
    /// CVX code (e.g. "158" for influenza, "208" for COVID-19 Pfizer)
    pub cvx_code: String,
    /// Vaccine name for display (e.g. "Influenza, seasonal, injectable")
    pub display: String,
    /// ISO 8601 date of administration (e.g. "2024-10-15")
    pub occurrence_date: String,
    /// Lot number of the vaccine vial
    pub lot_number: Option<String>,
    /// Expiration date of the lot (ISO 8601)
    pub expiration_date: Option<String>,
    /// Administering site (e.g. "left arm", "right deltoid")
    pub site: Option<String>,
    /// Route of administration (e.g. "intramuscular", "subcutaneous")
    pub route: Option<String>,
    /// Dose number in series (e.g. 1, 2, 3)
    pub dose_number: Option<i32>,
    /// "completed" | "entered-in-error" | "not-done"
    pub status: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Stored immunization record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmunizationRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR resource builders (pure functions — no I/O, fully testable)
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR R4 AllergyIntolerance resource from an `AllergyInput`.
pub fn build_allergy_fhir(id: &str, input: &AllergyInput) -> serde_json::Value {
    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");

    let allergy_type = input.allergy_type.as_deref().unwrap_or("allergy");

    let mut resource = serde_json::json!({
        "resourceType": "AllergyIntolerance",
        "id": id,
        "patient": {"reference": format!("Patient/{}", input.patient_id)},
        "clinicalStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                "code": clinical_status,
            }]
        },
        "verificationStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-verification",
                "code": "confirmed",
            }]
        },
        "type": allergy_type,
        "category": [input.category.as_str()],
    });

    // Substance coding
    let mut coding = serde_json::json!([{"display": input.substance}]);
    if let (Some(code), Some(system)) = (&input.substance_code, &input.substance_system) {
        coding = serde_json::json!([{
            "system": system,
            "code": code,
            "display": input.substance,
        }]);
    } else if let Some(code) = &input.substance_code {
        coding = serde_json::json!([{
            "code": code,
            "display": input.substance,
        }]);
    }
    resource["code"] = serde_json::json!({"coding": coding, "text": input.substance});

    // Reaction
    if let Some(reaction) = &input.reaction {
        let mut rxn = serde_json::json!({
            "manifestation": [{"text": reaction}]
        });
        if let Some(sev) = &input.severity {
            rxn["severity"] = serde_json::json!(sev);
        }
        resource["reaction"] = serde_json::json!([rxn]);
    }

    if let Some(onset) = &input.onset_date {
        resource["onsetDateTime"] = serde_json::json!(onset);
    }

    if let Some(notes) = &input.notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

/// Build a FHIR R4 Condition resource from a `ProblemInput`.
pub fn build_problem_fhir(id: &str, input: &ProblemInput) -> serde_json::Value {
    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");

    let mut resource = serde_json::json!({
        "resourceType": "Condition",
        "id": id,
        "subject": {"reference": format!("Patient/{}", input.patient_id)},
        "clinicalStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/condition-clinical",
                "code": clinical_status,
            }]
        },
        "verificationStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/condition-ver-status",
                "code": "confirmed",
            }]
        },
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/condition-category",
                "code": "problem-list-item",
                "display": "Problem List Item",
            }]
        }],
        "code": {
            "coding": [{
                "system": "http://hl7.org/fhir/sid/icd-10-cm",
                "code": input.icd10_code.as_str(),
                "display": input.display.as_str(),
            }],
            "text": input.display.as_str(),
        },
    });

    if let Some(onset) = &input.onset_date {
        resource["onsetDateTime"] = serde_json::json!(onset);
    }

    if let Some(abatement) = &input.abatement_date {
        resource["abatementDateTime"] = serde_json::json!(abatement);
    }

    if let Some(notes) = &input.notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

/// Build a FHIR R4 MedicationStatement resource from a `MedicationInput`.
pub fn build_medication_fhir(id: &str, input: &MedicationInput) -> serde_json::Value {
    let status = input.status.as_deref().unwrap_or("active");

    let mut medication_coding = serde_json::json!([{"display": input.display}]);
    if let Some(rxnorm) = &input.rxnorm_code {
        medication_coding = serde_json::json!([{
            "system": "http://www.nlm.nih.gov/research/umls/rxnorm",
            "code": rxnorm,
            "display": input.display,
        }]);
    }

    let mut resource = serde_json::json!({
        "resourceType": "MedicationStatement",
        "id": id,
        "status": status,
        "subject": {"reference": format!("Patient/{}", input.patient_id)},
        "medication": {
            "concept": {
                "coding": medication_coding,
                "text": input.display,
            }
        },
    });

    // Effective period
    match (&input.effective_start, &input.effective_end) {
        (Some(start), Some(end)) => {
            resource["effectivePeriod"] = serde_json::json!({"start": start, "end": end});
        }
        (Some(start), None) => {
            resource["effectivePeriod"] = serde_json::json!({"start": start});
        }
        _ => {}
    }

    if let Some(dosage) = &input.dosage {
        resource["dosage"] = serde_json::json!([{"text": dosage}]);
    }

    if let Some(prescriber) = &input.prescriber_id {
        resource["informationSource"] = serde_json::json!([{
            "reference": format!("Practitioner/{}", prescriber)
        }]);
    }

    if let Some(reason) = &input.reason {
        resource["reason"] = serde_json::json!([{"concept": {"text": reason}}]);
    }

    if let Some(notes) = &input.notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

/// Build a FHIR R4 Immunization resource from an `ImmunizationInput`.
pub fn build_immunization_fhir(id: &str, input: &ImmunizationInput) -> serde_json::Value {
    let status = input.status.as_deref().unwrap_or("completed");

    let mut resource = serde_json::json!({
        "resourceType": "Immunization",
        "id": id,
        "status": status,
        "patient": {"reference": format!("Patient/{}", input.patient_id)},
        "vaccineCode": {
            "coding": [{
                "system": "http://hl7.org/fhir/sid/cvx",
                "code": input.cvx_code.as_str(),
                "display": input.display.as_str(),
            }],
            "text": input.display.as_str(),
        },
        "occurrenceDateTime": input.occurrence_date.as_str(),
    });

    if let Some(lot) = &input.lot_number {
        resource["lotNumber"] = serde_json::json!(lot);
    }

    if let Some(exp) = &input.expiration_date {
        resource["expirationDate"] = serde_json::json!(exp);
    }

    if let Some(site) = &input.site {
        resource["site"] = serde_json::json!({"text": site});
    }

    if let Some(route) = &input.route {
        resource["route"] = serde_json::json!({"text": route});
    }

    if let Some(dose) = input.dose_number {
        resource["doseNumber"] = serde_json::json!(dose.to_string());
    }

    if let Some(notes) = &input.notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared audit-denied helper
// ─────────────────────────────────────────────────────────────────────────────

fn audit_denied(db: &Database, device_id: &DeviceId, user_id: &str, action: &str, detail: &str) {
    if let Ok(conn) = db.conn.lock() {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.to_string(),
                action: action.to_string(),
                resource_type: "Clinical".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(detail.to_string()),
            },
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Allergies (PTNT-08)
// ─────────────────────────────────────────────────────────────────────────────

/// Add an allergy/intolerance to a patient's record.
///
/// Creates a FHIR R4 AllergyIntolerance resource and inserts an index row.
/// Requires `ClinicalData:Create`.
#[tauri::command]
pub fn add_allergy(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: AllergyInput,
) -> Result<AllergyRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.allergy.add",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.substance.trim().is_empty() {
        return Err(AppError::Validation("substance is required".to_string()));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_allergy_fhir(&id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'AllergyIntolerance', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    )?;

    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");
    conn.execute(
        "INSERT INTO allergy_index (allergy_id, patient_id, clinical_status, category)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.patient_id, clinical_status, input.category],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.allergy.add".to_string(),
            resource_type: "AllergyIntolerance".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(AllergyRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List all allergies for a patient.
///
/// Requires `ClinicalData:Read`.
#[tauri::command]
pub fn list_allergies(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<Vec<AllergyRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.allergy.list",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
         FROM fhir_resources fr
         JOIN allergy_index ai ON ai.allergy_id = fr.id
         WHERE ai.patient_id = ?1
         ORDER BY fr.last_updated DESC",
    )?;

    let records = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(AllergyRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.allergy.list".to_string(),
            resource_type: "AllergyIntolerance".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} allergies", records.len())),
        },
    );

    Ok(records)
}

/// Update an existing allergy record.
///
/// Requires `ClinicalData:Update`.
#[tauri::command]
pub fn update_allergy(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    allergy_id: String,
    input: AllergyInput,
) -> Result<AllergyRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Update) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.allergy.update",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let version_result: Result<i64, _> = conn.query_row(
        "SELECT version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'AllergyIntolerance'",
        rusqlite::params![allergy_id],
        |row| row.get(0),
    );

    let current_version = match version_result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "clinical.allergy.update".to_string(),
                    resource_type: "AllergyIntolerance".to_string(),
                    resource_id: Some(allergy_id.clone()),
                    patient_id: Some(input.patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Not found".to_string()),
                },
            );
            return Err(AppError::NotFound(format!(
                "Allergy not found: {}",
                allergy_id
            )));
        }
        Err(e) => return Err(AppError::Database(e.to_string())),
        Ok(v) => v,
    };

    let new_version = current_version + 1;
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_allergy_fhir(&allergy_id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, allergy_id],
    )?;

    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");
    conn.execute(
        "UPDATE allergy_index SET clinical_status = ?1, category = ?2 WHERE allergy_id = ?3",
        rusqlite::params![clinical_status, input.category, allergy_id],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.allergy.update".to_string(),
            resource_type: "AllergyIntolerance".to_string(),
            resource_id: Some(allergy_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(AllergyRecord {
        id: allergy_id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

/// Delete an allergy record.
///
/// Requires `ClinicalData:Delete`.
#[tauri::command]
pub fn delete_allergy(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    allergy_id: String,
    patient_id: String,
) -> Result<(), AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Delete) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.allergy.delete",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = conn.execute(
        "DELETE FROM fhir_resources WHERE id = ?1 AND resource_type = 'AllergyIntolerance'",
        rusqlite::params![allergy_id],
    )?;

    if rows == 0 {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id,
                action: "clinical.allergy.delete".to_string(),
                resource_type: "AllergyIntolerance".to_string(),
                resource_id: Some(allergy_id.clone()),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Not found".to_string()),
            },
        );
        return Err(AppError::NotFound(format!(
            "Allergy not found: {}",
            allergy_id
        )));
    }

    // allergy_index has ON DELETE CASCADE from migration 10
    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.allergy.delete".to_string(),
            resource_type: "AllergyIntolerance".to_string(),
            resource_id: Some(allergy_id),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Problem list (PTNT-09)
// ─────────────────────────────────────────────────────────────────────────────

/// Add a diagnosis to the patient's problem list.
///
/// Creates a FHIR R4 Condition resource and inserts an index row.
/// Requires `ClinicalData:Create`.
#[tauri::command]
pub fn add_problem(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: ProblemInput,
) -> Result<ProblemRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.problem.add",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.icd10_code.trim().is_empty() {
        return Err(AppError::Validation("icd10_code is required".to_string()));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_problem_fhir(&id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Condition', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    )?;

    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");
    conn.execute(
        "INSERT INTO problem_index (problem_id, patient_id, clinical_status, icd10_code)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.patient_id, clinical_status, input.icd10_code],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.problem.add".to_string(),
            resource_type: "Condition".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(ProblemRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List all problems for a patient.
///
/// Optionally filter by clinical_status ("active", "inactive", "resolved").
/// Requires `ClinicalData:Read`.
#[tauri::command]
pub fn list_problems(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
    status_filter: Option<String>,
) -> Result<Vec<ProblemRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.problem.list",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let records = if let Some(status) = &status_filter {
        let mut stmt = conn.prepare(
            "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
             FROM fhir_resources fr
             JOIN problem_index pi ON pi.problem_id = fr.id
             WHERE pi.patient_id = ?1 AND pi.clinical_status = ?2
             ORDER BY fr.last_updated DESC",
        )?;
        stmt.query_map(rusqlite::params![patient_id, status], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(ProblemRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
             FROM fhir_resources fr
             JOIN problem_index pi ON pi.problem_id = fr.id
             WHERE pi.patient_id = ?1
             ORDER BY fr.last_updated DESC",
        )?;
        stmt.query_map(rusqlite::params![patient_id], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(ProblemRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.problem.list".to_string(),
            resource_type: "Condition".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} problems", records.len())),
        },
    );

    Ok(records)
}

/// Update an existing problem (e.g. change status from active → resolved).
///
/// Requires `ClinicalData:Update`.
#[tauri::command]
pub fn update_problem(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    problem_id: String,
    input: ProblemInput,
) -> Result<ProblemRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Update) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.problem.update",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let version_result: Result<i64, _> = conn.query_row(
        "SELECT version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'Condition'",
        rusqlite::params![problem_id],
        |row| row.get(0),
    );

    let current_version = match version_result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "clinical.problem.update".to_string(),
                    resource_type: "Condition".to_string(),
                    resource_id: Some(problem_id.clone()),
                    patient_id: Some(input.patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Not found".to_string()),
                },
            );
            return Err(AppError::NotFound(format!(
                "Problem not found: {}",
                problem_id
            )));
        }
        Err(e) => return Err(AppError::Database(e.to_string())),
        Ok(v) => v,
    };

    let new_version = current_version + 1;
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_problem_fhir(&problem_id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, problem_id],
    )?;

    let clinical_status = input.clinical_status.as_deref().unwrap_or("active");
    conn.execute(
        "UPDATE problem_index SET clinical_status = ?1, icd10_code = ?2 WHERE problem_id = ?3",
        rusqlite::params![clinical_status, input.icd10_code, problem_id],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.problem.update".to_string(),
            resource_type: "Condition".to_string(),
            resource_id: Some(problem_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(ProblemRecord {
        id: problem_id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Medications (PTNT-10)
// ─────────────────────────────────────────────────────────────────────────────

/// Add a medication to a patient's medication list.
///
/// Creates a FHIR R4 MedicationStatement resource and inserts an index row.
/// Requires `ClinicalData:Create`.
#[tauri::command]
pub fn add_medication(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: MedicationInput,
) -> Result<MedicationRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.medication.add",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.display.trim().is_empty() {
        return Err(AppError::Validation(
            "display (drug name) is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_medication_fhir(&id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'MedicationStatement', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    )?;

    let status = input.status.as_deref().unwrap_or("active");
    conn.execute(
        "INSERT INTO medication_index (medication_id, patient_id, status, rxnorm_code)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.patient_id, status, input.rxnorm_code],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.medication.add".to_string(),
            resource_type: "MedicationStatement".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(MedicationRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List medications for a patient.
///
/// Optionally filter by status ("active", "completed", etc.).
/// Requires `ClinicalData:Read`.
#[tauri::command]
pub fn list_medications(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
    status_filter: Option<String>,
) -> Result<Vec<MedicationRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.medication.list",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let records = if let Some(status) = &status_filter {
        let mut stmt = conn.prepare(
            "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
             FROM fhir_resources fr
             JOIN medication_index mi ON mi.medication_id = fr.id
             WHERE mi.patient_id = ?1 AND mi.status = ?2
             ORDER BY fr.last_updated DESC",
        )?;
        stmt.query_map(rusqlite::params![patient_id, status], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(MedicationRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
             FROM fhir_resources fr
             JOIN medication_index mi ON mi.medication_id = fr.id
             WHERE mi.patient_id = ?1
             ORDER BY fr.last_updated DESC",
        )?;
        stmt.query_map(rusqlite::params![patient_id], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(MedicationRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.medication.list".to_string(),
            resource_type: "MedicationStatement".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} medications", records.len())),
        },
    );

    Ok(records)
}

/// Update a medication (e.g. mark as stopped or change dosage).
///
/// Requires `ClinicalData:Update`.
#[tauri::command]
pub fn update_medication(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    medication_id: String,
    input: MedicationInput,
) -> Result<MedicationRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Update) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.medication.update",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let version_result: Result<i64, _> = conn.query_row(
        "SELECT version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'MedicationStatement'",
        rusqlite::params![medication_id],
        |row| row.get(0),
    );

    let current_version = match version_result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "clinical.medication.update".to_string(),
                    resource_type: "MedicationStatement".to_string(),
                    resource_id: Some(medication_id.clone()),
                    patient_id: Some(input.patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Not found".to_string()),
                },
            );
            return Err(AppError::NotFound(format!(
                "Medication not found: {}",
                medication_id
            )));
        }
        Err(e) => return Err(AppError::Database(e.to_string())),
        Ok(v) => v,
    };

    let new_version = current_version + 1;
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_medication_fhir(&medication_id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, medication_id],
    )?;

    let status = input.status.as_deref().unwrap_or("active");
    conn.execute(
        "UPDATE medication_index SET status = ?1, rxnorm_code = ?2 WHERE medication_id = ?3",
        rusqlite::params![status, input.rxnorm_code, medication_id],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.medication.update".to_string(),
            resource_type: "MedicationStatement".to_string(),
            resource_id: Some(medication_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(MedicationRecord {
        id: medication_id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Immunizations (PTNT-11)
// ─────────────────────────────────────────────────────────────────────────────

/// Record an immunization administration for a patient.
///
/// Creates a FHIR R4 Immunization resource and inserts an index row.
/// Requires `ClinicalData:Create`.
#[tauri::command]
pub fn add_immunization(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: ImmunizationInput,
) -> Result<ImmunizationRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.immunization.add",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    if input.patient_id.trim().is_empty() {
        return Err(AppError::Validation("patient_id is required".to_string()));
    }
    if input.cvx_code.trim().is_empty() {
        return Err(AppError::Validation("cvx_code is required".to_string()));
    }
    if input.occurrence_date.trim().is_empty() {
        return Err(AppError::Validation(
            "occurrence_date is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_immunization_fhir(&id, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Immunization', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    )?;

    conn.execute(
        "INSERT INTO immunization_index (immunization_id, patient_id, cvx_code, administered_date)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.patient_id, input.cvx_code, input.occurrence_date],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.immunization.add".to_string(),
            resource_type: "Immunization".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(ImmunizationRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List immunization history for a patient.
///
/// Results are ordered by administration date (most recent first).
/// Requires `ClinicalData:Read`.
#[tauri::command]
pub fn list_immunizations(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<Vec<ImmunizationRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::ClinicalData, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "clinical.immunization.list",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
         FROM fhir_resources fr
         JOIN immunization_index ii ON ii.immunization_id = fr.id
         WHERE ii.patient_id = ?1
         ORDER BY ii.administered_date DESC, fr.last_updated DESC",
    )?;

    let records = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(ImmunizationRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                version_id: row.get(2)?,
                last_updated: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "clinical.immunization.list".to_string(),
            resource_type: "Immunization".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} immunizations", records.len())),
        },
    );

    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// In-memory DB with migrations 1–10 applied inline for unit testing.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS fhir_resources (
                id TEXT PRIMARY KEY NOT NULL,
                resource_type TEXT NOT NULL,
                resource JSON NOT NULL,
                version_id INTEGER NOT NULL DEFAULT 1,
                last_updated TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Migration 10 index tables
            CREATE TABLE IF NOT EXISTS allergy_index (
                allergy_id      TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                clinical_status TEXT NOT NULL DEFAULT 'active',
                category        TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_allergy_patient   ON allergy_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_allergy_status    ON allergy_index(clinical_status);
            CREATE INDEX IF NOT EXISTS idx_allergy_category  ON allergy_index(category);

            CREATE TABLE IF NOT EXISTS problem_index (
                problem_id      TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                clinical_status TEXT NOT NULL DEFAULT 'active',
                icd10_code      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_problem_patient   ON problem_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_problem_status    ON problem_index(clinical_status);
            CREATE INDEX IF NOT EXISTS idx_problem_icd10     ON problem_index(icd10_code);

            CREATE TABLE IF NOT EXISTS medication_index (
                medication_id   TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'active',
                rxnorm_code     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_medication_patient ON medication_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_medication_status  ON medication_index(status);
            CREATE INDEX IF NOT EXISTS idx_medication_rxnorm  ON medication_index(rxnorm_code);

            CREATE TABLE IF NOT EXISTS immunization_index (
                immunization_id TEXT PRIMARY KEY NOT NULL
                                REFERENCES fhir_resources(id) ON DELETE CASCADE,
                patient_id      TEXT NOT NULL,
                cvx_code        TEXT NOT NULL,
                administered_date TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_immunization_patient ON immunization_index(patient_id);
            CREATE INDEX IF NOT EXISTS idx_immunization_cvx     ON immunization_index(cvx_code);
            CREATE INDEX IF NOT EXISTS idx_immunization_date    ON immunization_index(administered_date);

            CREATE TABLE IF NOT EXISTS audit_logs (
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

    // ── Helpers ────────────────────────────────────────────────────────────

    fn sample_allergy_input() -> AllergyInput {
        AllergyInput {
            patient_id: "pat-001".to_string(),
            category: "drug".to_string(),
            substance: "Penicillin".to_string(),
            substance_code: Some("7980".to_string()),
            substance_system: Some("http://www.nlm.nih.gov/research/umls/rxnorm".to_string()),
            clinical_status: Some("active".to_string()),
            allergy_type: Some("allergy".to_string()),
            severity: Some("severe".to_string()),
            reaction: Some("anaphylaxis".to_string()),
            onset_date: Some("2020-06-01".to_string()),
            notes: None,
        }
    }

    fn sample_problem_input() -> ProblemInput {
        ProblemInput {
            patient_id: "pat-001".to_string(),
            icd10_code: "I10".to_string(),
            display: "Essential (primary) hypertension".to_string(),
            clinical_status: Some("active".to_string()),
            onset_date: Some("2022-01-15".to_string()),
            abatement_date: None,
            notes: None,
        }
    }

    fn sample_medication_input() -> MedicationInput {
        MedicationInput {
            patient_id: "pat-001".to_string(),
            rxnorm_code: Some("1049502".to_string()),
            display: "Amoxicillin 500 MG Oral Capsule".to_string(),
            status: Some("active".to_string()),
            dosage: Some("500mg TID".to_string()),
            effective_start: Some("2024-03-01".to_string()),
            effective_end: None,
            prescriber_id: Some("prov-001".to_string()),
            reason: Some("Acute sinusitis".to_string()),
            notes: None,
        }
    }

    fn sample_immunization_input() -> ImmunizationInput {
        ImmunizationInput {
            patient_id: "pat-001".to_string(),
            cvx_code: "158".to_string(),
            display: "influenza, seasonal, injectable".to_string(),
            occurrence_date: "2024-10-15".to_string(),
            lot_number: Some("LOT-ABC123".to_string()),
            expiration_date: Some("2025-03-31".to_string()),
            site: Some("left deltoid".to_string()),
            route: Some("intramuscular".to_string()),
            dose_number: Some(1),
            status: Some("completed".to_string()),
            notes: None,
        }
    }

    // ── FHIR builder tests — Allergy ───────────────────────────────────────

    #[test]
    fn build_allergy_fhir_has_correct_resource_type() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        assert_eq!(fhir["resourceType"], "AllergyIntolerance");
    }

    #[test]
    fn build_allergy_fhir_links_to_patient() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        assert_eq!(fhir["patient"]["reference"], "Patient/pat-001");
    }

    #[test]
    fn build_allergy_fhir_has_clinical_status() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        let status_code = fhir["clinicalStatus"]["coding"][0]["code"]
            .as_str()
            .unwrap();
        assert_eq!(status_code, "active");
    }

    #[test]
    fn build_allergy_fhir_has_category() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        assert_eq!(fhir["category"][0], "drug");
    }

    #[test]
    fn build_allergy_fhir_has_substance_with_rxnorm_code() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        let coding = &fhir["code"]["coding"][0];
        assert_eq!(coding["code"], "7980");
        assert_eq!(
            coding["system"],
            "http://www.nlm.nih.gov/research/umls/rxnorm"
        );
        assert_eq!(coding["display"], "Penicillin");
    }

    #[test]
    fn build_allergy_fhir_has_reaction_with_severity() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        let rxn = &fhir["reaction"][0];
        assert_eq!(rxn["manifestation"][0]["text"], "anaphylaxis");
        assert_eq!(rxn["severity"], "severe");
    }

    #[test]
    fn build_allergy_fhir_has_onset_date() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("allergy-001", &input);
        assert_eq!(fhir["onsetDateTime"], "2020-06-01");
    }

    #[test]
    fn build_allergy_fhir_defaults_clinical_status_to_active() {
        let mut input = sample_allergy_input();
        input.clinical_status = None;
        let fhir = build_allergy_fhir("allergy-001", &input);
        assert_eq!(fhir["clinicalStatus"]["coding"][0]["code"], "active");
    }

    // ── FHIR builder tests — Problem ───────────────────────────────────────

    #[test]
    fn build_problem_fhir_has_correct_resource_type() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        assert_eq!(fhir["resourceType"], "Condition");
    }

    #[test]
    fn build_problem_fhir_links_to_patient() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        assert_eq!(fhir["subject"]["reference"], "Patient/pat-001");
    }

    #[test]
    fn build_problem_fhir_has_icd10_code() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        let coding = &fhir["code"]["coding"][0];
        assert_eq!(coding["system"], "http://hl7.org/fhir/sid/icd-10-cm");
        assert_eq!(coding["code"], "I10");
        assert_eq!(coding["display"], "Essential (primary) hypertension");
    }

    #[test]
    fn build_problem_fhir_has_problem_list_category() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        let cat_code = fhir["category"][0]["coding"][0]["code"].as_str().unwrap();
        assert_eq!(cat_code, "problem-list-item");
    }

    #[test]
    fn build_problem_fhir_has_clinical_status() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        assert_eq!(fhir["clinicalStatus"]["coding"][0]["code"], "active");
    }

    #[test]
    fn build_problem_fhir_has_onset_date() {
        let input = sample_problem_input();
        let fhir = build_problem_fhir("prob-001", &input);
        assert_eq!(fhir["onsetDateTime"], "2022-01-15");
    }

    #[test]
    fn build_problem_fhir_includes_abatement_date_when_resolved() {
        let mut input = sample_problem_input();
        input.clinical_status = Some("resolved".to_string());
        input.abatement_date = Some("2023-06-30".to_string());
        let fhir = build_problem_fhir("prob-001", &input);
        assert_eq!(fhir["abatementDateTime"], "2023-06-30");
        assert_eq!(fhir["clinicalStatus"]["coding"][0]["code"], "resolved");
    }

    // ── FHIR builder tests — Medication ────────────────────────────────────

    #[test]
    fn build_medication_fhir_has_correct_resource_type() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        assert_eq!(fhir["resourceType"], "MedicationStatement");
    }

    #[test]
    fn build_medication_fhir_links_to_patient() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        assert_eq!(fhir["subject"]["reference"], "Patient/pat-001");
    }

    #[test]
    fn build_medication_fhir_has_rxnorm_code() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        let coding = &fhir["medication"]["concept"]["coding"][0];
        assert_eq!(
            coding["system"],
            "http://www.nlm.nih.gov/research/umls/rxnorm"
        );
        assert_eq!(coding["code"], "1049502");
        assert_eq!(coding["display"], "Amoxicillin 500 MG Oral Capsule");
    }

    #[test]
    fn build_medication_fhir_has_status() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        assert_eq!(fhir["status"], "active");
    }

    #[test]
    fn build_medication_fhir_has_dosage() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        assert_eq!(fhir["dosage"][0]["text"], "500mg TID");
    }

    #[test]
    fn build_medication_fhir_has_effective_period() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("med-001", &input);
        assert_eq!(fhir["effectivePeriod"]["start"], "2024-03-01");
    }

    #[test]
    fn build_medication_fhir_without_rxnorm_uses_display_only() {
        let mut input = sample_medication_input();
        input.rxnorm_code = None;
        let fhir = build_medication_fhir("med-001", &input);
        let coding = &fhir["medication"]["concept"]["coding"][0];
        assert_eq!(coding["display"], "Amoxicillin 500 MG Oral Capsule");
        assert!(
            coding.get("code").map(|v| v.is_null()).unwrap_or(true)
                || coding.get("system").map(|v| v.is_null()).unwrap_or(true)
        );
    }

    // ── FHIR builder tests — Immunization ──────────────────────────────────

    #[test]
    fn build_immunization_fhir_has_correct_resource_type() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["resourceType"], "Immunization");
    }

    #[test]
    fn build_immunization_fhir_links_to_patient() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["patient"]["reference"], "Patient/pat-001");
    }

    #[test]
    fn build_immunization_fhir_has_cvx_code() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        let coding = &fhir["vaccineCode"]["coding"][0];
        assert_eq!(coding["system"], "http://hl7.org/fhir/sid/cvx");
        assert_eq!(coding["code"], "158");
    }

    #[test]
    fn build_immunization_fhir_has_occurrence_date() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["occurrenceDateTime"], "2024-10-15");
    }

    #[test]
    fn build_immunization_fhir_has_lot_number() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["lotNumber"], "LOT-ABC123");
    }

    #[test]
    fn build_immunization_fhir_has_expiration_date() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["expirationDate"], "2025-03-31");
    }

    #[test]
    fn build_immunization_fhir_has_site_and_route() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["site"]["text"], "left deltoid");
        assert_eq!(fhir["route"]["text"], "intramuscular");
    }

    #[test]
    fn build_immunization_fhir_has_dose_number() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["doseNumber"], "1");
    }

    #[test]
    fn build_immunization_fhir_defaults_status_to_completed() {
        let mut input = sample_immunization_input();
        input.status = None;
        let fhir = build_immunization_fhir("imm-001", &input);
        assert_eq!(fhir["status"], "completed");
    }

    // ── DB index tests ────────────────────────────────────────────────────

    #[test]
    fn allergy_index_row_inserted_and_cascade_deleted() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir = serde_json::json!({"resourceType":"AllergyIntolerance","id":"a1"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('a1', 'AllergyIntolerance', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO allergy_index (allergy_id, patient_id, clinical_status, category)
             VALUES ('a1', 'pat-001', 'active', 'drug')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allergy_index WHERE patient_id = 'pat-001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Cascade delete
        conn.execute("DELETE FROM fhir_resources WHERE id = 'a1'", [])
            .unwrap();
        let count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allergy_index WHERE allergy_id = 'a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count2, 0, "allergy_index cascade delete failed");
    }

    #[test]
    fn problem_index_row_inserted_and_cascade_deleted() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir = serde_json::json!({"resourceType":"Condition","id":"c1"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('c1', 'Condition', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO problem_index (problem_id, patient_id, clinical_status, icd10_code)
             VALUES ('c1', 'pat-001', 'active', 'I10')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM fhir_resources WHERE id = 'c1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM problem_index WHERE problem_id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "problem_index cascade delete failed");
    }

    #[test]
    fn medication_index_row_inserted_and_cascade_deleted() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir = serde_json::json!({"resourceType":"MedicationStatement","id":"m1"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('m1', 'MedicationStatement', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO medication_index (medication_id, patient_id, status, rxnorm_code)
             VALUES ('m1', 'pat-001', 'active', '1049502')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM fhir_resources WHERE id = 'm1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM medication_index WHERE medication_id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "medication_index cascade delete failed");
    }

    #[test]
    fn immunization_index_row_inserted_and_cascade_deleted() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir = serde_json::json!({"resourceType":"Immunization","id":"i1"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('i1', 'Immunization', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO immunization_index (immunization_id, patient_id, cvx_code, administered_date)
             VALUES ('i1', 'pat-001', '158', '2024-10-15')",
            [],
        ).unwrap();

        conn.execute("DELETE FROM fhir_resources WHERE id = 'i1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM immunization_index WHERE immunization_id = 'i1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "immunization_index cascade delete failed");
    }

    // ── PTNT requirements proof ────────────────────────────────────────────

    /// Proves PTNT-08: AllergyIntolerance FHIR structure matches spec.
    #[test]
    fn ptnt_08_allergy_intolerance_fhir_complete() {
        let input = sample_allergy_input();
        let fhir = build_allergy_fhir("a-test", &input);
        // Category
        assert_eq!(fhir["category"][0], "drug");
        // Substance with RxNorm code
        let coding = &fhir["code"]["coding"][0];
        assert_eq!(
            coding["system"],
            "http://www.nlm.nih.gov/research/umls/rxnorm"
        );
        assert_eq!(coding["code"], "7980");
        // Severity
        assert_eq!(fhir["reaction"][0]["severity"], "severe");
        // Reaction type
        assert_eq!(fhir["type"], "allergy");
        // Clinical status
        assert_eq!(fhir["clinicalStatus"]["coding"][0]["code"], "active");
    }

    /// Proves PTNT-09: Condition FHIR structure with ICD-10 coding and status.
    #[test]
    fn ptnt_09_condition_fhir_with_icd10_and_status() {
        let mut input = sample_problem_input();
        input.clinical_status = Some("active".to_string());
        let fhir = build_problem_fhir("c-test", &input);
        // ICD-10 system
        assert_eq!(
            fhir["code"]["coding"][0]["system"],
            "http://hl7.org/fhir/sid/icd-10-cm"
        );
        // ICD-10 code
        assert_eq!(fhir["code"]["coding"][0]["code"], "I10");
        // Status
        assert_eq!(fhir["clinicalStatus"]["coding"][0]["code"], "active");
        // Category = problem-list-item
        assert_eq!(
            fhir["category"][0]["coding"][0]["code"],
            "problem-list-item"
        );
    }

    /// Proves PTNT-10: MedicationStatement FHIR with RxNorm, dosage, and status.
    #[test]
    fn ptnt_10_medication_statement_fhir_complete() {
        let input = sample_medication_input();
        let fhir = build_medication_fhir("m-test", &input);
        // RxNorm system
        assert_eq!(
            fhir["medication"]["concept"]["coding"][0]["system"],
            "http://www.nlm.nih.gov/research/umls/rxnorm"
        );
        // RxNorm code
        assert_eq!(
            fhir["medication"]["concept"]["coding"][0]["code"],
            "1049502"
        );
        // Status values: active / completed / stopped / on-hold etc.
        assert_eq!(fhir["status"], "active");
        // Dosage
        assert!(fhir["dosage"].is_array());
    }

    /// Proves PTNT-11: Immunization FHIR with CVX code, lot, and date.
    #[test]
    fn ptnt_11_immunization_fhir_complete() {
        let input = sample_immunization_input();
        let fhir = build_immunization_fhir("i-test", &input);
        // CVX system
        assert_eq!(
            fhir["vaccineCode"]["coding"][0]["system"],
            "http://hl7.org/fhir/sid/cvx"
        );
        // CVX code
        assert_eq!(fhir["vaccineCode"]["coding"][0]["code"], "158");
        // Administration date
        assert_eq!(fhir["occurrenceDateTime"], "2024-10-15");
        // Lot number
        assert_eq!(fhir["lotNumber"], "LOT-ABC123");
        // Status
        assert_eq!(fhir["status"], "completed");
    }

    // ── Audit trail tests ─────────────────────────────────────────────────

    #[test]
    fn audit_entry_written_for_clinical_operations() {
        use crate::audit::entry::write_audit_entry;
        use crate::audit::AuditEntryInput;

        let conn = test_db();

        for action in [
            "clinical.allergy.add",
            "clinical.problem.add",
            "clinical.medication.add",
            "clinical.immunization.add",
        ] {
            let entry = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: "provider-001".to_string(),
                    action: action.to_string(),
                    resource_type: "Clinical".to_string(),
                    resource_id: Some("rec-001".to_string()),
                    patient_id: Some("pat-001".to_string()),
                    device_id: "dev-001".to_string(),
                    success: true,
                    details: None,
                },
            )
            .unwrap();
            assert!(entry.success);
            assert_eq!(entry.patient_id.as_deref(), Some("pat-001"));
        }
    }

    // ── multi-record list query ───────────────────────────────────────────

    #[test]
    fn allergy_list_returns_all_patient_allergies() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        for (id, cat) in [("a1", "drug"), ("a2", "food"), ("a3", "environment")] {
            let fhir = serde_json::json!({"resourceType":"AllergyIntolerance","id":id});
            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
                 VALUES (?1, 'AllergyIntolerance', ?2, 1, ?3, ?4, ?5)",
                rusqlite::params![id, serde_json::to_string(&fhir).unwrap(), now, now, now],
            ).unwrap();
            conn.execute(
                "INSERT INTO allergy_index (allergy_id, patient_id, clinical_status, category)
                 VALUES (?1, 'pat-001', 'active', ?2)",
                rusqlite::params![id, cat],
            )
            .unwrap();
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allergy_index WHERE patient_id = 'pat-001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3, "expected 3 allergies for pat-001");
    }

    #[test]
    fn problem_list_filters_by_status() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        for (id, status, code) in [("c1", "active", "I10"), ("c2", "resolved", "J06.9")] {
            let fhir = serde_json::json!({"resourceType":"Condition","id":id});
            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
                 VALUES (?1, 'Condition', ?2, 1, ?3, ?4, ?5)",
                rusqlite::params![id, serde_json::to_string(&fhir).unwrap(), now, now, now],
            ).unwrap();
            conn.execute(
                "INSERT INTO problem_index (problem_id, patient_id, clinical_status, icd10_code)
                 VALUES (?1, 'pat-001', ?2, ?3)",
                rusqlite::params![id, status, code],
            )
            .unwrap();
        }

        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM problem_index WHERE patient_id = 'pat-001' AND clinical_status = 'active'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let resolved_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM problem_index WHERE patient_id = 'pat-001' AND clinical_status = 'resolved'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_count, 1);
        assert_eq!(resolved_count, 1);
    }

    #[test]
    fn immunization_list_ordered_by_date_desc() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        for (id, date) in [
            ("i1", "2023-10-01"),
            ("i2", "2024-10-15"),
            ("i3", "2022-05-20"),
        ] {
            let fhir = serde_json::json!({"resourceType":"Immunization","id":id});
            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
                 VALUES (?1, 'Immunization', ?2, 1, ?3, ?4, ?5)",
                rusqlite::params![id, serde_json::to_string(&fhir).unwrap(), now, now, now],
            ).unwrap();
            conn.execute(
                "INSERT INTO immunization_index (immunization_id, patient_id, cvx_code, administered_date)
                 VALUES (?1, 'pat-001', '158', ?2)",
                rusqlite::params![id, date],
            ).unwrap();
        }

        // Most recent first
        let first_id: String = conn
            .query_row(
                "SELECT immunization_id FROM immunization_index WHERE patient_id = 'pat-001'
                 ORDER BY administered_date DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            first_id, "i2",
            "most recent immunization should be i2 (2024-10-15)"
        );
    }
}
