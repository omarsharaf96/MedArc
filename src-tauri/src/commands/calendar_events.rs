/// commands/calendar_events.rs — Non-patient calendar events (meetings, lunch, blocked time).
///
/// CRUD for time blocks that appear on the schedule without requiring a patient.
/// These are stored in the `calendar_events` table, not as FHIR resources.

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
// Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventInput {
    pub provider_id: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub color: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub event_id: String,
    pub provider_id: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub color: String,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCalendarEventInput {
    pub title: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub color: Option<String>,
    pub notes: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn validate_datetime(value: &str, field_name: &str) -> Result<(), AppError> {
    if chrono::DateTime::parse_from_rfc3339(value).is_ok() {
        return Ok(());
    }
    if chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").is_ok() {
        return Ok(());
    }
    if chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok() {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "{} is not a valid datetime format",
        field_name
    )))
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a non-patient calendar event.
#[tauri::command]
pub async fn create_calendar_event(
    input: CalendarEventInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<CalendarEvent, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Create)?;

    if input.title.trim().is_empty() {
        return Err(AppError::Validation("Title is required.".to_string()));
    }
    if input.start_time.trim().is_empty() || input.end_time.trim().is_empty() {
        return Err(AppError::Validation("Start and end time are required.".to_string()));
    }
    validate_datetime(&input.start_time, "start_time")?;
    validate_datetime(&input.end_time, "end_time")?;

    let event_id = uuid::Uuid::new_v4().to_string();
    let color = input.color.unwrap_or_else(|| "#000000".to_string());

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO calendar_events (event_id, provider_id, title, start_time, end_time, color, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            event_id,
            input.provider_id,
            input.title.trim(),
            input.start_time,
            input.end_time,
            color,
            input.notes,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "calendar_event.create".to_string(),
            resource_type: "CalendarEvent".to_string(),
            resource_id: Some(event_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("title={}", input.title.trim())),
        },
    )?;

    Ok(CalendarEvent {
        event_id,
        provider_id: input.provider_id,
        title: input.title.trim().to_string(),
        start_time: input.start_time,
        end_time: input.end_time,
        color,
        notes: input.notes,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

/// List calendar events for a date range.
#[tauri::command]
pub async fn list_calendar_events(
    start_date: String,
    end_date: String,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<CalendarEvent>, AppError> {
    let _sess = middleware::require_authenticated(&session)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ref pid) = provider_id {
        (
            "SELECT event_id, provider_id, title, start_time, end_time, color, notes, created_at
             FROM calendar_events
             WHERE start_time >= ?1 AND start_time < ?2 AND provider_id = ?3
             ORDER BY start_time".to_string(),
            vec![
                Box::new(start_date.clone()) as Box<dyn rusqlite::types::ToSql>,
                Box::new(end_date.clone()),
                Box::new(pid.clone()),
            ],
        )
    } else {
        (
            "SELECT event_id, provider_id, title, start_time, end_time, color, notes, created_at
             FROM calendar_events
             WHERE start_time >= ?1 AND start_time < ?2
             ORDER BY start_time".to_string(),
            vec![
                Box::new(start_date.clone()) as Box<dyn rusqlite::types::ToSql>,
                Box::new(end_date.clone()),
            ],
        )
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?;

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(CalendarEvent {
            event_id: row.get(0)?,
            provider_id: row.get(1)?,
            title: row.get(2)?,
            start_time: row.get(3)?,
            end_time: row.get(4)?,
            color: row.get(5)?,
            notes: row.get(6)?,
            created_at: row.get(7)?,
        })
    }).map_err(|e| AppError::Database(e.to_string()))?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(events)
}

/// Update a calendar event (title, start/end time, color, notes).
#[tauri::command]
pub async fn update_calendar_event(
    event_id: String,
    input: UpdateCalendarEventInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<CalendarEvent, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Update)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Build dynamic SET clauses for non-null fields
    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    if let Some(ref title) = input.title {
        sets.push(format!("title = ?{idx}"));
        params.push(Box::new(title.trim().to_string()));
        idx += 1;
    }
    if let Some(ref start_time) = input.start_time {
        validate_datetime(start_time, "start_time")?;
        sets.push(format!("start_time = ?{idx}"));
        params.push(Box::new(start_time.clone()));
        idx += 1;
    }
    if let Some(ref end_time) = input.end_time {
        validate_datetime(end_time, "end_time")?;
        sets.push(format!("end_time = ?{idx}"));
        params.push(Box::new(end_time.clone()));
        idx += 1;
    }
    if let Some(ref color) = input.color {
        sets.push(format!("color = ?{idx}"));
        params.push(Box::new(color.clone()));
        idx += 1;
    }
    if let Some(ref notes) = input.notes {
        sets.push(format!("notes = ?{idx}"));
        params.push(Box::new(notes.clone()));
        idx += 1;
    }

    if sets.is_empty() {
        return Err(AppError::Validation("No fields to update.".to_string()));
    }

    let sql = format!(
        "UPDATE calendar_events SET {} WHERE event_id = ?{idx}",
        sets.join(", ")
    );
    params.push(Box::new(event_id.clone()));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let updated = conn
        .execute(&sql, params_refs.as_slice())
        .map_err(|e| AppError::Database(e.to_string()))?;

    if updated == 0 {
        return Err(AppError::NotFound(format!("Calendar event {} not found.", event_id)));
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "calendar_event.update".to_string(),
            resource_type: "CalendarEvent".to_string(),
            resource_id: Some(event_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("fields={}", sets.join(","))),
        },
    )?;

    // Re-read and return the updated event
    let event = conn.query_row(
        "SELECT event_id, provider_id, title, start_time, end_time, color, notes, created_at
         FROM calendar_events WHERE event_id = ?1",
        rusqlite::params![event_id],
        |row| {
            Ok(CalendarEvent {
                event_id: row.get(0)?,
                provider_id: row.get(1)?,
                title: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                color: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    ).map_err(|e| AppError::Database(e.to_string()))?;

    Ok(event)
}

/// Delete a calendar event.
#[tauri::command]
pub async fn delete_calendar_event(
    event_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, Action::Delete)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let deleted = conn
        .execute(
            "DELETE FROM calendar_events WHERE event_id = ?1",
            rusqlite::params![event_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    if deleted == 0 {
        return Err(AppError::NotFound(format!("Calendar event {} not found.", event_id)));
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "calendar_event.delete".to_string(),
            resource_type: "CalendarEvent".to_string(),
            resource_id: Some(event_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_event_input_serializes() {
        let input = CalendarEventInput {
            provider_id: "prov1".into(),
            title: "Lunch".into(),
            start_time: "2026-03-16T12:00:00".into(),
            end_time: "2026-03-16T13:00:00".into(),
            color: None,
            notes: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"title\":\"Lunch\""));
    }
}
