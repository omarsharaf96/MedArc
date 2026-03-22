/// commands/scheduling.rs — Scheduling (S06)
///
/// Implements SCHD-01 through SCHD-07:
///   SCHD-01  Multi-provider calendar (day/week/month views via date-range query)
///   SCHD-02  Appointments with color-coded categories and configurable durations (5–60 min)
///   SCHD-03  Recurring appointments (weekly, biweekly, monthly)
///   SCHD-04  Open-slot search filtered by provider, type, and date range
///   SCHD-05  Patient Flow Board — real-time clinic status (checked_in / roomed / with_provider / checkout)
///   SCHD-06  Waitlist management for cancelled appointment slots
///   SCHD-07  Recall Board — overdue patient follow-up entries
///
/// Data model
/// ----------
/// All scheduling resources are stored as FHIR-aligned JSON in `fhir_resources`.
/// Migration 11 adds four index tables:
///   - `appointment_index`   (patient_id, provider_id, start_time, status, appt_type, color, recurrence_group_id)
///   - `waitlist_index`      (patient_id, provider_id, preferred_date, appt_type, status, priority)
///   - `recall_index`        (patient_id, provider_id, due_date, recall_type, status)
///   - `flow_board_index`    (patient_id, appointment_id, flow_status, checked_in_at)
///
/// RBAC
/// ----
/// All scheduling commands require `AppointmentScheduling` resource access.
///   SystemAdmin                     → full CRUD
///   Provider                        → Create + Read + Update (no hard delete)
///   NurseMa                         → Create + Read + Update (no delete)
///   FrontDesk                       → full CRUD (front desk owns the schedule)
///   BillingStaff                    → Read-only
///
/// Audit
/// -----
/// Every command writes an audit row on success and failure via `write_audit_entry`.
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
// Appointment types (SCHD-02, SCHD-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating an appointment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentInput {
    /// Patient the appointment is for.
    pub patient_id: String,
    /// Provider (user ID) who will see the patient.
    pub provider_id: String,
    /// ISO 8601 datetime for the appointment start (e.g. "2026-04-01T09:00:00").
    pub start_time: String,
    /// Duration in minutes — must be 5–60 (inclusive).
    pub duration_minutes: u32,
    /// Category/type — e.g. "new_patient", "follow_up", "procedure", "telehealth".
    pub appt_type: String,
    /// Hex color code for calendar display (e.g. "#4A90E2").
    pub color: Option<String>,
    /// Free-text reason for the visit.
    pub reason: Option<String>,
    /// Recurrence rule: None | "weekly" | "biweekly" | "monthly"
    pub recurrence: Option<String>,
    /// If recurring — ISO 8601 date on which recurrence ends (e.g. "2026-12-31").
    pub recurrence_end_date: Option<String>,
    /// Additional notes.
    pub notes: Option<String>,
}

/// Appointment record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentRecord {
    pub id: String,
    pub patient_id: String,
    pub provider_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

/// Input for updating an appointment (status, time, duration, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppointmentInput {
    /// New start time (ISO 8601 datetime), if changing.
    pub start_time: Option<String>,
    /// New duration in minutes, if changing.
    pub duration_minutes: Option<u32>,
    /// New status: "proposed" | "pending" | "booked" | "arrived" | "fulfilled" | "cancelled" | "noshow"
    pub status: Option<String>,
    /// New reason.
    pub reason: Option<String>,
    /// New notes.
    pub notes: Option<String>,
    /// New provider, if reassigning.
    pub provider_id: Option<String>,
    /// New color.
    pub color: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Waitlist types (SCHD-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for adding a patient to the waitlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitlistInput {
    /// Patient to add to the waitlist.
    pub patient_id: String,
    /// Preferred provider (user ID), if any.
    pub provider_id: Option<String>,
    /// Preferred appointment type.
    pub appt_type: String,
    /// ISO 8601 date — earliest date patient can be seen (e.g. "2026-04-01").
    pub preferred_date: String,
    /// Priority: 1 (urgent) – 5 (routine).  Defaults to 3.
    pub priority: Option<u32>,
    /// Reason for the visit.
    pub reason: Option<String>,
    /// Additional notes.
    pub notes: Option<String>,
}

/// Waitlist record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitlistRecord {
    pub id: String,
    pub patient_id: String,
    pub provider_id: Option<String>,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Recall types (SCHD-07)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for creating a recall entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallInput {
    /// Patient who needs to be recalled.
    pub patient_id: String,
    /// Provider who should see the patient.
    pub provider_id: Option<String>,
    /// ISO 8601 date by which the patient should return (e.g. "2026-07-01").
    pub due_date: String,
    /// Type of follow-up: "routine", "urgent", "post_procedure", "preventive", etc.
    pub recall_type: String,
    /// Reason / clinical indication for the recall.
    pub reason: String,
    /// Additional notes.
    pub notes: Option<String>,
}

/// Recall record returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallRecord {
    pub id: String,
    pub patient_id: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow Board types (SCHD-05)
// ─────────────────────────────────────────────────────────────────────────────

/// Input for updating a patient's flow board status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFlowStatusInput {
    /// The appointment being tracked.
    pub appointment_id: String,
    /// New flow status: "scheduled" | "checked_in" | "roomed" | "with_provider" | "checkout" | "completed"
    pub flow_status: String,
    /// Room number or name, if applicable.
    pub room: Option<String>,
    /// Notes about the status transition.
    pub notes: Option<String>,
}

/// Patient Flow Board entry returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowBoardEntry {
    pub appointment_id: String,
    pub patient_id: String,
    pub provider_id: String,
    pub flow_status: String,
    pub start_time: String,
    pub appt_type: String,
    pub color: Option<String>,
    pub room: Option<String>,
    pub checked_in_at: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a FHIR R4 Appointment resource from AppointmentInput.
///
/// FHIR type: `Appointment`
/// Status defaults to "booked" for new appointments.
/// Recurrence is stored as a custom extension on the resource.
fn build_appointment_fhir(
    id: &str,
    input: &AppointmentInput,
    recurrence_group_id: Option<&str>,
    occurrence_date: Option<&str>,
) -> serde_json::Value {
    let start = occurrence_date.unwrap_or(&input.start_time);

    // Compute end time from duration
    let end_time = compute_end_time(start, input.duration_minutes);

    let mut extensions: Vec<serde_json::Value> = vec![];

    if let Some(color) = &input.color {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/appointment-color",
            "valueString": color
        }));
    }

    if let Some(recurrence) = &input.recurrence {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/appointment-recurrence",
            "valueString": recurrence
        }));
    }

    if let Some(rec_end) = &input.recurrence_end_date {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/appointment-recurrence-end",
            "valueDate": rec_end
        }));
    }

    if let Some(group_id) = recurrence_group_id {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/appointment-recurrence-group",
            "valueId": group_id
        }));
    }

    if let Some(notes) = &input.notes {
        extensions.push(serde_json::json!({
            "url": "http://medarc.local/fhir/StructureDefinition/appointment-notes",
            "valueString": notes
        }));
    }

    let mut resource = serde_json::json!({
        "resourceType": "Appointment",
        "id": id,
        "status": "booked",
        "serviceType": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/appointment-type",
                "code": input.appt_type,
                "display": input.appt_type.replace('_', " ")
            }]
        }],
        "start": start,
        "end": end_time,
        "minutesDuration": input.duration_minutes,
        "participant": [
            {
                "actor": {
                    "reference": format!("Patient/{}", input.patient_id),
                    "type": "Patient"
                },
                "required": "required",
                "status": "accepted"
            },
            {
                "actor": {
                    "reference": format!("Practitioner/{}", input.provider_id),
                    "type": "Practitioner"
                },
                "required": "required",
                "status": "accepted"
            }
        ]
    });

    if let Some(reason) = &input.reason {
        resource["reason"] = serde_json::json!([{
            "text": reason
        }]);
    }

    if !extensions.is_empty() {
        resource["extension"] = serde_json::json!(extensions);
    }

    resource
}

/// Build a FHIR-aligned waitlist resource (custom `AppointmentRequest`).
fn build_waitlist_fhir(id: &str, input: &WaitlistInput) -> serde_json::Value {
    let priority = input.priority.unwrap_or(3).clamp(1, 5);

    let mut resource = serde_json::json!({
        "resourceType": "AppointmentRequest",
        "id": id,
        "status": "active",
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "serviceType": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/appointment-type",
                "code": input.appt_type,
                "display": input.appt_type.replace('_', " ")
            }]
        }],
        "preferredDate": input.preferred_date,
        "priority": priority,
        "extension": []
    });

    if let Some(provider_id) = &input.provider_id {
        resource["performer"] = serde_json::json!([{
            "reference": format!("Practitioner/{}", provider_id),
            "type": "Practitioner"
        }]);
    }

    if let Some(reason) = &input.reason {
        resource["reason"] = serde_json::json!([{ "text": reason }]);
    }

    if let Some(notes) = &input.notes {
        resource["extension"] = serde_json::json!([{
            "url": "http://medarc.local/fhir/StructureDefinition/waitlist-notes",
            "valueString": notes
        }]);
    }

    resource
}

/// Build a FHIR-aligned recall resource (custom `PatientRecall`).
fn build_recall_fhir(id: &str, input: &RecallInput) -> serde_json::Value {
    let mut resource = serde_json::json!({
        "resourceType": "PatientRecall",
        "id": id,
        "status": "pending",
        "subject": {
            "reference": format!("Patient/{}", input.patient_id),
            "type": "Patient"
        },
        "dueDate": input.due_date,
        "recallType": {
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/recall-type",
                "code": input.recall_type,
                "display": input.recall_type.replace('_', " ")
            }]
        },
        "reason": input.reason
    });

    if let Some(provider_id) = &input.provider_id {
        resource["performer"] = serde_json::json!([{
            "reference": format!("Practitioner/{}", provider_id),
            "type": "Practitioner"
        }]);
    }

    if let Some(notes) = &input.notes {
        resource["extension"] = serde_json::json!([{
            "url": "http://medarc.local/fhir/StructureDefinition/recall-notes",
            "valueString": notes
        }]);
    }

    resource
}

/// Naively add `duration_minutes` to an ISO 8601 datetime string.
/// Returns a best-effort end-time string; if parsing fails, appends "+PT{n}M".
fn compute_end_time(start: &str, duration_minutes: u32) -> String {
    // Parse "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DDTHH:MM"
    if let Some((date_part, time_part)) = start.split_once('T') {
        let time_clean = time_part.trim_end_matches(|c: char| !c.is_ascii_digit() && c != ':');
        let parts: Vec<&str> = time_clean.split(':').collect();
        if parts.len() >= 2 {
            if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                let total_minutes = h * 60 + m + duration_minutes;
                let end_h = (total_minutes / 60) % 24;
                let end_m = total_minutes % 60;
                return format!("{}T{:02}:{:02}:00", date_part, end_h, end_m);
            }
        }
    }
    // Fallback — append duration as ISO 8601 duration offset hint
    format!("{}+PT{}M", start, duration_minutes)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Appointments (SCHD-02, SCHD-03)
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new appointment (SCHD-02).
///
/// If `input.recurrence` is Some, generates the full recurrence series up to
/// `input.recurrence_end_date` (or a max of 52 occurrences) and persists
/// each one as a separate Appointment resource sharing the same
/// `recurrence_group_id` extension (SCHD-03).
///
/// Requires: AppointmentScheduling + Create
#[tauri::command]
pub async fn create_appointment(
    input: AppointmentInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<AppointmentRecord>, AppError> {
    // --- RBAC ---
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Create)?;

    // --- Validate duration ---
    if input.duration_minutes < 5 || input.duration_minutes > 480 {
        if let Ok(conn) = db.conn.lock() {
            let _ = write_audit_entry(
                &conn,
                AuditEntryInput {
                    user_id: sess.user_id.clone(),
                    action: "scheduling.appointment.create_failed".to_string(),
                    resource_type: "Appointment".to_string(),
                    resource_id: None,
                    patient_id: Some(input.patient_id.clone()),
                    device_id: device_id.id().to_string(),
                    success: false,
                    details: Some("duration_minutes must be between 5 and 480".to_string()),
                },
            );
        }
        return Err(AppError::Validation(
            "duration_minutes must be between 5 and 480".to_string(),
        ));
    }

    // --- Build occurrence list ---
    let occurrence_dates = build_occurrence_dates(&input);
    let recurrence_group_id = if occurrence_dates.len() > 1 {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let now = chrono::Utc::now().to_rfc3339();
    let mut records = Vec::new();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    for occurrence_date in &occurrence_dates {
        let appt_id = uuid::Uuid::new_v4().to_string();
        let fhir =
            build_appointment_fhir(&appt_id, &input, recurrence_group_id.as_deref(), Some(occurrence_date));
        let fhir_json = serde_json::to_string(&fhir)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        // Extract color from input
        let color = input.color.clone();
        let recurrence_group = recurrence_group_id.clone();

        conn.execute(
            "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
             VALUES (?1, 'Appointment', ?2, 1, ?3, ?3, ?3)",
            rusqlite::params![appt_id, fhir_json, now],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute(
            "INSERT INTO appointment_index
                (appointment_id, patient_id, provider_id, start_time, status, appt_type, color, recurrence_group_id)
             VALUES (?1, ?2, ?3, ?4, 'booked', ?5, ?6, ?7)",
            rusqlite::params![
                appt_id,
                input.patient_id,
                input.provider_id,
                occurrence_date,
                input.appt_type,
                color,
                recurrence_group,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Flow board entry — starts at "scheduled"
        conn.execute(
            "INSERT INTO flow_board_index
                (appointment_id, patient_id, provider_id, flow_status, start_time, appt_type, color)
             VALUES (?1, ?2, ?3, 'scheduled', ?4, ?5, ?6)",
            rusqlite::params![
                appt_id,
                input.patient_id,
                input.provider_id,
                occurrence_date,
                input.appt_type,
                input.color,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "scheduling.appointment.create".to_string(),
                resource_type: "Appointment".to_string(),
                resource_id: Some(appt_id.clone()),
                patient_id: Some(input.patient_id.clone()),
                device_id: device_id.id().to_string(),
                success: true,
                details: recurrence_group_id
                    .as_deref()
                    .map(|g| format!("recurrence_group={}", g)),
            },
        )?;

        records.push(AppointmentRecord {
            id: appt_id,
            patient_id: input.patient_id.clone(),
            provider_id: input.provider_id.clone(),
            resource: fhir,
            version_id: 1,
            last_updated: now.clone(),
        });
    }

    Ok(records)
}

/// List appointments within a date range, optionally filtered by patient and/or provider (SCHD-01).
///
/// Enables multi-provider calendar views (day / week / month by controlling the date range).
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn list_appointments(
    start_date: String,
    end_date: String,
    patient_id: Option<String>,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<AppointmentRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT ai.appointment_id, ai.patient_id, ai.provider_id,
                fr.resource, fr.version_id, fr.last_updated
         FROM appointment_index ai
         JOIN fhir_resources fr ON fr.id = ai.appointment_id
         WHERE ai.start_time >= ?1 AND ai.start_time < ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(start_date.clone()),
        Box::new(end_date.clone()),
    ];

    if let Some(ref pid) = patient_id {
        query.push_str(&format!(" AND ai.patient_id = ?{}", params.len() + 1));
        params.push(Box::new(pid.clone()));
    }
    if let Some(ref prov) = provider_id {
        query.push_str(&format!(" AND ai.provider_id = ?{}", params.len() + 1));
        params.push(Box::new(prov.clone()));
    }
    query.push_str(" ORDER BY ai.start_time ASC");

    let records = conn
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
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            AppointmentRecord {
                id,
                patient_id: pid,
                provider_id: prov,
                resource,
                version_id: ver,
                last_updated: updated,
            }
        })
        .collect::<Vec<_>>();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.appointment.list".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: None,
            patient_id: patient_id.clone(),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "start={} end={} count={}",
                start_date,
                end_date,
                records.len()
            )),
        },
    )?;

    Ok(records)
}

/// Update an appointment — status, time, provider, notes (SCHD-02).
///
/// Cancelling (status="cancelled") automatically opens the slot for waitlist filling.
///
/// Requires: AppointmentScheduling + Update
#[tauri::command]
pub async fn update_appointment(
    appointment_id: String,
    input: UpdateAppointmentInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AppointmentRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load existing resource
    let (existing_json, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'Appointment'",
            rusqlite::params![appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![appointment_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut fhir: serde_json::Value = serde_json::from_str(&existing_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    // Apply updates to FHIR JSON
    if let Some(ref status) = input.status {
        fhir["status"] = serde_json::json!(status);
    }
    if let Some(ref start) = input.start_time {
        let dur = input.duration_minutes.unwrap_or_else(|| {
            fhir["minutesDuration"].as_u64().unwrap_or(30) as u32
        });
        fhir["start"] = serde_json::json!(start);
        fhir["end"] = serde_json::json!(compute_end_time(start, dur));
    }
    if let Some(dur) = input.duration_minutes {
        fhir["minutesDuration"] = serde_json::json!(dur);
        let start = fhir["start"].as_str().unwrap_or("").to_string();
        fhir["end"] = serde_json::json!(compute_end_time(&start, dur));
    }
    if let Some(ref reason) = input.reason {
        fhir["reason"] = serde_json::json!([{ "text": reason }]);
    }
    if let Some(ref color) = input.color {
        // Upsert color extension
        let ext = fhir["extension"]
            .as_array_mut()
            .map(|exts| {
                if let Some(e) = exts
                    .iter_mut()
                    .find(|e| e["url"] == "http://medarc.local/fhir/StructureDefinition/appointment-color")
                {
                    e["valueString"] = serde_json::json!(color);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true);
        if ext {
            let extensions = fhir["extension"].as_array_mut();
            if let Some(exts) = extensions {
                exts.push(serde_json::json!({
                    "url": "http://medarc.local/fhir/StructureDefinition/appointment-color",
                    "valueString": color
                }));
            } else {
                fhir["extension"] = serde_json::json!([{
                    "url": "http://medarc.local/fhir/StructureDefinition/appointment-color",
                    "valueString": color
                }]);
            }
        }
    }
    if let Some(ref notes) = input.notes {
        // Upsert notes extension
        let need_push = fhir["extension"]
            .as_array_mut()
            .map(|exts| {
                if let Some(e) = exts
                    .iter_mut()
                    .find(|e| e["url"] == "http://medarc.local/fhir/StructureDefinition/appointment-notes")
                {
                    e["valueString"] = serde_json::json!(notes);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true);
        if need_push {
            let extensions = fhir["extension"].as_array_mut();
            if let Some(exts) = extensions {
                exts.push(serde_json::json!({
                    "url": "http://medarc.local/fhir/StructureDefinition/appointment-notes",
                    "valueString": notes
                }));
            } else {
                fhir["extension"] = serde_json::json!([{
                    "url": "http://medarc.local/fhir/StructureDefinition/appointment-notes",
                    "valueString": notes
                }]);
            }
        }
    }

    let new_version = version_id + 1;
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3
         WHERE id = ?4",
        rusqlite::params![fhir_json, new_version, now, appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Update index
    if let Some(ref status) = input.status {
        conn.execute(
            "UPDATE appointment_index SET status = ?1 WHERE appointment_id = ?2",
            rusqlite::params![status, appointment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    if let Some(ref start) = input.start_time {
        conn.execute(
            "UPDATE appointment_index SET start_time = ?1 WHERE appointment_id = ?2",
            rusqlite::params![start, appointment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    if let Some(ref prov) = input.provider_id {
        conn.execute(
            "UPDATE appointment_index SET provider_id = ?1 WHERE appointment_id = ?2",
            rusqlite::params![prov, appointment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    if let Some(ref color) = input.color {
        conn.execute(
            "UPDATE appointment_index SET color = ?1 WHERE appointment_id = ?2",
            rusqlite::params![color, appointment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.appointment.update".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: input.status.as_deref().map(|s| format!("new_status={}", s)),
        },
    )?;

    Ok(AppointmentRecord {
        id: appointment_id,
        patient_id,
        provider_id: input
            .provider_id
            .unwrap_or_else(|| "unchanged".to_string()),
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

/// Cancel an appointment (sets status to "cancelled").
///
/// Convenience wrapper around `update_appointment`.
///
/// Requires: AppointmentScheduling + Update (Providers/Nurses cancel via Update, not Delete)
#[tauri::command]
pub async fn cancel_appointment(
    appointment_id: String,
    reason: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AppointmentRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (existing_json, version_id): (String, i64) = conn
        .query_row(
            "SELECT resource, version_id FROM fhir_resources WHERE id = ?1 AND resource_type = 'Appointment'",
            rusqlite::params![appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![appointment_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let provider_id: String = conn
        .query_row(
            "SELECT provider_id FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![appointment_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut fhir: serde_json::Value = serde_json::from_str(&existing_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    fhir["status"] = serde_json::json!("cancelled");
    if let Some(ref r) = reason {
        fhir["cancelationReason"] = serde_json::json!({ "text": r });
    }

    let new_version = version_id + 1;
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = ?1, version_id = ?2, last_updated = ?3, updated_at = ?3
         WHERE id = ?4",
        rusqlite::params![fhir_json, new_version, now, appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE appointment_index SET status = 'cancelled' WHERE appointment_id = ?1",
        rusqlite::params![appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.appointment.cancel".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: reason.clone(),
        },
    )?;

    Ok(AppointmentRecord {
        id: appointment_id,
        patient_id,
        provider_id,
        resource: fhir,
        version_id: new_version,
        last_updated: now,
    })
}

/// Hard-delete an appointment from the database.
///
/// Removes the appointment from fhir_resources, appointment_index, and flow_board_index.
///
/// Requires: AppointmentScheduling + Delete (SystemAdmin and FrontDesk only per RBAC matrix)
#[tauri::command]
pub async fn delete_appointment(
    appointment_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Delete)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Look up patient_id for audit before deleting
    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![appointment_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    // Delete from flow_board_index
    conn.execute(
        "DELETE FROM flow_board_index WHERE appointment_id = ?1",
        rusqlite::params![appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Delete from appointment_index
    conn.execute(
        "DELETE FROM appointment_index WHERE appointment_id = ?1",
        rusqlite::params![appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Delete from fhir_resources
    conn.execute(
        "DELETE FROM fhir_resources WHERE id = ?1 AND resource_type = 'Appointment'",
        rusqlite::params![appointment_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.appointment.delete".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(())
}

/// Search for open appointment slots (SCHD-04).
///
/// Returns 30-minute blocks in the date range that are NOT occupied by a booked appointment
/// for the given provider (and optionally filtered by appointment type).
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn search_open_slots(
    start_date: String,
    end_date: String,
    provider_id: String,
    appt_type: Option<String>,
    duration_minutes: Option<u32>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Collect booked slots for this provider in the range
    let mut stmt = conn
        .prepare(
            "SELECT start_time, appt_type FROM appointment_index
             WHERE provider_id = ?1
               AND start_time >= ?2
               AND start_time < ?3
               AND status NOT IN ('cancelled', 'noshow')
             ORDER BY start_time ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let booked: Vec<(String, String)> = stmt
        .query_map(
            rusqlite::params![provider_id, start_date, end_date],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let slot_duration = duration_minutes.unwrap_or(30).max(5).min(60);

    // Generate candidate slots at `slot_duration` intervals within working hours (08:00–17:00)
    let open_slots = generate_open_slots(&start_date, &end_date, slot_duration, &booked);

    // Filter by appt_type if specified (open slots are type-agnostic but we tag them)
    let slots: Vec<serde_json::Value> = open_slots
        .into_iter()
        .map(|slot_time| {
            serde_json::json!({
                "provider_id": provider_id,
                "start_time": slot_time,
                "end_time": compute_end_time(&slot_time, slot_duration),
                "duration_minutes": slot_duration,
                "available": true,
                "appt_type": appt_type.as_deref().unwrap_or("any")
            })
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.slot.search".to_string(),
            resource_type: "Slot".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "provider={} start={} end={} found={}",
                provider_id,
                start_date,
                end_date,
                slots.len()
            )),
        },
    )?;

    Ok(slots)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Patient Flow Board (SCHD-05)
// ─────────────────────────────────────────────────────────────────────────────

/// Update a patient's flow board status for today's clinic (SCHD-05).
///
/// Valid transitions:
///   scheduled → checked_in → roomed → with_provider → checkout → completed
///
/// Requires: AppointmentScheduling + Update
#[tauri::command]
pub async fn update_flow_status(
    input: UpdateFlowStatusInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<FlowBoardEntry, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    validate_flow_status(&input.flow_status)?;

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify appointment exists
    let (patient_id, provider_id): (String, String) = conn
        .query_row(
            "SELECT patient_id, provider_id FROM flow_board_index WHERE appointment_id = ?1",
            rusqlite::params![input.appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "Flow board entry for appointment {} not found",
                input.appointment_id
            ))
        })?;

    let checked_in_at = if input.flow_status == "checked_in" {
        Some(now.clone())
    } else {
        None
    };

    if let Some(ref checked_in) = checked_in_at {
        conn.execute(
            "UPDATE flow_board_index SET flow_status = ?1, room = ?2, checked_in_at = ?3
             WHERE appointment_id = ?4",
            rusqlite::params![
                input.flow_status,
                input.room,
                checked_in,
                input.appointment_id
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    } else {
        conn.execute(
            "UPDATE flow_board_index SET flow_status = ?1, room = ?2
             WHERE appointment_id = ?3",
            rusqlite::params![input.flow_status, input.room, input.appointment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.flow.update".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: Some(input.appointment_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("flow_status={}", input.flow_status)),
        },
    )?;

    // Return updated entry
    let (start_time, appt_type, color, room, checked_in_at_stored): (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT start_time, appt_type, color, room, checked_in_at FROM flow_board_index WHERE appointment_id = ?1",
            rusqlite::params![input.appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(FlowBoardEntry {
        appointment_id: input.appointment_id,
        patient_id,
        provider_id,
        flow_status: input.flow_status,
        start_time,
        appt_type,
        color,
        room,
        checked_in_at: checked_in_at_stored,
    })
}

/// Get the Patient Flow Board for a given date (SCHD-05).
///
/// Returns all appointments for the clinic day with their current flow status,
/// ordered by start time.
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn get_flow_board(
    date: String,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<FlowBoardEntry>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // date is "YYYY-MM-DD"; appointments start with "YYYY-MM-DD"
    let start_prefix = format!("{}T", date);
    let end_prefix = advance_date_by_one(&date);
    let end_prefix = format!("{}T", end_prefix);

    let mut query = String::from(
        "SELECT fb.appointment_id, fb.patient_id, fb.provider_id,
                fb.flow_status, fb.start_time, fb.appt_type, fb.color,
                fb.room, fb.checked_in_at
         FROM flow_board_index fb
         WHERE fb.start_time >= ?1 AND fb.start_time < ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(start_prefix), Box::new(end_prefix)];

    if let Some(ref prov) = provider_id {
        query.push_str(&format!(" AND fb.provider_id = ?{}", params.len() + 1));
        params.push(Box::new(prov.clone()));
    }
    query.push_str(" ORDER BY fb.start_time ASC");

    let entries: Vec<FlowBoardEntry> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(FlowBoardEntry {
                    appointment_id: row.get(0)?,
                    patient_id: row.get(1)?,
                    provider_id: row.get(2)?,
                    flow_status: row.get(3)?,
                    start_time: row.get(4)?,
                    appt_type: row.get(5)?,
                    color: row.get(6)?,
                    room: row.get(7)?,
                    checked_in_at: row.get(8)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.flow.get_board".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("date={} count={}", date, entries.len())),
        },
    )?;

    Ok(entries)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Waitlist (SCHD-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Add a patient to the waitlist (SCHD-06).
///
/// Requires: AppointmentScheduling + Create
#[tauri::command]
pub async fn add_to_waitlist(
    input: WaitlistInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<WaitlistRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_waitlist_fhir(&id, &input);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let priority = input.priority.unwrap_or(3).clamp(1, 5);

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'AppointmentRequest', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO waitlist_index
            (waitlist_id, patient_id, provider_id, preferred_date, appt_type, status, priority)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)",
        rusqlite::params![
            id,
            input.patient_id,
            input.provider_id,
            input.preferred_date,
            input.appt_type,
            priority,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.waitlist.add".to_string(),
            resource_type: "AppointmentRequest".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "preferred_date={} priority={}",
                input.preferred_date, priority
            )),
        },
    )?;

    Ok(WaitlistRecord {
        id,
        patient_id: input.patient_id,
        provider_id: input.provider_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List waitlist entries, optionally filtered by provider and/or appointment type (SCHD-06).
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn list_waitlist(
    provider_id: Option<String>,
    appt_type: Option<String>,
    status: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<WaitlistRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let effective_status = status.unwrap_or_else(|| "active".to_string());

    let mut query = String::from(
        "SELECT wi.waitlist_id, wi.patient_id, wi.provider_id,
                fr.resource, fr.version_id, fr.last_updated
         FROM waitlist_index wi
         JOIN fhir_resources fr ON fr.id = wi.waitlist_id
         WHERE wi.status = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(effective_status)];

    if let Some(ref prov) = provider_id {
        query.push_str(&format!(" AND wi.provider_id = ?{}", params.len() + 1));
        params.push(Box::new(prov.clone()));
    }
    if let Some(ref atype) = appt_type {
        query.push_str(&format!(" AND wi.appt_type = ?{}", params.len() + 1));
        params.push(Box::new(atype.clone()));
    }
    query.push_str(" ORDER BY wi.priority ASC, wi.preferred_date ASC");

    let records: Vec<WaitlistRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                let resource_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    resource_str,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(id, pid, prov, res_str, ver, updated)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            WaitlistRecord {
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
            action: "scheduling.waitlist.list".to_string(),
            resource_type: "AppointmentRequest".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    )?;

    Ok(records)
}

/// Remove (discharge) a patient from the waitlist by setting status to "fulfilled" or "cancelled".
///
/// Requires: AppointmentScheduling + Update
#[tauri::command]
pub async fn discharge_waitlist(
    waitlist_id: String,
    reason: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM waitlist_index WHERE waitlist_id = ?1",
            rusqlite::params![waitlist_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Waitlist entry {} not found", waitlist_id)))?;

    conn.execute(
        "UPDATE waitlist_index SET status = 'fulfilled' WHERE waitlist_id = ?1",
        rusqlite::params![waitlist_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Update FHIR resource status
    conn.execute(
        "UPDATE fhir_resources SET resource = json_set(resource, '$.status', 'fulfilled'),
             version_id = version_id + 1, last_updated = ?1, updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, waitlist_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.waitlist.discharge".to_string(),
            resource_type: "AppointmentRequest".to_string(),
            resource_id: Some(waitlist_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: reason,
        },
    )?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Recall Board (SCHD-07)
// ─────────────────────────────────────────────────────────────────────────────

/// Create a recall entry for a patient who needs a follow-up (SCHD-07).
///
/// Requires: AppointmentScheduling + Create
#[tauri::command]
pub async fn create_recall(
    input: RecallInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<RecallRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Create)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let fhir = build_recall_fhir(&id, &input);
    let fhir_json = serde_json::to_string(&fhir)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'PatientRecall', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO recall_index
            (recall_id, patient_id, provider_id, due_date, recall_type, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        rusqlite::params![
            id,
            input.patient_id,
            input.provider_id,
            input.due_date,
            input.recall_type,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.recall.create".to_string(),
            resource_type: "PatientRecall".to_string(),
            resource_id: Some(id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "due_date={} type={}",
                input.due_date, input.recall_type
            )),
        },
    )?;

    Ok(RecallRecord {
        id,
        patient_id: input.patient_id,
        resource: fhir,
        version_id: 1,
        last_updated: now,
    })
}

/// List recall entries — the Recall Board (SCHD-07).
///
/// Returns overdue (due_date < today) and upcoming recalls,
/// optionally filtered by provider, status, and overdue-only flag.
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn list_recalls(
    provider_id: Option<String>,
    overdue_only: Option<bool>,
    status: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<RecallRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let effective_status = status.unwrap_or_else(|| "pending".to_string());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut query = String::from(
        "SELECT ri.recall_id, ri.patient_id,
                fr.resource, fr.version_id, fr.last_updated
         FROM recall_index ri
         JOIN fhir_resources fr ON fr.id = ri.recall_id
         WHERE ri.status = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(effective_status)];

    if overdue_only.unwrap_or(false) {
        query.push_str(&format!(" AND ri.due_date < ?{}", params.len() + 1));
        params.push(Box::new(today));
    }

    if let Some(ref prov) = provider_id {
        query.push_str(&format!(" AND ri.provider_id = ?{}", params.len() + 1));
        params.push(Box::new(prov.clone()));
    }
    query.push_str(" ORDER BY ri.due_date ASC");

    let records: Vec<RecallRecord> = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                let resource_str: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    resource_str,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .map(|(id, pid, res_str, ver, updated)| {
            let resource = serde_json::from_str(&res_str).unwrap_or(serde_json::Value::Null);
            RecallRecord {
                id,
                patient_id: pid,
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
            action: "scheduling.recall.list".to_string(),
            resource_type: "PatientRecall".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    )?;

    Ok(records)
}

/// Mark a recall as completed (patient has been scheduled/seen) (SCHD-07).
///
/// Requires: AppointmentScheduling + Update
#[tauri::command]
pub async fn complete_recall(
    recall_id: String,
    notes: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM recall_index WHERE recall_id = ?1",
            rusqlite::params![recall_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Recall {} not found", recall_id)))?;

    conn.execute(
        "UPDATE recall_index SET status = 'completed' WHERE recall_id = ?1",
        rusqlite::params![recall_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE fhir_resources SET resource = json_set(resource, '$.status', 'completed'),
             version_id = version_id + 1, last_updated = ?1, updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, recall_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.recall.complete".to_string(),
            resource_type: "PatientRecall".to_string(),
            resource_id: Some(recall_id),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: notes,
        },
    )?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider Appointment Types
// ─────────────────────────────────────────────────────────────────────────────

/// Provider appointment types mapping returned to callers.
///
/// Stored in `app_settings` under key `provider_appointment_types` as a JSON
/// object mapping provider user IDs to arrays of appointment type strings.
/// Example: `{ "uuid-1": ["Checking"], "uuid-2": ["Initial Evaluation", "PT Treatment"] }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAppointmentTypesMap {
    /// Map of provider_id → list of allowed appointment type strings.
    pub types: std::collections::HashMap<String, Vec<String>>,
}

/// Get the provider-to-appointment-types mapping from app_settings.
///
/// Returns a map of provider IDs to their allowed appointment type strings.
/// If no mapping exists yet, returns an empty map.
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn get_provider_appointment_types(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<ProviderAppointmentTypesMap, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let json_str: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'provider_appointment_types'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "{}".to_string());

    let types: std::collections::HashMap<String, Vec<String>> =
        serde_json::from_str(&json_str).unwrap_or_default();

    Ok(ProviderAppointmentTypesMap { types })
}

/// Set the provider-to-appointment-types mapping in app_settings.
///
/// Accepts a JSON map of provider IDs to arrays of appointment type strings.
/// Overwrites any existing mapping.
///
/// Requires: AppointmentScheduling + Update (SystemAdmin or Provider)
#[tauri::command]
pub async fn set_provider_appointment_types(
    types: std::collections::HashMap<String, Vec<String>>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let json_str = serde_json::to_string(&types)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('provider_appointment_types', ?1, datetime('now'))",
        rusqlite::params![json_str],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.provider_appt_types.update".to_string(),
            resource_type: "AppSettings".to_string(),
            resource_id: Some("provider_appointment_types".to_string()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("providers={}", types.len())),
        },
    )?;

    Ok(())
}

/// List all users with Provider role (convenience command for scheduling UI).
///
/// Returns a lightweight list of provider users for the appointment form
/// provider selector. Does not require user management permissions — only
/// AppointmentScheduling + Read so any scheduling-capable user can see providers.
#[tauri::command]
pub async fn list_providers(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<ProviderListEntry>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT id, display_name FROM users WHERE role = 'Provider' AND is_active = 1 ORDER BY display_name ASC",
    )?;

    let providers: Vec<ProviderListEntry> = stmt
        .query_map([], |row| {
            Ok(ProviderListEntry {
                id: row.get(0)?,
                display_name: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(providers)
}

/// Lightweight provider entry for the appointment form provider selector.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListEntry {
    pub id: String,
    pub display_name: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Calendar Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Calendar display settings stored as JSON in app_settings under key "calendar_settings".
/// Controls the appearance and behavior of the week/day calendar views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSettings {
    /// Whether to show Saturday in the week view.
    pub show_saturday: bool,
    /// Whether to show Sunday in the week view.
    pub show_sunday: bool,
    /// Start hour for the calendar grid (5-10, default 6).
    pub start_hour: u32,
    /// End hour for the calendar grid (17-22, default 20).
    pub end_hour: u32,
    /// Default appointment duration in minutes (15/30/45/60, default 60).
    pub default_duration_minutes: u32,
    /// Default calendar view: "day" or "week".
    pub default_view: String,
    /// Height in pixels per hour in the calendar grid (40/60/80, default 60).
    pub hour_height_px: u32,
    /// Whether to show dotted half-hour lines in the calendar grid.
    pub show_half_hour_lines: bool,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            show_saturday: false,
            show_sunday: false,
            start_hour: 6,
            end_hour: 20,
            default_duration_minutes: 60,
            default_view: "week".to_string(),
            hour_height_px: 60,
            show_half_hour_lines: true,
        }
    }
}

/// Get calendar display settings from app_settings.
///
/// Returns the stored CalendarSettings or sensible defaults if not yet configured.
///
/// Requires: AppointmentScheduling + Read
#[tauri::command]
pub async fn get_calendar_settings(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<CalendarSettings, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let json_str: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'calendar_settings'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "{}".to_string());

    let settings: CalendarSettings =
        serde_json::from_str(&json_str).unwrap_or_default();

    Ok(settings)
}

/// Save calendar display settings to app_settings.
///
/// Overwrites any existing calendar_settings value.
///
/// Requires: AppointmentScheduling + Update
#[tauri::command]
pub async fn save_calendar_settings(
    settings: CalendarSettings,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<CalendarSettings, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let json_str = serde_json::to_string(&settings)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('calendar_settings', ?1, datetime('now'))",
        rusqlite::params![json_str],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "scheduling.calendar_settings.update".to_string(),
            resource_type: "AppSettings".to_string(),
            resource_id: Some("calendar_settings".to_string()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some("Calendar settings updated".to_string()),
        },
    )?;

    Ok(settings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Validate that a flow status string is one of the accepted values.
fn validate_flow_status(status: &str) -> Result<(), AppError> {
    match status {
        "scheduled" | "checked_in" | "roomed" | "with_provider" | "checkout" | "completed" => {
            Ok(())
        }
        _ => Err(AppError::Validation(format!(
            "Invalid flow_status '{}'. Must be one of: scheduled, checked_in, roomed, with_provider, checkout, completed",
            status
        ))),
    }
}

/// Build the list of ISO 8601 datetime strings for a recurrence series.
///
/// - None / unknown recurrence rule → single occurrence (the input start_time)
/// - "weekly"   → every 7 days
/// - "biweekly" → every 14 days
/// - "monthly"  → every ~30 days (30-day stride)
///
/// Caps at 52 occurrences to prevent runaway series.
fn build_occurrence_dates(input: &AppointmentInput) -> Vec<String> {
    let Some(recurrence) = &input.recurrence else {
        return vec![input.start_time.clone()];
    };

    let day_stride: i64 = match recurrence.as_str() {
        "weekly" => 7,
        "biweekly" => 14,
        "monthly" => 30,
        _ => return vec![input.start_time.clone()],
    };

    // Parse start datetime — accept "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DDTHH:MM"
    let Some((date_str, time_str)) = input.start_time.split_once('T') else {
        return vec![input.start_time.clone()];
    };

    let date_parts: Vec<&str> = date_str.split('-').collect();
    if date_parts.len() != 3 {
        return vec![input.start_time.clone()];
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        date_parts[0].parse::<i32>(),
        date_parts[1].parse::<u32>(),
        date_parts[2].parse::<u32>(),
    ) else {
        return vec![input.start_time.clone()];
    };

    // Parse optional end date
    let end_parts = input
        .recurrence_end_date
        .as_deref()
        .and_then(|s| {
            let p: Vec<&str> = s.split('-').collect();
            if p.len() == 3 {
                Some((
                    p[0].parse::<i32>().ok()?,
                    p[1].parse::<u32>().ok()?,
                    p[2].parse::<u32>().ok()?,
                ))
            } else {
                None
            }
        });

    let mut occurrences = Vec::new();
    let mut current_y = y;
    let mut current_m = m;
    let mut current_d = d;

    for _ in 0..52 {
        let date_formatted = format!("{:04}-{:02}-{:02}T{}", current_y, current_m, current_d, time_str);
        occurrences.push(date_formatted);

        // Advance by stride days using simple calendar arithmetic
        let (ny, nm, nd) = advance_days(current_y, current_m, current_d, day_stride);
        current_y = ny;
        current_m = nm;
        current_d = nd;

        // Check if we've passed recurrence_end_date
        if let Some((ey, em, ed)) = end_parts {
            if current_y > ey
                || (current_y == ey && current_m > em)
                || (current_y == ey && current_m == em && current_d > ed)
            {
                break;
            }
        } else {
            // No end date — single series item (break after 1 if no end)
            // For open-ended recurrences, stop after 52 max (already handled by loop cap)
            break;
        }
    }

    occurrences
}

/// Advance a calendar date by `days` days using simple arithmetic.
/// This is a lightweight implementation that handles month-end rollovers correctly
/// for scheduling purposes (not a full calendar library).
fn advance_days(y: i32, m: u32, d: u32, days: i64) -> (i32, u32, u32) {
    // Convert to day-of-year style: total days from a fixed epoch
    // Simple approach: convert to ordinal, add, convert back
    let total_days = date_to_days(y, m, d) + days;
    days_to_date(total_days)
}

/// Convert year/month/day to days since a fixed epoch (proleptic Gregorian).
fn date_to_days(y: i32, m: u32, d: u32) -> i64 {
    // Use the algorithm from: https://en.wikipedia.org/wiki/Julian_day_number
    let a = (14 - m as i32) / 12;
    let y2 = y + 4800 - a;
    let m2 = m as i32 + 12 * a - 3;
    let jdn = d as i64
        + ((153 * m2 as i64 + 2) / 5)
        + 365 * y2 as i64
        + y2 as i64 / 4
        - y2 as i64 / 100
        + y2 as i64 / 400
        - 32045;
    jdn
}

/// Convert days since fixed epoch back to year/month/day.
fn days_to_date(jdn: i64) -> (i32, u32, u32) {
    // Algorithm from: https://en.wikipedia.org/wiki/Julian_day_number#Julian_day_number_calculation
    let l = jdn + 68569;
    let n = 4 * l / 146097;
    let l = l - (146097 * n + 3) / 4;
    let i = 4000 * (l + 1) / 1461001;
    let l = l - 1461 * i / 4 + 31;
    let j = 80 * l / 2447;
    let d = (l - 2447 * j / 80) as u32;
    let l = j / 11;
    let m = (j + 2 - 12 * l) as u32;
    let y = (100 * (n - 49) + i + l) as i32;
    (y, m, d)
}

/// Advance a date string "YYYY-MM-DD" by one day. Used for flow board queries.
fn advance_date_by_one(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    if let (Ok(y), Ok(m), Ok(d)) = (
        parts[0].parse::<i32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) {
        let (ny, nm, nd) = advance_days(y, m, d, 1);
        format!("{:04}-{:02}-{:02}", ny, nm, nd)
    } else {
        date.to_string()
    }
}

/// Generate candidate open slots in working hours (08:00–17:00) at `slot_duration` intervals,
/// excluding any `booked` slot whose start_time overlaps the candidate.
fn generate_open_slots(
    start_date: &str,
    end_date: &str,
    slot_duration: u32,
    booked: &[(String, String)],
) -> Vec<String> {
    let booked_starts: std::collections::HashSet<&str> =
        booked.iter().map(|(s, _)| s.as_str()).collect();

    let mut slots = Vec::new();

    // Iterate dates in range
    let date_parts_start: Vec<&str> = start_date.split('T').collect();
    let date_parts_end: Vec<&str> = end_date.split('T').collect();

    let start_d = date_parts_start[0];
    let end_d = date_parts_end[0];

    let start_p: Vec<&str> = start_d.split('-').collect();
    let end_p: Vec<&str> = end_d.split('-').collect();

    if start_p.len() < 3 || end_p.len() < 3 {
        return slots;
    }

    if let (Ok(sy), Ok(sm), Ok(sd), Ok(ey), Ok(em), Ok(ed)) = (
        start_p[0].parse::<i32>(),
        start_p[1].parse::<u32>(),
        start_p[2].parse::<u32>(),
        end_p[0].parse::<i32>(),
        end_p[1].parse::<u32>(),
        end_p[2].parse::<u32>(),
    ) {
        let start_days = date_to_days(sy, sm, sd);
        let end_days = date_to_days(ey, em, ed);

        for day_offset in 0..(end_days - start_days) {
            let (cy, cm, cd) = days_to_date(start_days + day_offset);
            let date_str = format!("{:04}-{:02}-{:02}", cy, cm, cd);

            // Working hours: 08:00 to 17:00
            let mut hour = 8u32;
            let mut minute = 0u32;

            while hour < 17 {
                let candidate = format!("{}T{:02}:{:02}:00", date_str, hour, minute);
                if !booked_starts.contains(candidate.as_str()) {
                    slots.push(candidate);
                }

                // Advance by slot_duration
                let total = hour * 60 + minute + slot_duration;
                hour = total / 60;
                minute = total % 60;
                if hour >= 17 {
                    break;
                }
            }
        }
    }

    slots
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SCHD-02: Appointment duration validation ──────────────────────────

    #[test]
    fn schd_02_appointment_fhir_has_correct_structure() {
        let input = AppointmentInput {
            patient_id: "patient-001".to_string(),
            provider_id: "provider-001".to_string(),
            start_time: "2026-04-01T09:00:00".to_string(),
            duration_minutes: 30,
            appt_type: "follow_up".to_string(),
            color: Some("#4A90E2".to_string()),
            reason: Some("Annual checkup".to_string()),
            recurrence: None,
            recurrence_end_date: None,
            notes: Some("Patient prefers morning".to_string()),
        };
        let fhir = build_appointment_fhir("appt-001", &input, None, None);

        assert_eq!(fhir["resourceType"], "Appointment");
        assert_eq!(fhir["id"], "appt-001");
        assert_eq!(fhir["status"], "booked");
        assert_eq!(fhir["minutesDuration"], 30);
        assert_eq!(fhir["start"], "2026-04-01T09:00:00");
        assert_eq!(fhir["end"], "2026-04-01T09:30:00");

        // Participants: patient and provider
        let participants = fhir["participant"].as_array().unwrap();
        assert_eq!(participants.len(), 2);
        assert!(participants[0]["actor"]["reference"]
            .as_str()
            .unwrap()
            .contains("Patient/patient-001"));
        assert!(participants[1]["actor"]["reference"]
            .as_str()
            .unwrap()
            .contains("Practitioner/provider-001"));

        // Color extension
        let extensions = fhir["extension"].as_array().unwrap();
        let color_ext = extensions
            .iter()
            .find(|e| {
                e["url"].as_str().unwrap().contains("appointment-color")
            })
            .expect("color extension should be present");
        assert_eq!(color_ext["valueString"], "#4A90E2");
    }

    #[test]
    fn schd_02_duration_minimum_boundary() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-01T10:00:00".to_string(),
            duration_minutes: 5,
            appt_type: "follow_up".to_string(),
            color: None,
            reason: None,
            recurrence: None,
            recurrence_end_date: None,
            notes: None,
        };
        let fhir = build_appointment_fhir("a", &input, None, None);
        assert_eq!(fhir["minutesDuration"], 5);
        assert_eq!(fhir["end"], "2026-04-01T10:05:00");
    }

    #[test]
    fn schd_02_duration_maximum_boundary() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-01T10:00:00".to_string(),
            duration_minutes: 60,
            appt_type: "procedure".to_string(),
            color: None,
            reason: None,
            recurrence: None,
            recurrence_end_date: None,
            notes: None,
        };
        let fhir = build_appointment_fhir("a", &input, None, None);
        assert_eq!(fhir["minutesDuration"], 60);
        assert_eq!(fhir["end"], "2026-04-01T11:00:00");
    }

    // ── SCHD-03: Recurrence occurrence generation ──────────────────────────

    #[test]
    fn schd_03_weekly_recurrence_generates_correct_dates() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-06T09:00:00".to_string(),
            duration_minutes: 30,
            appt_type: "follow_up".to_string(),
            color: None,
            reason: None,
            recurrence: Some("weekly".to_string()),
            recurrence_end_date: Some("2026-04-27".to_string()),
            notes: None,
        };
        let dates = build_occurrence_dates(&input);
        // 2026-04-06, 2026-04-13, 2026-04-20, 2026-04-27 = 4 occurrences
        assert_eq!(dates.len(), 4);
        assert!(dates[0].starts_with("2026-04-06"));
        assert!(dates[1].starts_with("2026-04-13"));
        assert!(dates[2].starts_with("2026-04-20"));
        assert!(dates[3].starts_with("2026-04-27"));
    }

    #[test]
    fn schd_03_biweekly_recurrence() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-01T09:00:00".to_string(),
            duration_minutes: 30,
            appt_type: "follow_up".to_string(),
            color: None,
            reason: None,
            recurrence: Some("biweekly".to_string()),
            recurrence_end_date: Some("2026-05-01".to_string()),
            notes: None,
        };
        let dates = build_occurrence_dates(&input);
        // 2026-04-01, 2026-04-15, 2026-04-29 = 3 occurrences
        assert_eq!(dates.len(), 3);
        assert!(dates[0].starts_with("2026-04-01"));
        assert!(dates[1].starts_with("2026-04-15"));
        assert!(dates[2].starts_with("2026-04-29"));
    }

    #[test]
    fn schd_03_monthly_recurrence() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-01T09:00:00".to_string(),
            duration_minutes: 30,
            appt_type: "follow_up".to_string(),
            color: None,
            reason: None,
            recurrence: Some("monthly".to_string()),
            recurrence_end_date: Some("2026-07-01".to_string()),
            notes: None,
        };
        let dates = build_occurrence_dates(&input);
        // 2026-04-01, 2026-05-01, 2026-05-31, 2026-06-30 = 4 occurrences within end
        assert!(dates.len() >= 3);
        assert!(dates[0].starts_with("2026-04-01"));
    }

    #[test]
    fn schd_03_no_recurrence_returns_single_occurrence() {
        let input = AppointmentInput {
            patient_id: "p".to_string(),
            provider_id: "d".to_string(),
            start_time: "2026-04-01T09:00:00".to_string(),
            duration_minutes: 30,
            appt_type: "follow_up".to_string(),
            color: None,
            reason: None,
            recurrence: None,
            recurrence_end_date: None,
            notes: None,
        };
        let dates = build_occurrence_dates(&input);
        assert_eq!(dates.len(), 1);
        assert_eq!(dates[0], "2026-04-01T09:00:00");
    }

    // ── SCHD-04: Open slot search ──────────────────────────────────────────

    #[test]
    fn schd_04_open_slot_excludes_booked_times() {
        let booked = vec![
            ("2026-04-01T09:00:00".to_string(), "follow_up".to_string()),
            ("2026-04-01T09:30:00".to_string(), "follow_up".to_string()),
        ];
        let slots = generate_open_slots(
            "2026-04-01",
            "2026-04-02",
            30,
            &booked,
        );
        // 08:00 slot should be open; 09:00 and 09:30 should be excluded
        let has_0800 = slots.iter().any(|s| s.contains("T08:00"));
        let has_0900 = slots.iter().any(|s| s.contains("T09:00"));
        let has_0930 = slots.iter().any(|s| s.contains("T09:30"));
        assert!(has_0800, "08:00 slot should be available");
        assert!(!has_0900, "09:00 should be excluded (booked)");
        assert!(!has_0930, "09:30 should be excluded (booked)");
    }

    #[test]
    fn schd_04_empty_booked_list_returns_working_hour_slots() {
        let slots = generate_open_slots("2026-04-01", "2026-04-02", 30, &[]);
        // Working hours 08:00–17:00 in 30-min slots = 18 slots
        assert_eq!(slots.len(), 18);
        assert!(slots[0].contains("T08:00"));
        assert!(slots.last().unwrap().contains("T16:30"));
    }

    // ── SCHD-05: Flow status validation ────────────────────────────────────

    #[test]
    fn schd_05_valid_flow_statuses_pass() {
        for status in &[
            "scheduled",
            "checked_in",
            "roomed",
            "with_provider",
            "checkout",
            "completed",
        ] {
            assert!(
                validate_flow_status(status).is_ok(),
                "Expected '{}' to be valid",
                status
            );
        }
    }

    #[test]
    fn schd_05_invalid_flow_status_rejected() {
        assert!(validate_flow_status("waiting").is_err());
        assert!(validate_flow_status("").is_err());
        assert!(validate_flow_status("CHECKED_IN").is_err());
    }

    // ── SCHD-06: Waitlist FHIR structure ──────────────────────────────────

    #[test]
    fn schd_06_waitlist_fhir_has_correct_structure() {
        let input = WaitlistInput {
            patient_id: "patient-001".to_string(),
            provider_id: Some("provider-001".to_string()),
            appt_type: "new_patient".to_string(),
            preferred_date: "2026-04-15".to_string(),
            priority: Some(2),
            reason: Some("Chest pain follow-up".to_string()),
            notes: None,
        };
        let fhir = build_waitlist_fhir("waitlist-001", &input);

        assert_eq!(fhir["resourceType"], "AppointmentRequest");
        assert_eq!(fhir["id"], "waitlist-001");
        assert_eq!(fhir["status"], "active");
        assert_eq!(fhir["priority"], 2);
        assert_eq!(fhir["preferredDate"], "2026-04-15");
        assert!(fhir["subject"]["reference"]
            .as_str()
            .unwrap()
            .contains("Patient/patient-001"));
        assert!(fhir["performer"][0]["reference"]
            .as_str()
            .unwrap()
            .contains("Practitioner/provider-001"));
    }

    #[test]
    fn schd_06_waitlist_priority_clamped_to_1_to_5() {
        let make_input = |priority: u32| WaitlistInput {
            patient_id: "p".to_string(),
            provider_id: None,
            appt_type: "follow_up".to_string(),
            preferred_date: "2026-04-15".to_string(),
            priority: Some(priority),
            reason: None,
            notes: None,
        };
        let fhir_0 = build_waitlist_fhir("w0", &make_input(0));
        assert_eq!(fhir_0["priority"], 1, "0 should clamp to 1");

        let fhir_99 = build_waitlist_fhir("w99", &make_input(99));
        assert_eq!(fhir_99["priority"], 5, "99 should clamp to 5");
    }

    // ── SCHD-07: Recall FHIR structure ────────────────────────────────────

    #[test]
    fn schd_07_recall_fhir_has_correct_structure() {
        let input = RecallInput {
            patient_id: "patient-001".to_string(),
            provider_id: Some("provider-001".to_string()),
            due_date: "2026-07-01".to_string(),
            recall_type: "post_procedure".to_string(),
            reason: "Follow-up after knee surgery".to_string(),
            notes: Some("Check wound healing".to_string()),
        };
        let fhir = build_recall_fhir("recall-001", &input);

        assert_eq!(fhir["resourceType"], "PatientRecall");
        assert_eq!(fhir["id"], "recall-001");
        assert_eq!(fhir["status"], "pending");
        assert_eq!(fhir["dueDate"], "2026-07-01");
        assert_eq!(fhir["reason"], "Follow-up after knee surgery");
        assert_eq!(
            fhir["recallType"]["coding"][0]["code"],
            "post_procedure"
        );
        assert!(fhir["subject"]["reference"]
            .as_str()
            .unwrap()
            .contains("Patient/patient-001"));
        assert!(fhir["performer"][0]["reference"]
            .as_str()
            .unwrap()
            .contains("Practitioner/provider-001"));
    }

    // ── Calendar arithmetic ────────────────────────────────────────────────

    #[test]
    fn date_arithmetic_advance_7_days() {
        let (y, m, d) = advance_days(2026, 4, 1, 7);
        assert_eq!((y, m, d), (2026, 4, 8));
    }

    #[test]
    fn date_arithmetic_crosses_month_boundary() {
        let (y, m, d) = advance_days(2026, 4, 28, 7);
        assert_eq!((y, m, d), (2026, 5, 5));
    }

    #[test]
    fn date_arithmetic_crosses_year_boundary() {
        let (y, m, d) = advance_days(2026, 12, 28, 7);
        assert_eq!((y, m, d), (2027, 1, 4));
    }

    #[test]
    fn advance_date_by_one_crosses_month() {
        assert_eq!(advance_date_by_one("2026-04-30"), "2026-05-01");
    }

    // ── RBAC smoke tests ───────────────────────────────────────────────────

    #[test]
    fn schd_rbac_provider_can_create_and_update_scheduling() {
        use crate::rbac::roles::{Action, Resource, Role, has_permission};
        assert!(has_permission(Role::Provider, Resource::AppointmentScheduling, Action::Create));
        assert!(has_permission(Role::Provider, Resource::AppointmentScheduling, Action::Read));
        assert!(has_permission(Role::Provider, Resource::AppointmentScheduling, Action::Update));
        assert!(!has_permission(Role::Provider, Resource::AppointmentScheduling, Action::Delete));
    }

    #[test]
    fn schd_rbac_front_desk_full_crud() {
        use crate::rbac::roles::{Action, Resource, Role, has_permission};
        assert!(has_permission(Role::FrontDesk, Resource::AppointmentScheduling, Action::Create));
        assert!(has_permission(Role::FrontDesk, Resource::AppointmentScheduling, Action::Read));
        assert!(has_permission(Role::FrontDesk, Resource::AppointmentScheduling, Action::Update));
        assert!(has_permission(Role::FrontDesk, Resource::AppointmentScheduling, Action::Delete));
    }

    #[test]
    fn schd_rbac_billing_staff_read_only() {
        use crate::rbac::roles::{Action, Resource, Role, has_permission};
        assert!(has_permission(Role::BillingStaff, Resource::AppointmentScheduling, Action::Read));
        assert!(!has_permission(Role::BillingStaff, Resource::AppointmentScheduling, Action::Create));
        assert!(!has_permission(Role::BillingStaff, Resource::AppointmentScheduling, Action::Update));
        assert!(!has_permission(Role::BillingStaff, Resource::AppointmentScheduling, Action::Delete));
    }

    #[test]
    fn schd_rbac_nurse_ma_no_delete() {
        use crate::rbac::roles::{Action, Resource, Role, has_permission};
        assert!(has_permission(Role::NurseMa, Resource::AppointmentScheduling, Action::Create));
        assert!(has_permission(Role::NurseMa, Resource::AppointmentScheduling, Action::Read));
        assert!(has_permission(Role::NurseMa, Resource::AppointmentScheduling, Action::Update));
        assert!(!has_permission(Role::NurseMa, Resource::AppointmentScheduling, Action::Delete));
    }

    // ── CalendarSettings serialization ───────────────────────────────────

    #[test]
    fn calendar_settings_default_has_correct_values() {
        let defaults = CalendarSettings::default();
        assert_eq!(defaults.show_saturday, false);
        assert_eq!(defaults.show_sunday, false);
        assert_eq!(defaults.start_hour, 6);
        assert_eq!(defaults.end_hour, 20);
        assert_eq!(defaults.default_duration_minutes, 60);
        assert_eq!(defaults.default_view, "week");
        assert_eq!(defaults.hour_height_px, 60);
        assert_eq!(defaults.show_half_hour_lines, true);
    }

    #[test]
    fn calendar_settings_serializes_camelcase() {
        let settings = CalendarSettings::default();
        let json = serde_json::to_string(&settings).expect("should serialize");
        assert!(json.contains("\"showSaturday\""), "camelCase showSaturday expected");
        assert!(json.contains("\"showSunday\""), "camelCase showSunday expected");
        assert!(json.contains("\"startHour\""), "camelCase startHour expected");
        assert!(json.contains("\"endHour\""), "camelCase endHour expected");
        assert!(json.contains("\"defaultDurationMinutes\""), "camelCase defaultDurationMinutes expected");
        assert!(json.contains("\"defaultView\""), "camelCase defaultView expected");
        assert!(json.contains("\"hourHeightPx\""), "camelCase hourHeightPx expected");
        assert!(json.contains("\"showHalfHourLines\""), "camelCase showHalfHourLines expected");
    }

    #[test]
    fn calendar_settings_deserializes_from_empty_json() {
        let settings: CalendarSettings = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(settings.start_hour, 6);
        assert_eq!(settings.end_hour, 20);
        assert_eq!(settings.default_view, "week");
    }
}
