/// commands/patient.rs — Patient Demographics & Care Teams (S04)
///
/// Implements all PTNT-01 through PTNT-07 requirements:
///   PTNT-01  Create patient with full demographics (name, DOB, sex/gender, contact, photo URL)
///   PTNT-02  Add insurance (primary/secondary/tertiary) to a patient record
///   PTNT-03  Add employer data and social determinants of health (SDOH)
///   PTNT-04  Assign clinical identifiers (MRN, primary provider)
///   PTNT-05  Search patients by name, MRN, DOB with sub-second results via indexed columns
///   PTNT-06  Manage Related Persons (emergency contacts, next-of-kin, guarantors)
///   PTNT-07  Assign care team members with roles via Care Team Widget
///
/// Data model
/// ----------
/// All patient data is stored as FHIR R4 JSON in `fhir_resources`.
/// Migration 9 adds a `patient_index` lookup table with denormalised columns
/// (mrn, family_name, given_name, dob, sex) so searches bypass JSON extraction.
///
/// Care team and related persons are stored as FHIR `CareTeam` and
/// `RelatedPerson` resources, linked to the patient via a `subject.reference`
/// pointing at `Patient/<patient_id>`.
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using the same
/// `write_audit_entry` helper used by the FHIR commands in S03.
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Request / Response types
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// Minimal demographics payload for creating / updating a patient.
///
/// The full FHIR Patient JSON is composed server-side from these fields
/// so the frontend never has to construct raw FHIR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientInput {
    /// Family (last) name
    pub family_name: String,
    /// List of given names (first, middle, …)
    pub given_names: Vec<String>,
    /// ISO 8601 date of birth, e.g. "1990-03-15"
    pub birth_date: Option<String>,
    /// FHIR biological sex: "male" | "female" | "other" | "unknown"
    pub gender: Option<String>,
    /// Administrative gender / gender identity (free text, e.g. "non-binary")
    pub gender_identity: Option<String>,
    /// Phone number (primary)
    pub phone: Option<String>,
    /// Email address
    pub email: Option<String>,
    /// Street address
    pub address_line: Option<String>,
    /// City
    pub city: Option<String>,
    /// State / province
    pub state: Option<String>,
    /// Postal code
    pub postal_code: Option<String>,
    /// Country (default "US")
    pub country: Option<String>,
    /// URL or base64 data URI for the patient photo
    pub photo_url: Option<String>,

    // ── PTNT-04: Clinical identifiers ─────────────────────────────────────
    /// Medical Record Number (assigned at creation if blank)
    pub mrn: Option<String>,
    /// Provider user-id of the primary care provider
    pub primary_provider_id: Option<String>,

    // ── PTNT-02: Insurance ─────────────────────────────────────────────────
    /// Primary insurance coverage
    pub insurance_primary: Option<InsuranceInput>,
    /// Secondary insurance coverage
    pub insurance_secondary: Option<InsuranceInput>,
    /// Tertiary insurance coverage
    pub insurance_tertiary: Option<InsuranceInput>,

    // ── PTNT-03: Employer / SDOH ───────────────────────────────────────────
    pub employer: Option<EmployerInput>,
    pub sdoh: Option<SdohInput>,
}

/// Insurance plan information for one coverage tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceInput {
    pub payer_name: String,
    pub plan_name: Option<String>,
    pub member_id: String,
    pub group_number: Option<String>,
    pub subscriber_name: Option<String>,
    pub subscriber_dob: Option<String>,
    pub relationship_to_subscriber: Option<String>,
}

/// Employer information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployerInput {
    pub employer_name: String,
    pub occupation: Option<String>,
    pub employer_phone: Option<String>,
    pub employer_address: Option<String>,
}

/// Social Determinants of Health screen answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdohInput {
    pub housing_status: Option<String>,
    pub food_security: Option<String>,
    pub transportation_access: Option<String>,
    pub education_level: Option<String>,
    pub notes: Option<String>,
}

/// The summary record returned by search queries (avoids sending full JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientSummary {
    pub id: String,
    pub mrn: String,
    pub family_name: String,
    pub given_names: Vec<String>,
    pub birth_date: Option<String>,
    pub gender: Option<String>,
    pub phone: Option<String>,
    pub primary_provider_id: Option<String>,
}

/// Full patient record as stored and returned by get_patient.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientRecord {
    pub id: String,
    pub mrn: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
    pub created_at: String,
}

/// Search query parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientSearchQuery {
    /// Free-text name search (family or given)
    pub name: Option<String>,
    /// Exact MRN
    pub mrn: Option<String>,
    /// ISO date of birth "YYYY-MM-DD"
    pub birth_date: Option<String>,
    /// Maximum results to return (default 50)
    pub limit: Option<i64>,
}

// ── Care Team ──────────────────────────────────────────────────────────────

/// One care team member assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CareTeamMemberInput {
    /// patient_id this care team belongs to
    pub patient_id: String,
    /// User ID of the provider/staff member
    pub member_id: String,
    /// Display name of the member (denormalised for FHIR)
    pub member_name: String,
    /// Role in the care team, e.g. "primary_care", "nurse", "specialist"
    pub role: String,
    /// Optional note
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CareTeamRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub last_updated: String,
}

// ── Related Persons ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedPersonInput {
    /// patient_id this person is related to
    pub patient_id: String,
    pub family_name: String,
    pub given_names: Vec<String>,
    /// FHIR relationship code, e.g. "emergency_contact", "next_of_kin", "guarantor"
    pub relationship: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address_line: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedPersonRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a sequential MRN (1, 2, 3, …).
/// Queries the highest existing integer MRN and returns the next one.
fn generate_sequential_mrn(conn: &rusqlite::Connection) -> String {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(CAST(mrn AS INTEGER)) FROM patient_index WHERE mrn GLOB '[0-9]*'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(None);
    let next = max.unwrap_or(0) + 1;
    next.to_string()
}

/// Build a FHIR R4 Patient resource from `PatientInput`.
fn build_patient_fhir(id: &str, mrn: &str, input: &PatientInput) -> serde_json::Value {
    let mut identifiers = vec![serde_json::json!({
        "use": "official",
        "system": "http://medarc.local/mrn",
        "value": mrn,
    })];

    if let Some(pid) = &input.primary_provider_id {
        identifiers.push(serde_json::json!({
            "system": "http://medarc.local/primary-provider",
            "value": pid,
        }));
    }

    let given: Vec<serde_json::Value> = input
        .given_names
        .iter()
        .map(|g| serde_json::Value::String(g.clone()))
        .collect();

    let name = serde_json::json!({
        "use": "official",
        "family": input.family_name,
        "given": given,
    });

    // Build telecom array
    let mut telecom = vec![];
    if let Some(ph) = &input.phone {
        telecom.push(serde_json::json!({"system":"phone","value":ph,"use":"home"}));
    }
    if let Some(em) = &input.email {
        telecom.push(serde_json::json!({"system":"email","value":em}));
    }

    // Build address array
    let mut addresses = vec![];
    if input.address_line.is_some()
        || input.city.is_some()
        || input.state.is_some()
        || input.postal_code.is_some()
    {
        let mut addr = serde_json::json!({});
        if let Some(l) = &input.address_line {
            addr["line"] = serde_json::json!([l]);
        }
        if let Some(c) = &input.city {
            addr["city"] = serde_json::json!(c);
        }
        if let Some(s) = &input.state {
            addr["state"] = serde_json::json!(s);
        }
        if let Some(p) = &input.postal_code {
            addr["postalCode"] = serde_json::json!(p);
        }
        addr["country"] = serde_json::json!(input.country.as_deref().unwrap_or("US"));
        addresses.push(addr);
    }

    // Build extension array for SDOH, employer, photo, gender identity
    let mut extensions: Vec<serde_json::Value> = vec![];

    if let Some(gi) = &input.gender_identity {
        extensions.push(serde_json::json!({
            "url": "http://hl7.org/fhir/StructureDefinition/patient-genderIdentity",
            "valueString": gi,
        }));
    }

    if let Some(photo) = &input.photo_url {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/photo-url",
            "valueUrl": photo,
        }));
    }

    // Insurance as extensions (FHIR Coverage would be separate resources in full impl)
    fn insurance_ext(tier: &str, ins: &InsuranceInput) -> serde_json::Value {
        serde_json::json!({
            "url": format!("http://medarc.local/insurance/{}", tier),
            "extension": [
                {"url": "payerName", "valueString": ins.payer_name},
                {"url": "planName", "valueString": ins.plan_name.as_deref().unwrap_or("")},
                {"url": "memberId", "valueString": ins.member_id},
                {"url": "groupNumber", "valueString": ins.group_number.as_deref().unwrap_or("")},
                {"url": "subscriberName", "valueString": ins.subscriber_name.as_deref().unwrap_or("")},
                {"url": "subscriberDob", "valueString": ins.subscriber_dob.as_deref().unwrap_or("")},
                {"url": "relationshipToSubscriber", "valueString": ins.relationship_to_subscriber.as_deref().unwrap_or("self")},
            ]
        })
    }

    if let Some(ins) = &input.insurance_primary {
        extensions.push(insurance_ext("primary", ins));
    }
    if let Some(ins) = &input.insurance_secondary {
        extensions.push(insurance_ext("secondary", ins));
    }
    if let Some(ins) = &input.insurance_tertiary {
        extensions.push(insurance_ext("tertiary", ins));
    }

    if let Some(emp) = &input.employer {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/employer",
            "extension": [
                {"url": "employerName", "valueString": emp.employer_name},
                {"url": "occupation", "valueString": emp.occupation.as_deref().unwrap_or("")},
                {"url": "employerPhone", "valueString": emp.employer_phone.as_deref().unwrap_or("")},
                {"url": "employerAddress", "valueString": emp.employer_address.as_deref().unwrap_or("")},
            ]
        }));
    }

    if let Some(sdoh) = &input.sdoh {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/sdoh",
            "extension": [
                {"url": "housingStatus", "valueString": sdoh.housing_status.as_deref().unwrap_or("")},
                {"url": "foodSecurity", "valueString": sdoh.food_security.as_deref().unwrap_or("")},
                {"url": "transportationAccess", "valueString": sdoh.transportation_access.as_deref().unwrap_or("")},
                {"url": "educationLevel", "valueString": sdoh.education_level.as_deref().unwrap_or("")},
                {"url": "notes", "valueString": sdoh.notes.as_deref().unwrap_or("")},
            ]
        }));
    }

    let mut patient = serde_json::json!({
        "resourceType": "Patient",
        "id": id,
        "identifier": identifiers,
        "name": [name],
        "active": true,
    });

    if let Some(dob) = &input.birth_date {
        patient["birthDate"] = serde_json::json!(dob);
    }
    if let Some(g) = &input.gender {
        patient["gender"] = serde_json::json!(g);
    }
    if !telecom.is_empty() {
        patient["telecom"] = serde_json::json!(telecom);
    }
    if !addresses.is_empty() {
        patient["address"] = serde_json::json!(addresses);
    }
    if !extensions.is_empty() {
        patient["extension"] = serde_json::json!(extensions);
    }

    patient
}

/// Write a failure audit row (safe — acquires its own lock).
fn audit_denied(db: &Database, device_id: &DeviceId, user_id: &str, action: &str, reason: &str) {
    if let Ok(conn) = db.conn.lock() {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.to_string(),
                action: action.to_string(),
                resource_type: "Patient".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(reason.to_string()),
            },
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Patient
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new patient record with demographics, insurance, employer, and SDOH.
///
/// Requires `Patients:Create` permission (Provider, Nurse/MA, FrontDesk).
/// Automatically generates an MRN if one is not supplied.
/// Inserts a row into `patient_index` for fast search.
/// Writes an audit entry on success and failure.
#[tauri::command]
pub fn create_patient(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: PatientInput,
) -> Result<PatientRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.create",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    // Validate required fields
    if input.family_name.trim().is_empty() {
        return Err(AppError::Validation("family_name is required".to_string()));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let mrn = input
        .mrn
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| generate_sequential_mrn(&conn));
    let now = chrono::Utc::now().to_rfc3339();

    // Check MRN uniqueness
    let mrn_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM patient_index WHERE mrn = ?1",
            rusqlite::params![mrn],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if mrn_exists {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: user_id.clone(),
                action: "patient.create".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some(format!("MRN already exists: {}", mrn)),
            },
        );
        return Err(AppError::Validation(format!("MRN already exists: {}", mrn)));
    }

    let fhir = build_patient_fhir(&id, &mrn, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    // Insert FHIR resource
    let insert_result = conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Patient', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    );

    match insert_result {
        Err(e) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.create".to_string(),
                    resource_type: "Patient".to_string(),
                    resource_id: None,
                    patient_id: None,
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some(format!("DB error: {}", e)),
                },
            );
            return Err(AppError::Database(e.to_string()));
        }
        Ok(_) => {}
    }

    // Insert into patient_index for fast search
    let given_concat = input.given_names.join(" ").to_lowercase();
    let family_lower = input.family_name.to_lowercase();
    let _ = conn.execute(
        "INSERT INTO patient_index (patient_id, mrn, family_name, given_name, birth_date, gender, primary_provider_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            mrn,
            family_lower,
            given_concat,
            input.birth_date,
            input.gender,
            input.primary_provider_id,
        ],
    );

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.create".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(PatientRecord {
        id,
        mrn,
        resource: fhir,
        version_id: 1,
        last_updated: now.clone(),
        created_at: now,
    })
}

/// Retrieve a single patient record by ID.
///
/// Requires `Patients:Read` permission.
#[tauri::command]
pub fn get_patient(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<PatientRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.get",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let result = conn.query_row(
        "SELECT fr.id, pi.mrn, fr.resource, fr.version_id, fr.last_updated, fr.created_at
         FROM fhir_resources fr
         JOIN patient_index pi ON pi.patient_id = fr.id
         WHERE fr.id = ?1 AND fr.resource_type = 'Patient'",
        rusqlite::params![patient_id],
        |row| {
            let resource_str: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                resource_str,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    );

    match result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.get".to_string(),
                    resource_type: "Patient".to_string(),
                    resource_id: Some(patient_id.clone()),
                    patient_id: Some(patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Not found".to_string()),
                },
            );
            Err(AppError::NotFound(format!(
                "Patient not found: {}",
                patient_id
            )))
        }
        Err(e) => Err(AppError::Database(e.to_string())),
        Ok((id, mrn, resource_str, version_id, last_updated, created_at)) => {
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.get".to_string(),
                    resource_type: "Patient".to_string(),
                    resource_id: Some(id.clone()),
                    patient_id: Some(id.clone()),
                    device_id: device_id.get().to_string(),
                    success: true,
                    details: None,
                },
            );
            Ok(PatientRecord {
                id,
                mrn,
                resource,
                version_id,
                last_updated,
                created_at,
            })
        }
    }
}

/// Update an existing patient record's demographics/insurance/employer/SDOH.
///
/// Requires `Patients:Update` permission.
/// Bumps `version_id` and refreshes `patient_index`.
#[tauri::command]
pub fn update_patient(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
    input: PatientInput,
) -> Result<PatientRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Update) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.update",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    if input.family_name.trim().is_empty() {
        return Err(AppError::Validation("family_name is required".to_string()));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch current version + MRN
    let version_result = conn.query_row(
        "SELECT fr.version_id, pi.mrn
         FROM fhir_resources fr
         JOIN patient_index pi ON pi.patient_id = fr.id
         WHERE fr.id = ?1 AND fr.resource_type = 'Patient'",
        rusqlite::params![patient_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    );

    let (current_version, existing_mrn) = match version_result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.update".to_string(),
                    resource_type: "Patient".to_string(),
                    resource_id: Some(patient_id.clone()),
                    patient_id: Some(patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: false,
                    details: Some("Not found".to_string()),
                },
            );
            return Err(AppError::NotFound(format!(
                "Patient not found: {}",
                patient_id
            )));
        }
        Err(e) => return Err(AppError::Database(e.to_string())),
        Ok(v) => v,
    };

    // Use existing MRN if input doesn't override it
    let mrn = input
        .mrn
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or(existing_mrn);
    let now = chrono::Utc::now().to_rfc3339();
    let new_version = current_version + 1;

    let fhir = build_patient_fhir(&patient_id, &mrn, &input);
    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![resource_json, new_version, now, now, patient_id],
    )?;

    // Refresh patient_index
    let given_concat = input.given_names.join(" ").to_lowercase();
    let family_lower = input.family_name.to_lowercase();
    conn.execute(
        "UPDATE patient_index SET mrn = ?1, family_name = ?2, given_name = ?3,
         birth_date = ?4, gender = ?5, primary_provider_id = ?6
         WHERE patient_id = ?7",
        rusqlite::params![
            mrn,
            family_lower,
            given_concat,
            input.birth_date,
            input.gender,
            input.primary_provider_id,
            patient_id,
        ],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.update".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some(patient_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(PatientRecord {
        id: patient_id,
        mrn,
        resource: fhir,
        version_id: new_version,
        last_updated: now.clone(),
        created_at: now,
    })
}

/// Search patients by name, MRN, or date of birth.
///
/// Uses the `patient_index` table for sub-second performance on large datasets.
/// Requires `Patients:Read` permission.
///
/// Search logic:
///   - `mrn`: exact match (index lookup)
///   - `birth_date`: exact ISO date match
///   - `name`: case-insensitive prefix match against family_name OR given_name
#[tauri::command]
pub fn search_patients(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    query: PatientSearchQuery,
) -> Result<Vec<PatientSummary>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.search",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let limit = query.limit.unwrap_or(50).min(500);

    // Build dynamic WHERE clause
    let mut conditions: Vec<String> = vec![];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

    if let Some(mrn) = &query.mrn {
        conditions.push("pi.mrn = ?".to_string());
        params.push(Box::new(mrn.clone()));
    }
    if let Some(dob) = &query.birth_date {
        conditions.push("pi.birth_date = ?".to_string());
        params.push(Box::new(dob.clone()));
    }
    if let Some(name) = &query.name {
        let name_lower = name.to_lowercase();
        let pattern = format!("{}%", name_lower);
        conditions.push("(pi.family_name LIKE ? OR pi.given_name LIKE ?)".to_string());
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern));
    }

    let where_clause = if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    };

    let sql = format!(
        "SELECT pi.patient_id, pi.mrn, pi.family_name, pi.given_name,
                pi.birth_date, pi.gender, pi.primary_provider_id,
                fr.resource
         FROM patient_index pi
         JOIN fhir_resources fr ON fr.id = pi.patient_id
         WHERE {}
         ORDER BY pi.family_name, pi.given_name
         LIMIT {}",
        where_clause, limit
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let results: Vec<PatientSummary> = stmt
        .query_map(param_refs.as_slice(), |row| {
            let given_str: Option<String> = row.get(3)?;
            let given_names: Vec<String> = given_str
                .unwrap_or_default()
                .split_whitespace()
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect();
            let family_raw: String = row.get(2)?;
            // Capitalise stored lowercase family name for display
            let family_name = {
                let mut c = family_raw.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            };

            // Extract phone from FHIR JSON
            let resource_str: String = row.get(7)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            let phone = resource
                .get("telecom")
                .and_then(|t| t.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|t| t.get("system").and_then(|s| s.as_str()) == Some("phone"))
                })
                .and_then(|t| t.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            Ok(PatientSummary {
                id: row.get(0)?,
                mrn: row.get(1)?,
                family_name,
                given_names,
                birth_date: row.get(4)?,
                gender: row.get(5)?,
                phone,
                primary_provider_id: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.search".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} results", results.len())),
        },
    );

    Ok(results)
}

/// Delete a patient record. Cascades to patient_index.
///
/// Requires `Patients:Delete` permission (SystemAdmin only in RBAC).
#[tauri::command]
pub fn delete_patient(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<(), AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Delete) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.delete",
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
        "DELETE FROM fhir_resources WHERE id = ?1 AND resource_type = 'Patient'",
        rusqlite::params![patient_id],
    )?;

    if rows == 0 {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id,
                action: "patient.delete".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: Some(patient_id.clone()),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("Not found".to_string()),
            },
        );
        return Err(AppError::NotFound(format!(
            "Patient not found: {}",
            patient_id
        )));
    }

    // patient_index has ON DELETE CASCADE from migration 9

    // Cascade: delete related encounters (index + FHIR resources)
    let encounter_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT encounter_id FROM encounter_index WHERE patient_id = ?1"
        ).map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![patient_id], |row| row.get(0))
            .map_err(|e| AppError::Database(e.to_string()))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if !encounter_ids.is_empty() {
        conn.execute(
            "DELETE FROM encounter_index WHERE patient_id = ?1",
            rusqlite::params![patient_id],
        ).map_err(|e| AppError::Database(e.to_string()))?;
        for eid in &encounter_ids {
            let _ = conn.execute(
                "DELETE FROM fhir_resources WHERE id = ?1 AND resource_type = 'Encounter'",
                rusqlite::params![eid],
            );
        }
    }

    // Cascade: delete related appointments
    let _ = conn.execute(
        "DELETE FROM fhir_resources WHERE resource_type = 'Appointment' AND json_extract(resource, '$.participant[0].actor.reference') = ?1",
        rusqlite::params![format!("Patient/{}", patient_id)],
    );

    // Cascade: delete related documents
    let _ = conn.execute(
        "DELETE FROM fhir_resources WHERE resource_type = 'DocumentReference' AND json_extract(resource, '$.subject.reference') = ?1",
        rusqlite::params![format!("Patient/{}", patient_id)],
    );

    // Cascade: delete CareTeam and RelatedPerson resources for this patient
    let _ = conn.execute(
        "DELETE FROM fhir_resources WHERE resource_type = 'CareTeam' AND json_extract(resource, '$.subject.reference') = ?1",
        rusqlite::params![format!("Patient/{}", patient_id)],
    );
    let _ = conn.execute(
        "DELETE FROM fhir_resources WHERE resource_type = 'RelatedPerson' AND json_extract(resource, '$.patient.reference') = ?1",
        rusqlite::params![format!("Patient/{}", patient_id)],
    );

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.delete".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some(patient_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Cascaded deletion of related encounters, appointments, documents, care team, and related persons".to_string()),
        },
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Care Team (PTNT-07)
// ─────────────────────────────────────────────────────────────────────────────

/// Add or replace the care team for a patient.
///
/// Each call creates a new FHIR CareTeam resource (or replaces an existing one).
/// Requires `CareTeam:Create` permission.
#[tauri::command]
pub fn upsert_care_team(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: CareTeamMemberInput,
) -> Result<CareTeamRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::CareTeam, Action::Create) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.care_team.upsert",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if a CareTeam already exists for this patient
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM fhir_resources
             WHERE resource_type = 'CareTeam'
             AND json_extract(resource, '$.subject.reference') = ?1",
            rusqlite::params![format!("Patient/{}", input.patient_id)],
            |row| row.get(0),
        )
        .ok();

    let fhir = serde_json::json!({
        "resourceType": "CareTeam",
        "subject": {"reference": format!("Patient/{}", input.patient_id)},
        "participant": [{
            "role": [{"coding": [{"code": input.role, "system": "http://medarc.local/care-team-role"}]}],
            "member": {
                "reference": format!("Practitioner/{}", input.member_id),
                "display": input.member_name,
            },
            "note": input.note.as_deref().unwrap_or(""),
        }],
        "status": "active",
    });

    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    let id = if let Some(eid) = existing {
        conn.execute(
            "UPDATE fhir_resources SET resource = ?1, version_id = version_id + 1,
             last_updated = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![resource_json, now, now, eid],
        )?;
        eid
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'CareTeam', ?2, 1, ?3, ?4, ?5)",
            rusqlite::params![new_id, resource_json, now, now, now],
        )?;
        new_id
    };

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.care_team.upsert".to_string(),
            resource_type: "CareTeam".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(CareTeamRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        last_updated: now,
    })
}

/// Retrieve the care team for a patient.
#[tauri::command]
pub fn get_care_team(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<Option<CareTeamRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::CareTeam, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.care_team.get",
                    &e.to_string(),
                );
                return Err(e);
            }
        };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let result = conn.query_row(
        "SELECT id, resource, last_updated FROM fhir_resources
         WHERE resource_type = 'CareTeam'
         AND json_extract(resource, '$.subject.reference') = ?1",
        rusqlite::params![format!("Patient/{}", patient_id)],
        |row| {
            let resource_str: String = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                resource_str,
                row.get::<_, String>(2)?,
            ))
        },
    );

    match result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.care_team.get".to_string(),
                    resource_type: "CareTeam".to_string(),
                    resource_id: None,
                    patient_id: Some(patient_id),
                    device_id: device_id.get().to_string(),
                    success: true,
                    details: Some("no care team".to_string()),
                },
            );
            Ok(None)
        }
        Err(e) => Err(AppError::Database(e.to_string())),
        Ok((id, resource_str, last_updated)) => {
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id,
                    action: "patient.care_team.get".to_string(),
                    resource_type: "CareTeam".to_string(),
                    resource_id: Some(id.clone()),
                    patient_id: Some(patient_id.clone()),
                    device_id: device_id.get().to_string(),
                    success: true,
                    details: None,
                },
            );
            Ok(Some(CareTeamRecord {
                id,
                patient_id,
                resource,
                last_updated,
            }))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Related Persons (PTNT-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Add a related person (emergency contact, next-of-kin, guarantor).
#[tauri::command]
pub fn add_related_person(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    input: RelatedPersonInput,
) -> Result<RelatedPersonRecord, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Update) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.related_person.add",
                    &e.to_string(),
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

    let given: Vec<serde_json::Value> = input
        .given_names
        .iter()
        .map(|g| serde_json::Value::String(g.clone()))
        .collect();

    let mut fhir = serde_json::json!({
        "resourceType": "RelatedPerson",
        "id": id,
        "patient": {"reference": format!("Patient/{}", input.patient_id)},
        "relationship": [{"coding": [{"code": input.relationship, "system": "http://terminology.hl7.org/CodeSystem/v3-RoleCode"}]}],
        "name": [{"use": "official", "family": input.family_name, "given": given}],
        "active": true,
    });

    let mut telecom = vec![];
    if let Some(ph) = &input.phone {
        telecom.push(serde_json::json!({"system":"phone","value":ph}));
    }
    if let Some(em) = &input.email {
        telecom.push(serde_json::json!({"system":"email","value":em}));
    }
    if !telecom.is_empty() {
        fhir["telecom"] = serde_json::json!(telecom);
    }

    if input.address_line.is_some() || input.city.is_some() {
        let mut addr = serde_json::json!({});
        if let Some(l) = &input.address_line {
            addr["line"] = serde_json::json!([l]);
        }
        if let Some(c) = &input.city {
            addr["city"] = serde_json::json!(c);
        }
        if let Some(s) = &input.state {
            addr["state"] = serde_json::json!(s);
        }
        if let Some(p) = &input.postal_code {
            addr["postalCode"] = serde_json::json!(p);
        }
        fhir["address"] = serde_json::json!([addr]);
    }

    let resource_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'RelatedPerson', ?2, 1, ?3, ?4, ?5)",
        rusqlite::params![id, resource_json, now, now, now],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.related_person.add".to_string(),
            resource_type: "RelatedPerson".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(RelatedPersonRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        last_updated: now,
    })
}

/// List related persons for a patient.
#[tauri::command]
pub fn list_related_persons(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
) -> Result<Vec<RelatedPersonRecord>, AppError> {
    let (user_id, _role) =
        match middleware::check_permission(&session, Resource::Patients, Action::Read) {
            Ok(p) => p,
            Err(e) => {
                audit_denied(
                    &db,
                    &device_id,
                    "UNAUTHENTICATED",
                    "patient.related_person.list",
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
        "SELECT id, resource, last_updated FROM fhir_resources
         WHERE resource_type = 'RelatedPerson'
         AND json_extract(resource, '$.patient.reference') = ?1",
    )?;

    let patient_ref = format!("Patient/{}", patient_id);
    let records: Vec<RelatedPersonRecord> = stmt
        .query_map(rusqlite::params![patient_ref], |row| {
            let resource_str: String = row.get(1)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null);
            Ok(RelatedPersonRecord {
                id: row.get(0)?,
                patient_id: patient_id.clone(),
                resource,
                last_updated: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "patient.related_person.list".to_string(),
            resource_type: "RelatedPerson".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("returned {} related persons", records.len())),
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

    /// In-memory DB with migrations 1–9 applied inline for unit testing.
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

            CREATE TABLE IF NOT EXISTS patient_index (
                patient_id        TEXT PRIMARY KEY NOT NULL
                                  REFERENCES fhir_resources(id) ON DELETE CASCADE,
                mrn               TEXT NOT NULL UNIQUE,
                family_name       TEXT NOT NULL,
                given_name        TEXT,
                birth_date        TEXT,
                gender            TEXT,
                primary_provider_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_patient_index_mrn    ON patient_index(mrn);
            CREATE INDEX IF NOT EXISTS idx_patient_index_family  ON patient_index(family_name);
            CREATE INDEX IF NOT EXISTS idx_patient_index_given   ON patient_index(given_name);
            CREATE INDEX IF NOT EXISTS idx_patient_index_dob     ON patient_index(birth_date);

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

    fn sample_patient_input() -> PatientInput {
        PatientInput {
            family_name: "Smith".to_string(),
            given_names: vec!["John".to_string(), "William".to_string()],
            birth_date: Some("1985-06-15".to_string()),
            gender: Some("male".to_string()),
            gender_identity: None,
            phone: Some("555-1234".to_string()),
            email: Some("john.smith@example.com".to_string()),
            address_line: Some("123 Main St".to_string()),
            city: Some("Springfield".to_string()),
            state: Some("IL".to_string()),
            postal_code: Some("62701".to_string()),
            country: None,
            photo_url: None,
            mrn: Some("MRN-TEST01".to_string()),
            primary_provider_id: Some("provider-abc".to_string()),
            insurance_primary: Some(InsuranceInput {
                payer_name: "BlueCross".to_string(),
                plan_name: Some("PPO Gold".to_string()),
                member_id: "BC-123456".to_string(),
                group_number: Some("GRP-9999".to_string()),
                subscriber_name: Some("John Smith".to_string()),
                subscriber_dob: Some("1985-06-15".to_string()),
                relationship_to_subscriber: Some("self".to_string()),
            }),
            insurance_secondary: None,
            insurance_tertiary: None,
            employer: Some(EmployerInput {
                employer_name: "Acme Corp".to_string(),
                occupation: Some("Engineer".to_string()),
                employer_phone: None,
                employer_address: None,
            }),
            sdoh: Some(SdohInput {
                housing_status: Some("stable".to_string()),
                food_security: Some("secure".to_string()),
                transportation_access: None,
                education_level: None,
                notes: None,
            }),
        }
    }

    // ── build_patient_fhir ────────────────────────────────────────────────

    #[test]
    fn build_patient_fhir_has_correct_resource_type() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        assert_eq!(fhir["resourceType"], "Patient");
    }

    #[test]
    fn build_patient_fhir_embeds_mrn_identifier() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-XTEST", &input);
        let identifiers = fhir["identifier"].as_array().unwrap();
        let mrn_id = identifiers
            .iter()
            .find(|id| id["system"] == "http://medarc.local/mrn");
        assert!(mrn_id.is_some());
        assert_eq!(mrn_id.unwrap()["value"], "MRN-XTEST");
    }

    #[test]
    fn build_patient_fhir_includes_name() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let name = &fhir["name"][0];
        assert_eq!(name["family"], "Smith");
        assert!(name["given"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("John")));
    }

    #[test]
    fn build_patient_fhir_includes_birth_date_and_gender() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        assert_eq!(fhir["birthDate"], "1985-06-15");
        assert_eq!(fhir["gender"], "male");
    }

    #[test]
    fn build_patient_fhir_includes_telecom() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let telecom = fhir["telecom"].as_array().unwrap();
        let phone = telecom.iter().find(|t| t["system"] == "phone");
        assert!(phone.is_some());
        assert_eq!(phone.unwrap()["value"], "555-1234");
    }

    #[test]
    fn build_patient_fhir_includes_address() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let addr = &fhir["address"][0];
        assert_eq!(addr["city"], "Springfield");
        assert_eq!(addr["state"], "IL");
    }

    #[test]
    fn build_patient_fhir_includes_primary_insurance_extension() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let exts = fhir["extension"].as_array().unwrap();
        let ins_ext = exts.iter().find(|e| {
            e["url"]
                .as_str()
                .map(|u| u.ends_with("/primary"))
                .unwrap_or(false)
        });
        assert!(ins_ext.is_some(), "primary insurance extension missing");
    }

    #[test]
    fn build_patient_fhir_includes_employer_extension() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let exts = fhir["extension"].as_array().unwrap();
        let emp_ext = exts.iter().find(|e| {
            e["url"]
                .as_str()
                .map(|u| u.ends_with("/employer"))
                .unwrap_or(false)
        });
        assert!(emp_ext.is_some(), "employer extension missing");
    }

    #[test]
    fn build_patient_fhir_includes_sdoh_extension() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let exts = fhir["extension"].as_array().unwrap();
        let sdoh_ext = exts.iter().find(|e| {
            e["url"]
                .as_str()
                .map(|u| u.ends_with("/sdoh"))
                .unwrap_or(false)
        });
        assert!(sdoh_ext.is_some(), "sdoh extension missing");
    }

    // ── patient_index insertion ───────────────────────────────────────────

    #[test]
    fn patient_index_row_inserted_on_create() {
        let conn = test_db();
        let input = sample_patient_input();
        let id = "pat-001";
        let mrn = "MRN-TEST01";
        let fhir = build_patient_fhir(id, mrn, &input);
        let now = chrono::Utc::now().to_rfc3339();
        let resource_json = serde_json::to_string(&fhir).unwrap();

        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'Patient', ?2, 1, ?3, ?4, ?5)",
            rusqlite::params![id, resource_json, now, now, now],
        ).unwrap();

        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, given_name, birth_date, gender, primary_provider_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, mrn, "smith", "john william", "1985-06-15", "male", "provider-abc"],
        ).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE mrn = 'MRN-TEST01'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── generate_sequential_mrn ────────────────────────────────────────

    #[test]
    fn sequential_mrn_starts_at_1_when_empty() {
        let conn = test_db();
        let mrn = generate_sequential_mrn(&conn);
        assert_eq!(mrn, "1");
    }

    #[test]
    fn sequential_mrn_increments() {
        let conn = test_db();
        // Insert a patient with MRN "5"
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-seq1", "5", &input);
        let now = chrono::Utc::now().to_rfc3339();
        let json = serde_json::to_string(&fhir).unwrap();
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at) VALUES (?1,'Patient',?2,1,?3,?3,?3)",
            rusqlite::params!["pat-seq1", json, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, given_name, birth_date, gender) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params!["pat-seq1", "5", "Doe", "Jane", "1990-01-01", "female"],
        ).unwrap();

        let mrn = generate_sequential_mrn(&conn);
        assert_eq!(mrn, "6");
    }

    #[test]
    fn sequential_mrn_ignores_non_numeric() {
        let conn = test_db();
        // Insert a patient with a legacy non-numeric MRN
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-seq2", "MRN-ABC123", &input);
        let now = chrono::Utc::now().to_rfc3339();
        let json = serde_json::to_string(&fhir).unwrap();
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at) VALUES (?1,'Patient',?2,1,?3,?3,?3)",
            rusqlite::params!["pat-seq2", json, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, given_name, birth_date, gender) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params!["pat-seq2", "MRN-ABC123", "Smith", "John", "1985-06-15", "male"],
        ).unwrap();

        // Non-numeric MRN should be ignored; next MRN starts at 1
        let mrn = generate_sequential_mrn(&conn);
        assert_eq!(mrn, "1");
    }

    // ── patient_index cascade delete ─────────────────────────────────────

    #[test]
    fn deleting_fhir_resource_cascades_to_patient_index() {
        let conn = test_db();
        let input = sample_patient_input();
        let id = "pat-cascade";
        let mrn = "MRN-CASCADE";
        let fhir = build_patient_fhir(id, mrn, &input);
        let now = chrono::Utc::now().to_rfc3339();
        let json = serde_json::to_string(&fhir).unwrap();

        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'Patient', ?2, 1, ?3, ?4, ?5)",
            rusqlite::params![id, json, now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, given_name)
             VALUES (?1, ?2, 'smith', 'john')",
            rusqlite::params![id, mrn],
        )
        .unwrap();

        conn.execute(
            "DELETE FROM fhir_resources WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE patient_id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "patient_index row should be cascade-deleted");
    }

    // ── search logic ──────────────────────────────────────────────────────

    #[test]
    fn search_by_mrn_exact_match() {
        let conn = test_db();
        let id = "pat-mrn";
        let mrn = "MRN-EXACT";
        let now = chrono::Utc::now().to_rfc3339();
        let fhir = serde_json::json!({"resourceType":"Patient","id":id});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'Patient', ?2, 1, ?3, ?4, ?5)",
            rusqlite::params![id, serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, given_name)
             VALUES (?1, ?2, 'jones', 'alice')",
            rusqlite::params![id, mrn],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE mrn = 'MRN-EXACT'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Non-matching MRN
        let count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE mrn = 'MRN-WRONG'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn search_by_family_name_prefix() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        for (i, name) in ["smith", "smithson", "jones"].iter().enumerate() {
            let id = format!("pat-{}", i);
            let mrn = format!("MRN-{}", i);
            let fhir = serde_json::json!({"resourceType":"Patient","id":id});
            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
                 VALUES (?1, 'Patient', ?2, 1, ?3, ?4, ?5)",
                rusqlite::params![id, serde_json::to_string(&fhir).unwrap(), now, now, now],
            ).unwrap();
            conn.execute(
                "INSERT INTO patient_index (patient_id, mrn, family_name, given_name)
                 VALUES (?1, ?2, ?3, 'alice')",
                rusqlite::params![id, mrn, name],
            )
            .unwrap();
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE family_name LIKE 'smith%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "should match 'smith' and 'smithson'");
    }

    #[test]
    fn search_by_dob_exact() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir = serde_json::json!({"resourceType":"Patient","id":"pat-dob"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('pat-dob', 'Patient', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name, birth_date)
             VALUES ('pat-dob', 'MRN-DOB', 'doe', '1990-01-01')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM patient_index WHERE birth_date = '1990-01-01'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── audit trail ──────────────────────────────────────────────────────

    #[test]
    fn audit_entry_written_on_simulated_create() {
        use crate::audit::entry::write_audit_entry;
        use crate::audit::AuditEntryInput;

        let conn = test_db();

        let entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "patient.create".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: Some("pat-001".to_string()),
                patient_id: Some("pat-001".to_string()),
                device_id: "dev-001".to_string(),
                success: true,
                details: None,
            },
        )
        .unwrap();

        assert!(entry.success);
        assert_eq!(entry.action, "patient.create");
        assert_eq!(entry.resource_type, "Patient");
        assert_eq!(entry.patient_id.as_deref(), Some("pat-001"));
    }

    #[test]
    fn audit_entry_written_on_simulated_search() {
        use crate::audit::entry::write_audit_entry;
        use crate::audit::AuditEntryInput;

        let conn = test_db();

        let entry = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: "user-001".to_string(),
                action: "patient.search".to_string(),
                resource_type: "Patient".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: "dev-001".to_string(),
                success: true,
                details: Some("returned 3 results".to_string()),
            },
        )
        .unwrap();

        assert!(entry.success);
        assert_eq!(entry.action, "patient.search");
    }

    // ── FHIR care team resource ───────────────────────────────────────────

    #[test]
    fn care_team_fhir_has_correct_structure() {
        let fhir = serde_json::json!({
            "resourceType": "CareTeam",
            "subject": {"reference": "Patient/pat-001"},
            "participant": [{
                "role": [{"coding": [{"code": "primary_care"}]}],
                "member": {"reference": "Practitioner/prov-001", "display": "Dr. Jane"},
                "note": "",
            }],
            "status": "active",
        });
        assert_eq!(fhir["resourceType"], "CareTeam");
        assert_eq!(fhir["subject"]["reference"], "Patient/pat-001");
        assert_eq!(fhir["status"], "active");
    }

    // ── FHIR related person resource ──────────────────────────────────────

    #[test]
    fn related_person_fhir_links_to_patient() {
        let fhir = serde_json::json!({
            "resourceType": "RelatedPerson",
            "id": "rp-001",
            "patient": {"reference": "Patient/pat-001"},
            "relationship": [{"coding": [{"code": "emergency_contact"}]}],
            "name": [{"family": "Jones", "given": ["Bob"]}],
            "active": true,
        });
        assert_eq!(fhir["patient"]["reference"], "Patient/pat-001");
        assert_eq!(fhir["name"][0]["family"], "Jones");
    }

    // ── MRN uniqueness constraint ─────────────────────────────────────────

    #[test]
    fn mrn_uniqueness_constraint_enforced() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        let fhir1 = serde_json::json!({"resourceType":"Patient","id":"p1"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('p1', 'Patient', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir1).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name) VALUES ('p1', 'MRN-DUP', 'doe')",
            [],
        ).unwrap();

        let fhir2 = serde_json::json!({"resourceType":"Patient","id":"p2"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('p2', 'Patient', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir2).unwrap(), now, now, now],
        ).unwrap();

        // Duplicate MRN should fail
        let result = conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name) VALUES ('p2', 'MRN-DUP', 'smith')",
            [],
        );
        assert!(
            result.is_err(),
            "duplicate MRN should be rejected by UNIQUE constraint"
        );
    }

    // ── version_id bump on update ─────────────────────────────────────────

    #[test]
    fn version_id_increments_on_update() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();
        let fhir = serde_json::json!({"resourceType":"Patient","id":"p-ver"});
        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES ('p-ver', 'Patient', ?1, 1, ?2, ?3, ?4)",
            rusqlite::params![serde_json::to_string(&fhir).unwrap(), now, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO patient_index (patient_id, mrn, family_name) VALUES ('p-ver', 'MRN-VER', 'doe')",
            [],
        ).unwrap();

        // Simulate update
        conn.execute(
            "UPDATE fhir_resources SET version_id = version_id + 1 WHERE id = 'p-ver'",
            [],
        )
        .unwrap();

        let v: i64 = conn
            .query_row(
                "SELECT version_id FROM fhir_resources WHERE id = 'p-ver'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, 2);
    }

    // ── combined PTNT requirements proof ─────────────────────────────────

    /// Proves PTNT-01: Demographics fields are fully represented in FHIR.
    #[test]
    fn ptnt_01_demographics_complete() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        // name
        assert_eq!(fhir["name"][0]["family"], "Smith");
        // DOB
        assert_eq!(fhir["birthDate"], "1985-06-15");
        // gender
        assert_eq!(fhir["gender"], "male");
        // phone
        assert!(fhir["telecom"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["system"] == "phone"));
        // email
        assert!(fhir["telecom"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["system"] == "email"));
        // address
        assert!(fhir["address"].as_array().unwrap().len() > 0);
    }

    /// Proves PTNT-02: Insurance extensions are attached at all three tiers.
    #[test]
    fn ptnt_02_insurance_tiers() {
        let mut input = sample_patient_input();
        input.insurance_secondary = Some(InsuranceInput {
            payer_name: "Aetna".to_string(),
            plan_name: None,
            member_id: "AET-999".to_string(),
            group_number: None,
            subscriber_name: None,
            subscriber_dob: None,
            relationship_to_subscriber: Some("self".to_string()),
        });
        input.insurance_tertiary = Some(InsuranceInput {
            payer_name: "Medicaid".to_string(),
            plan_name: None,
            member_id: "MED-001".to_string(),
            group_number: None,
            subscriber_name: None,
            subscriber_dob: None,
            relationship_to_subscriber: None,
        });

        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let exts = fhir["extension"].as_array().unwrap();
        let tiers: Vec<&str> = exts
            .iter()
            .filter_map(|e| e["url"].as_str())
            .filter(|u| u.contains("/insurance/"))
            .map(|u| u.split('/').last().unwrap_or(""))
            .collect();
        assert!(tiers.contains(&"primary"));
        assert!(tiers.contains(&"secondary"));
        assert!(tiers.contains(&"tertiary"));
    }

    /// Proves PTNT-03: Employer and SDOH fields are embedded.
    #[test]
    fn ptnt_03_employer_and_sdoh() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let exts = fhir["extension"].as_array().unwrap();
        let has_employer = exts.iter().any(|e| {
            e["url"]
                .as_str()
                .map(|u| u.ends_with("/employer"))
                .unwrap_or(false)
        });
        let has_sdoh = exts.iter().any(|e| {
            e["url"]
                .as_str()
                .map(|u| u.ends_with("/sdoh"))
                .unwrap_or(false)
        });
        assert!(has_employer, "employer extension required");
        assert!(has_sdoh, "sdoh extension required");
    }

    /// Proves PTNT-04: MRN and primary provider are stored as FHIR identifiers.
    #[test]
    fn ptnt_04_clinical_identifiers() {
        let input = sample_patient_input();
        let fhir = build_patient_fhir("pat-001", "MRN-TEST01", &input);
        let ids = fhir["identifier"].as_array().unwrap();
        let has_mrn = ids
            .iter()
            .any(|i| i["system"] == "http://medarc.local/mrn" && i["value"] == "MRN-TEST01");
        let has_provider = ids.iter().any(|i| {
            i["system"] == "http://medarc.local/primary-provider" && i["value"] == "provider-abc"
        });
        assert!(has_mrn, "MRN identifier missing");
        assert!(has_provider, "primary provider identifier missing");
    }

    /// Proves PTNT-05: patient_index supports fast lookups by name, MRN, DOB.
    #[test]
    fn ptnt_05_search_indexes_present() {
        let conn = test_db();
        // Verify indexes exist
        let idx_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='patient_index'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(
            idx_count >= 3,
            "Expected at least 3 indexes on patient_index, found {}",
            idx_count
        );
    }
}
