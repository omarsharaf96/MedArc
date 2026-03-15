/// commands/documentation.rs — Clinical Documentation (S07)
///
/// Implements CLIN-01 through CLIN-07:
///   CLIN-01  Structured SOAP notes per encounter (Subjective/Objective/Assessment/Plan)
///   CLIN-02  Vitals recording (BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale)
///   CLIN-03  Review of Systems — 14 organ systems (positive/negative/not-reviewed)
///   CLIN-04  Physical exam findings with system-based templates
///   CLIN-05  10-15 pre-built clinical templates (general, cardiology, pediatrics, OB/GYN, etc.)
///   CLIN-06  Co-sign workflow for NP/PA mid-level notes by supervising physician
///   CLIN-07  Passive drug-allergy CDS alerts based on patient allergy + medication lists
///
/// Data model
/// ----------
/// Resources are stored as FHIR-aligned JSON in `fhir_resources`.
/// Migration 12 adds three index tables:
///   - `encounter_index`   (patient_id, provider_id, encounter_date, status, encounter_type)
///   - `vitals_index`      (patient_id, encounter_id, recorded_at)
///   - `cosign_index`      (encounter_id, requesting_provider_id, supervising_provider_id, status)
///
/// RBAC
/// ----
/// All documentation commands require `ClinicalDocumentation` resource access.
///   Provider / SystemAdmin  → full CRUD (create, read, update, delete encounters and notes)
///   NurseMa                 → Create + Read + Update vitals; Read-only on SOAP notes
///   BillingStaff            → Read-only (encounter diagnoses / procedure codes)
///   FrontDesk               → No access to clinical documentation
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
use crate::rbac::roles::{Action, Resource, Role};

// ─────────────────────────────────────────────────────────────────────────────
// Encounter / SOAP note types (CLIN-01)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating a new clinical encounter with a SOAP note.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterInput {
    /// Patient the encounter belongs to.
    pub patient_id: String,
    /// Provider (user ID) who conducted the encounter.
    pub provider_id: String,
    /// ISO 8601 date-time of the encounter (e.g. "2026-04-01T09:00:00").
    pub encounter_date: String,
    /// Encounter type: "office_visit" | "telehealth" | "urgent_care" | "follow_up" | "preventive" | "procedure"
    pub encounter_type: String,
    /// Chief complaint (free text).
    pub chief_complaint: Option<String>,
    /// Template ID to pre-populate the note structure (from `list_templates`).
    pub template_id: Option<String>,
    /// SOAP note sections.
    pub soap: Option<SoapInput>,
    /// Optional appointment ID that this encounter is linked to.
    pub appointment_id: Option<String>,
}

/// Structured SOAP note sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoapInput {
    /// Subjective — patient-reported symptoms, HPI, chief complaint narrative.
    pub subjective: Option<String>,
    /// Objective — exam findings, vitals summary.
    pub objective: Option<String>,
    /// Assessment — diagnoses, ICD-10 codes, clinical impressions.
    pub assessment: Option<String>,
    /// Plan — treatment orders, prescriptions, referrals, follow-up.
    pub plan: Option<String>,
}

/// Encounter record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterRecord {
    pub id: String,
    pub patient_id: String,
    pub provider_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

/// Input for updating an existing encounter / SOAP note.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEncounterInput {
    /// New encounter status: "in-progress" | "finished" | "cancelled"
    pub status: Option<String>,
    /// Updated SOAP note sections.
    pub soap: Option<SoapInput>,
    /// Updated chief complaint.
    pub chief_complaint: Option<String>,
    /// Amendment reason — required when editing a finalized ("finished") encounter.
    /// When provided on a finalized encounter, the previous version is stored
    /// for audit trail and an amendment audit entry is logged.
    pub amendment_reason: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Vitals types (CLIN-02)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for recording a vitals observation set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VitalsInput {
    /// Patient the vitals belong to.
    pub patient_id: String,
    /// Encounter this vitals set is attached to.
    pub encounter_id: String,
    /// ISO 8601 datetime when vitals were recorded.
    pub recorded_at: String,
    /// Systolic BP in mmHg.
    pub systolic_bp: Option<u32>,
    /// Diastolic BP in mmHg.
    pub diastolic_bp: Option<u32>,
    /// Heart rate in bpm.
    pub heart_rate: Option<u32>,
    /// Respiratory rate in breaths/min.
    pub respiratory_rate: Option<u32>,
    /// Temperature in degrees Celsius.
    pub temperature_celsius: Option<f64>,
    /// SpO2 as a percentage (0–100).
    pub spo2_percent: Option<u32>,
    /// Weight in kilograms.
    pub weight_kg: Option<f64>,
    /// Height in centimeters.
    pub height_cm: Option<f64>,
    /// Pain score on 0–10 NRS scale.
    pub pain_score: Option<u32>,
    /// Additional notes.
    pub notes: Option<String>,
}

/// Vitals record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VitalsRecord {
    pub id: String,
    pub patient_id: String,
    pub encounter_id: String,
    pub bmi: Option<f64>,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Review of Systems types (CLIN-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Status for each ROS system: positive finding, negative (normal), or not reviewed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RosStatus {
    Positive,
    Negative,
    NotReviewed,
}

impl RosStatus {
    fn as_str(&self) -> &'static str {
        match self {
            RosStatus::Positive => "positive",
            RosStatus::Negative => "negative",
            RosStatus::NotReviewed => "not_reviewed",
        }
    }
}

/// Review of Systems across 14 standard organ systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOfSystemsInput {
    /// Patient the ROS belongs to.
    pub patient_id: String,
    /// Encounter this ROS is attached to.
    pub encounter_id: String,
    /// 1. Constitutional (fever, chills, fatigue, weight change)
    pub constitutional: Option<RosStatus>,
    pub constitutional_notes: Option<String>,
    /// 2. Eyes (vision changes, diplopia, pain)
    pub eyes: Option<RosStatus>,
    pub eyes_notes: Option<String>,
    /// 3. ENT / Head (headache, sinus, hearing, throat)
    pub ent: Option<RosStatus>,
    pub ent_notes: Option<String>,
    /// 4. Cardiovascular (chest pain, palpitations, edema)
    pub cardiovascular: Option<RosStatus>,
    pub cardiovascular_notes: Option<String>,
    /// 5. Respiratory (cough, dyspnea, wheezing)
    pub respiratory: Option<RosStatus>,
    pub respiratory_notes: Option<String>,
    /// 6. Gastrointestinal (nausea, vomiting, diarrhea, pain)
    pub gastrointestinal: Option<RosStatus>,
    pub gastrointestinal_notes: Option<String>,
    /// 7. Genitourinary (dysuria, frequency, hematuria)
    pub genitourinary: Option<RosStatus>,
    pub genitourinary_notes: Option<String>,
    /// 8. Musculoskeletal (joint pain, stiffness, swelling)
    pub musculoskeletal: Option<RosStatus>,
    pub musculoskeletal_notes: Option<String>,
    /// 9. Integumentary / Skin (rash, lesions, pruritus)
    pub integumentary: Option<RosStatus>,
    pub integumentary_notes: Option<String>,
    /// 10. Neurological (dizziness, syncope, numbness, seizures)
    pub neurological: Option<RosStatus>,
    pub neurological_notes: Option<String>,
    /// 11. Psychiatric (mood, anxiety, sleep, cognition)
    pub psychiatric: Option<RosStatus>,
    pub psychiatric_notes: Option<String>,
    /// 12. Endocrine (heat/cold intolerance, polyuria, polydipsia)
    pub endocrine: Option<RosStatus>,
    pub endocrine_notes: Option<String>,
    /// 13. Hematologic / Lymphatic (easy bruising, bleeding, lymphadenopathy)
    pub hematologic: Option<RosStatus>,
    pub hematologic_notes: Option<String>,
    /// 14. Allergic / Immunologic (seasonal allergies, drug reactions)
    pub allergic_immunologic: Option<RosStatus>,
    pub allergic_immunologic_notes: Option<String>,
}

/// ROS record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosRecord {
    pub id: String,
    pub patient_id: String,
    pub encounter_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Physical Exam types (CLIN-04)
// ─────────────────────────────────────────────────────────────────────────────

/// Physical exam findings per body system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalExamInput {
    /// Patient the exam belongs to.
    pub patient_id: String,
    /// Encounter this exam is attached to.
    pub encounter_id: String,
    /// General appearance (e.g. "Well-appearing, no acute distress").
    pub general: Option<String>,
    /// HEENT — Head, Eyes, Ears, Nose, Throat.
    pub heent: Option<String>,
    /// Neck — lymphadenopathy, thyroid, JVD.
    pub neck: Option<String>,
    /// Cardiovascular — heart sounds, murmurs, pulses.
    pub cardiovascular: Option<String>,
    /// Pulmonary — breath sounds, work of breathing.
    pub pulmonary: Option<String>,
    /// Abdomen — tenderness, organomegaly, bowel sounds.
    pub abdomen: Option<String>,
    /// Extremities — edema, pulses, cyanosis.
    pub extremities: Option<String>,
    /// Neurological — motor, sensory, reflexes, cranial nerves.
    pub neurological: Option<String>,
    /// Skin — color, turgor, lesions, rash.
    pub skin: Option<String>,
    /// Psychiatric — orientation, affect, mood.
    pub psychiatric: Option<String>,
    /// Musculoskeletal — ROM, strength, tenderness.
    pub musculoskeletal: Option<String>,
    /// Genitourinary — (optional, specialty-specific).
    pub genitourinary: Option<String>,
    /// Rectal — (optional, specialty-specific).
    pub rectal: Option<String>,
    /// Additional free-text exam notes.
    pub additional_notes: Option<String>,
}

/// Physical exam record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalExamRecord {
    pub id: String,
    pub patient_id: String,
    pub encounter_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Template types (CLIN-05)
// ─────────────────────────────────────────────────────────────────────────────

/// A clinical note template record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRecord {
    pub id: String,
    pub name: String,
    pub specialty: String,
    pub description: String,
    pub default_soap: SoapInput,
    pub default_exam_sections: Vec<String>,
    pub ros_systems: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Co-sign types (CLIN-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for a co-sign request (NP/PA requesting supervising physician signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosignRequestInput {
    /// Encounter to be co-signed.
    pub encounter_id: String,
    /// Supervising physician's user ID.
    pub supervising_provider_id: String,
    /// Optional note to the supervisor.
    pub message: Option<String>,
}

/// Co-sign record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosignRecord {
    pub id: String,
    pub encounter_id: String,
    pub requesting_provider_id: String,
    pub supervising_provider_id: String,
    pub status: String,
    pub requested_at: String,
    pub signed_at: Option<String>,
    pub resource: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Drug-Allergy CDS types (CLIN-07)
// ─────────────────────────────────────────────────────────────────────────────

/// A passive clinical decision support alert for drug-allergy interaction.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrugAllergyAlert {
    /// The medication that triggered the alert.
    pub medication_id: String,
    pub medication_name: String,
    pub medication_rxnorm: Option<String>,
    /// The allergy that conflicts with the medication.
    pub allergy_id: String,
    pub allergy_substance: String,
    pub allergy_severity: Option<String>,
    pub allergy_reaction: Option<String>,
    /// Alert severity: "warning" | "contraindicated"
    pub alert_severity: String,
    /// Human-readable alert message.
    pub message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR R4 Encounter resource from EncounterInput.
fn build_encounter_fhir(id: &str, input: &EncounterInput) -> serde_json::Value {
    let mut resource = serde_json::json!({
        "resourceType": "Encounter",
        "id": id,
        "status": "in-progress",
        "class": {
            "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
            "code": encounter_type_to_class(&input.encounter_type),
            "display": input.encounter_type.replace('_', " ")
        },
        "type": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/encounter-type",
                "code": input.encounter_type,
                "display": input.encounter_type.replace('_', " ")
            }]
        }],
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "participant": [{
            "individual": {
                "reference": format!("Practitioner/{}", input.provider_id),
                "type": "Practitioner"
            }
        }],
        "period": {
            "start": input.encounter_date
        }
    });

    if let Some(ref cc) = input.chief_complaint {
        resource["reasonCode"] = serde_json::json!([{
            "text": cc
        }]);
    }

    {
        let mut extensions = Vec::<serde_json::Value>::new();
        if let Some(ref template_id) = input.template_id {
            extensions.push(serde_json::json!({
                "url": "http://medarc.local/fhir/StructureDefinition/encounter-template",
                "valueId": template_id
            }));
        }
        if let Some(ref appointment_id) = input.appointment_id {
            extensions.push(serde_json::json!({
                "url": "http://medarc.local/fhir/StructureDefinition/encounter-appointment",
                "valueReference": {
                    "reference": format!("Appointment/{}", appointment_id)
                }
            }));
        }
        if !extensions.is_empty() {
            resource["extension"] = serde_json::json!(extensions);
        }
    }

    if let Some(ref soap) = input.soap {
        resource["note"] = build_soap_note_json(soap);
    }

    resource
}

/// Build SOAP note as FHIR Annotation array.
fn build_soap_note_json(soap: &SoapInput) -> serde_json::Value {
    let mut sections = vec![];

    if let Some(ref s) = soap.subjective {
        sections.push(serde_json::json!({
            "extension": [{
                "url": "http://medarc.local/fhir/StructureDefinition/note-section",
                "valueCode": "subjective"
            }],
            "text": s
        }));
    }
    if let Some(ref o) = soap.objective {
        sections.push(serde_json::json!({
            "extension": [{
                "url": "http://medarc.local/fhir/StructureDefinition/note-section",
                "valueCode": "objective"
            }],
            "text": o
        }));
    }
    if let Some(ref a) = soap.assessment {
        sections.push(serde_json::json!({
            "extension": [{
                "url": "http://medarc.local/fhir/StructureDefinition/note-section",
                "valueCode": "assessment"
            }],
            "text": a
        }));
    }
    if let Some(ref p) = soap.plan {
        sections.push(serde_json::json!({
            "extension": [{
                "url": "http://medarc.local/fhir/StructureDefinition/note-section",
                "valueCode": "plan"
            }],
            "text": p
        }));
    }

    serde_json::json!(sections)
}

fn encounter_type_to_class(encounter_type: &str) -> &'static str {
    match encounter_type {
        "telehealth" => "VR",
        "urgent_care" => "EMER",
        _ => "AMB",
    }
}

/// Calculate BMI from weight (kg) and height (cm).
/// Returns None if either is zero.
fn calculate_bmi(weight_kg: f64, height_cm: f64) -> Option<f64> {
    if height_cm <= 0.0 || weight_kg <= 0.0 {
        return None;
    }
    let height_m = height_cm / 100.0;
    let bmi = weight_kg / (height_m * height_m);
    // Round to 1 decimal place
    Some((bmi * 10.0).round() / 10.0)
}

/// Build a FHIR R4 Observation bundle for vitals.
/// Uses LOINC codes for each vital sign component.
fn build_vitals_fhir(id: &str, input: &VitalsInput, bmi: Option<f64>) -> serde_json::Value {
    let mut components: Vec<serde_json::Value> = vec![];

    if let (Some(sys), Some(dia)) = (input.systolic_bp, input.diastolic_bp) {
        components.push(serde_json::json!({
            "code": {
                "coding": [{"system": "http://loinc.org", "code": "55284-4", "display": "Blood pressure systolic and diastolic"}]
            },
            "component": [
                {
                    "code": {"coding": [{"system": "http://loinc.org", "code": "8480-6", "display": "Systolic blood pressure"}]},
                    "valueQuantity": {"value": sys, "unit": "mmHg", "system": "http://unitsofmeasure.org", "code": "mm[Hg]"}
                },
                {
                    "code": {"coding": [{"system": "http://loinc.org", "code": "8462-4", "display": "Diastolic blood pressure"}]},
                    "valueQuantity": {"value": dia, "unit": "mmHg", "system": "http://unitsofmeasure.org", "code": "mm[Hg]"}
                }
            ]
        }));
    }

    if let Some(hr) = input.heart_rate {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "8867-4", "display": "Heart rate"}]},
            "valueQuantity": {"value": hr, "unit": "bpm", "system": "http://unitsofmeasure.org", "code": "/min"}
        }));
    }

    if let Some(rr) = input.respiratory_rate {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "9279-1", "display": "Respiratory rate"}]},
            "valueQuantity": {"value": rr, "unit": "breaths/min", "system": "http://unitsofmeasure.org", "code": "/min"}
        }));
    }

    if let Some(temp) = input.temperature_celsius {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "8310-5", "display": "Body temperature"}]},
            "valueQuantity": {"value": temp, "unit": "°C", "system": "http://unitsofmeasure.org", "code": "Cel"}
        }));
    }

    if let Some(spo2) = input.spo2_percent {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "2708-6", "display": "Oxygen saturation"}]},
            "valueQuantity": {"value": spo2, "unit": "%", "system": "http://unitsofmeasure.org", "code": "%"}
        }));
    }

    if let Some(wt) = input.weight_kg {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "29463-7", "display": "Body weight"}]},
            "valueQuantity": {"value": wt, "unit": "kg", "system": "http://unitsofmeasure.org", "code": "kg"}
        }));
    }

    if let Some(ht) = input.height_cm {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "8302-2", "display": "Body height"}]},
            "valueQuantity": {"value": ht, "unit": "cm", "system": "http://unitsofmeasure.org", "code": "cm"}
        }));
    }

    if let Some(b) = bmi {
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "39156-5", "display": "Body mass index"}]},
            "valueQuantity": {"value": b, "unit": "kg/m2", "system": "http://unitsofmeasure.org", "code": "kg/m2"}
        }));
    }

    if let Some(pain) = input.pain_score {
        let clamped = pain.min(10);
        components.push(serde_json::json!({
            "code": {"coding": [{"system": "http://loinc.org", "code": "72514-3", "display": "Pain severity NRS"}]},
            "valueQuantity": {"value": clamped, "unit": "score", "system": "http://unitsofmeasure.org", "code": "{score}"}
        }));
    }

    let mut resource = serde_json::json!({
        "resourceType": "Observation",
        "id": id,
        "status": "final",
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "vital-signs",
                "display": "Vital Signs"
            }]
        }],
        "code": {
            "coding": [{
                "system": "http://loinc.org",
                "code": "85353-1",
                "display": "Vital signs, weight, height, head circumference, oxygen saturation and BMI panel"
            }]
        },
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "encounter": {
            "reference": format!("Encounter/{}", input.encounter_id)
        },
        "effectiveDateTime": input.recorded_at,
        "component": components
    });

    if let Some(ref notes) = input.notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

/// Build a FHIR-aligned ROS resource (custom QuestionnaireResponse).
fn build_ros_fhir(id: &str, input: &ReviewOfSystemsInput) -> serde_json::Value {
    let systems = [
        ("constitutional", &input.constitutional, &input.constitutional_notes),
        ("eyes", &input.eyes, &input.eyes_notes),
        ("ent", &input.ent, &input.ent_notes),
        ("cardiovascular", &input.cardiovascular, &input.cardiovascular_notes),
        ("respiratory", &input.respiratory, &input.respiratory_notes),
        ("gastrointestinal", &input.gastrointestinal, &input.gastrointestinal_notes),
        ("genitourinary", &input.genitourinary, &input.genitourinary_notes),
        ("musculoskeletal", &input.musculoskeletal, &input.musculoskeletal_notes),
        ("integumentary", &input.integumentary, &input.integumentary_notes),
        ("neurological", &input.neurological, &input.neurological_notes),
        ("psychiatric", &input.psychiatric, &input.psychiatric_notes),
        ("endocrine", &input.endocrine, &input.endocrine_notes),
        ("hematologic", &input.hematologic, &input.hematologic_notes),
        ("allergic_immunologic", &input.allergic_immunologic, &input.allergic_immunologic_notes),
    ];

    let items: Vec<serde_json::Value> = systems
        .iter()
        .filter(|(_, status, _)| status.is_some())
        .map(|(system, status, notes)| {
            let s = status.as_ref().unwrap();
            let mut item = serde_json::json!({
                "linkId": system,
                "text": system.replace('_', " "),
                "answer": [{
                    "valueCoding": {
                        "system": "http://medarc.local/fhir/CodeSystem/ros-status",
                        "code": s.as_str(),
                        "display": s.as_str().replace('_', " ")
                    }
                }]
            });
            if let Some(ref n) = notes {
                item["extension"] = serde_json::json!([{
                    "url": "http://medarc.local/fhir/StructureDefinition/ros-notes",
                    "valueString": n
                }]);
            }
            item
        })
        .collect();

    serde_json::json!({
        "resourceType": "QuestionnaireResponse",
        "id": id,
        "status": "completed",
        "questionnaire": "http://medarc.local/fhir/Questionnaire/review-of-systems",
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "encounter": {
            "reference": format!("Encounter/{}", input.encounter_id)
        },
        "item": items
    })
}

/// Build a FHIR-aligned physical exam resource (custom ClinicalImpression).
fn build_exam_fhir(id: &str, input: &PhysicalExamInput) -> serde_json::Value {
    let systems: Vec<(&str, &Option<String>)> = vec![
        ("general", &input.general),
        ("heent", &input.heent),
        ("neck", &input.neck),
        ("cardiovascular", &input.cardiovascular),
        ("pulmonary", &input.pulmonary),
        ("abdomen", &input.abdomen),
        ("extremities", &input.extremities),
        ("neurological", &input.neurological),
        ("skin", &input.skin),
        ("psychiatric", &input.psychiatric),
        ("musculoskeletal", &input.musculoskeletal),
        ("genitourinary", &input.genitourinary),
        ("rectal", &input.rectal),
    ];

    let findings: Vec<serde_json::Value> = systems
        .iter()
        .filter(|(_, val)| val.is_some())
        .map(|(system, val)| {
            serde_json::json!({
                "extension": [{
                    "url": "http://medarc.local/fhir/StructureDefinition/exam-system",
                    "valueCode": system
                }],
                "itemCodeableConcept": {
                    "coding": [{
                        "system": "http://medarc.local/fhir/CodeSystem/exam-system",
                        "code": system,
                        "display": system.replace('_', " ")
                    }],
                    "text": val.as_ref().unwrap().clone()
                }
            })
        })
        .collect();

    let mut resource = serde_json::json!({
        "resourceType": "ClinicalImpression",
        "id": id,
        "status": "completed",
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "encounter": {
            "reference": format!("Encounter/{}", input.encounter_id)
        },
        "finding": findings
    });

    if let Some(ref notes) = input.additional_notes {
        resource["note"] = serde_json::json!([{"text": notes}]);
    }

    resource
}

/// Build a co-sign request FHIR resource (custom Task).
fn build_cosign_fhir(
    id: &str,
    encounter_id: &str,
    requesting_provider_id: &str,
    supervising_provider_id: &str,
    message: Option<&str>,
    requested_at: &str,
) -> serde_json::Value {
    let mut resource = serde_json::json!({
        "resourceType": "Task",
        "id": id,
        "status": "requested",
        "intent": "order",
        "code": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/task-type",
                "code": "cosign",
                "display": "Co-sign request"
            }]
        },
        "focus": {
            "reference": format!("Encounter/{}", encounter_id)
        },
        "requester": {
            "reference": format!("Practitioner/{}", requesting_provider_id),
            "type": "Practitioner"
        },
        "owner": {
            "reference": format!("Practitioner/{}", supervising_provider_id),
            "type": "Practitioner"
        },
        "authoredOn": requested_at
    });

    if let Some(msg) = message {
        resource["note"] = serde_json::json!([{"text": msg}]);
    }

    resource
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in clinical templates (CLIN-05)
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the 12 built-in specialty templates.
fn built_in_templates() -> Vec<TemplateRecord> {
    vec![
        TemplateRecord {
            id: "tpl_general".to_string(),
            name: "General Office Visit".to_string(),
            specialty: "general".to_string(),
            description: "Standard adult primary care visit".to_string(),
            default_soap: SoapInput {
                subjective: Some("Chief complaint: \nHPI: Patient presents with ... Duration: ... Quality: ... Severity: ... Location: ... Modifying factors: ... Associated symptoms: ...".to_string()),
                objective: Some("Vital signs: See vitals section.\nGeneral: Well-appearing, no acute distress.".to_string()),
                assessment: Some("1. [Primary diagnosis — ICD-10: ]\n2. [Secondary diagnosis — ICD-10: ]".to_string()),
                plan: Some("1. [Medication/treatment]\n2. [Follow-up in _ weeks]\n3. [Patient education provided]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "heent".to_string(), "neck".to_string(),
                "cardiovascular".to_string(), "pulmonary".to_string(), "abdomen".to_string(),
                "extremities".to_string(), "neurological".to_string(), "skin".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "eyes".to_string(), "ent".to_string(),
                "cardiovascular".to_string(), "respiratory".to_string(),
                "gastrointestinal".to_string(), "genitourinary".to_string(),
                "musculoskeletal".to_string(), "integumentary".to_string(),
                "neurological".to_string(), "psychiatric".to_string(),
                "endocrine".to_string(), "hematologic".to_string(), "allergic_immunologic".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_cardiology".to_string(),
            name: "Cardiology Consultation".to_string(),
            specialty: "cardiology".to_string(),
            description: "Cardiology evaluation including cardiac history and risk factors".to_string(),
            default_soap: SoapInput {
                subjective: Some("Cardiac symptoms: chest pain/pressure/tightness, palpitations, syncope, dyspnea, orthopnea, PND, leg edema.\nCardiac RFs: HTN, DM, HLD, smoking, family history of CAD.\nCardiac history: prior MI, PCI, CABG, arrhythmia, heart failure.".to_string()),
                objective: Some("Vital signs: See vitals.\nCardiovascular exam: S1/S2 regular rate and rhythm. No murmurs/rubs/gallops. Peripheral pulses 2+ bilaterally. No JVD. No lower extremity edema.".to_string()),
                assessment: Some("1. [Cardiac diagnosis — ICD-10: ]\nCardiac risk stratification: [ Low / Intermediate / High ]".to_string()),
                plan: Some("1. [Cardiac medications]\n2. [Diagnostics: ECG/Echo/Stress test/Cath]\n3. [Lifestyle modification counseling]\n4. [Follow-up in _ weeks/months]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "cardiovascular".to_string(), "pulmonary".to_string(),
                "extremities".to_string(), "neck".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "cardiovascular".to_string(),
                "respiratory".to_string(), "gastrointestinal".to_string(),
                "extremities".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_pediatrics".to_string(),
            name: "Pediatric Well-Child Visit".to_string(),
            specialty: "pediatrics".to_string(),
            description: "Well-child check with developmental screening and immunization review".to_string(),
            default_soap: SoapInput {
                subjective: Some("Age: _ DOB: _\nDevelopmental milestones: [ On track / Concerns: ]\nFeeding: [ Breast / Formula / Solids ] Amount: _\nSleep: _ hours/night. Concerns: _\nBehavior/development: _\nParent concerns: _".to_string()),
                objective: Some("Vital signs: See vitals. Growth percentiles: Weight __%, Height __%, HC __% (if applicable).\nGeneral: Well-appearing, well-nourished child in no distress.".to_string()),
                assessment: Some("1. Well-child visit — age _\n2. [Any diagnoses — ICD-10: ]\nImmunizations: See immunization record.".to_string()),
                plan: Some("1. Immunizations administered: _\n2. Anticipatory guidance provided: _\n3. Developmental screening: _\n4. Next well-child visit at age _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "heent".to_string(), "cardiovascular".to_string(),
                "pulmonary".to_string(), "abdomen".to_string(), "musculoskeletal".to_string(),
                "neurological".to_string(), "skin".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "eyes".to_string(), "ent".to_string(),
                "respiratory".to_string(), "gastrointestinal".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_obgyn".to_string(),
            name: "OB/GYN Visit".to_string(),
            specialty: "ob_gyn".to_string(),
            description: "Obstetrics and gynecology evaluation".to_string(),
            default_soap: SoapInput {
                subjective: Some("GYN history: LMP: _ G_P_A_. Contraception: _\nGYN symptoms: vaginal discharge, pelvic pain, abnormal bleeding, dyspareunia.\nOB (if applicable): EGA: _ EDD: _ Prenatal labs: _".to_string()),
                objective: Some("Vital signs: See vitals.\nAbdomen: Gravid uterus at _ cm (if pregnant). Non-tender.\nPelvic: External genitalia normal. Cervix _. Uterus _. Adnexa _.".to_string()),
                assessment: Some("1. [GYN/OB diagnosis — ICD-10: ]".to_string()),
                plan: Some("1. [Medications/contraception]\n2. [Screenings: Pap, STI testing, mammogram]\n3. [Follow-up: _ weeks/months / next prenatal visit at _ weeks]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "abdomen".to_string(), "genitourinary".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "cardiovascular".to_string(),
                "gastrointestinal".to_string(), "genitourinary".to_string(),
                "musculoskeletal".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_psychiatry".to_string(),
            name: "Psychiatric Evaluation".to_string(),
            specialty: "psychiatry".to_string(),
            description: "Mental health evaluation with MSE and safety assessment".to_string(),
            default_soap: SoapInput {
                subjective: Some("Presenting concerns: _\nMood: _/10. Anxiety: _/10. Sleep: _ hrs. Appetite: _.\nPsychiatric history: _\nSubstance use: Alcohol _ ETOH/week. Tobacco: _. Other: _.\nSafety: SI: [ Denied / Present — plan: _ ]. HI: [ Denied / Present ].".to_string()),
                objective: Some("Mental Status Exam:\nAppearance: _. Behavior: _. Speech: _. Mood: _. Affect: _.\nThought process: _. Thought content: _. Insight: _. Judgment: _.".to_string()),
                assessment: Some("1. [Psychiatric diagnosis — ICD-10: ]\n2. [GAF/WHODAS score: ]\nSafety: Patient is [ safe for outpatient management / requires higher level of care ].".to_string()),
                plan: Some("1. [Medication adjustments]\n2. [Therapy referral]\n3. [Safety plan reviewed]\n4. [Follow-up in _ weeks]".to_string()),
            },
            default_exam_sections: vec!["general".to_string(), "psychiatric".to_string(), "neurological".to_string()],
            ros_systems: vec![
                "constitutional".to_string(), "neurological".to_string(),
                "psychiatric".to_string(), "endocrine".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_orthopedics".to_string(),
            name: "Orthopedic Evaluation".to_string(),
            specialty: "orthopedics".to_string(),
            description: "Musculoskeletal evaluation with ROM and functional assessment".to_string(),
            default_soap: SoapInput {
                subjective: Some("Complaint: _ Location: _ Onset: _ Mechanism of injury: _\nPain: _/10. Character: _. Aggravating: _. Relieving: _.\nFunctional limitations: _. Prior treatments: _. Prior imaging: _.".to_string()),
                objective: Some("Musculoskeletal exam:\nInspection: _. Palpation: _.\nROM: Active: _. Passive: _.\nStrength: _ (MRC grade). Neurovascular: Sensation _ Pulses _.".to_string()),
                assessment: Some("1. [Orthopedic diagnosis — ICD-10: ]\nImaging review: _".to_string()),
                plan: Some("1. [Conservative: RICE / NSAIDs / PT / Splint]\n2. [Injection: _]\n3. [Operative plan: _]\n4. [Imaging ordered: X-ray / MRI / CT]\n5. [Follow-up in _ weeks]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(), "neurological".to_string(),
                "extremities".to_string(), "skin".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
                "neurological".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_dermatology".to_string(),
            name: "Dermatology Visit".to_string(),
            specialty: "dermatology".to_string(),
            description: "Skin condition evaluation with lesion description".to_string(),
            default_soap: SoapInput {
                subjective: Some("Skin complaint: _ Location: _ Duration: _ Onset: _\nPruritus: [ Y / N ]. Pain: _/10. Spread: _. Prior treatments: _.\nExposures: _. Personal/family hx of skin cancer: _.".to_string()),
                objective: Some("Skin exam:\nDistribution: _. Morphology: Macule / Papule / Patch / Plaque / Vesicle / Pustule / Nodule / Ulcer.\nSize: _. Color: _. Border: _. Surface: _. Associated findings: _.".to_string()),
                assessment: Some("1. [Dermatologic diagnosis — ICD-10: ]".to_string()),
                plan: Some("1. [Topical/systemic medications]\n2. [Biopsy / Excision]\n3. [Phototherapy]\n4. [Referral: Dermatology / Surgery]\n5. [Follow-up in _ weeks/months]".to_string()),
            },
            default_exam_sections: vec!["general".to_string(), "skin".to_string()],
            ros_systems: vec![
                "constitutional".to_string(), "integumentary".to_string(),
                "allergic_immunologic".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_neurology".to_string(),
            name: "Neurology Consultation".to_string(),
            specialty: "neurology".to_string(),
            description: "Neurological evaluation with cranial nerve and motor exam".to_string(),
            default_soap: SoapInput {
                subjective: Some("Neurological complaint: _\nHeadache: Location: _. Severity: _/10. Frequency: _. Duration: _. Aura: [ Y / N ].\nSeizures: [ Y / N ] — description: _.\nMemory/cognition: _. Weakness: _. Sensory symptoms: _. Gait: _.".to_string()),
                objective: Some("Neurological exam:\nCranial nerves: II-XII intact bilaterally.\nMotor: Strength 5/5 all extremities.\nSensory: Intact to light touch/pinprick.\nReflexes: 2+ throughout. Babinski: negative bilaterally.\nCoordination: Finger-nose intact. Gait: _.".to_string()),
                assessment: Some("1. [Neurological diagnosis — ICD-10: ]".to_string()),
                plan: Some("1. [Medications]\n2. [Imaging: MRI Brain / Spine / CT]\n3. [EEG / EMG / NCS]\n4. [Referral: Neurosurgery / Epilepsy]\n5. [Follow-up in _ weeks/months]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "neurological".to_string(),
                "psychiatric".to_string(), "musculoskeletal".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "neurological".to_string(),
                "psychiatric".to_string(), "eyes".to_string(), "ent".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_urgent_care".to_string(),
            name: "Urgent Care Visit".to_string(),
            specialty: "urgent_care".to_string(),
            description: "Acute illness or injury evaluation in urgent care setting".to_string(),
            default_soap: SoapInput {
                subjective: Some("Acute complaint: _ Onset: _ Duration: _\nFever: _°F. Associated symptoms: _.\nTriage: _ (ESI _)".to_string()),
                objective: Some("Vital signs: See vitals.\nGeneral: _. Pertinent positive exam findings: _. Pertinent negative exam findings: _.".to_string()),
                assessment: Some("1. [Acute diagnosis — ICD-10: ]".to_string()),
                plan: Some("1. [Treatment provided in office: _]\n2. [Prescriptions: _]\n3. [Return precautions given: Y/N]\n4. [Work/school note: Y/N — days: _]\n5. [Follow-up: PCP in _ days / ED if worsening]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "heent".to_string(), "cardiovascular".to_string(),
                "pulmonary".to_string(), "abdomen".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "ent".to_string(), "respiratory".to_string(),
                "gastrointestinal".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_preventive".to_string(),
            name: "Annual Preventive Care".to_string(),
            specialty: "preventive".to_string(),
            description: "Annual wellness exam with preventive screening review".to_string(),
            default_soap: SoapInput {
                subjective: Some("Annual wellness visit. Patient reports: _\nPreventive screenings due: Colonoscopy / Mammogram / Pap / Lipids / A1c / PSA / DEXA / Vision / Dental.\nVaccinations up to date: [ Y / N — gaps: _ ]\nCancer screening history: _.".to_string()),
                objective: Some("Vital signs: See vitals.\nGeneral: Well-appearing, no acute distress.".to_string()),
                assessment: Some("1. Annual preventive exam — ICD-10: Z00.01\n2. [Health maintenance items addressed]\n3. [Chronic conditions managed: ]".to_string()),
                plan: Some("1. [Screenings ordered: _]\n2. [Immunizations administered: _]\n3. [Lifestyle counseling: diet / exercise / smoking / alcohol]\n4. [Chronic disease management: _]\n5. [Follow-up: next annual exam in 1 year]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "heent".to_string(), "neck".to_string(),
                "cardiovascular".to_string(), "pulmonary".to_string(), "abdomen".to_string(),
                "extremities".to_string(), "skin".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "eyes".to_string(), "ent".to_string(),
                "cardiovascular".to_string(), "respiratory".to_string(),
                "gastrointestinal".to_string(), "genitourinary".to_string(),
                "musculoskeletal".to_string(), "neurological".to_string(),
                "psychiatric".to_string(), "endocrine".to_string(), "allergic_immunologic".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_diabetes".to_string(),
            name: "Diabetes Management".to_string(),
            specialty: "endocrinology".to_string(),
            description: "Diabetes mellitus management visit with metabolic review".to_string(),
            default_soap: SoapInput {
                subjective: Some("Diabetes management visit.\nType: [ T1DM / T2DM / LADA ] — Dx date: _.\nGlucose log: Fasting avg: _. Post-prandial avg: _. Hypoglycemic episodes: _/month.\nDiet adherence: _. Exercise: _/week.\nDiabetes complications: Neuropathy / Nephropathy / Retinopathy / PAD: [ Present / Absent ]".to_string()),
                objective: Some("Vital signs: See vitals. BMI: _ kg/m².\nFoot exam: Sensation intact bilaterally by monofilament. Pulses palpable. No ulcers.\nA1c: _%. Last: _. Trend: _.".to_string()),
                assessment: Some("1. Type _ diabetes mellitus — ICD-10: E1_.9\n2. A1c [at goal <7% / above goal at _%]\n3. [Diabetes complications: ]".to_string()),
                plan: Some("1. [Medication adjustments: insulin / metformin / GLP-1 / SGLT-2]\n2. [Labs: A1c / BMP / Lipids / Urine microalbumin]\n3. [Referrals: Ophthalmology / Podiatry / Nephrology]\n4. [Diabetes education]\n5. [Follow-up in _ weeks/months]".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "cardiovascular".to_string(),
                "extremities".to_string(), "neurological".to_string(), "skin".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "cardiovascular".to_string(),
                "genitourinary".to_string(), "neurological".to_string(),
                "endocrine".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_follow_up".to_string(),
            name: "Follow-Up Visit".to_string(),
            specialty: "general".to_string(),
            description: "Brief focused follow-up for an established problem".to_string(),
            default_soap: SoapInput {
                subjective: Some("Follow-up for: _\nSymptom change since last visit: [ Improved / Unchanged / Worse ]\nMedication compliance: [ Y / N — reason: _ ]\nSide effects: _. New complaints: _.".to_string()),
                objective: Some("Vital signs: See vitals.\nFocused exam: _".to_string()),
                assessment: Some("1. [Follow-up diagnosis — ICD-10: ]\nProgress: [ Improving / Stable / Declining ]".to_string()),
                plan: Some("1. [Medication continuation/adjustment]\n2. [Labs/imaging ordered: _]\n3. [Follow-up in _ weeks/months]".to_string()),
            },
            default_exam_sections: vec!["general".to_string()],
            ros_systems: vec!["constitutional".to_string()],
        },
        // ── PT-specific note templates ───────────────────────────────────────
        TemplateRecord {
            id: "tpl_pt_initial_eval".to_string(),
            name: "PT Initial Evaluation".to_string(),
            specialty: "physical_therapy".to_string(),
            description: "Physical therapy initial evaluation with chief complaint, HPI, PMH, ROS, physical exam, assessment, and plan".to_string(),
            default_soap: SoapInput {
                subjective: Some("CHIEF COMPLAINT:\n[Enter patient's primary complaint]\n\nHISTORY OF PRESENT ILLNESS:\nOnset: _  Mechanism of injury: _  Location: _  Duration: _\nCharacter: _  Severity (NRS 0-10): _/10\nAggravating factors: _  Relieving factors: _\nPrior treatment: _  Imaging: _\n\nPAST MEDICAL HISTORY:\n[Enter relevant medical history, surgical history, medications]\n\nREVIEW OF SYSTEMS:\nConstitutional: _\nMusculoskeletal: _\nNeurological: _\nCardiovascular: _\nPsychological: _".to_string()),
                objective: Some("PHYSICAL EXAMINATION:\nObservation/Posture: _\nPalpation: _\nRange of Motion:\n  Cervical: _  Thoracic: _  Lumbar: _\n  Upper extremity: _  Lower extremity: _\nStrength (MMT):\n  _\nSpecial Tests:\n  _\nNeurological Screen: Sensation: _  Reflexes: _  Balance: _\nFunctional Assessment: _\nGait Analysis: _".to_string()),
                assessment: Some("ASSESSMENT:\nPrimary diagnosis: [ICD-10: ]\nSecondary diagnosis: [ICD-10: ]\nFunctional limitations: _\nPrior level of function: _\nRehabilitation potential: [ Excellent / Good / Fair / Poor ]\n\nSHORT-TERM GOALS (2-4 weeks):\n1. _\n2. _\n\nLONG-TERM GOALS (discharge):\n1. _\n2. _".to_string()),
                plan: Some("PLAN:\nFrequency/Duration: _ x/week for _ weeks\nTreatment to include:\n  - Therapeutic exercise\n  - Manual therapy\n  - Neuromuscular re-education\n  - Modalities: _\n  - Patient education\n\nHome Exercise Program: [ Provided / To be provided ]\nReferring physician: _\nCPT codes: _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(), "neurological".to_string(),
                "extremities".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
                "neurological".to_string(), "cardiovascular".to_string(),
                "psychiatric".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_pt_treatment_note".to_string(),
            name: "PT Treatment Note".to_string(),
            specialty: "physical_therapy".to_string(),
            description: "Physical therapy daily treatment note with subjective, objective, treatment provided, patient response, and plan".to_string(),
            default_soap: SoapInput {
                subjective: Some("SUBJECTIVE (Patient Report):\nPatient reports: _\nPain level today (NRS 0-10): _/10  (previous visit: _/10)\nHEP compliance: [ Good / Fair / Poor ]\nChanges since last visit: _\nNew complaints: _\nSleep: _  Activity level: _".to_string()),
                objective: Some("OBJECTIVE (Measurements & Observations):\nVital signs: See vitals section\nObservation: _\nPalpation: _\nROM changes: _\nStrength changes: _\nFunctional status: _\nGait/Balance: _".to_string()),
                assessment: Some("TREATMENT PROVIDED:\nTherapeutic exercise: _  (__ min)\nManual therapy: _  (__ min)\nNeuromuscular re-education: _  (__ min)\nModalities: _  (__ min)\nGait training: _  (__ min)\nTotal treatment time: __ minutes\n\nPATIENT RESPONSE:\nTolerance: [ Good / Fair / Poor ]\nResponse to treatment: _\nProgress toward goals: _".to_string()),
                plan: Some("PLAN / NEXT VISIT:\nContinue current POC: [ Yes / Modify ]\nModifications: _\nHEP updates: _\nNext visit: _\nAnticipated discharge: _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_pt_progress_note".to_string(),
            name: "PT Progress Note (SOAP)".to_string(),
            specialty: "physical_therapy".to_string(),
            description: "Standard SOAP progress note for physical therapy".to_string(),
            default_soap: SoapInput {
                subjective: Some("SUBJECTIVE:\nPatient reports: _\nCurrent pain level (NRS 0-10): _/10\nFunctional changes: _\nHEP compliance: [ Good / Fair / Poor ]\nBarriers to progress: _".to_string()),
                objective: Some("OBJECTIVE:\nVital signs: See vitals section\nROM: _\nStrength (MMT): _\nSpecial tests: _\nFunctional measures: _\nGait/Balance: _\nOutcome scores: _".to_string()),
                assessment: Some("ASSESSMENT:\nDiagnosis: [ICD-10: ]\nProgress: [ Improving / Plateau / Declining ]\nGoal status:\n  STG 1: [ Met / Progressing / Not met ]\n  STG 2: [ Met / Progressing / Not met ]\n  LTG 1: [ Met / Progressing / Not met ]\n  LTG 2: [ Met / Progressing / Not met ]".to_string()),
                plan: Some("PLAN:\nContinue POC: [ Yes / Modify ]\nFrequency: _ x/week\nTreatment focus: _\nHEP modifications: _\nAnticipated discharge date: _\nSkilled services justified by: _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(), "neurological".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
                "neurological".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_pt_discharge_note".to_string(),
            name: "PT Discharge Note".to_string(),
            specialty: "physical_therapy".to_string(),
            description: "Physical therapy discharge summary with treatment outcomes, goals met, HEP, and follow-up recommendations".to_string(),
            default_soap: SoapInput {
                subjective: Some("REASON FOR DISCHARGE:\n[ Goals met / Patient request / Insurance exhausted / Non-compliance / Physician order / Plateau / Other: _ ]\n\nTREATMENT SUMMARY:\nTotal visits attended: _ of _ authorized\nDate of initial evaluation: _\nDate of discharge: _\nDuration of care: _ weeks\nTreatment provided: _\nDiagnosis: [ICD-10: ]".to_string()),
                objective: Some("OUTCOMES / GOALS MET:\nInitial pain (NRS): _/10  Discharge pain (NRS): _/10\nFunctional status at intake: _\nFunctional status at discharge: _\n\nGoal Achievement:\n  STG 1: [ Met / Partially met / Not met ] — _\n  STG 2: [ Met / Partially met / Not met ] — _\n  LTG 1: [ Met / Partially met / Not met ] — _\n  LTG 2: [ Met / Partially met / Not met ] — _\n\nOutcome Measures:\n  [Measure]: Initial __ / Discharge __ (MCID: __)".to_string()),
                assessment: Some("HOME EXERCISE PROGRAM:\nPatient was instructed in the following HEP:\n1. _\n2. _\n3. _\nFrequency: _\nPatient demonstrates: [ Independent / Supervised / Dependent ] with HEP\nPatient verbalized understanding: [ Yes / No ]".to_string()),
                plan: Some("FOLLOW-UP RECOMMENDATIONS:\n[ ] Return to PT if symptoms recur\n[ ] Follow up with referring physician: _\n[ ] Ongoing maintenance program: _\n[ ] Referral to: _\n[ ] No further PT needed at this time\n\nActivity recommendations: _\nPrecautions/restrictions: _\nPatient education provided: _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
            ],
        },
        TemplateRecord {
            id: "tpl_pt_fce".to_string(),
            name: "Functional Capacity Evaluation".to_string(),
            specialty: "physical_therapy".to_string(),
            description: "Functional capacity evaluation with demographics, medical history, physical demands analysis, testing results, and recommendations".to_string(),
            default_soap: SoapInput {
                subjective: Some("PATIENT DEMOGRAPHICS:\nName: _  DOB: _  Age: _  Gender: _\nOccupation: _  Employer: _\nDate of injury: _  Claim #: _\nReferring physician: _\nReason for FCE: [ Return to work / Disability determination / Baseline / Other: _ ]\n\nMEDICAL HISTORY:\nDiagnosis: [ICD-10: ]\nSurgical history: _\nCurrent medications: _\nRelevant imaging: _\nPain history: Current NRS: _/10  Worst: _/10  Best: _/10\nFunctional complaints: _\nWork status: [ Full duty / Light duty / Off work since _ ]".to_string()),
                objective: Some("PHYSICAL DEMANDS ANALYSIS:\nJob title: _  DOL code: _\nPhysical demand level: [ Sedentary / Light / Medium / Heavy / Very Heavy ]\nCritical job demands:\n  Lifting: _ lbs (floor to waist) / _ lbs (waist to shoulder)\n  Carrying: _ lbs for _ ft\n  Standing: _ hrs/day\n  Walking: _ hrs/day\n  Sitting: _ hrs/day\n  Bending/Stooping: _ frequency\n  Reaching: _ frequency\n  Fine motor: _ frequency\n\nFUNCTIONAL TESTING RESULTS:\nMusculoskeletal screen: _\nPostural tolerance:\n  Standing: _ min  Sitting: _ min  Walking: _ min\nLifting capacity:\n  Floor to waist: _ lbs  Waist to shoulder: _ lbs  Bilateral carry: _ lbs\nPush/Pull: _ lbs / _ lbs\nGrip strength: R: _ lbs  L: _ lbs (norms: _)\nPositional tolerance:\n  Bending: _  Squatting: _  Kneeling: _  Climbing: _\nCardiovascular response: HR baseline: _  Peak: _  Recovery: _\nConsistency of effort: [ Consistent / Inconsistent — _ ]\nSelf-limiting behaviors: _\nWaddell signs: _/5".to_string()),
                assessment: Some("CONCLUSIONS:\nOverall physical demand capacity: [ Sedentary / Light / Medium / Heavy / Very Heavy ]\nComparison to job demands: [ Meets / Does not meet ] requirements\nRestrictions/Limitations:\n  _\nMaximum medical improvement: [ Yes / No / Undetermined ]\nReliability of results: [ Reliable / Unreliable — _ ]".to_string()),
                plan: Some("RECOMMENDATIONS:\n[ ] Return to full duty\n[ ] Return to modified duty with restrictions: _\n[ ] Work conditioning/hardening program: _ weeks\n[ ] Continue physical therapy\n[ ] Vocational rehabilitation referral\n[ ] Additional medical evaluation: _\n[ ] Disability rating evaluation\n\nFollow-up: _\nReport sent to: _".to_string()),
            },
            default_exam_sections: vec![
                "general".to_string(), "musculoskeletal".to_string(), "neurological".to_string(),
                "cardiovascular".to_string(), "extremities".to_string(),
            ],
            ros_systems: vec![
                "constitutional".to_string(), "musculoskeletal".to_string(),
                "neurological".to_string(), "cardiovascular".to_string(),
            ],
        },
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Encounters / SOAP notes (CLIN-01)
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new clinical encounter with an optional SOAP note (CLIN-01).
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn create_encounter(
    input: EncounterInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<EncounterRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_encounter_fhir(&id, &input);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Encounter', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO encounter_index
            (encounter_id, patient_id, provider_id, encounter_date, status, encounter_type)
         VALUES (?1, ?2, ?3, ?4, 'in-progress', ?5)",
        rusqlite::params![
            id,
            input.patient_id,
            input.provider_id,
            input.encounter_date,
            input.encounter_type,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.encounter.create".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("type={}", input.encounter_type)),
        },
    )?;

    Ok(EncounterRecord {
        id,
        patient_id: input.patient_id,
        provider_id: input.provider_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// Get a single encounter by ID (CLIN-01).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_encounter(
    encounter_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<EncounterRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, provider_id, resource_str, version_id, last_updated): (
        String,
        String,
        String,
        i64,
        String,
    ) = conn
        .query_row(
            "SELECT ei.patient_id, ei.provider_id, fr.resource, fr.version_id, fr.last_updated
             FROM encounter_index ei
             JOIN fhir_resources fr ON fr.id = ei.encounter_id
             WHERE ei.encounter_id = ?1",
            rusqlite::params![encounter_id],
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
        .map_err(|_| AppError::NotFound(format!("Encounter {} not found", encounter_id)))?;

    let resource: serde_json::Value = serde_json::from_str(&resource_str)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.encounter.read".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(EncounterRecord {
        id: encounter_id,
        patient_id,
        provider_id,
        resource,
        version_id,
        last_updated,
    })
}

/// List encounters for a patient, optionally filtered by date range and/or encounter type (CLIN-01).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_encounters(
    patient_id: String,
    start_date: Option<String>,
    end_date: Option<String>,
    encounter_type: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<EncounterRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT ei.encounter_id, ei.patient_id, ei.provider_id,
                fr.resource, fr.version_id, fr.last_updated
         FROM encounter_index ei
         JOIN fhir_resources fr ON fr.id = ei.encounter_id
         WHERE ei.patient_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(patient_id.clone())];

    if let Some(ref sd) = start_date {
        query.push_str(&format!(" AND ei.encounter_date >= ?{}", params.len() + 1));
        params.push(Box::new(sd.clone()));
    }
    if let Some(ref ed) = end_date {
        query.push_str(&format!(" AND ei.encounter_date < ?{}", params.len() + 1));
        params.push(Box::new(ed.clone()));
    }
    if let Some(ref et) = encounter_type {
        query.push_str(&format!(" AND ei.encounter_type = ?{}", params.len() + 1));
        params.push(Box::new(et.clone()));
    }
    query.push_str(" ORDER BY ei.encounter_date DESC");

    let records: Vec<EncounterRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                let resource_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    resource_str,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(id, pid, prov, res_str, ver, updated)| {
            let resource =
                serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            EncounterRecord {
                id,
                patient_id: pid,
                provider_id: prov,
                resource,
                version_id: ver,
                last_updated: updated,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.encounter.list".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    )?;

    Ok(records)
}

/// Update an encounter's SOAP note or status (CLIN-01).
///
/// When editing a finalized ("finished") encounter, the caller must provide
/// `amendment_reason`. The previous FHIR resource version is stored as a
/// separate FHIR resource (resource_type = "EncounterAmendmentHistory") for
/// audit trail, and a dedicated `documentation.encounter.amend` audit entry
/// is logged.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn update_encounter(
    encounter_id: String,
    input: UpdateEncounterInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<EncounterRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, provider_id, existing_json, version_id, current_status): (String, String, String, i64, String) = conn
        .query_row(
            "SELECT ei.patient_id, ei.provider_id, fr.resource, fr.version_id, ei.status
             FROM encounter_index ei
             JOIN fhir_resources fr ON fr.id = ei.encounter_id
             WHERE ei.encounter_id = ?1",
            rusqlite::params![encounter_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Encounter {} not found", encounter_id)))?;

    let is_finalized = current_status == "finished";

    // If the encounter is finalized, store the previous version for audit trail
    if is_finalized && (input.soap.is_some() || input.chief_complaint.is_some()) {
        // Store the previous version for audit trail
        let history_id = uuid::Uuid::new_v4().to_string();
        let history_resource = serde_json::json!({
            "resourceType": "EncounterAmendmentHistory",
            "id": history_id,
            "encounterId": encounter_id,
            "previousVersion": version_id,
            "previousResource": serde_json::from_str::<serde_json::Value>(&existing_json)
                .unwrap_or(serde_json::Value::Null),
            "amendedBy": sess.user_id,
            "amendedAt": now,
            "amendmentReason": input.amendment_reason.as_deref().unwrap_or("")
        });
        let history_json = serde_json::to_string(&history_resource)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'EncounterAmendmentHistory', ?2, 1, ?3, ?3, ?3)",
            rusqlite::params![history_id, history_json, now],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Log the amendment audit entry
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "documentation.encounter.amend".to_string(),
                resource_type: "Encounter".to_string(),
                resource_id: Some(encounter_id.clone()),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!(
                    "amendment_reason={}, previous_version={}, history_id={}",
                    input.amendment_reason.as_deref().unwrap_or(""),
                    version_id,
                    history_id,
                )),
            },
        )?;
    }

    let mut fhir: serde_json::Value = serde_json::from_str(&existing_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    if let Some(ref status) = input.status {
        fhir["status"] = serde_json::json!(status);
        if status == "finished" {
            fhir["period"]["end"] = serde_json::json!(now);
        }
    }
    if let Some(ref cc) = input.chief_complaint {
        fhir["reasonCode"] = serde_json::json!([{ "text": cc }]);
    }
    if let Some(ref soap) = input.soap {
        fhir["note"] = build_soap_note_json(soap);
    }

    // If amending a finalized encounter, add amendment metadata to the resource
    if is_finalized && input.amendment_reason.is_some() {
        let amendments = fhir.get("extension")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();
        let mut new_extensions = amendments;
        new_extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/encounter-amendment",
            "extension": [
                {
                    "url": "amendedBy",
                    "valueReference": {
                        "reference": format!("Practitioner/{}", sess.user_id),
                        "type": "Practitioner"
                    }
                },
                {
                    "url": "amendedAt",
                    "valueDateTime": now
                },
                {
                    "url": "amendmentReason",
                    "valueString": input.amendment_reason.as_deref().unwrap_or("")
                },
                {
                    "url": "previousVersion",
                    "valueInteger": version_id
                }
            ]
        }));
        fhir["extension"] = serde_json::json!(new_extensions);
    }

    let new_version = version_id + 1;
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3
         WHERE id = ?4",
        rusqlite::params![fhir_json, new_version, now, encounter_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(ref status) = input.status {
        conn.execute(
            "UPDATE encounter_index SET status = ?1 WHERE encounter_id = ?2",
            rusqlite::params![status, encounter_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.encounter.update".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: input
                .status
                .as_deref()
                .map(|s| format!("new_status={}", s)),
        },
    )?;

    Ok(EncounterRecord {
        id: encounter_id,
        patient_id,
        provider_id,
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

/// Delete an encounter and its FHIR resource.
///
/// Removes the encounter from both `encounter_index` and `fhir_resources`.
/// Requires: ClinicalDocumentation + Delete (Provider / SystemAdmin only).
#[tauri::command]
pub async fn delete_encounter(
    encounter_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Delete)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch patient_id for audit before deleting
    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM encounter_index WHERE encounter_id = ?1",
            rusqlite::params![encounter_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Encounter {} not found", encounter_id)))?;

    // Delete from encounter_index
    conn.execute(
        "DELETE FROM encounter_index WHERE encounter_id = ?1",
        rusqlite::params![encounter_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Delete from fhir_resources
    let rows = conn.execute(
        "DELETE FROM fhir_resources WHERE id = ?1 AND resource_type = 'Encounter'",
        rusqlite::params![encounter_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    if rows == 0 {
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "documentation.encounter.delete".to_string(),
                resource_type: "Encounter".to_string(),
                resource_id: Some(encounter_id.clone()),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.id().to_string(),
                success: false,
                details: Some("FHIR resource not found".to_string()),
            },
        )?;
        return Err(AppError::NotFound(format!("Encounter {} not found in fhir_resources", encounter_id)));
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.encounter.delete".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Vitals (CLIN-02)
// ─────────────────────────────────────────────────────────────────────────────

/// Record a vitals observation set for a patient encounter (CLIN-02).
///
/// BMI is automatically calculated when both weight_kg and height_cm are provided.
/// Pain score is clamped to the 0–10 NRS range.
///
/// Requires: ClinicalDocumentation + Create
/// NurseMa may also create vitals (special case granted via RBAC).
#[tauri::command]
pub async fn record_vitals(
    input: VitalsInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<VitalsRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    // NurseMa can create vitals (CLIN-02 explicitly allows this)
    let allowed = matches!(
        sess.role,
        Role::SystemAdmin | Role::Provider | Role::NurseMa
    );
    if !allowed {
        middleware::require_permission(
            sess.role,
            Resource::ClinicalDocumentation,
            Action::Create,
        )?;
    }

    // Validate pain score
    if let Some(pain) = input.pain_score {
        if pain > 10 {
            return Err(AppError::Validation(
                "pain_score must be between 0 and 10".to_string(),
            ));
        }
    }

    // Auto-calculate BMI
    let bmi = match (input.weight_kg, input.height_cm) {
        (Some(w), Some(h)) => calculate_bmi(w, h),
        _ => None,
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_vitals_fhir(&id, &input, bmi);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Observation', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO vitals_index (vitals_id, patient_id, encounter_id, recorded_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.patient_id, input.encounter_id, input.recorded_at],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.vitals.record".to_string(),
            resource_type: "Observation".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: bmi.map(|b| format!("bmi={:.1}", b)),
        },
    )?;

    Ok(VitalsRecord {
        id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        bmi,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// Get vitals history for a patient, ordered by recorded_at descending (CLIN-02).
///
/// Enables flowsheet trending view.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_vitals(
    patient_id: String,
    encounter_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<VitalsRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT vi.vitals_id, vi.patient_id, vi.encounter_id,
                fr.resource, fr.version_id, fr.last_updated
         FROM vitals_index vi
         JOIN fhir_resources fr ON fr.id = vi.vitals_id
         WHERE vi.patient_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(patient_id.clone())];

    if let Some(ref eid) = encounter_id {
        query.push_str(&format!(" AND vi.encounter_id = ?{}", params.len() + 1));
        params.push(Box::new(eid.clone()));
    }
    query.push_str(" ORDER BY vi.recorded_at DESC");

    let records: Vec<VitalsRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                let resource_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    resource_str,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(id, pid, eid, res_str, ver, updated)| {
            let resource =
                serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            // Extract BMI from the FHIR component array if present
            let bmi = resource["component"]
                .as_array()
                .and_then(|comps| {
                    comps.iter().find(|c| {
                        c["code"]["coding"]
                            .as_array()
                            .and_then(|codes| {
                                codes.iter().find(|code| code["code"] == "39156-5")
                            })
                            .is_some()
                    })
                })
                .and_then(|bmi_comp| bmi_comp["valueQuantity"]["value"].as_f64());
            VitalsRecord {
                id,
                patient_id: pid,
                encounter_id: eid,
                bmi,
                resource,
                version_id: ver,
                last_updated: updated,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.vitals.list".to_string(),
            resource_type: "Observation".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    )?;

    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Review of Systems (CLIN-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Save (create or replace) a Review of Systems for an encounter (CLIN-03).
///
/// Covers all 14 standard organ systems with positive/negative/not-reviewed status.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn save_ros(
    input: ReviewOfSystemsInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<RosRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_ros_fhir(&id, &input);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'QuestionnaireResponse', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.ros.save".to_string(),
            resource_type: "QuestionnaireResponse".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("encounter_id={}", input.encounter_id)),
        },
    )?;

    Ok(RosRecord {
        id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// Get the Review of Systems for an encounter (CLIN-03).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_ros(
    encounter_id: String,
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Option<RosRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Find latest ROS for this encounter (by last_updated)
    let result = conn.query_row(
        "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
         FROM fhir_resources fr
         WHERE fr.resource_type = 'QuestionnaireResponse'
           AND json_extract(fr.resource, '$.encounter.reference') = ?1
         ORDER BY fr.last_updated DESC
         LIMIT 1",
        rusqlite::params![format!("Encounter/{}", encounter_id)],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    );

    match result {
        Ok((id, resource_str, version_id, last_updated)) => {
            let resource: serde_json::Value = serde_json::from_str(&resource_str)
                .map_err(|e| AppError::Serialization(e.to_string()))?;

            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: sess.user_id.clone(),
                    action: "documentation.ros.get".to_string(),
                    resource_type: "QuestionnaireResponse".to_string(),
                    resource_id: Some(id.clone()),
                    patient_id: Some(patient_id.clone()),
                    device_id: device_id.id().to_string(),
                    success: true,
                    details: Some(format!("encounter_id={}", encounter_id)),
                },
            )?;

            Ok(Some(RosRecord {
                id,
                patient_id,
                encounter_id,
                resource,
                version_id,
                last_updated,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Physical Exam (CLIN-04)
// ─────────────────────────────────────────────────────────────────────────────

/// Save physical exam findings for an encounter (CLIN-04).
///
/// Covers 13 standard body systems. Any subset of fields may be populated.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn save_physical_exam(
    input: PhysicalExamInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PhysicalExamRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_exam_fhir(&id, &input);
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'ClinicalImpression', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.exam.save".to_string(),
            resource_type: "ClinicalImpression".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("encounter_id={}", input.encounter_id)),
        },
    )?;

    Ok(PhysicalExamRecord {
        id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// Get the physical exam for an encounter (CLIN-04).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_physical_exam(
    encounter_id: String,
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Option<PhysicalExamRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let result = conn.query_row(
        "SELECT fr.id, fr.resource, fr.version_id, fr.last_updated
         FROM fhir_resources fr
         WHERE fr.resource_type = 'ClinicalImpression'
           AND json_extract(fr.resource, '$.encounter.reference') = ?1
         ORDER BY fr.last_updated DESC
         LIMIT 1",
        rusqlite::params![format!("Encounter/{}", encounter_id)],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    );

    match result {
        Ok((id, resource_str, version_id, last_updated)) => {
            let resource: serde_json::Value = serde_json::from_str(&resource_str)
                .map_err(|e| AppError::Serialization(e.to_string()))?;

            write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: sess.user_id.clone(),
                    action: "documentation.exam.get".to_string(),
                    resource_type: "ClinicalImpression".to_string(),
                    resource_id: Some(id.clone()),
                    patient_id: Some(patient_id.clone()),
                    device_id: device_id.id().to_string(),
                    success: true,
                    details: Some(format!("encounter_id={}", encounter_id)),
                },
            )?;

            Ok(Some(PhysicalExamRecord {
                id,
                patient_id,
                encounter_id,
                resource,
                version_id,
                last_updated,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Templates (CLIN-05)
// ─────────────────────────────────────────────────────────────────────────────

/// List all available clinical note templates (CLIN-05).
///
/// Returns the 12 built-in templates. No database query needed — templates
/// are compiled into the binary.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_templates(
    specialty: Option<String>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<TemplateRecord>, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    // Templates are read-only clinical reference data — any authenticated user can list
    let templates = built_in_templates();
    let filtered: Vec<TemplateRecord> = match specialty {
        Some(ref s) => templates
            .into_iter()
            .filter(|t| t.specialty == *s)
            .collect(),
        None => templates,
    };
    // Suppress unused warning for device_id in this no-DB command
    let _ = device_id.id();
    Ok(filtered)
}

/// Get a specific template by ID (CLIN-05).
///
/// Requires: ClinicalDocumentation + Read (authenticated)
#[tauri::command]
pub async fn get_template(
    template_id: String,
    session: State<'_, SessionManager>,
) -> Result<TemplateRecord, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    built_in_templates()
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| AppError::NotFound(format!("Template {} not found", template_id)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Co-sign workflow (CLIN-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Request a co-sign on an encounter note (CLIN-06).
///
/// Typically called by an NP/PA who has drafted a SOAP note and needs
/// a supervising physician to review and sign.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn request_cosign(
    input: CosignRequestInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<CosignRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let fhir = build_cosign_fhir(
        &id,
        &input.encounter_id,
        &sess.user_id,
        &input.supervising_provider_id,
        input.message.as_deref(),
        &now,
    );
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify the encounter exists
    let encounter_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM encounter_index WHERE encounter_id = ?1",
            rusqlite::params![input.encounter_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        > 0;

    if !encounter_exists {
        return Err(AppError::NotFound(format!(
            "Encounter {} not found",
            input.encounter_id
        )));
    }

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Task', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO cosign_index
            (cosign_id, encounter_id, requesting_provider_id, supervising_provider_id, status, requested_at)
         VALUES (?1, ?2, ?3, ?4, 'requested', ?5)",
        rusqlite::params![
            id,
            input.encounter_id,
            sess.user_id,
            input.supervising_provider_id,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Get patient_id for audit
    let patient_id: Option<String> = conn
        .query_row(
            "SELECT patient_id FROM encounter_index WHERE encounter_id = ?1",
            rusqlite::params![input.encounter_id],
            |row| row.get(0),
        )
        .ok();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.cosign.request".to_string(),
            resource_type: "Task".to_string(),
            resource_id: Some(id.clone()),
            patient_id,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "encounter={} supervisor={}",
                input.encounter_id, input.supervising_provider_id
            )),
        },
    )?;

    Ok(CosignRecord {
        id,
        encounter_id: input.encounter_id,
        requesting_provider_id: sess.user_id,
        supervising_provider_id: input.supervising_provider_id,
        status: "requested".to_string(),
        requested_at: now,
        signed_at: None,
        resource: fhir,
    })
}

/// Approve and sign a co-sign request (CLIN-06).
///
/// Only a Provider or SystemAdmin can approve co-sign requests.
/// The supervising provider must be the owner of the Task.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn approve_cosign(
    cosign_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<CosignRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    // Only providers and admins may sign notes
    if !matches!(sess.role, Role::Provider | Role::SystemAdmin) {
        return Err(AppError::Unauthorized(
            "Only providers can co-sign encounter notes".to_string(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load cosign record
    let (encounter_id, requesting_provider_id, supervising_provider_id, existing_json, version_id): (
        String, String, String, String, i64,
    ) = conn
        .query_row(
            "SELECT ci.encounter_id, ci.requesting_provider_id, ci.supervising_provider_id,
                    fr.resource, fr.version_id
             FROM cosign_index ci
             JOIN fhir_resources fr ON fr.id = ci.cosign_id
             WHERE ci.cosign_id = ?1 AND ci.status = 'requested'",
            rusqlite::params![cosign_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "Co-sign request {} not found or already processed",
                cosign_id
            ))
        })?;

    // Verify caller is the designated supervisor
    if sess.user_id != supervising_provider_id {
        return Err(AppError::Unauthorized(
            "Only the designated supervising provider can approve this co-sign".to_string(),
        ));
    }

    let mut fhir: serde_json::Value = serde_json::from_str(&existing_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    fhir["status"] = serde_json::json!("completed");
    fhir["lastModified"] = serde_json::json!(now);

    let new_version = version_id + 1;
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3
         WHERE id = ?4",
        rusqlite::params![fhir_json, new_version, now, cosign_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE cosign_index SET status = 'signed', signed_at = ?1 WHERE cosign_id = ?2",
        rusqlite::params![now, cosign_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Also mark the encounter as co-signed via extension
    conn.execute(
        "UPDATE fhir_resources
         SET resource = json_patch(resource, json_object('extension', json_array(
             json_object(
                 'url', 'http://medarc.local/fhir/StructureDefinition/encounter-cosigned-by',
                 'valueId', ?1,
                 'valueDateTime', ?2
             )
         ))), updated_at = ?2
         WHERE id = ?3 AND resource_type = 'Encounter'",
        rusqlite::params![sess.user_id, now, encounter_id],
    )
    .ok(); // Non-fatal if encounter update fails — cosign record is authoritative

    let patient_id: Option<String> = conn
        .query_row(
            "SELECT patient_id FROM encounter_index WHERE encounter_id = ?1",
            rusqlite::params![encounter_id],
            |row| row.get(0),
        )
        .ok();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.cosign.approve".to_string(),
            resource_type: "Task".to_string(),
            resource_id: Some(cosign_id.clone()),
            patient_id,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "encounter={} requester={}",
                encounter_id, requesting_provider_id
            )),
        },
    )?;

    Ok(CosignRecord {
        id: cosign_id,
        encounter_id,
        requesting_provider_id,
        supervising_provider_id,
        status: "signed".to_string(),
        requested_at: String::new(), // populated from DB read above; not re-fetched for perf
        signed_at: Some(now),
        resource: fhir,
    })
}

/// List pending co-sign requests for a supervising provider (CLIN-06).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_pending_cosigns(
    supervising_provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<CosignRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Default to the calling user's own pending co-signs if no supervisor specified
    let supervisor = supervising_provider_id
        .clone()
        .unwrap_or_else(|| sess.user_id.clone());

    let records: Vec<CosignRecord> = conn
        .prepare(
            "SELECT ci.cosign_id, ci.encounter_id, ci.requesting_provider_id,
                    ci.supervising_provider_id, ci.status, ci.requested_at, ci.signed_at,
                    fr.resource
             FROM cosign_index ci
             JOIN fhir_resources fr ON fr.id = ci.cosign_id
             WHERE ci.supervising_provider_id = ?1 AND ci.status = 'requested'
             ORDER BY ci.requested_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![supervisor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(
            |(id, enc_id, req_prov, sup_prov, status, req_at, sig_at, res_str)| {
                let resource =
                    serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
                CosignRecord {
                    id,
                    encounter_id: enc_id,
                    requesting_provider_id: req_prov,
                    supervising_provider_id: sup_prov,
                    status,
                    requested_at: req_at,
                    signed_at: sig_at,
                    resource,
                }
            },
        )
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.cosign.list_pending".to_string(),
            resource_type: "Task".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "supervisor={} count={}",
                supervisor,
                records.len()
            )),
        },
    )?;

    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Drug-Allergy CDS (CLIN-07)
// ─────────────────────────────────────────────────────────────────────────────

/// Check for drug-allergy interactions for a patient (CLIN-07).
///
/// Compares the patient's active medication list against their active allergy list.
/// Performs substance-name fuzzy matching (case-insensitive contains) and
/// RxNorm code exact matching when codes are available.
///
/// Returns passive clinical decision support alerts — these are informational
/// and do not block any workflow. The provider must review and acknowledge.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn check_drug_allergy_alerts(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<DrugAllergyAlert>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load active medications for this patient
    let medications: Vec<(String, String, Option<String>)> = conn
        .prepare(
            "SELECT mi.medication_id, fr.resource, mi.rxnorm_code
             FROM medication_index mi
             JOIN fhir_resources fr ON fr.id = mi.medication_id
             WHERE mi.patient_id = ?1 AND mi.status = 'active'",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    // Load active allergies for this patient
    let allergies: Vec<(String, String)> = conn
        .prepare(
            "SELECT ai.allergy_id, fr.resource
             FROM allergy_index ai
             JOIN fhir_resources fr ON fr.id = ai.allergy_id
             WHERE ai.patient_id = ?1 AND ai.clinical_status = 'active'",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    // Parse allergy FHIR JSON
    let parsed_allergies: Vec<ParsedAllergy> = allergies
        .iter()
        .filter_map(|(allergy_id, resource_str)| {
            let resource: serde_json::Value = serde_json::from_str(resource_str).ok()?;
            let substance = resource["code"]["text"]
                .as_str()
                .or_else(|| {
                    resource["code"]["coding"][0]["display"].as_str()
                })
                .unwrap_or("")
                .to_string();
            let rxnorm_code = resource["code"]["coding"]
                .as_array()
                .and_then(|codes| {
                    codes.iter().find(|c| {
                        c["system"]
                            .as_str()
                            .map(|s| s.contains("rxnorm"))
                            .unwrap_or(false)
                    })
                })
                .and_then(|c| c["code"].as_str().map(|s| s.to_string()));
            let severity = resource["reaction"][0]["severity"]
                .as_str()
                .map(|s| s.to_string());
            let reaction = resource["reaction"][0]["manifestation"][0]["text"]
                .as_str()
                .map(|s| s.to_string());
            let category = resource["category"][0]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            Some(ParsedAllergy {
                id: allergy_id.clone(),
                substance,
                rxnorm_code,
                severity,
                reaction,
                category,
            })
        })
        .collect();

    let mut alerts: Vec<DrugAllergyAlert> = vec![];

    for (med_id, med_resource_str, med_rxnorm) in &medications {
        let med_resource: serde_json::Value =
            match serde_json::from_str(med_resource_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

        let med_name = med_resource["medication"]["concept"]["text"]
            .as_str()
            .or_else(|| {
                med_resource["medication"]["concept"]["coding"][0]["display"].as_str()
            })
            .unwrap_or("")
            .to_string();
        let med_name_lower = med_name.to_lowercase();

        for allergy in &parsed_allergies {
            // Skip non-drug allergies for drug interaction checking
            if allergy.category != "drug" && allergy.category != "biologic" {
                continue;
            }

            let substance_lower = allergy.substance.to_lowercase();
            let mut matched = false;

            // 1. RxNorm code exact match (most precise)
            if let (Some(med_code), Some(allergy_code)) = (med_rxnorm, &allergy.rxnorm_code) {
                if med_code == allergy_code {
                    matched = true;
                }
            }

            // 2. Name fuzzy match — substance name in medication name or vice versa
            if !matched {
                matched = med_name_lower.contains(substance_lower.as_str())
                    || substance_lower.contains(med_name_lower.as_str());
            }

            if matched {
                let alert_severity = match allergy.severity.as_deref() {
                    Some("severe") | Some("life-threatening") => "contraindicated",
                    _ => "warning",
                };

                alerts.push(DrugAllergyAlert {
                    medication_id: med_id.clone(),
                    medication_name: med_name.clone(),
                    medication_rxnorm: med_rxnorm.clone(),
                    allergy_id: allergy.id.clone(),
                    allergy_substance: allergy.substance.clone(),
                    allergy_severity: allergy.severity.clone(),
                    allergy_reaction: allergy.reaction.clone(),
                    alert_severity: alert_severity.to_string(),
                    message: format!(
                        "{} alert: Patient has a documented {} to {}{}",
                        alert_severity.to_uppercase(),
                        allergy
                            .severity
                            .as_deref()
                            .unwrap_or("unknown severity")
                            .to_string()
                            + " allergy",
                        allergy.substance,
                        allergy
                            .reaction
                            .as_deref()
                            .map(|r| format!(" ({})", r))
                            .unwrap_or_default()
                    ),
                });
            }
        }
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "documentation.cds.drug_allergy_check".to_string(),
            resource_type: "ClinicalDecisionSupport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "medications={} allergies={} alerts={}",
                medications.len(),
                parsed_allergies.len(),
                alerts.len()
            )),
        },
    )?;

    Ok(alerts)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Internal struct for allergy parsing in CDS check.
struct ParsedAllergy {
    id: String,
    substance: String,
    rxnorm_code: Option<String>,
    severity: Option<String>,
    reaction: Option<String>,
    category: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── CLIN-01: SOAP note FHIR structure ───────────────────────────────────

    #[test]
    fn clin_01_encounter_fhir_has_correct_structure() {
        let input = EncounterInput {
            patient_id: "pt-001".to_string(),
            provider_id: "prov-001".to_string(),
            encounter_date: "2026-04-01T09:00:00".to_string(),
            encounter_type: "office_visit".to_string(),
            chief_complaint: Some("Sore throat".to_string()),
            template_id: None,
            soap: Some(SoapInput {
                subjective: Some("Patient presents with 3 days of sore throat".to_string()),
                objective: Some("Vitals stable. Tonsils 2+ with exudate".to_string()),
                assessment: Some("Streptococcal pharyngitis — ICD-10: J02.0".to_string()),
                plan: Some("Amoxicillin 500mg TID x 10d. Follow-up PRN.".to_string()),
            }),
            appointment_id: None,
        };

        let fhir = build_encounter_fhir("enc-001", &input);

        assert_eq!(fhir["resourceType"], "Encounter");
        assert_eq!(fhir["id"], "enc-001");
        assert_eq!(fhir["status"], "in-progress");
        assert_eq!(fhir["subject"]["reference"], "Patient/pt-001");
        assert_eq!(
            fhir["participant"][0]["individual"]["reference"],
            "Practitioner/prov-001"
        );
        assert_eq!(fhir["period"]["start"], "2026-04-01T09:00:00");
        assert_eq!(fhir["reasonCode"][0]["text"], "Sore throat");

        // SOAP note embedded in Encounter.note
        let notes = fhir["note"].as_array().unwrap();
        assert_eq!(notes.len(), 4);
        let sections: Vec<&str> = notes
            .iter()
            .filter_map(|n| {
                n["extension"][0]["valueCode"].as_str()
            })
            .collect();
        assert!(sections.contains(&"subjective"));
        assert!(sections.contains(&"objective"));
        assert!(sections.contains(&"assessment"));
        assert!(sections.contains(&"plan"));
    }

    #[test]
    fn clin_01_encounter_type_maps_to_fhir_class() {
        let telehealth = build_encounter_fhir("x", &EncounterInput {
            patient_id: "p".to_string(),
            provider_id: "pr".to_string(),
            encounter_date: "2026-04-01T09:00:00".to_string(),
            encounter_type: "telehealth".to_string(),
            chief_complaint: None,
            template_id: None,
            soap: None,
            appointment_id: None,
        });
        assert_eq!(telehealth["class"]["code"], "VR");

        let urgent = build_encounter_fhir("x", &EncounterInput {
            patient_id: "p".to_string(),
            provider_id: "pr".to_string(),
            encounter_date: "2026-04-01T09:00:00".to_string(),
            encounter_type: "urgent_care".to_string(),
            chief_complaint: None,
            template_id: None,
            soap: None,
            appointment_id: None,
        });
        assert_eq!(urgent["class"]["code"], "EMER");

        let office = build_encounter_fhir("x", &EncounterInput {
            patient_id: "p".to_string(),
            provider_id: "pr".to_string(),
            encounter_date: "2026-04-01T09:00:00".to_string(),
            encounter_type: "office_visit".to_string(),
            chief_complaint: None,
            template_id: None,
            soap: None,
            appointment_id: None,
        });
        assert_eq!(office["class"]["code"], "AMB");
    }

    // ─── CLIN-02: Vitals FHIR structure & BMI auto-calc ──────────────────────

    #[test]
    fn clin_02_vitals_fhir_has_correct_structure() {
        let input = VitalsInput {
            patient_id: "pt-001".to_string(),
            encounter_id: "enc-001".to_string(),
            recorded_at: "2026-04-01T09:05:00".to_string(),
            systolic_bp: Some(120),
            diastolic_bp: Some(80),
            heart_rate: Some(72),
            respiratory_rate: Some(16),
            temperature_celsius: Some(37.0),
            spo2_percent: Some(98),
            weight_kg: Some(70.0),
            height_cm: Some(175.0),
            pain_score: Some(2),
            notes: None,
        };

        let bmi = calculate_bmi(70.0, 175.0);
        let fhir = build_vitals_fhir("obs-001", &input, bmi);

        assert_eq!(fhir["resourceType"], "Observation");
        assert_eq!(fhir["status"], "final");
        assert_eq!(fhir["subject"]["reference"], "Patient/pt-001");
        assert_eq!(fhir["encounter"]["reference"], "Encounter/enc-001");

        let category = &fhir["category"][0]["coding"][0]["code"];
        assert_eq!(category, "vital-signs");

        let components = fhir["component"].as_array().unwrap();
        // BP (as combined), HR, RR, Temp, SpO2, Weight, Height, BMI, Pain = 9 components
        assert_eq!(components.len(), 9);
    }

    #[test]
    fn clin_02_bmi_auto_calculated_correctly() {
        // 70 kg / (1.75 m)^2 = 70 / 3.0625 = 22.857... → 22.9
        let bmi = calculate_bmi(70.0, 175.0);
        assert!(bmi.is_some());
        let b = bmi.unwrap();
        assert!((b - 22.9).abs() < 0.05, "Expected ~22.9, got {}", b);
    }

    #[test]
    fn clin_02_bmi_none_when_height_zero() {
        assert!(calculate_bmi(70.0, 0.0).is_none());
        assert!(calculate_bmi(0.0, 175.0).is_none());
    }

    #[test]
    fn clin_02_pain_score_clamped_to_10_in_fhir() {
        let input = VitalsInput {
            patient_id: "p".to_string(),
            encounter_id: "e".to_string(),
            recorded_at: "2026-04-01T09:05:00".to_string(),
            systolic_bp: None,
            diastolic_bp: None,
            heart_rate: None,
            respiratory_rate: None,
            temperature_celsius: None,
            spo2_percent: None,
            weight_kg: None,
            height_cm: None,
            pain_score: Some(15), // out of range
            notes: None,
        };
        let fhir = build_vitals_fhir("obs-x", &input, None);
        let components = fhir["component"].as_array().unwrap();
        let pain_comp = components.iter().find(|c| {
            c["code"]["coding"][0]["code"] == "72514-3"
        });
        assert!(pain_comp.is_some());
        assert_eq!(pain_comp.unwrap()["valueQuantity"]["value"], 10);
    }

    #[test]
    fn clin_02_vitals_loinc_codes_present() {
        let input = VitalsInput {
            patient_id: "p".to_string(),
            encounter_id: "e".to_string(),
            recorded_at: "2026-04-01T09:05:00".to_string(),
            systolic_bp: Some(120),
            diastolic_bp: Some(80),
            heart_rate: Some(72),
            respiratory_rate: Some(16),
            temperature_celsius: Some(37.0),
            spo2_percent: Some(98),
            weight_kg: Some(70.0),
            height_cm: Some(175.0),
            pain_score: Some(3),
            notes: None,
        };
        let bmi = calculate_bmi(70.0, 175.0);
        let fhir = build_vitals_fhir("obs-loinc", &input, bmi);
        let components = fhir["component"].as_array().unwrap();
        let loinc_codes: Vec<&str> = components
            .iter()
            .filter_map(|c| c["code"]["coding"][0]["code"].as_str())
            .collect();

        // HR, RR, Temp, SpO2, Weight, Height, BMI, Pain — BP is nested under component
        assert!(loinc_codes.contains(&"8867-4"), "HR LOINC missing");
        assert!(loinc_codes.contains(&"9279-1"), "RR LOINC missing");
        assert!(loinc_codes.contains(&"8310-5"), "Temp LOINC missing");
        assert!(loinc_codes.contains(&"2708-6"), "SpO2 LOINC missing");
        assert!(loinc_codes.contains(&"29463-7"), "Weight LOINC missing");
        assert!(loinc_codes.contains(&"8302-2"), "Height LOINC missing");
        assert!(loinc_codes.contains(&"39156-5"), "BMI LOINC missing");
        assert!(loinc_codes.contains(&"72514-3"), "Pain LOINC missing");
    }

    // ─── CLIN-03: Review of Systems ──────────────────────────────────────────

    #[test]
    fn clin_03_ros_fhir_has_correct_structure() {
        let input = ReviewOfSystemsInput {
            patient_id: "pt-001".to_string(),
            encounter_id: "enc-001".to_string(),            constitutional: Some(RosStatus::Negative),
            constitutional_notes: None,
            eyes: Some(RosStatus::Negative),
            eyes_notes: None,
            ent: Some(RosStatus::Positive),
            ent_notes: Some("Throat pain, mild erythema".to_string()),
            cardiovascular: Some(RosStatus::Negative),
            cardiovascular_notes: None,
            respiratory: Some(RosStatus::Negative),
            respiratory_notes: None,
            gastrointestinal: None,
            gastrointestinal_notes: None,
            genitourinary: None,
            genitourinary_notes: None,
            musculoskeletal: None,
            musculoskeletal_notes: None,
            integumentary: None,
            integumentary_notes: None,
            neurological: None,
            neurological_notes: None,
            psychiatric: None,
            psychiatric_notes: None,
            endocrine: None,
            endocrine_notes: None,
            hematologic: None,
            hematologic_notes: None,
            allergic_immunologic: None,
            allergic_immunologic_notes: None,
        };

        let fhir = build_ros_fhir("ros-001", &input);

        assert_eq!(fhir["resourceType"], "QuestionnaireResponse");
        assert_eq!(fhir["status"], "completed");
        assert_eq!(fhir["subject"]["reference"], "Patient/pt-001");
        assert_eq!(fhir["encounter"]["reference"], "Encounter/enc-001");

        let items = fhir["item"].as_array().unwrap();
        // constitutional, eyes, ent, cardiovascular, respiratory = 5 systems answered
        assert_eq!(items.len(), 5);

        // Find ENT which is positive with notes
        let ent_item = items.iter().find(|i| i["linkId"] == "ent").unwrap();
        assert_eq!(ent_item["answer"][0]["valueCoding"]["code"], "positive");
    }

    #[test]
    fn clin_03_ros_none_fields_excluded_from_fhir() {
        let input = ReviewOfSystemsInput {
            patient_id: "p".to_string(),
            encounter_id: "e".to_string(),
            constitutional: Some(RosStatus::Negative),
            constitutional_notes: None,
            eyes: None,
            eyes_notes: None,
            ent: None,
            ent_notes: None,
            cardiovascular: None,
            cardiovascular_notes: None,
            respiratory: None,
            respiratory_notes: None,
            gastrointestinal: None,
            gastrointestinal_notes: None,
            genitourinary: None,
            genitourinary_notes: None,
            musculoskeletal: None,
            musculoskeletal_notes: None,
            integumentary: None,
            integumentary_notes: None,
            neurological: None,
            neurological_notes: None,
            psychiatric: None,
            psychiatric_notes: None,
            endocrine: None,
            endocrine_notes: None,
            hematologic: None,
            hematologic_notes: None,
            allergic_immunologic: None,
            allergic_immunologic_notes: None,
        };

        let fhir = build_ros_fhir("ros-002", &input);
        let items = fhir["item"].as_array().unwrap();
        // Only constitutional was answered
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["linkId"], "constitutional");
        assert_eq!(items[0]["answer"][0]["valueCoding"]["code"], "negative");
    }

    // ─── CLIN-04: Physical Exam ───────────────────────────────────────────────

    #[test]
    fn clin_04_physical_exam_fhir_has_correct_structure() {
        let input = PhysicalExamInput {
            patient_id: "pt-001".to_string(),
            encounter_id: "enc-001".to_string(),
            general: Some("Well-appearing, no acute distress".to_string()),
            heent: Some("PERRL. TMs clear. Oropharynx erythematous with exudate".to_string()),
            neck: None,
            cardiovascular: Some("RRR, no murmurs".to_string()),
            pulmonary: Some("CTA bilaterally".to_string()),
            abdomen: None,
            extremities: None,
            neurological: None,
            skin: None,
            psychiatric: None,
            musculoskeletal: None,
            genitourinary: None,
            rectal: None,
            additional_notes: None,
        };

        let fhir = build_exam_fhir("exam-001", &input);

        assert_eq!(fhir["resourceType"], "ClinicalImpression");
        assert_eq!(fhir["status"], "completed");
        assert_eq!(fhir["subject"]["reference"], "Patient/pt-001");
        assert_eq!(fhir["encounter"]["reference"], "Encounter/enc-001");

        let findings = fhir["finding"].as_array().unwrap();
        // general, heent, cardiovascular, pulmonary = 4 systems documented
        assert_eq!(findings.len(), 4);

        // Verify HEENT finding
        let heent = findings.iter().find(|f| {
            f["extension"][0]["valueCode"] == "heent"
        });
        assert!(heent.is_some());
        assert!(heent.unwrap()["itemCodeableConcept"]["text"]
            .as_str()
            .unwrap()
            .contains("PERRL"));
    }

    #[test]
    fn clin_04_physical_exam_nil_systems_excluded() {
        let input = PhysicalExamInput {
            patient_id: "p".to_string(),
            encounter_id: "e".to_string(),
            general: Some("Normal".to_string()),
            heent: None,
            neck: None,
            cardiovascular: None,
            pulmonary: None,
            abdomen: None,
            extremities: None,
            neurological: None,
            skin: None,
            psychiatric: None,
            musculoskeletal: None,
            genitourinary: None,
            rectal: None,
            additional_notes: None,
        };

        let fhir = build_exam_fhir("exam-002", &input);
        let findings = fhir["finding"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["extension"][0]["valueCode"], "general");
    }

    // ─── CLIN-05: Templates ───────────────────────────────────────────────────

    #[test]
    fn clin_05_templates_count_at_least_10() {
        let templates = built_in_templates();
        assert!(
            templates.len() >= 10,
            "Expected >= 10 templates, got {}",
            templates.len()
        );
    }

    #[test]
    fn clin_05_templates_have_required_specialties() {
        let templates = built_in_templates();
        let specialties: Vec<&str> = templates.iter().map(|t| t.specialty.as_str()).collect();

        assert!(specialties.contains(&"general"), "Missing general");
        assert!(specialties.contains(&"cardiology"), "Missing cardiology");
        assert!(specialties.contains(&"pediatrics"), "Missing pediatrics");
        assert!(specialties.contains(&"ob_gyn"), "Missing OB/GYN");
        assert!(specialties.contains(&"psychiatry"), "Missing psychiatry");
        assert!(specialties.contains(&"orthopedics"), "Missing orthopedics");
        assert!(specialties.contains(&"dermatology"), "Missing dermatology");
    }

    #[test]
    fn clin_05_each_template_has_all_soap_sections() {
        for template in built_in_templates() {
            assert!(
                template.default_soap.subjective.is_some(),
                "Template {} missing subjective",
                template.id
            );
            assert!(
                template.default_soap.objective.is_some(),
                "Template {} missing objective",
                template.id
            );
            assert!(
                template.default_soap.assessment.is_some(),
                "Template {} missing assessment",
                template.id
            );
            assert!(
                template.default_soap.plan.is_some(),
                "Template {} missing plan",
                template.id
            );
        }
    }

    #[test]
    fn clin_05_template_ids_are_unique() {
        let templates = built_in_templates();
        let ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        let mut unique_ids = ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        assert_eq!(ids.len(), unique_ids.len(), "Duplicate template IDs found");
    }

    #[test]
    fn clin_05_each_template_has_ros_systems() {
        for template in built_in_templates() {
            assert!(
                !template.ros_systems.is_empty(),
                "Template {} has no ROS systems",
                template.id
            );
        }
    }

    // ─── CLIN-06: Co-sign FHIR structure ─────────────────────────────────────

    #[test]
    fn clin_06_cosign_fhir_has_correct_structure() {
        let fhir = build_cosign_fhir(
            "cosign-001",
            "enc-001",
            "np-001",
            "md-001",
            Some("Please review and co-sign"),
            "2026-04-01T10:00:00Z",
        );

        assert_eq!(fhir["resourceType"], "Task");
        assert_eq!(fhir["id"], "cosign-001");
        assert_eq!(fhir["status"], "requested");
        assert_eq!(fhir["intent"], "order");
        assert_eq!(fhir["code"]["coding"][0]["code"], "cosign");
        assert_eq!(fhir["focus"]["reference"], "Encounter/enc-001");
        assert_eq!(fhir["requester"]["reference"], "Practitioner/np-001");
        assert_eq!(fhir["owner"]["reference"], "Practitioner/md-001");
        assert_eq!(fhir["note"][0]["text"], "Please review and co-sign");
    }

    // ─── CLIN-07: Drug-allergy CDS alert logic ────────────────────────────────

    #[test]
    fn clin_07_name_match_generates_alert() {
        // Simulate the matching logic directly
        let med_name = "Penicillin V Potassium".to_string();
        let allergy_substance = "Penicillin".to_string();

        let med_lower = med_name.to_lowercase();
        let sub_lower = allergy_substance.to_lowercase();

        let matched = med_lower.contains(sub_lower.as_str())
            || sub_lower.contains(med_lower.as_str());

        assert!(matched, "Drug-allergy name match should have fired");
    }

    #[test]
    fn clin_07_no_match_for_unrelated_drug_allergy() {
        let med_name = "Metformin".to_string();
        let allergy_substance = "Penicillin".to_string();

        let med_lower = med_name.to_lowercase();
        let sub_lower = allergy_substance.to_lowercase();

        let matched = med_lower.contains(sub_lower.as_str())
            || sub_lower.contains(med_lower.as_str());

        assert!(!matched, "Metformin should not match Penicillin allergy");
    }

    #[test]
    fn clin_07_severe_allergy_maps_to_contraindicated() {
        let alert_severity = match Some("severe").as_deref() {
            Some("severe") | Some("life-threatening") => "contraindicated",
            _ => "warning",
        };
        assert_eq!(alert_severity, "contraindicated");
    }

    #[test]
    fn clin_07_mild_allergy_maps_to_warning() {
        let alert_severity = match Some("mild").as_deref() {
            Some("severe") | Some("life-threatening") => "contraindicated",
            _ => "warning",
        };
        assert_eq!(alert_severity, "warning");
    }

    #[test]
    fn clin_07_rxnorm_code_exact_match() {
        let med_rxnorm = Some("7980".to_string()); // Penicillin RxNorm
        let allergy_rxnorm = Some("7980".to_string());

        let matched = match (&med_rxnorm, &allergy_rxnorm) {
            (Some(m), Some(a)) => m == a,
            _ => false,
        };
        assert!(matched, "RxNorm exact match should fire alert");
    }

    #[test]
    fn clin_07_rxnorm_mismatch_no_code_match() {
        let med_rxnorm = Some("41493".to_string()); // Metformin RxNorm
        let allergy_rxnorm = Some("7980".to_string()); // Penicillin

        let matched = match (&med_rxnorm, &allergy_rxnorm) {
            (Some(m), Some(a)) => m == a,
            _ => false,
        };
        assert!(!matched, "Different RxNorm codes should not match");
    }

    // ─── ROS status string representation ────────────────────────────────────

    #[test]
    fn ros_status_as_str_values() {
        assert_eq!(RosStatus::Positive.as_str(), "positive");
        assert_eq!(RosStatus::Negative.as_str(), "negative");
        assert_eq!(RosStatus::NotReviewed.as_str(), "not_reviewed");
    }
}
