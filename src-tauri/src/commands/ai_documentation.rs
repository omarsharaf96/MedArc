/// commands/ai_documentation.rs — AI documentation samples and exercise library.
///
/// Stores sample clinical notes and exercises used by the AI for note generation.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSample {
    pub id: String,
    pub note_type: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSampleInput {
    pub note_type: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExercise {
    pub id: String,
    pub category: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExerciseInput {
    pub category: String,
    pub name: String,
    pub description: Option<String>,
}

// ── Note Samples ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn save_note_sample(
    input: NoteSampleInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<NoteSample, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let valid_types = ["initial_eval", "progress_note", "daily_treatment"];
    if !valid_types.contains(&input.note_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid note_type '{}'. Must be one of: {}",
            input.note_type,
            valid_types.join(", ")
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Upsert: one sample per note_type (replace if exists)
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM ai_note_samples WHERE note_type = ?1 LIMIT 1",
            rusqlite::params![input.note_type],
            |row| row.get(0),
        )
        .ok();

    let id = existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    conn.execute(
        "INSERT OR REPLACE INTO ai_note_samples (id, note_type, title, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT created_at FROM ai_note_samples WHERE id = ?1), ?5), ?5)",
        rusqlite::params![id, input.note_type, input.title.trim(), input.content, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "ai_documentation.save_sample".to_string(),
            resource_type: "AiNoteSample".to_string(),
            resource_id: Some(id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("note_type={}", input.note_type)),
        },
    )?;

    Ok(NoteSample {
        id,
        note_type: input.note_type,
        title: input.title,
        content: input.content,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub async fn list_note_samples(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<NoteSample>, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, note_type, title, content, created_at, updated_at FROM ai_note_samples ORDER BY note_type",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(NoteSample {
                id: row.get(0)?,
                note_type: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut samples = Vec::new();
    for r in rows {
        samples.push(r.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(samples)
}

#[tauri::command]
pub async fn delete_note_sample(
    sample_id: String,
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

    conn.execute(
        "DELETE FROM ai_note_samples WHERE id = ?1",
        rusqlite::params![sample_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "ai_documentation.delete_sample".to_string(),
            resource_type: "AiNoteSample".to_string(),
            resource_id: Some(sample_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;
    Ok(())
}

// ── Exercise Library ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn add_ai_exercise(
    input: AiExerciseInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AiExercise, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let valid_cats = [
        "therapeutic_exercise",
        "neuromuscular_reeducation",
        "therapeutic_activities",
    ];
    if !valid_cats.contains(&input.category.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid category '{}'. Must be one of: {}",
            input.category,
            valid_cats.join(", ")
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO ai_exercise_library (id, category, name, description) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, input.category, input.name.trim(), input.description],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "ai_documentation.add_exercise".to_string(),
            resource_type: "AiExercise".to_string(),
            resource_id: Some(id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "category={}, name={}",
                input.category,
                input.name.trim()
            )),
        },
    )?;

    Ok(AiExercise {
        id,
        category: input.category,
        name: input.name,
        description: input.description,
        created_at: now,
    })
}

#[tauri::command]
pub async fn list_ai_exercises(
    category: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<AiExercise>, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        if let Some(ref cat) = category {
            (
                "SELECT id, category, name, description, created_at FROM ai_exercise_library WHERE category = ?1 ORDER BY name".to_string(),
                vec![Box::new(cat.clone()) as Box<dyn rusqlite::types::ToSql>],
            )
        } else {
            (
                "SELECT id, category, name, description, created_at FROM ai_exercise_library ORDER BY category, name".to_string(),
                vec![],
            )
        };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(AiExercise {
                id: row.get(0)?,
                category: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut exercises = Vec::new();
    for r in rows {
        exercises.push(r.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(exercises)
}

#[tauri::command]
pub async fn delete_ai_exercise(
    exercise_id: String,
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

    conn.execute(
        "DELETE FROM ai_exercise_library WHERE id = ?1",
        rusqlite::params![exercise_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "ai_documentation.delete_exercise".to_string(),
            resource_type: "AiExercise".to_string(),
            resource_id: Some(exercise_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;
    Ok(())
}
