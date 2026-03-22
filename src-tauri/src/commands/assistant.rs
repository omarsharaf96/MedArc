/// commands/assistant.rs — AI Assistant for PanaceaEMR (Phase 1: Time-Saving Assistant)
///
/// Provides conversational AI commands for daily clinic tasks:
///   1. `send_assistant_message`   — send a message, get AI response with optional actions
///   2. `list_conversations`       — list conversation history
///   3. `get_conversation`         — get messages in a conversation
///   4. `delete_conversation`      — delete a conversation
///   5. `execute_assistant_action` — execute a parsed action (scheduling, patient lookup, etc.)
///
/// RBAC: All commands require authentication. Action execution checks per-action permissions.
/// Audit: Every assistant interaction is audit-logged. Message content is NOT logged (ePHI).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action as RbacAction, Resource};

use super::llm_integration::{call_llm_chat, ChatMessage};

// ─────────────────────────────────────────────────────────────────────────────
// System Prompt
// ─────────────────────────────────────────────────────────────────────────────

fn build_system_prompt(conn: &rusqlite::Connection) -> String {
    let now = chrono::Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let day = now.format("%A").to_string();

    // Load sample notes from ai_note_samples for style reference
    let sample_notes_section = load_sample_notes_for_prompt(conn);

    format!(
        r#"You are PanaceaEMR Assistant, an AI helper for a Physical Therapy clinic management system.
You help with scheduling, patient lookup, clinical data queries, and daily task management.

Today's date is {date}. The current day of the week is {day}.
Clinic hours are 8:00 AM to 6:00 PM, Monday through Friday.

When the user asks you to perform an action, include a JSON action block in your response.
Wrap the action block in triple backticks with the label "actions":

```actions
[{{"action": "action_name", "param1": "value1"}}]
```

Available actions:

1. schedule_appointment — Schedule a new appointment
   Parameters:
   - patientName (string, required) — patient's full name
   - appointmentType (string) — "pt_treatment", "initial_eval", "follow_up", "new_patient" (default: "pt_treatment")
   - startTime (string, required) — ISO 8601 datetime, e.g. "2026-03-16T10:00:00"
   - durationMinutes (number) — 15, 30, 45, or 60 (default: 60)
   - recurrence (string or null) — "weekly", "biweekly", "monthly", or null
   - recurrenceEndDate (string or null) — ISO 8601 date for when recurrence ends
   - reason (string or null) — reason for visit

2. list_appointments — List appointments for a date
   Parameters:
   - date (string, required) — ISO 8601 date, e.g. "2026-03-16"
   - patientName (string or null) — filter by patient name

3. cancel_appointment — Cancel appointments (requires ID from a previous list)
   Parameters:
   - appointmentId (string, required) — the appointment ID
   - reason (string or null) — cancellation reason

4. search_patients — Search for patients by name
   Parameters:
   - query (string, required) — patient name to search

5. find_inactive_patients — Find patients not seen recently
   Parameters:
   - daysSinceLastVisit (number, required) — minimum days since last visit

6. export_note_pdf — Export a patient's most recent encounter note as PDF
   Parameters:
   - patientName (string, required) — patient's full name

7. export_progress_report — Export a progress report PDF for a patient
   Parameters:
   - patientName (string, required) — patient's full name

8. export_chart — Export a patient's full chart as PDF
   Parameters:
   - patientName (string, required) — patient's full name

9. create_note — Create a clinical note for a patient
   Parameters:
   - patientName (string, required) — patient's full name
   - noteType (string, required) — "initial_eval", "progress_note", "treatment_note", "discharge_note", or "fce"
   - templateId (string, optional) — template to use: tpl_pt_initial_eval, tpl_pt_progress_note, tpl_pt_treatment_note, tpl_pt_discharge_note, tpl_pt_fce, tpl_general, tpl_follow_up, etc.
   - noteContent (string, required) — the clinical note in SOAP format.

   CRITICAL FORMATTING RULES for noteContent:
   - Start DIRECTLY with "SUBJECTIVE:" — no preamble, no introduction
   - Use exactly these section headers: SUBJECTIVE:, OBJECTIVE:, ASSESSMENT:, PLAN:
   - Each section header must be on its own line
   - Write professional, detailed clinical content under each section
   - End with the Plan content — no postamble, no "let me know" text
   - Reference the patient's prior notes and clinical data when available
   - Match the style of any sample notes configured in the system

10. get_patient_summary — Get a patient's clinical summary (demographics, diagnoses, allergies, medications, recent encounters)
    Parameters:
    - patientName (string, required) — patient's full name

11. get_patient_notes — Get a patient's encounter notes
    Parameters:
    - patientName (string, required) — patient's full name
    - limit (number, optional, default 5) — max number of notes to return

12. search_documents — Search a patient's uploaded documents (returns metadata only)
    Parameters:
    - patientName (string, required) — patient's full name
    - category (string, optional) — one of: "referral-rx", "imaging", "consent-forms", "intake-surveys", "insurance", "legal", "hep", "other"

13. get_patient_clinical_data — Get specific clinical data for a patient
    Parameters:
    - patientName (string, required) — patient's full name
    - dataType (string, required) — one of: "conditions", "allergies", "medications", "labs", "vitals"

14. read_document — Read the contents of a patient's uploaded document
    Parameters:
    - documentId (string) — the document ID from a previous search_documents result
    - patientName (string) — if you don't have a documentId, provide the patient's name to retrieve their most recent document
    - category (string, optional) — filter by category when using patientName: "referral-rx", "imaging", "consent-forms", "intake-surveys", "insurance", "legal", "hep", "other"
    You MUST provide either documentId or patientName.
    Returns the text content of the document. Works with PDFs (extracts text) and text files. For images, returns a description of the file metadata.

Rules:
- Always describe what you will do BEFORE the action block so the user can review.
- For recurring appointments, calculate the recurrence_end_date from the user's request.
- If you need more information (e.g., which patient, what time), ASK before generating actions.
- Use 24-hour time in ISO format for startTime.
- Be concise, professional, and helpful.
- When listing information (appointments, patients), just describe the query — the system will execute it.
- Do NOT make up patient IDs. If a cancel is requested, ask the user to first list appointments so you can get the ID.
- For export actions, look up the patient first and use the most recent encounter.
- When asked to create a note for a patient, FIRST use get_patient_summary or get_patient_notes to understand the patient's context, THEN create the note using that context.
- For create_note, you MUST write the complete clinical note in SOAP format and include the FULL text in the noteContent parameter. The noteContent is what gets saved directly into the patient's medical record. Write it as a complete, professional clinical note — not a summary or outline.
- Output ONLY the clinical note text in SOAP format for noteContent. Do NOT include any preamble like "Here is the note" or postamble like "Let me know if you need changes". Start directly with "SUBJECTIVE:" and end with the plan content.{sample_notes_section}"#
    )
}

/// Load sample notes from `ai_note_samples` and format them for inclusion in the system prompt.
fn load_sample_notes_for_prompt(conn: &rusqlite::Connection) -> String {
    let mut stmt = match conn.prepare(
        "SELECT note_type, title, content FROM ai_note_samples ORDER BY note_type",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(), // Table may not exist yet
    };

    let samples: Vec<(String, String, String)> = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return String::new(),
    };

    if samples.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n--- STYLE REFERENCE NOTES ---\nThe following are sample clinical notes that demonstrate the preferred writing style, format, and level of detail. When creating notes, match this style closely.\n");

    for (note_type, title, content) in &samples {
        let type_label = match note_type.as_str() {
            "initial_eval" => "Initial Evaluation",
            "progress_note" => "Progress Note",
            "daily_treatment" => "Daily Treatment Note",
            _ => note_type.as_str(),
        };
        section.push_str(&format!(
            "\n[Sample {} — {}]\n{}\n",
            type_label, title, content
        ));
    }

    section.push_str("--- END STYLE REFERENCE NOTES ---");
    section
}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Input for sending a message to the assistant.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    pub message: String,
    pub conversation_id: Option<String>,
}

/// A parsed action from the assistant's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantAction {
    pub action: String,
    #[serde(flatten)]
    pub params: serde_json::Value,
}

/// Response from the assistant.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantResponse {
    pub conversation_id: String,
    pub message_id: String,
    pub content: String,
    pub actions: Option<Vec<AssistantAction>>,
    pub model_used: String,
}

/// Summary of a conversation for listing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_message: Option<String>,
}

/// A single message in a conversation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub actions_json: Option<String>,
    pub created_at: String,
}

/// Input for executing an assistant action.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteActionInput {
    pub action: String,
    pub params: serde_json::Value,
    pub conversation_id: String,
}

/// Result of an action execution.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse action blocks from the assistant's response.
///
/// Looks for ```actions ... ``` blocks and extracts the JSON array.
fn parse_actions(response: &str) -> Option<Vec<AssistantAction>> {
    // Find ```actions block
    let start_marker = "```actions";
    let end_marker = "```";

    let start = response.find(start_marker)?;
    let after_marker = start + start_marker.len();
    let rest = &response[after_marker..];
    let end = rest.find(end_marker)?;
    let json_str = rest[..end].trim();

    // Parse as JSON array
    serde_json::from_str::<Vec<AssistantAction>>(json_str).ok()
}

/// Extract the display text from a response (everything outside ```actions blocks).
fn extract_display_text(response: &str) -> String {
    let start_marker = "```actions";
    let end_marker = "```";

    if let Some(start) = response.find(start_marker) {
        let before = &response[..start];
        let after_marker = start + start_marker.len();
        let rest = &response[after_marker..];
        if let Some(end) = rest.find(end_marker) {
            let after = &rest[end + end_marker.len()..];
            format!("{}{}", before.trim(), after.trim())
                .trim()
                .to_string()
        } else {
            before.trim().to_string()
        }
    } else {
        response.trim().to_string()
    }
}

/// Auto-generate a conversation title from the first user message.
fn generate_title(message: &str) -> String {
    let truncated: String = message.chars().take(60).collect();
    if message.len() > 60 {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

/// Strip any conversational preamble before the first SOAP section header.
///
/// AI models often prepend text like "Here's the progress note I've created for..."
/// before the actual clinical content. This function finds the first SOAP marker
/// and returns everything from that point onward.
fn strip_ai_wrapper(text: &str) -> &str {
    // SOAP section markers — must appear at the START of a line (not in bullet lists)
    let markers = [
        "SUBJECTIVE:", "Subjective:", "subjective:",
        "OBJECTIVE:", "Objective:", "objective:",
        "ASSESSMENT:", "Assessment:", "assessment:",
        "PLAN:", "Plan:", "plan:",
    ];
    // Short markers (S:, O:, A:, P:) only match at line start to avoid false positives
    let short_markers = ["S:", "O:", "A:", "P:"];

    let mut earliest_pos: Option<usize> = None;

    for marker in &markers {
        // Find marker that's either at position 0 or preceded by a newline
        let mut search_from = 0;
        while search_from < text.len() {
            if let Some(pos) = text[search_from..].find(marker) {
                let abs_pos = search_from + pos;
                // Must be at start of text or start of a line (after \n)
                // Also must NOT be preceded by "- " or "* " (bullet list)
                if abs_pos == 0 || text.as_bytes().get(abs_pos - 1) == Some(&b'\n') {
                    earliest_pos = Some(match earliest_pos {
                        Some(prev) => prev.min(abs_pos),
                        None => abs_pos,
                    });
                    break;
                }
                search_from = abs_pos + 1;
            } else {
                break;
            }
        }
    }

    // Only check short markers if no long marker was found
    if earliest_pos.is_none() {
        for marker in &short_markers {
            if let Some(pos) = text.find(marker) {
                if pos == 0 || text.as_bytes().get(pos - 1) == Some(&b'\n') {
                    earliest_pos = Some(match earliest_pos {
                        Some(prev) => prev.min(pos),
                        None => pos,
                    });
                }
            }
        }
    }

    match earliest_pos {
        Some(pos) => &text[pos..],
        None => text, // No SOAP markers found, return as-is
    }
}

/// Strip any conversational postamble after the plan section content.
///
/// AI models often append text like "Let me know if you need any changes" after
/// the clinical note content. This function removes common postamble patterns.
fn strip_ai_postamble(text: &str) -> &str {
    let postamble_patterns = [
        "\nLet me know",
        "\nI hope this",
        "\nFeel free to",
        "\nPlease let me know",
        "\nDon't hesitate",
        "\nIs there anything",
        "\nWould you like",
        "\nIf you need",
        "\nI've created",
        "\nThis note",
    ];

    let mut end_pos = text.len();
    for pattern in &postamble_patterns {
        // Case-insensitive search for the pattern
        let lower = text.to_lowercase();
        let pattern_lower = pattern.to_lowercase();
        if let Some(pos) = lower.find(&pattern_lower) {
            if pos < end_pos {
                end_pos = pos;
            }
        }
    }

    text[..end_pos].trim_end()
}

/// Parsed SOAP sections from clinical note text.
struct SoapSections {
    subjective: String,
    objective: String,
    assessment: String,
    plan: String,
}

/// Parse SOAP sections from clinical note content.
///
/// Looks for section headers like "SUBJECTIVE:", "Subjective:", "S:", etc.
/// Splits content into four SOAP sections. If no SOAP markers are found,
/// puts everything into the subjective section.
fn parse_soap_sections(raw_content: &str) -> SoapSections {
    // First strip any AI wrapper/postamble
    let content = strip_ai_wrapper(raw_content);
    let content = strip_ai_postamble(content);

    // Regex-like approach: find section boundaries using case-insensitive markers
    let lower = content.to_lowercase();

    // Find the start position of each section
    let subjective_markers = ["subjective:", "s:"];
    let objective_markers = ["objective:", "o:"];
    let assessment_markers = ["assessment:", "a:"];
    let plan_markers = ["plan:", "p:"];

    let find_section_start = |markers: &[&str]| -> Option<(usize, usize)> {
        for marker in markers {
            if let Some(pos) = lower.find(marker) {
                return Some((pos, pos + marker.len()));
            }
        }
        None
    };

    let subj_range = find_section_start(&subjective_markers);
    let obj_range = find_section_start(&objective_markers);
    let assess_range = find_section_start(&assessment_markers);
    let plan_range = find_section_start(&plan_markers);

    // If we can't find at least subjective, put everything in subjective
    if subj_range.is_none() && obj_range.is_none() && assess_range.is_none() && plan_range.is_none() {
        return SoapSections {
            subjective: content.to_string(),
            objective: String::new(),
            assessment: String::new(),
            plan: String::new(),
        };
    }

    // Collect section boundaries sorted by position
    let mut boundaries: Vec<(&str, usize, usize)> = Vec::new();
    if let Some((start, content_start)) = subj_range {
        boundaries.push(("subjective", start, content_start));
    }
    if let Some((start, content_start)) = obj_range {
        // Avoid "O:" matching inside words — only match if at line start or after newline
        let valid = start == 0 || content.as_bytes().get(start.saturating_sub(1)) == Some(&b'\n');
        if valid || content_start - start > 2 {
            boundaries.push(("objective", start, content_start));
        }
    }
    if let Some((start, content_start)) = assess_range {
        let valid = start == 0 || content.as_bytes().get(start.saturating_sub(1)) == Some(&b'\n');
        if valid || content_start - start > 2 {
            boundaries.push(("assessment", start, content_start));
        }
    }
    if let Some((start, content_start)) = plan_range {
        let valid = start == 0 || content.as_bytes().get(start.saturating_sub(1)) == Some(&b'\n');
        if valid || content_start - start > 2 {
            boundaries.push(("plan", start, content_start));
        }
    }

    // Sort by position
    boundaries.sort_by_key(|&(_, start, _)| start);

    let mut subjective = String::new();
    let mut objective = String::new();
    let mut assessment = String::new();
    let mut plan = String::new();

    for i in 0..boundaries.len() {
        let (section_name, _, content_start) = boundaries[i];
        let section_end = if i + 1 < boundaries.len() {
            boundaries[i + 1].1
        } else {
            content.len()
        };
        let section_text = content[content_start..section_end].trim().to_string();

        match section_name {
            "subjective" => subjective = section_text,
            "objective" => objective = section_text,
            "assessment" => assessment = section_text,
            "plan" => plan = section_text,
            _ => {}
        }
    }

    SoapSections {
        subjective,
        objective,
        assessment,
        plan,
    }
}

/// Max conversation history messages to include in LLM context.
const MAX_HISTORY_MESSAGES: usize = 20;

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Send a message to the AI assistant and receive a response.
///
/// Creates a new conversation if `conversation_id` is not provided.
/// Returns the assistant's response with any parsed actions.
#[tauri::command]
pub async fn send_assistant_message(
    input: SendMessageInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AssistantResponse, AppError> {
    let sess = middleware::require_authenticated(&session)?;

    if input.message.trim().is_empty() {
        return Err(AppError::Validation(
            "Message cannot be empty".to_string(),
        ));
    }

    // Create or fetch conversation
    let conversation_id = if let Some(conv_id) = &input.conversation_id {
        // Verify conversation exists and belongs to user
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![conv_id, sess.user_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !exists {
            return Err(AppError::NotFound(
                "Conversation not found".to_string(),
            ));
        }

        // Update timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![conv_id],
        )?;

        conv_id.clone()
    } else {
        // Create new conversation
        let conv_id = uuid::Uuid::new_v4().to_string();
        let title = generate_title(&input.message);
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO conversations (id, user_id, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![conv_id, sess.user_id, title],
        )?;
        conv_id
    };

    // Store user message
    let user_msg_id = uuid::Uuid::new_v4().to_string();
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO assistant_messages (id, conversation_id, role, content) VALUES (?1, ?2, 'user', ?3)",
            rusqlite::params![user_msg_id, conversation_id, input.message],
        )?;
    }

    // Load conversation history for context
    let history: Vec<ChatMessage> = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT role, content FROM assistant_messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![conversation_id, MAX_HISTORY_MESSAGES as i64],
            |row| {
                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                })
            },
        )?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Call LLM — build system prompt with DB access for sample notes
    let system_prompt = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        build_system_prompt(&conn)
    };
    let (raw_response, model_used) =
        call_llm_chat(&db, &system_prompt, history).await?;

    // Parse actions from response
    let actions = parse_actions(&raw_response);
    let display_text = extract_display_text(&raw_response);
    let actions_json = actions
        .as_ref()
        .map(|a| serde_json::to_string(a).unwrap_or_default());

    // Store assistant message
    let assistant_msg_id = uuid::Uuid::new_v4().to_string();
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO assistant_messages (id, conversation_id, role, content, actions_json)
             VALUES (?1, ?2, 'assistant', ?3, ?4)",
            rusqlite::params![assistant_msg_id, conversation_id, display_text, actions_json],
        )?;
    }

    // Audit log — do NOT log message content (ePHI)
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "assistant.send_message".to_string(),
                resource_type: "AiAssistant".to_string(),
                resource_id: Some(conversation_id.clone()),
                patient_id: None,
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!("model={}", model_used)),
            },
        )?;
    }

    Ok(AssistantResponse {
        conversation_id,
        message_id: assistant_msg_id,
        content: display_text,
        actions,
        model_used,
    })
}

/// List all conversations for the current user (most recent first).
#[tauri::command]
pub async fn list_conversations(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<ConversationSummary>, AppError> {
    let sess = middleware::require_authenticated(&session)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.created_at, c.updated_at,
                (SELECT content FROM assistant_messages
                 WHERE conversation_id = c.id
                 ORDER BY created_at DESC LIMIT 1) as last_message
         FROM conversations c
         WHERE c.user_id = ?1
         ORDER BY c.updated_at DESC
         LIMIT 50",
    )?;

    let rows = stmt.query_map(rusqlite::params![sess.user_id], |row| {
        Ok(ConversationSummary {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            last_message: row.get(4)?,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get all messages in a conversation.
#[tauri::command]
pub async fn get_conversation(
    conversation_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<Vec<ConversationMessage>, AppError> {
    let sess = middleware::require_authenticated(&session)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify ownership
    let owner: String = conn
        .query_row(
            "SELECT user_id FROM conversations WHERE id = ?1",
            rusqlite::params![conversation_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound("Conversation not found".to_string()))?;

    if owner != sess.user_id {
        return Err(AppError::Unauthorized(
            "Not your conversation".to_string(),
        ));
    }

    let mut stmt = conn.prepare(
        "SELECT id, role, content, actions_json, created_at
         FROM assistant_messages
         WHERE conversation_id = ?1
         ORDER BY created_at ASC",
    )?;

    let rows = stmt.query_map(rusqlite::params![conversation_id], |row| {
        Ok(ConversationMessage {
            id: row.get(0)?,
            role: row.get(1)?,
            content: row.get(2)?,
            actions_json: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Delete a conversation and all its messages.
#[tauri::command]
pub async fn delete_conversation(
    conversation_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let sess = middleware::require_authenticated(&session)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify ownership
    let owner: String = conn
        .query_row(
            "SELECT user_id FROM conversations WHERE id = ?1",
            rusqlite::params![conversation_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound("Conversation not found".to_string()))?;

    if owner != sess.user_id {
        return Err(AppError::Unauthorized(
            "Not your conversation".to_string(),
        ));
    }

    // CASCADE deletes messages
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1",
        rusqlite::params![conversation_id],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "assistant.delete_conversation".to_string(),
            resource_type: "AiAssistant".to_string(),
            resource_id: Some(conversation_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(())
}

/// Clear the actions_json on a message after confirm/dismiss so it doesn't re-appear.
#[tauri::command]
pub async fn clear_message_actions(
    message_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let _sess = middleware::require_authenticated(&session)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE assistant_messages SET actions_json = NULL WHERE id = ?1",
        rusqlite::params![message_id],
    )?;

    Ok(())
}

/// Execute an action parsed from the assistant's response.
///
/// This handles the actual command execution (scheduling, patient lookup, etc.)
/// with proper RBAC checks for each action type.
#[tauri::command]
pub async fn execute_assistant_action(
    input: ExecuteActionInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ActionResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;

    let result = match input.action.as_str() {
        "schedule_appointment" => {
            execute_schedule_appointment(&db, &sess, &device_id, &input.params).await
        }
        "list_appointments" => execute_list_appointments(&db, &sess, &input.params).await,
        "cancel_appointment" => {
            execute_cancel_appointment(&db, &sess, &device_id, &input.params).await
        }
        "search_patients" => execute_search_patients(&db, &sess, &input.params).await,
        "find_inactive_patients" => {
            execute_find_inactive_patients(&db, &sess, &input.params).await
        }
        "export_note_pdf" => {
            execute_export_note_pdf(&db, &sess, &device_id, &input.params).await
        }
        "export_progress_report" => {
            execute_export_progress_report(&db, &sess, &device_id, &input.params).await
        }
        "export_chart" => {
            execute_export_chart(&db, &sess, &device_id, &input.params).await
        }
        "create_note" => {
            execute_create_note(&db, &sess, &device_id, &input.params).await
        }
        "get_patient_summary" => {
            execute_get_patient_summary(&db, &sess, &input.params).await
        }
        "get_patient_notes" => {
            execute_get_patient_notes(&db, &sess, &input.params).await
        }
        "search_documents" => {
            execute_search_documents(&db, &sess, &input.params).await
        }
        "get_patient_clinical_data" => {
            execute_get_patient_clinical_data(&db, &sess, &input.params).await
        }
        "read_document" => {
            execute_read_document(&db, &sess, &input.params).await
        }
        _ => Err(AppError::Validation(format!(
            "Unknown action: {}",
            input.action
        ))),
    };

    // Store the result as an assistant message in the conversation
    let result_msg = match &result {
        Ok(r) => r.message.clone(),
        Err(e) => format!("Action failed: {}", e),
    };

    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let msg_id = uuid::Uuid::new_v4().to_string();
        let _ = conn.execute(
            "INSERT INTO assistant_messages (id, conversation_id, role, content)
             VALUES (?1, ?2, 'assistant', ?3)",
            rusqlite::params![msg_id, input.conversation_id, result_msg],
        );
    }

    // Audit log
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: format!("assistant.execute_action.{}", input.action),
                resource_type: "AiAssistant".to_string(),
                resource_id: Some(input.conversation_id),
                patient_id: None,
                device_id: device_id.id().to_string(),
                success: result.is_ok(),
                details: Some(format!("action={}", input.action)),
            },
        )?;
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Action Executors
// ─────────────────────────────────────────────────────────────────────────────

/// Schedule an appointment from the assistant.
async fn execute_schedule_appointment(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, RbacAction::Create)?;

    let patient_name = params
        .get("patientName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("patientName is required".to_string()))?;

    let start_time = params
        .get("startTime")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("startTime is required".to_string()))?;

    let appt_type = params
        .get("appointmentType")
        .and_then(|v| v.as_str())
        .unwrap_or("pt_treatment");

    let duration_minutes: u32 = params
        .get("durationMinutes")
        .and_then(|v| v.as_u64())
        .unwrap_or(60) as u32;

    let recurrence = params.get("recurrence").and_then(|v| v.as_str());
    let recurrence_end_date = params.get("recurrenceEndDate").and_then(|v| v.as_str());
    let reason = params.get("reason").and_then(|v| v.as_str());

    // Look up patient by name
    let (patient_id, matched_patient_name) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT pi.patient_id, (pi.given_name || ' ' || pi.family_name) FROM patient_index pi
             WHERE (pi.given_name || ' ' || pi.family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )?;
        let matches: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![patient_name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        match matches.len() {
            0 => {
                return Ok(ActionResult {
                    success: false,
                    message: format!(
                        "No patient found matching '{}'. Please check the name and try again.",
                        patient_name
                    ),
                    data: None,
                });
            }
            1 => matches[0].clone(),
            _ => {
                let names: Vec<String> = matches.iter().map(|(_, n)| n.clone()).collect();
                return Ok(ActionResult {
                    success: false,
                    message: format!(
                        "Multiple patients found matching '{}': {}. Please be more specific.",
                        patient_name,
                        names.join(", ")
                    ),
                    data: None,
                });
            }
        }
    };

    // Create the appointment
    let resource_id = uuid::Uuid::new_v4().to_string();
    let recurrence_group_id = if recurrence.is_some() {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Build FHIR Appointment resource
    let fhir_resource = serde_json::json!({
        "resourceType": "Appointment",
        "id": resource_id,
        "status": "booked",
        "appointmentType": {
            "coding": [{"code": appt_type, "display": appt_type}]
        },
        "start": start_time,
        "minutesDuration": duration_minutes,
        "participant": [
            {
                "actor": {"reference": format!("Patient/{}", patient_id)},
                "status": "accepted"
            },
            {
                "actor": {"reference": format!("Practitioner/{}", sess.user_id)},
                "status": "accepted"
            }
        ],
        "description": reason.unwrap_or(""),
    });

    // Generate appointment slots (handle recurrence)
    let mut appointments_created = 0;

    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Parse start time
        let base_dt = chrono::NaiveDateTime::parse_from_str(start_time, "%Y-%m-%dT%H:%M:%S")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(start_time, "%Y-%m-%dT%H:%M"))
            .map_err(|e| {
                AppError::Validation(format!(
                    "Invalid start time '{}': {}",
                    start_time, e
                ))
            })?;

        let dates = if let Some(rec) = recurrence {
            let end_date = recurrence_end_date
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .unwrap_or_else(|| base_dt.date() + chrono::Duration::days(30));

            generate_recurring_dates(base_dt, rec, end_date)
        } else {
            vec![base_dt]
        };

        for dt in &dates {
            let rid = uuid::Uuid::new_v4().to_string();
            let dt_str = dt.format("%Y-%m-%dT%H:%M:%S").to_string();

            let mut appt_resource = fhir_resource.clone();
            appt_resource["id"] = serde_json::json!(rid);
            appt_resource["start"] = serde_json::json!(dt_str);

            let resource_json = serde_json::to_string(&appt_resource)
                .map_err(|e| AppError::Serialization(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated)
                 VALUES (?1, 'Appointment', ?2, 1, ?3)",
                rusqlite::params![rid, resource_json, now],
            )?;

            conn.execute(
                "INSERT INTO appointment_index
                 (appointment_id, patient_id, provider_id, start_time, status, appt_type, color, recurrence_group_id)
                 VALUES (?1, ?2, ?3, ?4, 'booked', ?5, ?6, ?7)",
                rusqlite::params![
                    rid,
                    patient_id,
                    sess.user_id,
                    dt_str,
                    appt_type,
                    "#4A90E2",
                    recurrence_group_id,
                ],
            )?;

            appointments_created += 1;
        }

        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "assistant.schedule_appointment".to_string(),
                resource_type: "Appointment".to_string(),
                resource_id: recurrence_group_id.clone(),
                patient_id: Some(patient_id.clone()),
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!("count={}", appointments_created)),
            },
        )?;
    }

    Ok(ActionResult {
        success: true,
        message: format!(
            "Successfully scheduled {} appointment(s) for {}.",
            appointments_created, matched_patient_name
        ),
        data: Some(serde_json::json!({
            "appointmentsCreated": appointments_created,
            "patientId": patient_id,
        })),
    })
}

/// Generate recurring dates from a base datetime and recurrence rule.
fn generate_recurring_dates(
    base: chrono::NaiveDateTime,
    recurrence: &str,
    end_date: chrono::NaiveDate,
) -> Vec<chrono::NaiveDateTime> {
    let mut dates = vec![base];
    let interval = match recurrence {
        "weekly" => chrono::Duration::weeks(1),
        "biweekly" => chrono::Duration::weeks(2),
        "monthly" => chrono::Duration::weeks(4), // Approximation
        _ => return dates,
    };

    let mut current = base + interval;
    while current.date() <= end_date {
        dates.push(current);
        current = current + interval;
    }

    dates
}

/// List appointments for a date.
async fn execute_list_appointments(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::AppointmentScheduling, RbacAction::Read)?;

    let date = params
        .get("date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("date is required".to_string()))?;

    let patient_name = params.get("patientName").and_then(|v| v.as_str());

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let start_of_day = format!("{}T00:00:00", date);
    let end_of_day = format!("{}T23:59:59", date);

    let query = if patient_name.is_some() {
        "SELECT ai.appointment_id, ai.start_time, ai.appt_type, ai.status,
                (pi.given_name || ' ' || pi.family_name), ai.provider_id
         FROM appointment_index ai
         JOIN patient_index pi ON pi.patient_id = ai.patient_id
         WHERE ai.start_time >= ?1 AND ai.start_time <= ?2
           AND ai.status != 'cancelled'
           AND (pi.given_name || ' ' || pi.family_name) LIKE '%' || ?3 || '%'
         ORDER BY ai.start_time"
    } else {
        "SELECT ai.appointment_id, ai.start_time, ai.appt_type, ai.status,
                (pi.given_name || ' ' || pi.family_name), ai.provider_id
         FROM appointment_index ai
         JOIN patient_index pi ON pi.patient_id = ai.patient_id
         WHERE ai.start_time >= ?1 AND ai.start_time <= ?2
           AND ai.status != 'cancelled'
         ORDER BY ai.start_time"
    };

    let mut stmt = conn.prepare(query)?;
    let rows: Vec<serde_json::Value> = if let Some(name) = patient_name {
        stmt.query_map(rusqlite::params![start_of_day, end_of_day, name], |row| {
            Ok(serde_json::json!({
                "appointmentId": row.get::<_, String>(0)?,
                "startTime": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "patientName": row.get::<_, String>(4)?,
                "providerId": row.get::<_, String>(5)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect()
    } else {
        stmt.query_map(rusqlite::params![start_of_day, end_of_day], |row| {
            Ok(serde_json::json!({
                "appointmentId": row.get::<_, String>(0)?,
                "startTime": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "patientName": row.get::<_, String>(4)?,
                "providerId": row.get::<_, String>(5)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect()
    };

    let count = rows.len();
    let message = if count == 0 {
        format!("No appointments found for {}.", date)
    } else {
        let mut lines = vec![format!("Found {} appointment(s) for {}:", count, date)];
        for appt in &rows {
            let time = appt["startTime"].as_str().unwrap_or("");
            // Extract just the time portion
            let time_display = if time.contains('T') {
                time.split('T').nth(1).unwrap_or(time)
            } else {
                time
            };
            lines.push(format!(
                "  - {} — {} ({})",
                time_display,
                appt["patientName"].as_str().unwrap_or("Unknown"),
                appt["type"].as_str().unwrap_or(""),
            ));
        }
        lines.join("\n")
    };

    Ok(ActionResult {
        success: true,
        message,
        data: Some(serde_json::json!({ "appointments": rows })),
    })
}

/// Cancel an appointment.
async fn execute_cancel_appointment(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(
        sess.role,
        Resource::AppointmentScheduling,
        RbacAction::Update,
    )?;

    let appointment_id = params
        .get("appointmentId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("appointmentId is required".to_string()))?;

    let reason = params
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("Cancelled via assistant");

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify appointment exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![appointment_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !exists {
        return Ok(ActionResult {
            success: false,
            message: format!("Appointment '{}' not found.", appointment_id),
            data: None,
        });
    }

    // Update status in index
    conn.execute(
        "UPDATE appointment_index SET status = 'cancelled' WHERE appointment_id = ?1",
        rusqlite::params![appointment_id],
    )?;

    // Update FHIR resource
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE fhir_resources SET
           resource = json_set(resource, '$.status', 'cancelled', '$.cancelationReason.text', ?2),
           last_updated = ?3,
           version_id = version_id + 1
         WHERE id = ?1 AND resource_type = 'Appointment'",
        rusqlite::params![appointment_id, reason, now],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "assistant.cancel_appointment".to_string(),
            resource_type: "Appointment".to_string(),
            resource_id: Some(appointment_id.to_string()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("reason={}", reason)),
        },
    )?;

    Ok(ActionResult {
        success: true,
        message: format!("Appointment cancelled successfully."),
        data: None,
    })
}

/// Search for patients by name.
async fn execute_search_patients(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::Patients, RbacAction::Read)?;

    let query = params
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("query is required".to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT pi.patient_id, (pi.given_name || ' ' || pi.family_name), pi.birth_date
         FROM patient_index pi
         WHERE (pi.given_name || ' ' || pi.family_name) LIKE '%' || ?1 || '%'
         ORDER BY pi.family_name, pi.given_name
         LIMIT 20",
    )?;

    let rows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![query], |row| {
            Ok(serde_json::json!({
                "patientId": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "dateOfBirth": row.get::<_, Option<String>>(2)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let count = rows.len();
    let message = if count == 0 {
        format!("No patients found matching '{}'.", query)
    } else {
        let mut lines = vec![format!("Found {} patient(s):", count)];
        for p in &rows {
            lines.push(format!(
                "  - {} (DOB: {})",
                p["name"].as_str().unwrap_or("Unknown"),
                p["dateOfBirth"].as_str().unwrap_or("N/A"),
            ));
        }
        lines.join("\n")
    };

    Ok(ActionResult {
        success: true,
        message,
        data: Some(serde_json::json!({ "patients": rows })),
    })
}

/// Find patients who haven't been seen in a given number of days.
async fn execute_find_inactive_patients(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::Patients, RbacAction::Read)?;

    let days: i64 = params
        .get("daysSinceLastVisit")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::Validation("daysSinceLastVisit is required".to_string()))?;

    let cutoff_date = (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Find patients whose most recent appointment is before the cutoff
    let mut stmt = conn.prepare(
        "SELECT pi.patient_id, (pi.given_name || ' ' || pi.family_name), MAX(ai.start_time) as last_visit
         FROM patient_index pi
         LEFT JOIN appointment_index ai ON ai.patient_id = pi.patient_id
           AND ai.status IN ('booked', 'fulfilled', 'arrived')
         GROUP BY pi.patient_id
         HAVING last_visit IS NULL OR last_visit < ?1
         ORDER BY last_visit ASC
         LIMIT 30",
    )?;

    let rows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![cutoff_date], |row| {
            Ok(serde_json::json!({
                "patientId": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "lastVisit": row.get::<_, Option<String>>(2)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let count = rows.len();
    let message = if count == 0 {
        format!("All patients have been seen within the last {} days.", days)
    } else {
        let mut lines = vec![format!(
            "Found {} patient(s) not seen in {} days:",
            count, days
        )];
        for p in &rows {
            let last = p["lastVisit"].as_str().unwrap_or("Never");
            let last_display = if last.contains('T') {
                last.split('T').next().unwrap_or(last)
            } else {
                last
            };
            lines.push(format!(
                "  - {} (Last visit: {})",
                p["name"].as_str().unwrap_or("Unknown"),
                last_display,
            ));
        }
        lines.join("\n")
    };

    Ok(ActionResult {
        success: true,
        message,
        data: Some(serde_json::json!({ "patients": rows })),
    })
}

/// Export a patient's most recent encounter note as PDF.
///
/// Looks up the patient by name, finds the most recent encounter, and returns
/// the IDs so the frontend can call the actual PDF generation command.
async fn execute_export_note_pdf(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"]
        .as_str()
        .unwrap_or("")
        .trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Fuzzy match patient
    let mut stmt = conn
        .prepare(
            "SELECT patient_id, given_name, family_name FROM patient_index
             WHERE (given_name || ' ' || family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patients: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![patient_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if patients.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: format!("No patient found matching '{}'.", patient_name),
            data: None,
        });
    }
    if patients.len() > 1 {
        let names: Vec<String> = patients.iter().map(|(_, g, f)| format!("{} {}", g, f)).collect();
        return Ok(ActionResult {
            success: false,
            message: format!("Multiple patients found: {}. Please be more specific.", names.join(", ")),
            data: None,
        });
    }

    let (patient_id, given, family) = &patients[0];

    // Find most recent encounter
    let encounter = conn
        .query_row(
            "SELECT encounter_id FROM encounter_index
             WHERE patient_id = ?1
             ORDER BY encounter_date DESC LIMIT 1",
            rusqlite::params![patient_id],
            |row| row.get::<_, String>(0),
        )
        .ok();

    if encounter.is_none() {
        return Ok(ActionResult {
            success: false,
            message: format!("No encounters found for {} {}.", given, family),
            data: None,
        });
    }

    let encounter_id = encounter.unwrap();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "assistant.export_note_pdf".to_string(),
            resource_type: "Encounter".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(ActionResult {
        success: true,
        message: format!("Exporting encounter note for {} {}...", given, family),
        data: Some(serde_json::json!({
            "autoInvoke": "generateEncounterNotePdf",
            "encounterId": encounter_id,
        })),
    })
}

/// Export a progress report PDF for a patient.
///
/// Looks up the patient by name and returns the patient ID so the frontend
/// can call the progress report generation command.
async fn execute_export_progress_report(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"]
        .as_str()
        .unwrap_or("")
        .trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT patient_id, given_name, family_name FROM patient_index
             WHERE (given_name || ' ' || family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patients: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![patient_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if patients.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: format!("No patient found matching '{}'.", patient_name),
            data: None,
        });
    }
    if patients.len() > 1 {
        let names: Vec<String> = patients.iter().map(|(_, g, f)| format!("{} {}", g, f)).collect();
        return Ok(ActionResult {
            success: false,
            message: format!("Multiple patients found: {}. Please be more specific.", names.join(", ")),
            data: None,
        });
    }

    let (patient_id, given, family) = &patients[0];

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "assistant.export_progress_report".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some(patient_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(ActionResult {
        success: true,
        message: format!("Exporting progress report for {} {}...", given, family),
        data: Some(serde_json::json!({
            "autoInvoke": "generateProgressReport",
            "patientId": patient_id,
        })),
    })
}

/// Export a patient's full chart as PDF.
///
/// Looks up the patient by name and returns the patient ID so the frontend
/// can call the chart export generation command.
async fn execute_export_chart(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"]
        .as_str()
        .unwrap_or("")
        .trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT patient_id, given_name, family_name FROM patient_index
             WHERE (given_name || ' ' || family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patients: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![patient_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if patients.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: format!("No patient found matching '{}'.", patient_name),
            data: None,
        });
    }
    if patients.len() > 1 {
        let names: Vec<String> = patients.iter().map(|(_, g, f)| format!("{} {}", g, f)).collect();
        return Ok(ActionResult {
            success: false,
            message: format!("Multiple patients found: {}. Please be more specific.", names.join(", ")),
            data: None,
        });
    }

    let (patient_id, given, family) = &patients[0];

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "assistant.export_chart".to_string(),
            resource_type: "Patient".to_string(),
            resource_id: Some(patient_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    )?;

    Ok(ActionResult {
        success: true,
        message: format!("Exporting full chart for {} {}...", given, family),
        data: Some(serde_json::json!({
            "autoInvoke": "generateChartExport",
            "patientId": patient_id,
        })),
    })
}

/// Create a clinical note for a patient.
///
/// Looks up the patient, creates a new encounter with the given note type,
/// stores the transcript as the subjective/chief complaint, and returns
/// the encounter ID so the frontend can navigate to it.
async fn execute_create_note(
    db: &Database,
    sess: &middleware::SessionContext,
    device_id: &DeviceId,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Create)?;

    let patient_name = params["patientName"].as_str().unwrap_or("").trim();
    let note_type = params["noteType"].as_str().unwrap_or("progress_note").trim();
    let template_id = params.get("templateId").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
    // noteContent is the full SOAP note generated by the LLM
    let note_content = params["noteContent"].as_str().unwrap_or("").trim();
    // Fallback to transcript for backward compatibility
    let transcript = if note_content.is_empty() {
        params["transcript"].as_str().unwrap_or("").trim()
    } else {
        note_content
    };

    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }
    if transcript.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Note content is required.".to_string(),
            data: None,
        });
    }

    let valid_types = ["initial_eval", "progress_note", "treatment_note", "discharge_note", "fce"];
    if !valid_types.contains(&note_type) {
        return Ok(ActionResult {
            success: false,
            message: format!(
                "Invalid note type '{}'. Must be one of: {}",
                note_type,
                valid_types.join(", ")
            ),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Fuzzy match patient
    let mut stmt = conn
        .prepare(
            "SELECT patient_id, given_name, family_name FROM patient_index
             WHERE (given_name || ' ' || family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patients: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![patient_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if patients.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: format!("No patient found matching '{}'.", patient_name),
            data: None,
        });
    }
    if patients.len() > 1 {
        let names: Vec<String> = patients.iter().map(|(_, g, f)| format!("{} {}", g, f)).collect();
        return Ok(ActionResult {
            success: false,
            message: format!(
                "Multiple patients found: {}. Please be more specific.",
                names.join(", ")
            ),
            data: None,
        });
    }

    let (patient_id, given, family) = &patients[0];

    // Create encounter with SOAP note
    let encounter_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let encounter_type = match note_type {
        "initial_eval" => "initial_evaluation",
        "treatment_note" => "office_visit",
        "progress_note" => "progress_note",
        "discharge_note" => "discharge",
        "fce" => "functional_capacity_evaluation",
        _ => "office_visit",
    };

    // Parse SOAP sections from the note content.
    // If templateId is provided and content starts with '{', try parsing as structured JSON
    // and convert to SOAP text via json_note_to_soap_text. Otherwise, parse as SOAP text.
    let sections = if let Some(ref tid) = template_id {
        if transcript.trim_start().starts_with('{') {
            // Try to parse as structured JSON from template schema
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(transcript) {
                let (subj, obj, assess, plan) = super::llm_integration::json_note_to_soap_text(&parsed, tid);
                SoapSections {
                    subjective: subj,
                    objective: obj,
                    assessment: assess,
                    plan: plan,
                }
            } else {
                // JSON parse failed — fall back to SOAP text parsing
                parse_soap_sections(transcript)
            }
        } else {
            parse_soap_sections(transcript)
        }
    } else {
        parse_soap_sections(transcript)
    };

    let mut soap_entries = Vec::new();
    if !sections.subjective.is_empty() {
        soap_entries.push(serde_json::json!({
            "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/note-section", "valueCode": "subjective"}],
            "text": sections.subjective
        }));
    }
    if !sections.objective.is_empty() {
        soap_entries.push(serde_json::json!({
            "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/note-section", "valueCode": "objective"}],
            "text": sections.objective
        }));
    }
    if !sections.assessment.is_empty() {
        soap_entries.push(serde_json::json!({
            "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/note-section", "valueCode": "assessment"}],
            "text": sections.assessment
        }));
    }
    if !sections.plan.is_empty() {
        soap_entries.push(serde_json::json!({
            "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/note-section", "valueCode": "plan"}],
            "text": sections.plan
        }));
    }
    // Fallback: if no sections were parsed, put the full content in subjective
    if soap_entries.is_empty() {
        soap_entries.push(serde_json::json!({
            "extension": [{"url": "http://medarc.local/fhir/StructureDefinition/note-section", "valueCode": "subjective"}],
            "text": strip_ai_postamble(strip_ai_wrapper(transcript))
        }));
    }
    let soap_note = serde_json::Value::Array(soap_entries);

    let encounter_resource = serde_json::json!({
        "resourceType": "Encounter",
        "id": encounter_id,
        "status": "in-progress",
        "class": {
            "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
            "code": "AMB",
            "display": "ambulatory"
        },
        "type": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/encounter-type",
                "code": encounter_type,
                "display": encounter_type.replace('_', " ")
            }],
            "text": encounter_type.replace('_', " ")
        }],
        "subject": {
            "reference": format!("Patient/{}", patient_id),
            "type": "Patient"
        },
        "participant": [{
            "individual": {
                "reference": format!("Practitioner/{}", sess.user_id),
                "type": "Practitioner"
            }
        }],
        "period": {
            "start": &now
        },
        "reasonCode": [{
            "text": sections.subjective.chars().take(500).collect::<String>()
        }],
        "note": soap_note
    });

    // Insert into fhir_resources (include created_at, updated_at to match normal encounter creation)
    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'Encounter', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![encounter_id, encounter_resource.to_string(), now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert into encounter_index
    conn.execute(
        "INSERT INTO encounter_index (encounter_id, patient_id, encounter_date, encounter_type, provider_id, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'in-progress')",
        rusqlite::params![
            encounter_id,
            patient_id,
            now.split('T').next().unwrap_or(&now),
            encounter_type,
            sess.user_id
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: format!("assistant.create_note.{}", note_type),
            resource_type: "Encounter".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("note_type={}", note_type)),
        },
    )?;

    let type_label = match note_type {
        "initial_eval" => "Initial Evaluation",
        "progress_note" => "Progress Note",
        "treatment_note" => "Treatment Note",
        "discharge_note" => "Discharge Note",
        "fce" => "Functional Capacity Evaluation",
        _ => note_type,
    };

    Ok(ActionResult {
        success: true,
        message: format!(
            "{} created for {} {}. You can now open the encounter to review and finalize the note.",
            type_label, given, family
        ),
        data: Some(serde_json::json!({
            "encounterId": encounter_id,
            "patientId": patient_id,
            "noteType": note_type,
            "autoNavigate": "encounter-workspace",
        })),
    })
}

/// Helper: look up a single patient by fuzzy name match.
/// Returns (patient_id, given_name, family_name) or an ActionResult error message.
fn lookup_patient_single(
    conn: &rusqlite::Connection,
    patient_name: &str,
) -> Result<Result<(String, String, String), ActionResult>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT patient_id, given_name, family_name FROM patient_index
             WHERE (given_name || ' ' || family_name) LIKE '%' || ?1 || '%'
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patients: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![patient_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if patients.is_empty() {
        return Ok(Err(ActionResult {
            success: false,
            message: format!("No patient found matching '{}'.", patient_name),
            data: None,
        }));
    }
    if patients.len() > 1 {
        let names: Vec<String> = patients.iter().map(|(_, g, f)| format!("{} {}", g, f)).collect();
        return Ok(Err(ActionResult {
            success: false,
            message: format!(
                "Multiple patients found: {}. Please be more specific.",
                names.join(", ")
            ),
            data: None,
        }));
    }

    Ok(Ok(patients.into_iter().next().unwrap()))
}

/// Get a patient's clinical summary (demographics, diagnoses, allergies, medications, recent encounters).
async fn execute_get_patient_summary(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::Patients, RbacAction::Read)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"].as_str().unwrap_or("").trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let patient = lookup_patient_single(&conn, patient_name)?;
    let (patient_id, given, family) = match patient {
        Ok(p) => p,
        Err(result) => return Ok(result),
    };

    // Demographics
    let demographics: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT birth_date, gender, mrn FROM patient_index WHERE patient_id = ?1",
            rusqlite::params![patient_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    let mut lines = vec![format!("=== Clinical Summary for {} {} ===", given, family)];
    if let Some((dob, gender, mrn)) = &demographics {
        lines.push(format!(
            "DOB: {} | Gender: {} | MRN: {}",
            dob.as_deref().unwrap_or("N/A"),
            gender.as_deref().unwrap_or("N/A"),
            mrn.as_deref().unwrap_or("N/A"),
        ));
    }

    // Active Conditions (diagnoses)
    {
        let mut stmt = conn.prepare(
            "SELECT json_extract(resource, '$.code.text'), json_extract(resource, '$.clinicalStatus.coding[0].code')
             FROM fhir_resources
             WHERE resource_type = 'Condition'
               AND json_extract(resource, '$.subject.reference') = ?1
             ORDER BY last_updated DESC LIMIT 20",
        ).map_err(|e| AppError::Database(e.to_string()))?;

        let ref_str = format!("Patient/{}", patient_id);
        let conditions: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![ref_str], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                    row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "unknown".to_string()),
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if conditions.is_empty() {
            lines.push("\nDiagnoses: None on file".to_string());
        } else {
            lines.push("\nDiagnoses:".to_string());
            for (text, status) in &conditions {
                lines.push(format!("  - {} ({})", text, status));
            }
        }
    }

    // Allergies
    {
        let mut stmt = conn.prepare(
            "SELECT json_extract(resource, '$.code.text'), json_extract(resource, '$.type')
             FROM fhir_resources
             WHERE resource_type = 'AllergyIntolerance'
               AND json_extract(resource, '$.patient.reference') = ?1
             ORDER BY last_updated DESC LIMIT 20",
        ).map_err(|e| AppError::Database(e.to_string()))?;

        let ref_str = format!("Patient/{}", patient_id);
        let allergies: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![ref_str], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                    row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "allergy".to_string()),
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if allergies.is_empty() {
            lines.push("\nAllergies: NKDA (No Known Drug Allergies)".to_string());
        } else {
            lines.push("\nAllergies:".to_string());
            for (text, allergy_type) in &allergies {
                lines.push(format!("  - {} ({})", text, allergy_type));
            }
        }
    }

    // Medications
    {
        let mut stmt = conn.prepare(
            "SELECT json_extract(resource, '$.medicationCodeableConcept.text'), json_extract(resource, '$.status')
             FROM fhir_resources
             WHERE resource_type = 'MedicationStatement'
               AND json_extract(resource, '$.subject.reference') = ?1
             ORDER BY last_updated DESC LIMIT 20",
        ).map_err(|e| AppError::Database(e.to_string()))?;

        let ref_str = format!("Patient/{}", patient_id);
        let medications: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![ref_str], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                    row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "unknown".to_string()),
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if medications.is_empty() {
            lines.push("\nMedications: None on file".to_string());
        } else {
            lines.push("\nMedications:".to_string());
            for (text, status) in &medications {
                lines.push(format!("  - {} ({})", text, status));
            }
        }
    }

    // Recent encounters (last 5)
    {
        let mut stmt = conn.prepare(
            "SELECT ei.encounter_id, ei.encounter_date, ei.encounter_type,
                    json_extract(r.resource, '$.reasonCode[0].text')
             FROM encounter_index ei
             JOIN fhir_resources r ON r.id = ei.encounter_id
             WHERE ei.patient_id = ?1
             ORDER BY ei.encounter_date DESC
             LIMIT 5",
        ).map_err(|e| AppError::Database(e.to_string()))?;

        let encounters: Vec<(String, String, String, Option<String>)> = stmt
            .query_map(rusqlite::params![patient_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if encounters.is_empty() {
            lines.push("\nRecent Encounters: None".to_string());
        } else {
            lines.push("\nRecent Encounters:".to_string());
            for (_, date, enc_type, reason) in &encounters {
                let reason_display = reason.as_deref().unwrap_or("No chief complaint");
                let reason_short: String = reason_display.chars().take(80).collect();
                lines.push(format!(
                    "  - {} — {} — {}",
                    date,
                    enc_type.replace('_', " "),
                    reason_short
                ));
            }
        }
    }

    let summary = lines.join("\n");

    Ok(ActionResult {
        success: true,
        message: summary,
        data: Some(serde_json::json!({
            "patientId": patient_id,
            "patientName": format!("{} {}", given, family),
        })),
    })
}

/// Get a patient's encounter notes with SOAP content.
async fn execute_get_patient_notes(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"].as_str().unwrap_or("").trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(5)
        .min(20);

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let patient = lookup_patient_single(&conn, patient_name)?;
    let (patient_id, given, family) = match patient {
        Ok(p) => p,
        Err(result) => return Ok(result),
    };

    let mut stmt = conn.prepare(
        "SELECT ei.encounter_id, ei.encounter_date, ei.encounter_type, r.resource
         FROM encounter_index ei
         JOIN fhir_resources r ON r.id = ei.encounter_id
         WHERE ei.patient_id = ?1
         ORDER BY ei.encounter_date DESC
         LIMIT ?2",
    ).map_err(|e| AppError::Database(e.to_string()))?;

    let encounters: Vec<(String, String, String, String)> = stmt
        .query_map(rusqlite::params![patient_id, limit], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if encounters.is_empty() {
        return Ok(ActionResult {
            success: true,
            message: format!("No encounter notes found for {} {}.", given, family),
            data: None,
        });
    }

    let mut lines = vec![format!(
        "=== Encounter Notes for {} {} ({} most recent) ===",
        given, family, encounters.len()
    )];

    for (enc_id, date, enc_type, resource_json) in &encounters {
        lines.push(format!(
            "\n--- {} — {} (ID: {}) ---",
            date,
            enc_type.replace('_', " "),
            &enc_id[..8.min(enc_id.len())]
        ));

        // Parse the FHIR resource to extract SOAP note content
        if let Ok(resource) = serde_json::from_str::<serde_json::Value>(resource_json) {
            if let Some(notes) = resource["note"].as_array() {
                for note in notes {
                    let section = note["extension"]
                        .as_array()
                        .and_then(|exts| {
                            exts.iter().find_map(|ext| {
                                if ext["url"].as_str() == Some("http://medarc.local/fhir/StructureDefinition/note-section") {
                                    ext["valueCode"].as_str().map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or_else(|| "note".to_string());
                    let text = note["text"].as_str().unwrap_or("");
                    if !text.is_empty() {
                        lines.push(format!("{}:", section.to_uppercase()));
                        lines.push(text.to_string());
                    }
                }
            }
        }
    }

    let notes_text = lines.join("\n");

    Ok(ActionResult {
        success: true,
        message: notes_text,
        data: Some(serde_json::json!({
            "patientId": patient_id,
            "noteCount": encounters.len(),
        })),
    })
}

/// Search a patient's uploaded documents (returns metadata only, not content).
async fn execute_search_documents(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"].as_str().unwrap_or("").trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let category = params.get("category").and_then(|v| v.as_str());

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let patient = lookup_patient_single(&conn, patient_name)?;
    let (patient_id, given, family) = match patient {
        Ok(p) => p,
        Err(result) => return Ok(result),
    };

    // Map frontend category names to DB values
    let db_category = category.map(|c| match c {
        "referral-rx" => "referral_rx",
        "consent-forms" => "consent_forms",
        "intake-surveys" => "intake_surveys",
        "hep" => "home_exercise_program",
        other => other,
    });

    // Try multiple patient_id formats: raw id, with Patient/ prefix
    let patient_id_variants = vec![
        patient_id.clone(),
        format!("Patient/{}", patient_id),
    ];

    let (query, use_category) = if db_category.is_some() {
        (
            "SELECT dci.document_id, dci.file_name, dci.category, dci.mime_type, dci.file_size, dci.uploaded_at
             FROM document_category_index dci
             WHERE (dci.patient_id = ?1 OR dci.patient_id = ?2) AND dci.category = ?3
             ORDER BY dci.uploaded_at DESC
             LIMIT 30",
            true,
        )
    } else {
        (
            "SELECT dci.document_id, dci.file_name, dci.category, dci.mime_type, dci.file_size, dci.uploaded_at
             FROM document_category_index dci
             WHERE (dci.patient_id = ?1 OR dci.patient_id = ?2)
             ORDER BY dci.uploaded_at DESC
             LIMIT 30",
            false,
        )
    };

    let mut stmt = conn.prepare(query).map_err(|e| AppError::Database(e.to_string()))?;

    let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "documentId": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "category": row.get::<_, String>(2)?,
            "contentType": row.get::<_, Option<String>>(3)?,
            "fileSizeBytes": row.get::<_, Option<i64>>(4)?,
            "uploadedAt": row.get::<_, String>(5)?,
        }))
    };

    let docs: Vec<serde_json::Value> = if use_category {
        stmt.query_map(
            rusqlite::params![&patient_id_variants[0], &patient_id_variants[1], db_category.unwrap_or("")],
            row_mapper,
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect()
    } else {
        stmt.query_map(
            rusqlite::params![&patient_id_variants[0], &patient_id_variants[1]],
            row_mapper,
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect()
    };

    // If no results from document_category_index, also check fhir_resources for DocumentReference
    let docs = if docs.is_empty() && category.is_none() {
        let patient_ref = format!("Patient/{}", patient_id);
        let mut fhir_stmt = conn.prepare(
            "SELECT fr.id,
                    COALESCE(json_extract(fr.resource, '$.content[0].attachment.title'),
                             json_extract(fr.resource, '$.description'),
                             json_extract(fr.resource, '$.content[0].attachment.url')),
                    json_extract(fr.resource, '$.content[0].attachment.contentType'),
                    fr.last_updated,
                    json_extract(fr.resource, '$.category[0].coding[0].code')
             FROM fhir_resources fr
             WHERE fr.resource_type = 'DocumentReference'
             AND json_extract(fr.resource, '$.subject.reference') = ?1
             ORDER BY fr.last_updated DESC
             LIMIT 30"
        ).map_err(|e| AppError::Database(e.to_string()))?;
        let fhir_docs: Vec<serde_json::Value> = fhir_stmt.query_map(
            rusqlite::params![patient_ref],
            |row| {
                let title = row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "Untitled".to_string());
                let category = row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "other".to_string());
                Ok(serde_json::json!({
                    "documentId": row.get::<_, String>(0)?,
                    "title": title,
                    "category": category,
                    "contentType": row.get::<_, Option<String>>(2)?,
                    "uploadedAt": row.get::<_, Option<String>>(3)?,
                }))
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
        fhir_docs
    } else {
        docs
    };

    let count = docs.len();
    let message = if count == 0 {
        let cat_msg = if let Some(cat) = category {
            format!(" in category '{}'", cat)
        } else {
            String::new()
        };
        format!(
            "No documents found for {} {}{}.",
            given, family, cat_msg
        )
    } else {
        let mut msg_lines = vec![format!(
            "Found {} document(s) for {} {}:",
            count, given, family
        )];
        for doc in &docs {
            let doc_id = doc["documentId"].as_str().unwrap_or("?");
            let title = doc["title"].as_str().unwrap_or("Untitled");
            let cat = doc["category"].as_str().unwrap_or("other");
            let date = doc["uploadedAt"].as_str().unwrap_or("N/A");
            let content_type = doc["contentType"].as_str().unwrap_or("unknown");
            msg_lines.push(format!(
                "  - {} [{}] ({}) — uploaded {} — ID: {}",
                title,
                cat.replace('_', " "),
                content_type,
                date.split('T').next().unwrap_or(date),
                doc_id
            ));
        }
        msg_lines.join("\n")
    };

    Ok(ActionResult {
        success: true,
        message,
        data: Some(serde_json::json!({
            "documents": docs,
            "patientId": patient_id,
        })),
    })
}

/// Get specific clinical data for a patient (conditions, allergies, medications, labs, vitals).
async fn execute_get_patient_clinical_data(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    let patient_name = params["patientName"].as_str().unwrap_or("").trim();
    if patient_name.is_empty() {
        return Ok(ActionResult {
            success: false,
            message: "Patient name is required.".to_string(),
            data: None,
        });
    }

    let data_type = params["dataType"].as_str().unwrap_or("").trim();
    let valid_types = ["conditions", "allergies", "medications", "labs", "vitals"];
    if !valid_types.contains(&data_type) {
        return Ok(ActionResult {
            success: false,
            message: format!(
                "Invalid dataType '{}'. Must be one of: {}",
                data_type,
                valid_types.join(", ")
            ),
            data: None,
        });
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let patient = lookup_patient_single(&conn, patient_name)?;
    let (patient_id, given, family) = match patient {
        Ok(p) => p,
        Err(result) => return Ok(result),
    };

    let ref_str = format!("Patient/{}", patient_id);

    let message = match data_type {
        "conditions" => {
            let mut stmt = conn.prepare(
                "SELECT json_extract(resource, '$.code.text'),
                        json_extract(resource, '$.clinicalStatus.coding[0].code'),
                        json_extract(resource, '$.onsetDateTime')
                 FROM fhir_resources
                 WHERE resource_type = 'Condition'
                   AND json_extract(resource, '$.subject.reference') = ?1
                 ORDER BY last_updated DESC LIMIT 30",
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let items: Vec<(String, String, Option<String>)> = stmt
                .query_map(rusqlite::params![ref_str], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                        row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "unknown".to_string()),
                        row.get(2)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            if items.is_empty() {
                format!("No conditions/diagnoses on file for {} {}.", given, family)
            } else {
                let mut lines = vec![format!("Conditions for {} {} ({}):", given, family, items.len())];
                for (text, status, onset) in &items {
                    let onset_str = onset.as_deref().unwrap_or("N/A");
                    lines.push(format!("  - {} | Status: {} | Onset: {}", text, status, onset_str));
                }
                lines.join("\n")
            }
        }
        "allergies" => {
            let mut stmt = conn.prepare(
                "SELECT json_extract(resource, '$.code.text'),
                        json_extract(resource, '$.type'),
                        json_extract(resource, '$.criticality')
                 FROM fhir_resources
                 WHERE resource_type = 'AllergyIntolerance'
                   AND json_extract(resource, '$.patient.reference') = ?1
                 ORDER BY last_updated DESC LIMIT 30",
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let items: Vec<(String, String, Option<String>)> = stmt
                .query_map(rusqlite::params![ref_str], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                        row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "allergy".to_string()),
                        row.get(2)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            if items.is_empty() {
                format!("No allergies on file for {} {} (NKDA).", given, family)
            } else {
                let mut lines = vec![format!("Allergies for {} {} ({}):", given, family, items.len())];
                for (text, allergy_type, criticality) in &items {
                    let crit = criticality.as_deref().unwrap_or("N/A");
                    lines.push(format!("  - {} | Type: {} | Criticality: {}", text, allergy_type, crit));
                }
                lines.join("\n")
            }
        }
        "medications" => {
            let mut stmt = conn.prepare(
                "SELECT json_extract(resource, '$.medicationCodeableConcept.text'),
                        json_extract(resource, '$.status'),
                        json_extract(resource, '$.dosage[0].text')
                 FROM fhir_resources
                 WHERE resource_type = 'MedicationStatement'
                   AND json_extract(resource, '$.subject.reference') = ?1
                 ORDER BY last_updated DESC LIMIT 30",
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let items: Vec<(String, String, Option<String>)> = stmt
                .query_map(rusqlite::params![ref_str], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                        row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "unknown".to_string()),
                        row.get(2)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            if items.is_empty() {
                format!("No medications on file for {} {}.", given, family)
            } else {
                let mut lines = vec![format!("Medications for {} {} ({}):", given, family, items.len())];
                for (text, status, dosage) in &items {
                    let dosage_str = dosage.as_deref().unwrap_or("no dosage info");
                    lines.push(format!("  - {} | Status: {} | Dosage: {}", text, status, dosage_str));
                }
                lines.join("\n")
            }
        }
        "labs" => {
            // Query DiagnosticReport and Observation with category 'laboratory'
            let mut stmt = conn.prepare(
                "SELECT resource_type,
                        json_extract(resource, '$.code.text'),
                        json_extract(resource, '$.status'),
                        json_extract(resource, '$.effectiveDateTime'),
                        json_extract(resource, '$.valueQuantity.value'),
                        json_extract(resource, '$.valueQuantity.unit')
                 FROM fhir_resources
                 WHERE (resource_type = 'DiagnosticReport'
                        OR (resource_type = 'Observation'
                            AND json_extract(resource, '$.category[0].coding[0].code') = 'laboratory'))
                   AND json_extract(resource, '$.subject.reference') = ?1
                 ORDER BY last_updated DESC LIMIT 30",
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let items: Vec<(String, String, String, Option<String>, Option<f64>, Option<String>)> = stmt
                .query_map(rusqlite::params![ref_str], |row| {
                    Ok((
                        row.get(0)?,
                        row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "Unknown".to_string()),
                        row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "unknown".to_string()),
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            if items.is_empty() {
                format!("No lab results on file for {} {}.", given, family)
            } else {
                let mut lines = vec![format!("Lab Results for {} {} ({}):", given, family, items.len())];
                for (res_type, text, status, date, value, unit) in &items {
                    let date_str = date.as_deref().unwrap_or("N/A");
                    let value_str = match (value, unit) {
                        (Some(v), Some(u)) => format!("{} {}", v, u),
                        (Some(v), None) => format!("{}", v),
                        _ => "N/A".to_string(),
                    };
                    lines.push(format!(
                        "  - [{}] {} | Status: {} | Date: {} | Value: {}",
                        res_type, text, status, date_str, value_str
                    ));
                }
                lines.join("\n")
            }
        }
        "vitals" => {
            let mut stmt = conn.prepare(
                "SELECT json_extract(resource, '$.code.text'),
                        json_extract(resource, '$.valueQuantity.value'),
                        json_extract(resource, '$.valueQuantity.unit'),
                        json_extract(resource, '$.effectiveDateTime')
                 FROM fhir_resources
                 WHERE resource_type = 'Observation'
                   AND json_extract(resource, '$.category[0].coding[0].code') = 'vital-signs'
                   AND json_extract(resource, '$.subject.reference') = ?1
                 ORDER BY last_updated DESC LIMIT 20",
            ).map_err(|e| AppError::Database(e.to_string()))?;

            let items: Vec<(String, Option<f64>, Option<String>, Option<String>)> = stmt
                .query_map(rusqlite::params![ref_str], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_else(|| "Unknown".to_string()),
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            if items.is_empty() {
                format!("No vital signs on file for {} {}.", given, family)
            } else {
                let mut lines = vec![format!("Vital Signs for {} {} ({}):", given, family, items.len())];
                for (text, value, unit, date) in &items {
                    let date_str = date.as_deref().unwrap_or("N/A");
                    let value_str = match (value, unit.as_deref()) {
                        (Some(v), Some(u)) => format!("{} {}", v, u),
                        (Some(v), None) => format!("{}", v),
                        _ => "N/A".to_string(),
                    };
                    lines.push(format!("  - {} = {} ({})", text, value_str, date_str));
                }
                lines.join("\n")
            }
        }
        _ => unreachable!(), // validated above
    };

    Ok(ActionResult {
        success: true,
        message,
        data: Some(serde_json::json!({
            "patientId": patient_id,
            "dataType": data_type,
        })),
    })
}

/// Read the contents of a patient's uploaded document.
///
/// Retrieves the document's base64 content from the FHIR resource,
/// decodes it, and for PDFs extracts readable text. For images,
/// returns file metadata since image content can't be rendered as text.
async fn execute_read_document(
    db: &Database,
    sess: &middleware::SessionContext,
    params: &serde_json::Value,
) -> Result<ActionResult, AppError> {
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, RbacAction::Read)?;

    // Accept multiple parameter name variants — LLMs may use camelCase,
    // snake_case, or shortened forms.
    let document_id = params["documentId"]
        .as_str()
        .or_else(|| params["document_id"].as_str())
        .or_else(|| params["id"].as_str())
        .or_else(|| params["docId"].as_str())
        .or_else(|| params["fileName"].as_str())
        .or_else(|| params["file_name"].as_str())
        .or_else(|| params["title"].as_str())
        .unwrap_or("")
        .trim();

    // If no document identifier was provided, try to look up by patient name
    // so the user can say "read John Smith's referral" without first calling
    // search_documents.
    if document_id.is_empty() {
        let patient_name = params["patientName"]
            .as_str()
            .or_else(|| params["patient_name"].as_str())
            .or_else(|| params["patient"].as_str())
            .unwrap_or("")
            .trim();
        let category = params.get("category").and_then(|v| v.as_str());

        if !patient_name.is_empty() {
            // Look up the patient and their most recent document inside a
            // scoped block so the MutexGuard is dropped before any .await.
            let doc_id: Option<String> = {
                let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
                let patient = lookup_patient_single(&conn, patient_name)?;
                let (patient_id, _given, _family) = match patient {
                    Ok(p) => p,
                    Err(result) => return Ok(result),
                };

                let patient_id_variants = vec![
                    patient_id.clone(),
                    format!("Patient/{}", patient_id),
                ];

                // Map frontend category names to DB values
                let db_category = category.map(|c| match c {
                    "referral-rx" => "referral_rx",
                    "consent-forms" => "consent_forms",
                    "intake-surveys" => "intake_surveys",
                    "hep" => "home_exercise_program",
                    other => other,
                });

                if let Some(cat) = db_category {
                    conn.query_row(
                        "SELECT dci.document_id FROM document_category_index dci
                         WHERE (dci.patient_id = ?1 OR dci.patient_id = ?2) AND dci.category = ?3
                         ORDER BY dci.uploaded_at DESC LIMIT 1",
                        rusqlite::params![&patient_id_variants[0], &patient_id_variants[1], cat],
                        |row| row.get(0),
                    ).ok()
                } else {
                    conn.query_row(
                        "SELECT dci.document_id FROM document_category_index dci
                         WHERE (dci.patient_id = ?1 OR dci.patient_id = ?2)
                         ORDER BY dci.uploaded_at DESC LIMIT 1",
                        rusqlite::params![&patient_id_variants[0], &patient_id_variants[1]],
                        |row| row.get(0),
                    ).ok()
                }
            }; // conn lock dropped here

            match doc_id {
                Some(id) => {
                    // Recursively call with the resolved document ID
                    let new_params = serde_json::json!({ "documentId": id });
                    return Box::pin(execute_read_document(db, sess, &new_params)).await;
                }
                None => {
                    return Ok(ActionResult {
                        success: false,
                        message: format!("No documents found for patient '{}'.", patient_name),
                        data: None,
                    });
                }
            }
        }

        return Ok(ActionResult {
            success: false,
            message: "documentId is required. Use search_documents first to find document IDs, or provide a patientName to retrieve their most recent document.".to_string(),
            data: None,
        });
    }

    // Fetch document data inside a scoped lock so we can release it before any async LLM calls.
    // Try by document_id first, then by filename (the AI sometimes passes the filename instead of ID).
    let (mime_type, file_name, resource_json) = {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.query_row(
            "SELECT dci.mime_type, dci.file_name, fr.resource
             FROM document_category_index dci
             JOIN fhir_resources fr ON fr.id = dci.document_id
             WHERE dci.document_id = ?1",
            rusqlite::params![document_id],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            )),
        )
        .or_else(|_| {
            // Try by FHIR resource ID
            conn.query_row(
                "SELECT
                    COALESCE(json_extract(resource, '$.content[0].attachment.contentType'), 'unknown'),
                    COALESCE(json_extract(resource, '$.content[0].attachment.title'), json_extract(resource, '$.description'), 'unknown'),
                    resource
                 FROM fhir_resources
                 WHERE id = ?1 AND resource_type = 'DocumentReference'",
                rusqlite::params![document_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                )),
            )
        })
        .or_else(|_| {
            // Fallback: try matching by filename in document_category_index
            conn.query_row(
                "SELECT dci.mime_type, dci.file_name, fr.resource
                 FROM document_category_index dci
                 JOIN fhir_resources fr ON fr.id = dci.document_id
                 WHERE dci.file_name = ?1
                 LIMIT 1",
                rusqlite::params![document_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                )),
            )
        })
        .or_else(|_| {
            // Last resort: fuzzy filename match
            conn.query_row(
                "SELECT dci.mime_type, dci.file_name, fr.resource
                 FROM document_category_index dci
                 JOIN fhir_resources fr ON fr.id = dci.document_id
                 WHERE dci.file_name LIKE '%' || ?1 || '%'
                 LIMIT 1",
                rusqlite::params![document_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                )),
            )
        })
        .map_err(|_| AppError::NotFound(format!("Document '{}' not found", document_id)))?
    }; // conn lock dropped here

    // Extract base64 content from FHIR resource
    let resource: serde_json::Value = serde_json::from_str(&resource_json)
        .unwrap_or(serde_json::Value::Null);

    let content_b64 = resource
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("attachment"))
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    let content_b64 = match content_b64 {
        Some(ref b64) => b64.as_str(),
        None => {
            return Ok(ActionResult {
                success: false,
                message: format!(
                    "Document '{}' ({}) exists but its content is not stored inline. It may need to be viewed directly in the app.",
                    file_name, mime_type
                ),
                data: None,
            });
        }
    };

    // Decode base64 — repair chunked base64 (strip internal padding, re-pad correctly)
    use base64::Engine as _;
    let cleaned_b64: String = content_b64.chars().filter(|&c| c != '=' && c != '\n' && c != '\r' && c != ' ').collect();
    let padded_b64 = match cleaned_b64.len() % 4 {
        2 => format!("{}==", cleaned_b64),
        3 => format!("{}=", cleaned_b64),
        _ => cleaned_b64,
    };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&padded_b64)
        .map_err(|e| AppError::Serialization(format!("Failed to decode document: {}", e)))?;

    // Extract text based on content type
    let extracted_text = if mime_type.contains("pdf") {
        // For PDFs: extract text content
        // Simple PDF text extraction — look for text between BT/ET markers or stream content
        extract_pdf_text(&decoded)
    } else if mime_type.starts_with("text/") {
        // Plain text
        String::from_utf8_lossy(&decoded).to_string()
    } else if mime_type.starts_with("image/") {
        // Send image to AI for vision analysis
        // Re-encode to clean base64 (the decoded bytes are the raw image)
        let clean_b64 = base64::engine::general_purpose::STANDARD.encode(&decoded);
        match super::llm_integration::call_llm_vision(
            db,
            "You are a medical document analysis assistant. Describe the contents of this image in detail. If it is a medical document (referral, prescription, lab result, imaging report, insurance card, etc.), extract all relevant clinical information. If it is a medical image (X-ray, MRI, etc.), describe what you observe but note that your analysis is not a substitute for a radiologist's interpretation.",
            &format!("This image is from a patient's medical chart. File name: {}. Please describe its contents in detail and extract any relevant clinical information.", file_name),
            &clean_b64,
            &mime_type,
        ).await {
            Ok((analysis, _model)) => {
                format!("[Image: {} — AI Analysis]\n\n{}", file_name, analysis)
            }
            Err(_) => {
                format!(
                    "[Image file: {} ({}, {} bytes). Vision analysis requires Claude API or Bedrock credentials. Configure in Settings > AI/LLM, then try again.]",
                    file_name, mime_type, decoded.len()
                )
            }
        }
    } else {
        format!(
            "[Binary file: {} ({}, {} bytes). Content cannot be displayed as text.]",
            file_name, mime_type, decoded.len()
        )
    };

    // Truncate very large documents for the AI context
    let truncated = if extracted_text.len() > 8000 {
        format!("{}...\n\n[Document truncated — showing first 8000 characters of {} total]",
            &extracted_text[..8000], extracted_text.len())
    } else {
        extracted_text.clone()
    };

    Ok(ActionResult {
        success: true,
        message: format!(
            "Document: {} ({})\n\n{}",
            file_name, mime_type, truncated
        ),
        data: Some(serde_json::json!({
            "documentId": document_id,
            "fileName": file_name,
            "mimeType": mime_type,
            "textLength": extracted_text.len(),
        })),
    })
}

/// Simple PDF text extraction.
///
/// Extracts readable text from PDF binary data by scanning for text operators.
/// This is a basic extraction that handles most simple PDFs without requiring
/// a full PDF parsing library.
fn extract_pdf_text(pdf_bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(pdf_bytes);
    let mut text_parts: Vec<String> = Vec::new();

    // Strategy 1: Look for text between parentheses in BT...ET blocks
    // PDF text objects: BT ... (text here) Tj ... ET
    let mut in_bt = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed == "BT" {
            in_bt = true;
            continue;
        }
        if trimmed == "ET" {
            in_bt = false;
            continue;
        }
        if in_bt {
            // Extract text from Tj, TJ, ' and " operators
            // Simple: find content between ( and )
            let mut i = 0;
            let bytes = trimmed.as_bytes();
            while i < bytes.len() {
                if bytes[i] == b'(' {
                    let start = i + 1;
                    let mut depth = 1;
                    i += 1;
                    while i < bytes.len() && depth > 0 {
                        if bytes[i] == b'(' && (i == 0 || bytes[i - 1] != b'\\') {
                            depth += 1;
                        } else if bytes[i] == b')' && (i == 0 || bytes[i - 1] != b'\\') {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    if depth == 0 {
                        let content = &trimmed[start..i - 1];
                        // Unescape common PDF escape sequences
                        let unescaped = content
                            .replace("\\n", "\n")
                            .replace("\\r", "\r")
                            .replace("\\t", "\t")
                            .replace("\\(", "(")
                            .replace("\\)", ")")
                            .replace("\\\\", "\\");
                        if !unescaped.trim().is_empty() {
                            text_parts.push(unescaped);
                        }
                    }
                } else {
                    i += 1;
                }
            }
        }
    }

    if text_parts.is_empty() {
        // Strategy 2: Look for any readable text sequences in the PDF
        // This catches text in simpler PDF structures
        let mut readable = String::new();
        let mut consecutive_printable = 0;
        for &byte in pdf_bytes {
            if byte >= 0x20 && byte < 0x7F || byte == b'\n' || byte == b'\r' || byte == b'\t' {
                readable.push(byte as char);
                if byte >= 0x20 && byte < 0x7F {
                    consecutive_printable += 1;
                }
            } else {
                if consecutive_printable > 20 {
                    // Keep this segment
                } else {
                    // Too short, likely binary noise — trim it
                    let trim_from = readable.len().saturating_sub(consecutive_printable);
                    readable.truncate(trim_from);
                }
                consecutive_printable = 0;
            }
        }
        // Filter out PDF structure keywords
        let filtered: String = readable
            .lines()
            .filter(|line| {
                let t = line.trim();
                !t.is_empty()
                    && !t.starts_with('%')
                    && !t.starts_with("<<")
                    && !t.starts_with(">>")
                    && !t.ends_with("obj")
                    && !t.starts_with("endobj")
                    && !t.starts_with("stream")
                    && !t.starts_with("endstream")
                    && !t.starts_with("xref")
                    && !t.starts_with("trailer")
                    && t.len() > 3
            })
            .collect::<Vec<_>>()
            .join("\n");

        if filtered.len() > 50 {
            return filtered;
        }

        "[PDF content could not be extracted as text. The document may contain scanned images or use encoded fonts. View it directly in the Documents tab.]".to_string()
    } else {
        text_parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_actions_basic() {
        let response = r#"I'll schedule that appointment for you.

```actions
[{"action": "schedule_appointment", "patientName": "John Smith", "startTime": "2026-03-16T10:00:00", "durationMinutes": 30}]
```

Let me know if you need anything else."#;

        let actions = parse_actions(response).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "schedule_appointment");
    }

    #[test]
    fn parse_actions_no_block() {
        let response = "I don't have enough information. What time would you like?";
        assert!(parse_actions(response).is_none());
    }

    #[test]
    fn extract_display_text_with_actions() {
        let response = r#"I'll schedule that.

```actions
[{"action": "schedule_appointment"}]
```

Done!"#;

        let text = extract_display_text(response);
        assert!(text.contains("I'll schedule that."));
        assert!(text.contains("Done!"));
        assert!(!text.contains("actions"));
    }

    #[test]
    fn extract_display_text_no_actions() {
        let response = "Just a regular message.";
        assert_eq!(extract_display_text(response), "Just a regular message.");
    }

    #[test]
    fn generate_title_short() {
        assert_eq!(
            generate_title("Schedule John"),
            "Schedule John"
        );
    }

    #[test]
    fn generate_title_long() {
        let long_msg = "Schedule John Smith for Physical Therapy Treatment every Tuesday and Thursday for the next month starting next week";
        let title = generate_title(long_msg);
        assert!(title.ends_with("..."));
        assert!(title.len() <= 64);
    }

    #[test]
    fn generate_recurring_dates_weekly() {
        let base =
            chrono::NaiveDateTime::parse_from_str("2026-03-16T10:00:00", "%Y-%m-%dT%H:%M:%S")
                .unwrap();
        let end = chrono::NaiveDate::parse_from_str("2026-04-06", "%Y-%m-%d").unwrap();
        let dates = generate_recurring_dates(base, "weekly", end);
        assert_eq!(dates.len(), 4); // Mar 16, 23, 30, Apr 6
    }

    #[test]
    fn generate_recurring_dates_biweekly() {
        let base =
            chrono::NaiveDateTime::parse_from_str("2026-03-16T10:00:00", "%Y-%m-%dT%H:%M:%S")
                .unwrap();
        let end = chrono::NaiveDate::parse_from_str("2026-04-13", "%Y-%m-%d").unwrap();
        let dates = generate_recurring_dates(base, "biweekly", end);
        assert_eq!(dates.len(), 3); // Mar 16, Mar 30, Apr 13
    }

    #[test]
    fn strip_ai_wrapper_with_preamble() {
        let text = "Here's the progress note I've created for John Smith:\n\nSUBJECTIVE:\nPatient reports improvement.";
        let stripped = strip_ai_wrapper(text);
        assert!(stripped.starts_with("SUBJECTIVE:"));
        assert!(stripped.contains("Patient reports improvement."));
    }

    #[test]
    fn strip_ai_wrapper_no_preamble() {
        let text = "SUBJECTIVE:\nPatient reports improvement.\nOBJECTIVE:\nROM WNL.";
        let stripped = strip_ai_wrapper(text);
        assert_eq!(stripped, text);
    }

    #[test]
    fn strip_ai_wrapper_no_soap_markers() {
        let text = "This is just a regular note with no SOAP structure.";
        let stripped = strip_ai_wrapper(text);
        assert_eq!(stripped, text);
    }

    #[test]
    fn strip_ai_wrapper_ignores_bullet_list_soap() {
        let text = "Got it! Here's what I'll document:\n- Subjective — Chief complaint of LBP\n- Objective — ROM, MMT\n- Assessment — Mechanical LBP\n- Plan — 2x/week\n\nSUBJECTIVE:\nPatient reports dull low back pain.";
        let stripped = strip_ai_wrapper(text);
        assert!(stripped.starts_with("SUBJECTIVE:"), "Should skip bullet list 'Subjective' and find line-start 'SUBJECTIVE:'. Got: {}", &stripped[..50.min(stripped.len())]);
    }

    #[test]
    fn strip_ai_postamble_with_postamble() {
        let text = "Plan content here.\n\nLet me know if you need any changes to this note.";
        let stripped = strip_ai_postamble(text);
        assert_eq!(stripped, "Plan content here.");
    }

    #[test]
    fn strip_ai_postamble_no_postamble() {
        let text = "Plan content here.\n- Follow up in 2 weeks.";
        let stripped = strip_ai_postamble(text);
        assert_eq!(stripped, text);
    }

    #[test]
    fn strip_ai_postamble_feel_free() {
        let text = "Plan content.\nFeel free to adjust the note as needed.";
        let stripped = strip_ai_postamble(text);
        assert_eq!(stripped, "Plan content.");
    }

    #[test]
    fn parse_soap_sections_full() {
        let content = "SUBJECTIVE:\nPatient reports pain 5/10.\n\nOBJECTIVE:\nROM WNL. MMT 4/5.\n\nASSESSMENT:\nImproving with therapy.\n\nPLAN:\nContinue PT 2x/week.";
        let sections = parse_soap_sections(content);
        assert!(sections.subjective.contains("pain 5/10"));
        assert!(sections.objective.contains("ROM WNL"));
        assert!(sections.assessment.contains("Improving"));
        assert!(sections.plan.contains("Continue PT"));
    }

    #[test]
    fn parse_soap_sections_with_preamble_and_postamble() {
        let content = "Here is the note I created:\n\nSUBJECTIVE:\nPain 5/10.\n\nOBJECTIVE:\nROM WNL.\n\nASSESSMENT:\nImproving.\n\nPLAN:\nContinue PT.\n\nLet me know if you want changes.";
        let sections = parse_soap_sections(content);
        assert!(sections.subjective.contains("Pain 5/10"));
        assert!(sections.objective.contains("ROM WNL"));
        assert!(sections.assessment.contains("Improving"));
        assert!(sections.plan.contains("Continue PT"));
        assert!(!sections.plan.contains("Let me know"));
    }

    #[test]
    fn parse_soap_sections_no_markers() {
        let content = "This is just a free-text note with no SOAP structure at all.";
        let sections = parse_soap_sections(content);
        assert_eq!(sections.subjective, content);
        assert!(sections.objective.is_empty());
        assert!(sections.assessment.is_empty());
        assert!(sections.plan.is_empty());
    }

    #[test]
    fn parse_soap_sections_lowercase_markers() {
        let content = "Subjective:\nPain 5/10.\n\nObjective:\nROM WNL.\n\nAssessment:\nImproving.\n\nPlan:\nContinue PT.";
        let sections = parse_soap_sections(content);
        assert!(sections.subjective.contains("Pain 5/10"));
        assert!(sections.objective.contains("ROM WNL"));
        assert!(sections.assessment.contains("Improving"));
        assert!(sections.plan.contains("Continue PT"));
    }
}
