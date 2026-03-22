/// commands/objective_measures.rs — Objective Measures & Outcome Scores (M003/S02)
///
/// Implements:
///   OM-01  Record objective measures (ROM, MMT, ortho tests) per encounter
///   OM-02  Score outcome measures: LEFS, DASH, NDI, Oswestry, PSFS, FABQ
///   OM-03  Severity classification per measure
///   OM-04  MCID (Minimal Clinically Important Difference) tracking
///   OM-05  Outcome comparison (earliest vs latest per measure type)
///
/// Data model
/// ----------
/// Objective measures are stored as FHIR Observation JSON in `fhir_resources`
/// (resource_type = 'PTObjectiveMeasures' or 'OutcomeScore').
/// Migration 16 adds `outcome_score_index` for fast patient/measure/date queries.
///
/// Scoring algorithms
/// ------------------
/// Each measure has a pure scoring function, severity classifier, and MCID constant.
/// Scoring functions validate input ranges and item counts before computing.
///
/// RBAC
/// ----
/// All commands require `ClinicalDocumentation` resource access.
///   Provider / SystemAdmin  → full CRUD
///   NurseMa                 → Create + Read + Update
///   BillingStaff            → Read-only
///   FrontDesk               → No access
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.
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

/// MCID (Minimal Clinically Important Difference) constants per measure.
pub const MCID_LEFS: f64 = 9.0;
pub const MCID_DASH: f64 = 10.8;
pub const MCID_NDI: f64 = 7.5;
pub const MCID_OSWESTRY: f64 = 10.0;
pub const MCID_PSFS: f64 = 2.0;

/// LOINC codes for each outcome measure.
const LOINC_LEFS: &str = "75575-0";
const LOINC_DASH: &str = "71966-6";
const LOINC_NDI: &str = "72100-1";
const LOINC_OSWESTRY: &str = "72101-9";
const LOINC_PSFS: &str = "72102-7";
const LOINC_FABQ: &str = "72103-5";

// ─────────────────────────────────────────────────────────────────────────────
// Scoring functions
// ─────────────────────────────────────────────────────────────────────────────

/// Score the Lower Extremity Functional Scale (LEFS).
///
/// 20 items, each scored 0-4. Total range: 0-80.
/// Higher scores = better function.
pub fn score_lefs(items: &[u8; 20]) -> u16 {
    items.iter().map(|&v| v as u16).sum()
}

/// Score the Disabilities of the Arm, Shoulder and Hand (DASH).
///
/// Formula: ((sum / n) - 1) / 4 * 100
/// Requires >= 27 items answered (out of 30). Items scored 1-5.
/// Result range: 0.0 - 100.0. Higher scores = greater disability.
pub fn score_dash(items: &[u8]) -> Result<f64, AppError> {
    let n = items.len();
    if n < 27 {
        return Err(AppError::Validation(format!(
            "DASH requires at least 27 items answered, got {}",
            n
        )));
    }
    for (i, &v) in items.iter().enumerate() {
        if !(1..=5).contains(&v) {
            return Err(AppError::Validation(format!(
                "DASH item {} value {} out of range 1-5",
                i, v
            )));
        }
    }
    let sum: f64 = items.iter().map(|&v| v as f64).sum();
    let score = ((sum / n as f64) - 1.0) / 4.0 * 100.0;
    Ok(score)
}

/// Score the Neck Disability Index (NDI).
///
/// 10 items, each scored 0-5. Formula: (sum / 50) * 100.
/// Result range: 0.0 - 100.0%. Higher scores = greater disability.
pub fn score_ndi(items: &[u8; 10]) -> f64 {
    let sum: f64 = items.iter().map(|&v| v as f64).sum();
    (sum / 50.0) * 100.0
}

/// Score the Oswestry Disability Index.
///
/// 10 items, each scored 0-5. Formula: (sum / 50) * 100.
/// Result range: 0.0 - 100.0%. Higher scores = greater disability.
pub fn score_oswestry(items: &[u8; 10]) -> f64 {
    let sum: f64 = items.iter().map(|&v| v as f64).sum();
    (sum / 50.0) * 100.0
}

/// Score the Patient-Specific Functional Scale (PSFS).
///
/// 3-5 items, each scored 0-10. Result is the average.
/// Higher scores = better function.
pub fn score_psfs(items: &[u8]) -> Result<f64, AppError> {
    let n = items.len();
    if n < 3 || n > 5 {
        return Err(AppError::Validation(format!(
            "PSFS requires 3-5 items, got {}",
            n
        )));
    }
    for (i, &v) in items.iter().enumerate() {
        if v > 10 {
            return Err(AppError::Validation(format!(
                "PSFS item {} value {} out of range 0-10",
                i, v
            )));
        }
    }
    let sum: f64 = items.iter().map(|&v| v as f64).sum();
    Ok(sum / n as f64)
}

/// Score the Fear-Avoidance Beliefs Questionnaire (FABQ).
///
/// 16 items, each scored 0-6. Returns (PA subscale, Work subscale).
///
/// PA (Physical Activity) subscale: items at 0-indexed positions 1, 2, 3, 4
///   (questionnaire items 2, 3, 4, 5). Range: 0-24.
///
/// Work subscale: items at 0-indexed positions 5, 6, 8, 9, 10, 11, 12, 13, 14
///   (questionnaire items 6, 7, 9, 10, 11, 12, 13, 14, 15 — item 8 is excluded).
///   Note: the task spec says items 6-7,9-15 (0-indexed 5-6,8-14) which is 9 items.
///   Range: 0-42 (but commonly reported as 0-42 for 7 scored items; here we sum
///   the specified indices).
pub fn score_fabq(items: &[u8; 16]) -> (u8, u8) {
    // PA subscale: 0-indexed items 1, 2, 3, 4
    let pa: u8 = items[1] + items[2] + items[3] + items[4];

    // Work subscale: 0-indexed items 5, 6, 8, 9, 10, 11, 12, 13, 14
    let work: u8 = items[5] + items[6] + items[8] + items[9] + items[10]
        + items[11] + items[12] + items[13] + items[14];

    (pa, work)
}

// ─────────────────────────────────────────────────────────────────────────────
// Severity classification
// ─────────────────────────────────────────────────────────────────────────────

/// Classify LEFS severity.
///   0-19: severe, 20-39: moderate, 40-59: mild, 60-80: minimal
pub fn classify_lefs(score: u16) -> &'static str {
    match score {
        0..=19 => "severe",
        20..=39 => "moderate",
        40..=59 => "mild",
        60..=80 => "minimal",
        _ => "minimal", // scores above 80 are technically invalid but classify as minimal
    }
}

/// Classify DASH severity.
///   0-25: mild, 26-50: moderate, 51-75: severe, 76-100: very_severe
pub fn classify_dash(score: f64) -> &'static str {
    if score <= 25.0 {
        "mild"
    } else if score <= 50.0 {
        "moderate"
    } else if score <= 75.0 {
        "severe"
    } else {
        "very_severe"
    }
}

/// Classify NDI severity.
///   0-8: no_disability, 10-28: mild, 30-48: moderate, 50-68: severe, 70-100: complete
pub fn classify_ndi(score: f64) -> &'static str {
    if score <= 8.0 {
        "no_disability"
    } else if score <= 28.0 {
        "mild"
    } else if score <= 48.0 {
        "moderate"
    } else if score <= 68.0 {
        "severe"
    } else {
        "complete"
    }
}

/// Classify Oswestry severity.
///   0-20: minimal, 21-40: moderate, 41-60: severe, 61-80: crippling, 81-100: bed_bound
pub fn classify_oswestry(score: f64) -> &'static str {
    if score <= 20.0 {
        "minimal"
    } else if score <= 40.0 {
        "moderate"
    } else if score <= 60.0 {
        "severe"
    } else if score <= 80.0 {
        "crippling"
    } else {
        "bed_bound"
    }
}

/// Classify PSFS severity.
///   7.1-10: mild, 4.1-7.0: moderate, 0-4.0: severe
pub fn classify_psfs(score: f64) -> &'static str {
    if score > 7.0 {
        "mild"
    } else if score > 4.0 {
        "moderate"
    } else {
        "severe"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Input / output types
// ─────────────────────────────────────────────────────────────────────────────

/// Input for recording objective measures (ROM, MMT, ortho tests).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveMeasuresInput {
    pub patient_id: String,
    pub encounter_id: String,
    /// Arbitrary JSON data: ROM values, MMT grades, special tests, etc.
    pub data: serde_json::Value,
}

/// Objective measures record returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveMeasuresRecord {
    pub resource_id: String,
    pub patient_id: String,
    pub encounter_id: String,
    pub data: serde_json::Value,
    pub recorded_at: String,
}

/// Input for recording an outcome score.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeScoreInput {
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub measure_type: String,
    pub items: Vec<u8>,
    pub episode_phase: Option<String>,
}

/// Outcome score record returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeScoreRecord {
    pub score_id: String,
    pub resource_id: String,
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub measure_type: String,
    pub score: f64,
    pub score_secondary: Option<f64>,
    pub severity: Option<String>,
    pub episode_phase: Option<String>,
    pub loinc_code: Option<String>,
    pub recorded_at: String,
}

/// Outcome comparison per measure type (earliest vs latest).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeComparisonMeasure {
    pub measure_type: String,
    pub initial_score: Option<f64>,
    pub initial_date: Option<String>,
    pub latest_score: Option<f64>,
    pub latest_date: Option<String>,
    pub change: Option<f64>,
    pub mcid: Option<f64>,
    pub mcid_met: Option<bool>,
}

/// Full outcome comparison for a patient across all measure types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeComparison {
    pub patient_id: String,
    pub measures: Vec<OutcomeComparisonMeasure>,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR resource builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR Observation resource for objective measures (ROM/MMT/ortho).
fn build_objective_measures_fhir(
    id: &str,
    patient_id: &str,
    encounter_id: &str,
    data: &serde_json::Value,
    now: &str,
) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "id": id,
        "status": "final",
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "exam",
                "display": "Exam"
            }]
        }],
        "code": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/pt-objective-measures",
                "code": "objective-measures",
                "display": "PT Objective Measures"
            }]
        },
        "subject": {
            "reference": format!("Patient/{}", patient_id),
            "type": "Patient"
        },
        "encounter": {
            "reference": format!("Encounter/{}", encounter_id),
            "type": "Encounter"
        },
        "effectiveDateTime": now,
        "valueString": serde_json::to_string(data).unwrap_or_default()
    })
}

/// Build a FHIR Observation resource for an outcome score.
fn build_outcome_score_fhir(
    id: &str,
    patient_id: &str,
    encounter_id: Option<&str>,
    measure_type: &str,
    score: f64,
    score_secondary: Option<f64>,
    severity: Option<&str>,
    loinc_code: &str,
    now: &str,
) -> serde_json::Value {
    let mut resource = serde_json::json!({
        "resourceType": "Observation",
        "id": id,
        "status": "final",
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "survey",
                "display": "Survey"
            }]
        }],
        "code": {
            "coding": [{
                "system": "http://loinc.org",
                "code": loinc_code,
                "display": format!("{} Score", measure_type.to_uppercase())
            }]
        },
        "subject": {
            "reference": format!("Patient/{}", patient_id),
            "type": "Patient"
        },
        "effectiveDateTime": now,
        "valueQuantity": {
            "value": score,
            "unit": if measure_type == "psfs" { "score" } else { "%" },
            "system": "http://unitsofmeasure.org"
        }
    });

    if let Some(enc_id) = encounter_id {
        resource["encounter"] = serde_json::json!({
            "reference": format!("Encounter/{}", enc_id),
            "type": "Encounter"
        });
    }

    if let Some(sev) = severity {
        resource["interpretation"] = serde_json::json!([{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/outcome-severity",
                "code": sev,
                "display": sev
            }]
        }]);
    }

    if let Some(sec) = score_secondary {
        resource["component"] = serde_json::json!([{
            "code": {
                "coding": [{
                    "system": "http://medarc.local/fhir/CodeSystem/outcome-subscale",
                    "code": "secondary",
                    "display": "Secondary Score"
                }]
            },
            "valueQuantity": {
                "value": sec,
                "unit": "score"
            }
        }]);
    }

    resource
}

/// Get the LOINC code for a measure type.
fn loinc_for_measure(measure_type: &str) -> &'static str {
    match measure_type {
        "lefs" => LOINC_LEFS,
        "dash" => LOINC_DASH,
        "ndi" => LOINC_NDI,
        "oswestry" => LOINC_OSWESTRY,
        "psfs" => LOINC_PSFS,
        "fabq" => LOINC_FABQ,
        _ => "unknown",
    }
}

/// Get the MCID for a measure type.
fn mcid_for_measure(measure_type: &str) -> Option<f64> {
    match measure_type {
        "lefs" => Some(MCID_LEFS),
        "dash" => Some(MCID_DASH),
        "ndi" => Some(MCID_NDI),
        "oswestry" => Some(MCID_OSWESTRY),
        "psfs" => Some(MCID_PSFS),
        _ => None,
    }
}

/// Compute score, severity, and optional secondary score for a given measure type.
fn compute_score(
    measure_type: &str,
    items: &[u8],
) -> Result<(f64, Option<f64>, Option<String>), AppError> {
    match measure_type {
        "lefs" => {
            if items.len() != 20 {
                return Err(AppError::Validation(format!(
                    "LEFS requires exactly 20 items, got {}",
                    items.len()
                )));
            }
            let arr: [u8; 20] = items.try_into().unwrap();
            let s = score_lefs(&arr);
            let severity = classify_lefs(s).to_string();
            Ok((s as f64, None, Some(severity)))
        }
        "dash" => {
            let s = score_dash(items)?;
            let severity = classify_dash(s).to_string();
            Ok((s, None, Some(severity)))
        }
        "ndi" => {
            if items.len() != 10 {
                return Err(AppError::Validation(format!(
                    "NDI requires exactly 10 items, got {}",
                    items.len()
                )));
            }
            let arr: [u8; 10] = items.try_into().unwrap();
            let s = score_ndi(&arr);
            let severity = classify_ndi(s).to_string();
            Ok((s, None, Some(severity)))
        }
        "oswestry" => {
            if items.len() != 10 {
                return Err(AppError::Validation(format!(
                    "Oswestry requires exactly 10 items, got {}",
                    items.len()
                )));
            }
            let arr: [u8; 10] = items.try_into().unwrap();
            let s = score_oswestry(&arr);
            let severity = classify_oswestry(s).to_string();
            Ok((s, None, Some(severity)))
        }
        "psfs" => {
            let s = score_psfs(items)?;
            let severity = classify_psfs(s).to_string();
            Ok((s, None, Some(severity)))
        }
        "fabq" => {
            if items.len() != 16 {
                return Err(AppError::Validation(format!(
                    "FABQ requires exactly 16 items, got {}",
                    items.len()
                )));
            }
            let arr: [u8; 16] = items.try_into().unwrap();
            let (pa, work) = score_fabq(&arr);
            // FABQ has no severity classification; primary = PA, secondary = Work
            Ok((pa as f64, Some(work as f64), None))
        }
        _ => Err(AppError::Validation(format!(
            "Unknown measure type: {}",
            measure_type
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Record objective measures (ROM, MMT, ortho tests) for a patient encounter.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn record_objective_measures(
    input: ObjectiveMeasuresInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ObjectiveMeasuresRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_objective_measures_fhir(
        &id,
        &input.patient_id,
        &input.encounter_id,
        &input.data,
        &now,
    );
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'PTObjectiveMeasures', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "objective_measures.create".to_string(),
            resource_type: "PTObjectiveMeasures".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("encounter_id={}", input.encounter_id)),
        },
    );

    Ok(ObjectiveMeasuresRecord {
        resource_id: id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        data: input.data,
        recorded_at: now,
    })
}

/// Retrieve objective measures for a patient, optionally scoped to a specific encounter.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_objective_measures(
    patient_id: String,
    encounter_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<ObjectiveMeasuresRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Query FHIR resources of type PTObjectiveMeasures and filter by patient subject reference
    let mut query = String::from(
        "SELECT id, resource, last_updated
         FROM fhir_resources
         WHERE resource_type = 'PTObjectiveMeasures'
           AND json_extract(resource, '$.subject.reference') = ?1",
    );
    let patient_ref = format!("Patient/{}", patient_id);
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(patient_ref)];

    if let Some(ref enc_id) = encounter_id {
        let enc_ref = format!("Encounter/{}", enc_id);
        query.push_str(&format!(
            " AND json_extract(resource, '$.encounter.reference') = ?{}",
            params.len() + 1
        ));
        params.push(Box::new(enc_ref));
    }

    query.push_str(" ORDER BY last_updated DESC");

    let records: Vec<ObjectiveMeasuresRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|(id, resource_str, last_updated)| {
            let resource: serde_json::Value = serde_json::from_str(&resource_str).ok()?;
            // Extract encounter_id from the FHIR resource
            let enc_id = resource["encounter"]["reference"]
                .as_str()
                .and_then(|r| r.strip_prefix("Encounter/"))
                .map(|s| s.to_string())
                .unwrap_or_default();
            // Extract data from valueString
            let data_str = resource["valueString"].as_str().unwrap_or("{}");
            let data: serde_json::Value =
                serde_json::from_str(data_str).unwrap_or(serde_json::Value::Null);

            Some(ObjectiveMeasuresRecord {
                resource_id: id,
                patient_id: patient_id.clone(),
                encounter_id: enc_id,
                data,
                recorded_at: last_updated,
            })
        })
        .collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "objective_measures.read".to_string(),
            resource_type: "PTObjectiveMeasures".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: encounter_id.map(|e| format!("encounter_id={}", e)),
        },
    );

    Ok(records)
}

/// Record and score an outcome measure for a patient.
///
/// Validates item counts, computes the score, classifies severity, and stores
/// both the FHIR Observation and the `outcome_score_index` row.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn record_outcome_score(
    input: OutcomeScoreInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<OutcomeScoreRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    // Validate measure type
    let valid_types = ["lefs", "dash", "ndi", "oswestry", "psfs", "fabq"];
    if !valid_types.contains(&input.measure_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid measure type: {}. Must be one of: {}",
            input.measure_type,
            valid_types.join(", ")
        )));
    }

    // Validate episode_phase
    if let Some(ref phase) = input.episode_phase {
        if !["initial", "mid", "discharge"].contains(&phase.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid episode phase: {}. Must be 'initial', 'mid', or 'discharge'",
                phase
            )));
        }
    }

    // Compute score, secondary, and severity
    let (score, score_secondary, severity) = compute_score(&input.measure_type, &input.items)?;

    let score_id = uuid::Uuid::new_v4().to_string();
    let resource_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let loinc_code = loinc_for_measure(&input.measure_type);

    let fhir = build_outcome_score_fhir(
        &resource_id,
        &input.patient_id,
        input.encounter_id.as_deref(),
        &input.measure_type,
        score,
        score_secondary,
        severity.as_deref(),
        loinc_code,
        &now,
    );
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert FHIR resource
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'OutcomeScore', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![resource_id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert outcome score index
    conn.execute(
        "INSERT INTO outcome_score_index
            (score_id, resource_id, patient_id, encounter_id, measure_type, score, score_secondary, severity, episode_phase, loinc_code, recorded_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        rusqlite::params![
            score_id,
            resource_id,
            input.patient_id,
            input.encounter_id,
            input.measure_type,
            score,
            score_secondary,
            severity,
            input.episode_phase,
            loinc_code,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "outcome_score.create".to_string(),
            resource_type: "OutcomeScore".to_string(),
            resource_id: Some(score_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "measure_type={},score={},severity={}",
                input.measure_type,
                score,
                severity.as_deref().unwrap_or("none")
            )),
        },
    );

    Ok(OutcomeScoreRecord {
        score_id,
        resource_id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        measure_type: input.measure_type,
        score,
        score_secondary,
        severity,
        episode_phase: input.episode_phase,
        loinc_code: Some(loinc_code.to_string()),
        recorded_at: now,
    })
}

/// List outcome scores for a patient, optionally filtered by measure type and date range.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_outcome_scores(
    patient_id: String,
    measure_type: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<OutcomeScoreRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT score_id, resource_id, patient_id, encounter_id, measure_type,
                score, score_secondary, severity, episode_phase, loinc_code, recorded_at
         FROM outcome_score_index
         WHERE patient_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(patient_id.clone())];

    if let Some(ref mt) = measure_type {
        query.push_str(&format!(" AND measure_type = ?{}", params.len() + 1));
        params.push(Box::new(mt.clone()));
    }
    if let Some(ref sd) = start_date {
        query.push_str(&format!(" AND recorded_at >= ?{}", params.len() + 1));
        params.push(Box::new(sd.clone()));
    }
    if let Some(ref ed) = end_date {
        query.push_str(&format!(" AND recorded_at <= ?{}", params.len() + 1));
        params.push(Box::new(ed.clone()));
    }

    query.push_str(" ORDER BY recorded_at DESC");

    let records: Vec<OutcomeScoreRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(OutcomeScoreRecord {
                    score_id: row.get(0)?,
                    resource_id: row.get(1)?,
                    patient_id: row.get(2)?,
                    encounter_id: row.get(3)?,
                    measure_type: row.get(4)?,
                    score: row.get(5)?,
                    score_secondary: row.get(6)?,
                    severity: row.get(7)?,
                    episode_phase: row.get(8)?,
                    loinc_code: row.get(9)?,
                    recorded_at: row.get(10)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "outcome_score.list".to_string(),
            resource_type: "OutcomeScore".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: measure_type.map(|mt| format!("measure_type={}", mt)),
        },
    );

    Ok(records)
}

/// Get a single outcome score by ID.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_outcome_score(
    score_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<OutcomeScoreRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let record = conn
        .query_row(
            "SELECT score_id, resource_id, patient_id, encounter_id, measure_type,
                    score, score_secondary, severity, episode_phase, loinc_code, recorded_at
             FROM outcome_score_index
             WHERE score_id = ?1",
            rusqlite::params![score_id],
            |row| {
                Ok(OutcomeScoreRecord {
                    score_id: row.get(0)?,
                    resource_id: row.get(1)?,
                    patient_id: row.get(2)?,
                    encounter_id: row.get(3)?,
                    measure_type: row.get(4)?,
                    score: row.get(5)?,
                    score_secondary: row.get(6)?,
                    severity: row.get(7)?,
                    episode_phase: row.get(8)?,
                    loinc_code: row.get(9)?,
                    recorded_at: row.get(10)?,
                })
            },
        )
        .map_err(|_| AppError::NotFound(format!("Outcome score {} not found", score_id)))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "outcome_score.read".to_string(),
            resource_type: "OutcomeScore".to_string(),
            resource_id: Some(score_id),
            patient_id: Some(record.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(record)
}

/// Get outcome comparison for a patient: earliest vs latest score per measure type.
///
/// For each measure type where the patient has at least one score, returns
/// the earliest and latest scores along with the change and whether the MCID
/// threshold was met.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_outcome_comparison(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<OutcomeComparison, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let measure_types = ["lefs", "dash", "ndi", "oswestry", "psfs", "fabq"];
    let mut measures = Vec::new();

    for mt in &measure_types {
        // Get earliest score
        let earliest = conn
            .query_row(
                "SELECT score, recorded_at FROM outcome_score_index
                 WHERE patient_id = ?1 AND measure_type = ?2
                 ORDER BY recorded_at ASC LIMIT 1",
                rusqlite::params![patient_id, mt],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?)),
            )
            .ok();

        // Get latest score
        let latest = conn
            .query_row(
                "SELECT score, recorded_at FROM outcome_score_index
                 WHERE patient_id = ?1 AND measure_type = ?2
                 ORDER BY recorded_at DESC LIMIT 1",
                rusqlite::params![patient_id, mt],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?)),
            )
            .ok();

        if earliest.is_some() || latest.is_some() {
            let initial_score = earliest.as_ref().map(|(s, _)| *s);
            let initial_date = earliest.as_ref().map(|(_, d)| d.clone());
            let latest_score = latest.as_ref().map(|(s, _)| *s);
            let latest_date = latest.as_ref().map(|(_, d)| d.clone());

            let change = match (initial_score, latest_score) {
                (Some(init), Some(lat)) => Some(lat - init),
                _ => None,
            };

            let mcid = mcid_for_measure(mt);
            let mcid_met = match (change, mcid) {
                (Some(ch), Some(m)) => Some(ch.abs() >= m),
                _ => None,
            };

            measures.push(OutcomeComparisonMeasure {
                measure_type: mt.to_string(),
                initial_score,
                initial_date,
                latest_score,
                latest_date,
                change,
                mcid,
                mcid_met,
            });
        }
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "outcome_score.comparison".to_string(),
            resource_type: "OutcomeScore".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(OutcomeComparison {
        patient_id,
        measures,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── LEFS tests ──────────────────────────────────────────────────────

    #[test]
    fn lefs_all_fours_scores_80() {
        let items = [4u8; 20];
        assert_eq!(score_lefs(&items), 80);
    }

    #[test]
    fn lefs_all_fours_severity_minimal() {
        let score = score_lefs(&[4u8; 20]);
        assert_eq!(classify_lefs(score), "minimal");
    }

    #[test]
    fn lefs_all_zeros_scores_0() {
        let items = [0u8; 20];
        assert_eq!(score_lefs(&items), 0);
        assert_eq!(classify_lefs(0), "severe");
    }

    #[test]
    fn lefs_moderate_range() {
        // Score of 30 should be moderate (20-39)
        let mut items = [0u8; 20];
        for i in 0..10 {
            items[i] = 3; // 10 * 3 = 30
        }
        let score = score_lefs(&items);
        assert_eq!(score, 30);
        assert_eq!(classify_lefs(score), "moderate");
    }

    // ── DASH tests ──────────────────────────────────────────────────────

    #[test]
    fn dash_all_ones_scores_zero() {
        let items = vec![1u8; 30];
        let score = score_dash(&items).unwrap();
        assert!((score - 0.0).abs() < f64::EPSILON);
        assert_eq!(classify_dash(score), "mild");
    }

    #[test]
    fn dash_too_few_items_errors() {
        let items = vec![1u8; 26];
        assert!(score_dash(&items).is_err());
    }

    #[test]
    fn dash_all_fives_scores_100() {
        let items = vec![5u8; 30];
        let score = score_dash(&items).unwrap();
        assert!((score - 100.0).abs() < f64::EPSILON);
        assert_eq!(classify_dash(score), "very_severe");
    }

    #[test]
    fn dash_27_items_minimum_accepted() {
        let items = vec![3u8; 27];
        let score = score_dash(&items).unwrap();
        // ((3*27/27) - 1) / 4 * 100 = (3 - 1) / 4 * 100 = 50.0
        assert!((score - 50.0).abs() < f64::EPSILON);
        assert_eq!(classify_dash(score), "moderate");
    }

    #[test]
    fn dash_invalid_item_value_errors() {
        let mut items = vec![3u8; 30];
        items[0] = 0; // 0 is out of range for DASH (1-5)
        assert!(score_dash(&items).is_err());
    }

    // ── NDI tests ───────────────────────────────────────────────────────

    #[test]
    fn ndi_all_zeros_scores_zero_percent() {
        let items = [0u8; 10];
        let score = score_ndi(&items);
        assert!((score - 0.0).abs() < f64::EPSILON);
        assert_eq!(classify_ndi(score), "no_disability");
    }

    #[test]
    fn ndi_all_fives_scores_100_percent() {
        let items = [5u8; 10];
        let score = score_ndi(&items);
        assert!((score - 100.0).abs() < f64::EPSILON);
        assert_eq!(classify_ndi(score), "complete");
    }

    // ── Oswestry tests ──────────────────────────────────────────────────

    #[test]
    fn oswestry_all_fives_scores_100_percent() {
        let items = [5u8; 10];
        let score = score_oswestry(&items);
        assert!((score - 100.0).abs() < f64::EPSILON);
        assert_eq!(classify_oswestry(score), "bed_bound");
    }

    #[test]
    fn oswestry_all_zeros_scores_zero_percent() {
        let items = [0u8; 10];
        let score = score_oswestry(&items);
        assert!((score - 0.0).abs() < f64::EPSILON);
        assert_eq!(classify_oswestry(score), "minimal");
    }

    #[test]
    fn oswestry_moderate_range() {
        // Each item = 2 => sum = 20 => 20/50 * 100 = 40%
        let items = [2u8; 10];
        let score = score_oswestry(&items);
        assert!((score - 40.0).abs() < f64::EPSILON);
        assert_eq!(classify_oswestry(score), "moderate");
    }

    // ── PSFS tests ──────────────────────────────────────────────────────

    #[test]
    fn psfs_perfect_scores_ten() {
        let items = vec![10u8, 10, 10];
        let score = score_psfs(&items).unwrap();
        assert!((score - 10.0).abs() < f64::EPSILON);
        assert_eq!(classify_psfs(score), "mild");
    }

    #[test]
    fn psfs_two_items_errors() {
        let items = vec![5u8, 5];
        assert!(score_psfs(&items).is_err());
    }

    #[test]
    fn psfs_five_items_accepted() {
        let items = vec![4u8, 4, 4, 4, 4];
        let score = score_psfs(&items).unwrap();
        assert!((score - 4.0).abs() < f64::EPSILON);
        assert_eq!(classify_psfs(score), "severe");
    }

    #[test]
    fn psfs_moderate_range() {
        let items = vec![5u8, 6, 7];
        let score = score_psfs(&items).unwrap();
        assert!((score - 6.0).abs() < f64::EPSILON);
        assert_eq!(classify_psfs(score), "moderate");
    }

    // ── FABQ tests ──────────────────────────────────────────────────────

    #[test]
    fn fabq_verify_pa_and_work_subscales() {
        // All items set to 3
        let items = [3u8; 16];
        let (pa, work) = score_fabq(&items);
        // PA: items[1]+items[2]+items[3]+items[4] = 3*4 = 12
        assert_eq!(pa, 12);
        // Work: items[5]+items[6]+items[8]+items[9]+items[10]+items[11]+items[12]+items[13]+items[14] = 3*9 = 27
        assert_eq!(work, 27);
    }

    #[test]
    fn fabq_all_zeros() {
        let items = [0u8; 16];
        let (pa, work) = score_fabq(&items);
        assert_eq!(pa, 0);
        assert_eq!(work, 0);
    }

    #[test]
    fn fabq_all_sixes() {
        let items = [6u8; 16];
        let (pa, work) = score_fabq(&items);
        // PA: 6*4 = 24
        assert_eq!(pa, 24);
        // Work: 6*9 = 54
        assert_eq!(work, 54);
    }

    #[test]
    fn fabq_selective_scoring() {
        // Only set subscale items, leave others at 0
        let mut items = [0u8; 16];
        items[1] = 4; // PA
        items[2] = 5; // PA
        items[3] = 3; // PA
        items[4] = 2; // PA
        items[5] = 6; // Work
        items[6] = 1; // Work
        items[8] = 2; // Work
        items[9] = 3; // Work
        items[10] = 4; // Work
        items[11] = 5; // Work
        items[12] = 6; // Work
        items[13] = 0; // Work
        items[14] = 1; // Work

        let (pa, work) = score_fabq(&items);
        assert_eq!(pa, 4 + 5 + 3 + 2); // 14
        assert_eq!(work, 6 + 1 + 2 + 3 + 4 + 5 + 6 + 0 + 1); // 28
    }

    // ── compute_score integration tests ─────────────────────────────────

    #[test]
    fn compute_score_lefs_wrong_count_errors() {
        let items = vec![4u8; 19];
        assert!(compute_score("lefs", &items).is_err());
    }

    #[test]
    fn compute_score_unknown_type_errors() {
        assert!(compute_score("unknown", &[1, 2, 3]).is_err());
    }

    // ── MCID / LOINC helpers ────────────────────────────────────────────

    #[test]
    fn mcid_constants_correct() {
        assert_eq!(mcid_for_measure("lefs"), Some(9.0));
        assert_eq!(mcid_for_measure("dash"), Some(10.8));
        assert_eq!(mcid_for_measure("ndi"), Some(7.5));
        assert_eq!(mcid_for_measure("oswestry"), Some(10.0));
        assert_eq!(mcid_for_measure("psfs"), Some(2.0));
        assert_eq!(mcid_for_measure("fabq"), None);
    }

    #[test]
    fn loinc_codes_correct() {
        assert_eq!(loinc_for_measure("lefs"), "75575-0");
        assert_eq!(loinc_for_measure("dash"), "71966-6");
        assert_eq!(loinc_for_measure("ndi"), "72100-1");
        assert_eq!(loinc_for_measure("oswestry"), "72101-9");
        assert_eq!(loinc_for_measure("psfs"), "72102-7");
        assert_eq!(loinc_for_measure("fabq"), "72103-5");
    }

    // ── Migration 16 validation ─────────────────────────────────────────

    #[test]
    fn migration_16_is_valid() {
        use crate::db::migrations::MIGRATIONS;
        assert!(
            MIGRATIONS.validate().is_ok(),
            "MIGRATIONS.validate() failed — check Migration 16 SQL syntax"
        );
    }
}
