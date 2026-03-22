/// commands/llm_integration.rs — Ollama LLM integration for PT note generation (M003/S03/T02)
///
/// Provides five Tauri commands:
///   1. `check_ollama_status`       — health-check Ollama at localhost:11434
///   2. `generate_note_draft`       — generate a PT note (progress or initial eval) from transcript
///   3. `suggest_cpt_codes`         — extract CPT code suggestions from note text
///   4. `extract_objective_data`    — pull ROM, pain, MMT from transcript
///   5. `configure_llm_settings`    — persist LLM provider/model preferences
///
/// RBAC: All generation commands require ClinicalDocumentation + Create.
/// Audit: Every generation call is audit-logged (transcript text is NEVER logged — only the
///        action + patient_id). This is critical for HIPAA compliance because transcripts are ePHI.
///
/// Ollama HTTP endpoints:
///   GET  http://localhost:11434/api/tags      — list models / health check
///   POST http://localhost:11434/api/generate   — text generation
///
/// Default model: llama3.1:8b, fallback: phi3:mini.
///
/// AWS Bedrock fallback is available when Ollama is unreachable and Bedrock credentials
/// have been configured via `configure_llm_settings`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

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

/// Default Ollama base URL.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default model for note generation.
const DEFAULT_MODEL: &str = "llama3.1:8b";

/// Fallback model when the primary is unavailable.
const FALLBACK_MODEL: &str = "phi3:mini";

/// HTTP timeout for Ollama health checks (seconds).
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 30;

/// HTTP timeout for generation requests (seconds).
const GENERATION_TIMEOUT_SECS: u64 = 120;

// ─────────────────────────────────────────────────────────────────────────────
// Prompt Templates
// ─────────────────────────────────────────────────────────────────────────────

/// System prompt for progress note (SOAP-PT) generation.
const PROGRESS_NOTE_SYSTEM_PROMPT: &str = r#"You are a Physical Therapy clinical documentation assistant. Given a transcript of a PT session, generate a structured progress note in SOAP-PT format. Return ONLY valid JSON matching this exact structure:

{
  "subjective": {
    "patient_report": "<patient's subjective complaints and status>",
    "pain_nrs": <numeric pain rating 0-10 or null>,
    "hep_compliance": "<yes/no/partial or null>",
    "barriers": "<any barriers to progress or null>"
  },
  "objective": {
    "treatments": [{"cpt_code": "<CPT code>", "minutes": <minutes>}],
    "exercises": "<exercises performed>"
  },
  "assessment": {
    "progress_status": "<progressing/plateau/regressing>",
    "narrative": "<clinical assessment narrative>"
  },
  "plan": {
    "next_session": "<plan for next session>",
    "hep_updates": "<any home exercise program changes>"
  }
}

For each field, also track your confidence:
- "high": the information was explicitly stated in the transcript
- "medium": the information was reasonably inferred from context
- "low": the information was not mentioned and you are guessing or using defaults

Wrap your response in a JSON object with two keys: "fields" (the note above) and "confidence" (a flat map of dot-notation field paths to confidence levels).

Common PT CPT codes: 97110 (therapeutic exercise), 97112 (neuromuscular re-education), 97140 (manual therapy), 97530 (therapeutic activities), 97116 (gait training), 97535 (self-care/ADL training)."#;

/// System prompt for initial evaluation generation.
const INITIAL_EVAL_SYSTEM_PROMPT: &str = r#"You are a Physical Therapy clinical documentation assistant. Given a transcript of an initial evaluation session, generate a structured initial evaluation note. Return ONLY valid JSON matching this exact structure:

{
  "history_of_present_illness": "<detailed HPI>",
  "past_medical_history": "<relevant PMH or null>",
  "medications": "<current medications mentioned or null>",
  "subjective": {
    "patient_report": "<chief complaint and patient goals>",
    "pain_nrs": <numeric pain rating 0-10 or null>,
    "functional_limitations": "<reported functional limitations>"
  },
  "objective": {
    "rom_measurements": {"<joint_motion>": "<degrees or WNL>"},
    "mmt_grades": {"<muscle_group>": "<grade 0-5>"},
    "special_tests": {"<test_name>": "<positive/negative>"},
    "posture_observations": "<postural findings or null>",
    "gait_analysis": "<gait findings or null>"
  },
  "assessment": {
    "pt_diagnosis": "<physical therapy diagnosis>",
    "icd10_codes": ["<code>"],
    "eval_complexity": "<low/moderate/high>",
    "prognosis": "<good/fair/poor>"
  },
  "plan": {
    "treatment_frequency": "<e.g. 2x/week for 6 weeks>",
    "short_term_goals": ["<goal with timeframe>"],
    "long_term_goals": ["<goal with timeframe>"],
    "plan_of_care": "<treatment plan narrative>",
    "hep": "<home exercise program>"
  }
}

Wrap your response in a JSON object with two keys: "fields" (the note above) and "confidence" (a flat map of dot-notation field paths to confidence levels: "high", "medium", "low")."#;

/// System prompt for CPT code suggestion.
const CPT_SUGGESTION_SYSTEM_PROMPT: &str = r#"You are a Physical Therapy billing assistant. Given the text of a PT note, suggest appropriate CPT codes for the services documented. Return ONLY a valid JSON array of objects:

[
  {"code": "<CPT code>", "description": "<description>", "minutes": <minutes>, "confidence": "<high/medium/low>"}
]

Common PT CPT codes:
- 97110: Therapeutic exercise (strength, endurance, flexibility, ROM)
- 97112: Neuromuscular re-education (balance, coordination, posture, proprioception)
- 97140: Manual therapy (mobilization, manipulation, manual traction)
- 97530: Therapeutic activities (functional activities using dynamic movements)
- 97116: Gait training (includes stair climbing)
- 97161: PT evaluation, low complexity (1-2 body regions, low acuity)
- 97162: PT evaluation, moderate complexity (3+ body regions, moderate acuity)
- 97163: PT evaluation, high complexity (3+ body regions, high acuity)
- 97535: Self-care/home management training (ADL training)
- 97542: Wheelchair management training
- 97150: Therapeutic procedure, group (2+ patients)

Only suggest codes clearly supported by the documentation. Use "high" confidence for explicitly documented services, "medium" for implied services, and "low" for services that might have been provided but are not clearly documented."#;

/// System prompt for extracting objective data from transcript.
const EXTRACT_OBJECTIVE_SYSTEM_PROMPT: &str = r#"You are a Physical Therapy clinical data extraction assistant. Given a transcript of a PT session, extract any objective measurements mentioned. Return ONLY valid JSON:

{
  "rom_values": {"<joint_motion>": "<measurement in degrees or descriptive>"},
  "pain_scores": {"<context>": <numeric 0-10>},
  "mmt_grades": {"<muscle_group>": "<grade 0-5 or descriptive>"}
}

Only include data that was explicitly mentioned in the transcript. If a category has no data, use an empty object {}. For ROM, use standard joint/motion naming (e.g., "shoulder_flexion", "knee_extension"). For pain scores, note the context (e.g., "current", "with_activity", "at_rest"). For MMT, use standard muscle group names."#;

// ─────────────────────────────────────────────────────────────────────────────
// Types — Ollama API
// ─────────────────────────────────────────────────────────────────────────────

/// Response from Ollama /api/tags endpoint.
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelInfo>>,
}

/// Individual model info from Ollama.
#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    name: String,
}

/// Request body for Ollama /api/generate endpoint.
#[derive(Debug, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    system: String,
    stream: bool,
    options: OllamaOptions,
}

/// Generation options for Ollama.
#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: i32,
}

/// Response from Ollama /api/generate endpoint.
#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types — Command inputs / outputs
// ─────────────────────────────────────────────────────────────────────────────

/// Status of the Ollama service.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    /// Whether Ollama is reachable and responding.
    pub available: bool,
    /// List of model names installed on the local Ollama instance.
    pub models: Vec<String>,
    /// Error message if Ollama is unavailable, with setup instructions.
    pub error: Option<String>,
}

/// Result of a note generation call.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDraftResult {
    /// The type of note that was generated (e.g. "progress_note", "initial_eval").
    pub note_type: String,
    /// The structured fields of the generated note.
    pub fields: serde_json::Value,
    /// Confidence levels for each field (dot-notation path -> high/medium/low).
    pub confidence: HashMap<String, String>,
    /// Which LLM model was used for generation.
    pub model_used: String,
    /// Wall-clock generation time in milliseconds.
    pub generation_time_ms: u64,
}

/// A single CPT code suggestion.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CptSuggestion {
    /// CPT code (e.g. "97110").
    pub code: String,
    /// Human-readable description of the service.
    pub description: String,
    /// Suggested minutes for the service.
    pub minutes: u32,
    /// Confidence level: "high", "medium", or "low".
    pub confidence: String,
}

/// Objective data extracted from a transcript.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedObjectiveData {
    /// ROM measurements keyed by joint/motion (e.g. "shoulder_flexion": "160 degrees").
    #[serde(alias = "rom_values")]
    pub rom_values: Option<HashMap<String, serde_json::Value>>,
    /// Pain scores keyed by context (e.g. "current": 5).
    #[serde(alias = "pain_scores")]
    pub pain_scores: Option<HashMap<String, serde_json::Value>>,
    /// Manual muscle testing grades keyed by muscle group.
    #[serde(alias = "mmt_grades")]
    pub mmt_grades: Option<HashMap<String, String>>,
}

/// Optional patient context to improve note generation quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientContext {
    /// Patient ID (for audit logging).
    pub patient_id: Option<String>,
    /// Patient demographics summary (e.g. "65yo male").
    pub demographics: Option<String>,
    /// Summary of prior note for continuity.
    pub prior_note_summary: Option<String>,
    /// Diagnosis or reason for referral.
    pub diagnosis: Option<String>,
}

/// LLM settings input for configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettingsInput {
    /// Provider: "ollama", "claude", or "bedrock".
    pub provider: String,
    /// Model name override (e.g. "llama3.1:8b") — used for Ollama.
    pub model: Option<String>,
    /// Custom Ollama URL (default: http://localhost:11434).
    pub ollama_url: Option<String>,
    /// API key: Claude API key when provider="claude", AWS access key when provider="bedrock".
    pub api_key: Option<String>,
    /// AWS secret key for Bedrock (stored encrypted in SQLCipher).
    pub api_secret: Option<String>,
    /// AWS region for Bedrock (e.g. "us-east-1").
    pub bedrock_region: Option<String>,
    /// Bedrock model override (e.g. "anthropic.claude-3-haiku-20240307-v1:0").
    pub bedrock_model: Option<String>,
    /// Claude model override (e.g. "claude-sonnet-4-20250514").
    pub claude_model: Option<String>,
}

/// Stored LLM settings returned to callers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub provider: String,
    pub model: Option<String>,
    pub ollama_url: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build the Ollama tags URL from a base URL.
pub fn build_ollama_tags_url(base_url: &str) -> String {
    format!("{}/api/tags", base_url.trim_end_matches('/'))
}

/// Build the Ollama generate URL from a base URL.
pub fn build_ollama_generate_url(base_url: &str) -> String {
    format!("{}/api/generate", base_url.trim_end_matches('/'))
}

/// Construct the full prompt for a progress note generation request.
pub fn build_progress_note_prompt(transcript: &str, patient_context: &Option<PatientContext>, clinical_context: &str) -> String {
    let mut prompt = String::new();

    if !clinical_context.is_empty() {
        prompt.push_str(clinical_context);
        prompt.push('\n');
    }

    if let Some(ctx) = patient_context {
        if let Some(ref demo) = ctx.demographics {
            prompt.push_str(&format!("Patient demographics: {}\n", demo));
        }
        if let Some(ref diag) = ctx.diagnosis {
            prompt.push_str(&format!("Diagnosis: {}\n", diag));
        }
        if let Some(ref prior) = ctx.prior_note_summary {
            prompt.push_str(&format!("Prior note summary: {}\n", prior));
        }
        prompt.push('\n');
    }

    prompt.push_str("Session transcript:\n");
    prompt.push_str(transcript);

    prompt
}

/// Construct the full prompt for an initial evaluation generation request.
pub fn build_initial_eval_prompt(transcript: &str, patient_context: &Option<PatientContext>, clinical_context: &str) -> String {
    let mut prompt = String::new();

    if !clinical_context.is_empty() {
        prompt.push_str(clinical_context);
        prompt.push('\n');
    }

    if let Some(ctx) = patient_context {
        if let Some(ref demo) = ctx.demographics {
            prompt.push_str(&format!("Patient demographics: {}\n", demo));
        }
        if let Some(ref diag) = ctx.diagnosis {
            prompt.push_str(&format!("Referral diagnosis: {}\n", diag));
        }
        prompt.push('\n');
    }

    prompt.push_str("Initial evaluation transcript:\n");
    prompt.push_str(transcript);

    prompt
}

/// Parse the LLM's JSON response for a note draft.
///
/// The LLM response should contain a JSON object with "fields" and "confidence" keys.
/// Also handles {"soap":..., "metadata":...} format.
/// This function extracts them, using robust JSON extraction to handle think tags, code fences, and prose.
pub fn parse_note_draft_response(
    raw_response: &str,
) -> Result<(serde_json::Value, HashMap<String, String>), AppError> {
    let json_str = extract_json_from_response(raw_response)?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| AppError::Serialization(format!("Failed to parse LLM response as JSON: {}. Raw response starts with: {}", e, &raw_response[..raw_response.len().min(200)])))?;

    // Handle {"fields": ..., "confidence": ...} format
    if let Some(fields) = parsed.get("fields") {
        let confidence: HashMap<String, String> = if let Some(conf) = parsed.get("confidence") {
            serde_json::from_value(conf.clone()).unwrap_or_default()
        } else {
            HashMap::new()
        };
        return Ok((fields.clone(), confidence));
    }

    // Handle {"soap": ..., "metadata": ...} format
    if let Some(soap) = parsed.get("soap") {
        let confidence: HashMap<String, String> = if let Some(meta) = parsed.get("metadata") {
            if let Some(conf) = meta.get("confidence") {
                serde_json::from_value(conf.clone()).unwrap_or_default()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };
        return Ok((soap.clone(), confidence));
    }

    // Bare JSON (no wrapper) — use the whole object as fields
    Ok((parsed, HashMap::new()))
}

/// Parse the LLM's JSON response for CPT code suggestions.
pub fn parse_cpt_suggestions(raw_response: &str) -> Result<Vec<CptSuggestion>, AppError> {
    let cleaned = strip_code_fences(raw_response);

    let parsed: serde_json::Value = serde_json::from_str(&cleaned).map_err(|e| {
        AppError::Serialization(format!(
            "Failed to parse CPT suggestion response: {}",
            e
        ))
    })?;

    // The response should be an array of CPT suggestions
    let arr = if parsed.is_array() {
        parsed
    } else if let Some(arr) = parsed.get("suggestions") {
        arr.clone()
    } else {
        return Err(AppError::Serialization(
            "CPT suggestion response is not an array".to_string(),
        ));
    };

    let suggestions: Vec<CptSuggestion> = serde_json::from_value(arr).map_err(|e| {
        AppError::Serialization(format!("Failed to deserialize CPT suggestions: {}", e))
    })?;

    Ok(suggestions)
}

/// Parse the LLM's JSON response for extracted objective data.
pub fn parse_extracted_objective_data(raw_response: &str) -> Result<ExtractedObjectiveData, AppError> {
    let cleaned = strip_code_fences(raw_response);

    let data: ExtractedObjectiveData = serde_json::from_str(&cleaned).map_err(|e| {
        AppError::Serialization(format!(
            "Failed to parse extracted objective data: {}",
            e
        ))
    })?;

    Ok(data)
}

/// Strip markdown code fences (```json ... ```) from LLM output.
pub fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    // Check for ```json at the start
    let without_prefix = if trimmed.starts_with("```json") {
        &trimmed[7..]
    } else if trimmed.starts_with("```") {
        &trimmed[3..]
    } else {
        trimmed
    };

    // Remove trailing ```
    let without_suffix = if without_prefix.trim_end().ends_with("```") {
        let end = without_prefix.rfind("```").unwrap();
        &without_prefix[..end]
    } else {
        without_prefix
    };

    without_suffix.trim().to_string()
}

/// Read LLM settings from the app_settings table.
fn read_llm_settings(conn: &rusqlite::Connection) -> LlmSettings {
    let provider = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_provider'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "ollama".to_string());

    let model = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_model'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();

    let ollama_url = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_ollama_url'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();

    LlmSettings {
        provider,
        model,
        ollama_url,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FullLlmSettings — returned by get_llm_settings command
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullLlmSettings {
    pub provider: String,
    pub model: Option<String>,
    pub ollama_url: Option<String>,
    pub claude_api_key: Option<String>,
    pub claude_model: Option<String>,
    pub bedrock_access_key: Option<String>,
    pub bedrock_secret_key: Option<String>,
    pub bedrock_region: Option<String>,
    pub bedrock_model: Option<String>,
}

fn read_setting(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn mask_secret(value: Option<String>) -> Option<String> {
    value.map(|v| {
        if v.len() <= 4 {
            v
        } else {
            format!(
                "{}{}",
                "\u{2022}".repeat(v.len().min(8).saturating_sub(4)),
                &v[v.len() - 4..]
            )
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Claude API credentials
// ─────────────────────────────────────────────────────────────────────────────

/// Claude API credentials read synchronously from the database.
struct ClaudeCredentials {
    api_key: String,
    model: String,
}

/// Read Claude API credentials from the app_settings table (synchronous).
fn read_claude_credentials(conn: &rusqlite::Connection) -> Result<ClaudeCredentials, AppError> {
    let api_key = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_claude_api_key'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| {
            AppError::Validation(
                "Claude API key not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;

    let model = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_claude_model'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

    Ok(ClaudeCredentials { api_key, model })
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat message type for multi-turn conversations
// ─────────────────────────────────────────────────────────────────────────────

/// A single message in a chat conversation (used by assistant and multi-turn LLM calls).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Sample notes and clinical context helpers
// ─────────────────────────────────────────────────────────────────────────────

fn load_sample_notes(conn: &rusqlite::Connection, note_type: &str) -> Vec<(String, String)> {
    let mut stmt = match conn.prepare(
        "SELECT title, content FROM ai_note_samples WHERE note_type = ?1 ORDER BY created_at DESC LIMIT 5",
    ) {
        Ok(s) => s,
        Err(_) => return vec![], // Table may not exist yet
    };

    let results: Vec<(String, String)> = match stmt.query_map(rusqlite::params![note_type], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    };
    results
}

fn build_style_reference(samples: &[(String, String)]) -> String {
    if samples.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n\n--- STYLE REFERENCE ---\nThe following are sample clinical notes that demonstrate the preferred writing style, format, and level of detail. When generating notes, match this style closely.\n",
    );

    for (title, content) in samples {
        out.push_str(&format!("\n[Sample — {}]\n{}\n", title, content));
    }

    out.push_str("--- END STYLE REFERENCE ---\n");
    out
}

fn load_patient_clinical_context(conn: &rusqlite::Connection, patient_id: &str) -> String {
    let mut sections: Vec<String> = Vec::new();

    // Patient demographics
    if let Ok(demo) = conn.query_row(
        "SELECT json_extract(resource, '$.name[0].given[0]') || ' ' || json_extract(resource, '$.name[0].family'),
                json_extract(resource, '$.birthDate'),
                json_extract(resource, '$.gender')
         FROM fhir_resources WHERE resource_type = 'Patient' AND resource_id = ?1",
        rusqlite::params![patient_id],
        |row| {
            Ok(format!(
                "Patient: {} | DOB: {} | Gender: {}",
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
            ))
        },
    ) {
        sections.push(demo);
    }

    // Active conditions
    {
        let mut stmt = match conn.prepare(
            "SELECT json_extract(resource, '$.code.text'),
                    json_extract(resource, '$.clinicalStatus.coding[0].code')
             FROM fhir_resources
             WHERE resource_type = 'Condition'
               AND json_extract(resource, '$.subject.reference') = ('Patient/' || ?1)
             ORDER BY rowid DESC LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };
        let conditions: Vec<String> = stmt
            .query_map(rusqlite::params![patient_id], |row| {
                Ok(format!(
                    "- {} ({})",
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, String>(1).unwrap_or_else(|_| "active".to_string()),
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !conditions.is_empty() {
            sections.push(format!("Active Conditions:\n{}", conditions.join("\n")));
        }
    }

    // Medications
    {
        let mut stmt = match conn.prepare(
            "SELECT json_extract(resource, '$.medicationCodeableConcept.text')
             FROM fhir_resources
             WHERE resource_type = 'MedicationStatement'
               AND json_extract(resource, '$.subject.reference') = ('Patient/' || ?1)
             ORDER BY rowid DESC LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return if sections.is_empty() { String::new() } else { format!("\n--- PATIENT CLINICAL CONTEXT ---\n{}\n--- END PATIENT CLINICAL CONTEXT ---\n", sections.join("\n\n")) },
        };
        let meds: Vec<String> = stmt
            .query_map(rusqlite::params![patient_id], |row| {
                Ok(format!("- {}", row.get::<_, String>(0).unwrap_or_default()))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !meds.is_empty() {
            sections.push(format!("Medications:\n{}", meds.join("\n")));
        }
    }

    // Allergies
    {
        let mut stmt = match conn.prepare(
            "SELECT json_extract(resource, '$.code.text')
             FROM fhir_resources
             WHERE resource_type = 'AllergyIntolerance'
               AND json_extract(resource, '$.patient.reference') = ('Patient/' || ?1)
             ORDER BY rowid DESC LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return if sections.is_empty() { String::new() } else { format!("\n--- PATIENT CLINICAL CONTEXT ---\n{}\n--- END PATIENT CLINICAL CONTEXT ---\n", sections.join("\n\n")) },
        };
        let allergies: Vec<String> = stmt
            .query_map(rusqlite::params![patient_id], |row| {
                Ok(format!("- {}", row.get::<_, String>(0).unwrap_or_default()))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !allergies.is_empty() {
            sections.push(format!("Allergies:\n{}", allergies.join("\n")));
        }
    }

    // Recent notes (last 3)
    {
        let mut stmt = match conn.prepare(
            "SELECT json_extract(resource, '$.type.text'),
                    json_extract(resource, '$.date'),
                    substr(json_extract(resource, '$.content[0].attachment.data'), 1, 500)
             FROM fhir_resources
             WHERE resource_type = 'DocumentReference'
               AND json_extract(resource, '$.subject.reference') = ('Patient/' || ?1)
             ORDER BY json_extract(resource, '$.date') DESC LIMIT 3",
        ) {
            Ok(s) => s,
            Err(_) => return if sections.is_empty() { String::new() } else { format!("\n--- PATIENT CLINICAL CONTEXT ---\n{}\n--- END PATIENT CLINICAL CONTEXT ---\n", sections.join("\n\n")) },
        };
        let notes: Vec<String> = stmt
            .query_map(rusqlite::params![patient_id], |row| {
                Ok(format!(
                    "- {} ({}): {}...",
                    row.get::<_, String>(0).unwrap_or_else(|_| "Note".to_string()),
                    row.get::<_, String>(1).unwrap_or_default(),
                    row.get::<_, String>(2).unwrap_or_default(),
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !notes.is_empty() {
            sections.push(format!("Recent Notes:\n{}", notes.join("\n")));
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        "\n--- PATIENT CLINICAL CONTEXT ---\n{}\n--- END PATIENT CLINICAL CONTEXT ---\n",
        sections.join("\n\n")
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Robust JSON extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Strip <think>...</think> tags that some models (e.g. deepseek-r1) include.
fn strip_think_tags(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            result = format!("{}{}", &result[..start], &result[end + 8..]);
        } else {
            // Unclosed <think> — strip from <think> to end
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Extract a JSON object or array from an LLM response, handling code fences,
/// think tags, and surrounding prose.
pub fn extract_json_from_response(raw: &str) -> Result<String, AppError> {
    // 1. Strip think tags
    let cleaned = strip_think_tags(raw);

    // 2. Strip code fences
    let cleaned = strip_code_fences(&cleaned);

    // 3. Try parsing as-is
    if serde_json::from_str::<serde_json::Value>(&cleaned).is_ok() {
        return Ok(cleaned);
    }

    // 4. Try to find a JSON object or array in the text
    // Find the first { or [
    let obj_start = cleaned.find('{');
    let arr_start = cleaned.find('[');

    let start = match (obj_start, arr_start) {
        (Some(o), Some(a)) => std::cmp::min(o, a),
        (Some(o), None) => o,
        (None, Some(a)) => a,
        (None, None) => {
            return Err(AppError::Serialization(format!(
                "No JSON object or array found in LLM response. Raw starts with: {}",
                &raw[..raw.len().min(200)]
            )));
        }
    };

    let is_object = cleaned.as_bytes()[start] == b'{';
    let close_char = if is_object { '}' } else { ']' };

    // Walk forward to find the matching close
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let mut end_pos = None;

    for (i, ch) in cleaned[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '{' || ch == '[' {
            depth += 1;
        } else if ch == '}' || ch == ']' {
            depth -= 1;
            if depth == 0 && ch == close_char {
                end_pos = Some(start + i + 1);
                break;
            }
        }
    }

    let end = end_pos.ok_or_else(|| {
        AppError::Serialization(format!(
            "Unterminated JSON in LLM response. Raw starts with: {}",
            &raw[..raw.len().min(200)]
        ))
    })?;

    let candidate = &cleaned[start..end];
    serde_json::from_str::<serde_json::Value>(candidate).map_err(|e| {
        AppError::Serialization(format!(
            "Extracted JSON is not valid: {}. Fragment: {}",
            e,
            &candidate[..candidate.len().min(300)]
        ))
    })?;

    Ok(candidate.to_string())
}

/// Convert structured JSON fields to SOAP text sections.
pub fn json_note_to_soap_text(
    fields: &serde_json::Value,
    template_id: &str,
) -> (String, String, String, String) {
    let extract_section = |key: &str| -> String {
        match fields.get(key) {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) if v.is_object() => {
                let obj = v.as_object().unwrap();
                obj.iter()
                    .map(|(k, val)| {
                        let label = k.replace('_', " ");
                        let text = match val {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Array(arr) => arr
                                .iter()
                                .filter_map(|a| a.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            serde_json::Value::Null => "N/A".to_string(),
                            other => other.to_string(),
                        };
                        format!("{}: {}", label, text)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Some(v) => v.to_string(),
            None => String::new(),
        }
    };

    // Map template types to their SOAP-equivalent section keys
    let (s_keys, o_keys, a_keys, p_keys): (&[&str], &[&str], &[&str], &[&str]) = match template_id
    {
        "pt_initial_eval" => (
            &["history_of_present_illness", "past_medical_history", "medications", "subjective"],
            &["objective"],
            &["assessment"],
            &["plan"],
        ),
        "fce" => (
            &["patient_info"],
            &["physical_demands", "musculoskeletal", "functional_testing"],
            &["conclusions"],
            &[],
        ),
        _ => (
            &["subjective"],
            &["objective"],
            &["assessment"],
            &["plan"],
        ),
    };

    let build_section = |keys: &[&str]| -> String {
        keys.iter()
            .map(|k| extract_section(k))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    (
        build_section(s_keys),
        build_section(o_keys),
        build_section(a_keys),
        build_section(p_keys),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Multi-turn chat helpers (used by assistant and note generation)
// ─────────────────────────────────────────────────────────────────────────────

/// Ollama /api/chat request.
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaChatMessage>,
}

/// Call Ollama /api/chat (multi-turn).
async fn call_ollama_chat(
    ollama_url: &str,
    model: &str,
    system_prompt: &str,
    messages: &[ChatMessage],
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let mut chat_messages = vec![OllamaChatMessage {
        role: "system".to_string(),
        content: system_prompt.to_string(),
    }];

    for msg in messages {
        chat_messages.push(OllamaChatMessage {
            role: msg.role.clone(),
            content: msg.content.clone(),
        });
    }

    let request = OllamaChatRequest {
        model: model.to_string(),
        messages: chat_messages,
        stream: false,
        options: OllamaOptions {
            temperature: 0.3,
            num_predict: 4096,
        },
    };

    let url = format!("{}/api/chat", ollama_url.trim_end_matches('/'));
    let response = client.post(&url).json(&request).send().await.map_err(|e| {
        AppError::Serialization(format!(
            "Ollama chat request failed. Is Ollama running at {}? Error: {}",
            ollama_url, e
        ))
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Serialization(format!(
            "Ollama chat returned HTTP {}: {}",
            status, body
        )));
    }

    let resp: OllamaChatResponse = response.json().await.map_err(|e| {
        AppError::Serialization(format!("Failed to parse Ollama chat response: {}", e))
    })?;

    Ok(resp
        .message
        .map(|m| m.content)
        .unwrap_or_default())
}

/// Call Claude Messages API directly (not via Bedrock).
async fn call_claude_generate(
    credentials: &ClaudeCredentials,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, String), AppError> {
    let body = serde_json::json!({
        "model": credentials.model,
        "max_tokens": 4096,
        "temperature": 0.3,
        "system": system_prompt,
        "messages": [
            {"role": "user", "content": user_prompt}
        ]
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Content-Type", "application/json")
        .header("x-api-key", &credentials.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Serialization(format!("Claude API request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::Serialization(format!(
            "Claude API returned HTTP {}: {}",
            status, body_text
        )));
    }

    let resp_json: serde_json::Value = response.json().await.map_err(|e| {
        AppError::Serialization(format!("Failed to parse Claude API response: {}", e))
    })?;

    let text = resp_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok((text, format!("claude:{}", credentials.model)))
}

/// Call the LLM with an image for vision analysis.
///
/// Sends a base64-encoded image to Claude (direct API or Bedrock) for analysis.
/// Returns `(response_text, model_used)`.
/// Falls back through providers: Claude API → Bedrock → error.
pub(crate) async fn call_llm_vision(
    db: &crate::db::connection::Database,
    system_prompt: &str,
    text_prompt: &str,
    image_base64: &str,
    media_type: &str,
) -> Result<(String, String), AppError> {
    // Read credentials synchronously
    let (settings, claude_creds, bedrock_creds) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let cc = read_claude_credentials(&conn).ok();
        let bc = read_bedrock_credentials(&conn).ok();
        (s, cc, bc)
    };

    // Claude API supports vision natively
    if let Some(creds) = &claude_creds {
        if settings.provider == "claude" || settings.provider == "ollama" {
            let model = if creds.model.is_empty() { "claude-sonnet-4-6" } else { &creds.model };
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "temperature": 0.3,
                "system": system_prompt,
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": image_base64
                            }
                        },
                        {
                            "type": "text",
                            "text": text_prompt
                        }
                    ]
                }]
            });

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
                .build()
                .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

            let response = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &creds.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::Serialization(format!("Claude vision request failed: {}", e)))?;

            if response.status().is_success() {
                let resp: serde_json::Value = response.json().await.map_err(|e| {
                    AppError::Serialization(format!("Failed to parse Claude response: {}", e))
                })?;
                let text = resp
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                return Ok((text, format!("claude:{}", model)));
            }
        }
    }

    // Bedrock also supports vision via the Anthropic model
    if let Some(creds) = &bedrock_creds {
        let default_model = "us.anthropic.claude-sonnet-4-6";
        // BedrockCredentials may or may not have a model field — read from settings
        let model_id = settings.model.as_deref().unwrap_or(default_model);
        let body_json = serde_json::json!({
            "anthropic_version": "bedrock-2023-05-31",
            "max_tokens": 4096,
            "temperature": 0.3,
            "system": system_prompt,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": image_base64
                        }
                    },
                    {
                        "type": "text",
                        "text": text_prompt
                    }
                ]
            }]
        });
        let body_bytes = serde_json::to_vec(&body_json)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize: {}", e)))?;

        let host = format!("bedrock-runtime.{}.amazonaws.com", creds.region);
        let uri = format!("/model/{}/invoke", model_id);
        let url = format!("https://{}{}", host, uri);

        // SigV4 signing
        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let payload_hash = hex::encode(Sha256::digest(&body_bytes));
        let canonical_headers = format!("content-type:application/json\nhost:{}\nx-amz-date:{}\n", host, amz_date);
        let signed_headers = "content-type;host;x-amz-date";
        let canonical_request = format!("POST\n{}\n\n{}\n{}\n{}", uri, canonical_headers, signed_headers, payload_hash);
        let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let scope = format!("{}/{}/bedrock/aws4_request", date_stamp, creds.region);
        let sts = format!("AWS4-HMAC-SHA256\n{}\n{}\n{}", amz_date, scope, canonical_hash);

        // HMAC-SHA256 chain for signing key
        let sign = |key: &[u8], data: &[u8]| -> Vec<u8> {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key size");
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        };
        let k_date = sign(format!("AWS4{}", creds.secret_key).as_bytes(), date_stamp.as_bytes());
        let k_region = sign(&k_date, creds.region.as_bytes());
        let k_service = sign(&k_region, b"bedrock");
        let k_signing = sign(&k_service, b"aws4_request");
        let signature = hex::encode(sign(&k_signing, sts.as_bytes()));
        let auth = format!("AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}", creds.access_key, scope, signed_headers, signature);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Host", &host)
            .header("X-Amz-Date", &amz_date)
            .header("Authorization", &auth)
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| AppError::Serialization(format!("Bedrock vision request failed: {}", e)))?;

        if response.status().is_success() {
            let resp: serde_json::Value = response.json().await.map_err(|e| {
                AppError::Serialization(format!("Failed to parse Bedrock response: {}", e))
            })?;
            let text = resp
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            return Ok((text, format!("bedrock:{}", model_id)));
        }
    }

    Err(AppError::Serialization(
        "Vision analysis requires Claude API or AWS Bedrock credentials. Configure one in Settings > AI/LLM.".to_string()
    ))
}

/// Call Claude Messages API with multi-turn history.
async fn call_claude_chat(
    credentials: &ClaudeCredentials,
    system_prompt: &str,
    messages: &[ChatMessage],
) -> Result<(String, String), AppError> {
    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        })
        .collect();

    let body = serde_json::json!({
        "model": credentials.model,
        "max_tokens": 4096,
        "temperature": 0.3,
        "system": system_prompt,
        "messages": api_messages
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Content-Type", "application/json")
        .header("x-api-key", &credentials.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Serialization(format!("Claude API chat request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::Serialization(format!(
            "Claude API returned HTTP {}: {}",
            status, body_text
        )));
    }

    let resp_json: serde_json::Value = response.json().await.map_err(|e| {
        AppError::Serialization(format!("Failed to parse Claude API response: {}", e))
    })?;

    let text = resp_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok((text, format!("claude:{}", credentials.model)))
}

/// Unified LLM chat function used by the assistant module and other callers.
/// Reads provider settings from DB, routes to the appropriate backend.
pub(crate) async fn call_llm_chat(
    db: &State<'_, Database>,
    system_prompt: &str,
    messages: Vec<ChatMessage>,
) -> Result<(String, String), AppError> {
    let (settings, ollama_url, claude_creds, bedrock_creds) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let url = s
            .ollama_url
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        let cc = read_claude_credentials(&conn).ok();
        let bc = read_bedrock_credentials(&conn).ok();
        (s, url, cc, bc)
    };

    // If user explicitly configured Claude API
    if settings.provider == "claude" {
        let creds = claude_creds.ok_or_else(|| {
            AppError::Validation("Claude API key not configured.".to_string())
        })?;
        return call_claude_chat(&creds, system_prompt, &messages).await;
    }

    // If user explicitly configured Bedrock
    if settings.provider == "bedrock" {
        let creds = bedrock_creds.ok_or_else(|| {
            AppError::Validation(
                "AWS Bedrock credentials not configured. Go to Settings > AI/LLM to configure."
                    .to_string(),
            )
        })?;
        // Build a single prompt from messages for Bedrock generate
        let user_prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        return call_bedrock_generate(&creds, system_prompt, &user_prompt).await;
    }

    // Default: try Ollama chat
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

    let url = build_ollama_tags_url(&ollama_url);
    let ollama_available = match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let tags: OllamaTagsResponse = resp
                .json()
                .await
                .unwrap_or(OllamaTagsResponse { models: None });
            let models: Vec<String> = tags
                .models
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.name)
                .collect();
            Some(models)
        }
        _ => None,
    };

    match ollama_available {
        Some(available_models) => {
            let model = select_model(&settings, &available_models);
            let response =
                call_ollama_chat(&ollama_url, &model, system_prompt, &messages).await?;
            Ok((response, model))
        }
        None => {
            // Ollama unavailable — try Claude fallback, then Bedrock fallback
            if let Some(cc) = claude_creds {
                if let Ok(result) = call_claude_chat(&cc, system_prompt, &messages).await {
                    return Ok(result);
                }
            }
            if let Some(bc) = bedrock_creds {
                let user_prompt = messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                if let Ok(result) = call_bedrock_generate(&bc, system_prompt, &user_prompt).await {
                    return Ok(result);
                }
            }
            Err(AppError::Serialization(
                "LLM unavailable. Ollama is not running and no cloud AI provider is configured. \
                 Please start Ollama (ollama serve) or configure Claude/Bedrock in Settings > AI/LLM."
                    .to_string(),
            ))
        }
    }
}

/// Determine which model to use based on settings and available models.
fn select_model(settings: &LlmSettings, available_models: &[String]) -> String {
    // If user configured a model and it's available, use it
    if let Some(ref configured) = settings.model {
        if available_models.is_empty() || available_models.iter().any(|m| m.starts_with(configured.as_str())) {
            return configured.clone();
        }
    }

    // Try default model
    if available_models.iter().any(|m| m.starts_with(DEFAULT_MODEL)) {
        return DEFAULT_MODEL.to_string();
    }

    // Try fallback model
    if available_models.iter().any(|m| m.starts_with(FALLBACK_MODEL)) {
        return FALLBACK_MODEL.to_string();
    }

    // If we have any models at all, use the first one
    if let Some(first) = available_models.first() {
        return first.clone();
    }

    // Default even if not confirmed available
    DEFAULT_MODEL.to_string()
}

/// Call Ollama /api/generate with the given parameters.
async fn call_ollama_generate(
    ollama_url: &str,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let request = OllamaGenerateRequest {
        model: model.to_string(),
        prompt: user_prompt.to_string(),
        system: system_prompt.to_string(),
        stream: false,
        options: OllamaOptions {
            temperature: 0.3,
            num_predict: 4096,
        },
    };

    let url = build_ollama_generate_url(ollama_url);

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            AppError::Serialization(format!(
                "Ollama request failed. Is Ollama running at {}? Error: {}",
                ollama_url, e
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Serialization(format!(
            "Ollama returned HTTP {}: {}",
            status, body
        )));
    }

    let gen_response: OllamaGenerateResponse = response.json().await.map_err(|e| {
        AppError::Serialization(format!("Failed to parse Ollama response: {}", e))
    })?;

    Ok(gen_response.response)
}

/// Bedrock credentials read synchronously from the database.
struct BedrockCredentials {
    access_key: String,
    secret_key: String,
    region: String,
}

/// Read Bedrock credentials from the app_settings table (synchronous).
fn read_bedrock_credentials(conn: &rusqlite::Connection) -> Result<BedrockCredentials, AppError> {
    let access_key = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_bedrock_access_key'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| {
            AppError::Validation(
                "AWS Bedrock access key not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;

    let secret_key = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_bedrock_secret_key'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| {
            AppError::Validation(
                "AWS Bedrock secret key not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;

    let region = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'llm_bedrock_region'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "us-east-1".to_string());

    Ok(BedrockCredentials {
        access_key,
        secret_key,
        region,
    })
}

/// Call AWS Bedrock Claude Haiku as a fallback when Ollama is unavailable.
///
/// Credentials are passed in directly (read from DB before calling this async fn)
/// to avoid holding a MutexGuard across an await point.
async fn call_bedrock_generate(
    credentials: &BedrockCredentials,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, String), AppError> {
    let model_id = "anthropic.claude-3-haiku-20240307-v1:0";

    // Build the Bedrock InvokeModel request body
    let body = serde_json::json!({
        "anthropic_version": "bedrock-2023-05-31",
        "max_tokens": 4096,
        "temperature": 0.3,
        "system": system_prompt,
        "messages": [
            {
                "role": "user",
                "content": user_prompt
            }
        ]
    });

    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| AppError::Serialization(format!("Failed to serialize request body: {}", e)))?;

    let host = format!("bedrock-runtime.{}.amazonaws.com", credentials.region);
    let uri = format!("/model/{}/invoke", model_id);
    let url = format!("https://{}{}", host, uri);

    // SigV4 signing
    let now = chrono::Utc::now();
    let date_stamp = now.format("%Y%m%d").to_string();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let payload_hash = hex::encode(Sha256::digest(&body_bytes));
    let canonical_headers = format!("content-type:application/json\nhost:{}\nx-amz-date:{}\n", host, amz_date);
    let signed_headers = "content-type;host;x-amz-date";
    let canonical_request = format!("POST\n{}\n\n{}\n{}\n{}", uri, canonical_headers, signed_headers, payload_hash);
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let scope = format!("{}/{}/bedrock/aws4_request", date_stamp, credentials.region);
    let sts = format!("AWS4-HMAC-SHA256\n{}\n{}\n{}", amz_date, scope, canonical_hash);

    let sign = |key: &[u8], data: &[u8]| -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    };
    let k_date = sign(format!("AWS4{}", credentials.secret_key).as_bytes(), date_stamp.as_bytes());
    let k_region = sign(&k_date, credentials.region.as_bytes());
    let k_service = sign(&k_region, b"bedrock");
    let k_signing = sign(&k_service, b"aws4_request");
    let signature = hex::encode(sign(&k_signing, sts.as_bytes()));
    let auth = format!("AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}", credentials.access_key, scope, signed_headers, signature);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Host", &host)
        .header("X-Amz-Date", &amz_date)
        .header("Authorization", &auth)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| {
            AppError::Serialization(format!("Bedrock request failed: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Serialization(format!(
            "Bedrock returned HTTP {}: {}",
            status, body
        )));
    }

    let resp_json: serde_json::Value = response.json().await.map_err(|e| {
        AppError::Serialization(format!("Failed to parse Bedrock response: {}", e))
    })?;

    let text = resp_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok((text, format!("bedrock:{}", model_id)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get the full LLM settings (with secrets masked).
///
/// Requires authentication.
#[tauri::command]
pub async fn get_llm_settings(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<FullLlmSettings, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = read_llm_settings(&conn);
    let claude_api_key = mask_secret(read_setting(&conn, "llm_claude_api_key"));
    let claude_model = read_setting(&conn, "llm_claude_model");
    let bedrock_access_key = mask_secret(read_setting(&conn, "llm_bedrock_access_key"));
    let bedrock_secret_key = mask_secret(read_setting(&conn, "llm_bedrock_secret_key"));
    let bedrock_region = read_setting(&conn, "llm_bedrock_region");
    let bedrock_model = read_setting(&conn, "llm_bedrock_model");

    Ok(FullLlmSettings {
        provider: settings.provider,
        model: settings.model,
        ollama_url: settings.ollama_url,
        claude_api_key,
        claude_model,
        bedrock_access_key,
        bedrock_secret_key,
        bedrock_region,
        bedrock_model,
    })
}

/// Check the Ollama service status and list available models.
///
/// This does NOT require authentication — it is a health check that the frontend
/// uses to display the Ollama connection indicator.
#[tauri::command]
pub async fn check_ollama_status(
    db: State<'_, Database>,
) -> Result<OllamaStatus, AppError> {
    let ollama_url = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let settings = read_llm_settings(&conn);
        settings.ollama_url.unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string())
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(OllamaStatus {
                available: false,
                models: vec![],
                error: Some(format!("Failed to create HTTP client: {}", e)),
            });
        }
    };

    let url = build_ollama_tags_url(&ollama_url);

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<OllamaTagsResponse>().await {
                    Ok(tags) => {
                        let models: Vec<String> = tags
                            .models
                            .unwrap_or_default()
                            .into_iter()
                            .map(|m| m.name)
                            .collect();
                        Ok(OllamaStatus {
                            available: true,
                            models,
                            error: None,
                        })
                    }
                    Err(e) => Ok(OllamaStatus {
                        available: true,
                        models: vec![],
                        error: Some(format!("Ollama responded but could not parse model list: {}", e)),
                    }),
                }
            } else {
                Ok(OllamaStatus {
                    available: false,
                    models: vec![],
                    error: Some(format!(
                        "Ollama returned HTTP {}. Is Ollama running at {}?",
                        response.status(),
                        ollama_url
                    )),
                })
            }
        }
        Err(e) => Ok(OllamaStatus {
            available: false,
            models: vec![],
            error: Some(format!(
                "Cannot reach Ollama at {}. Please ensure Ollama is installed and running. \
                 Install: https://ollama.ai — then run: ollama serve. Error: {}",
                ollama_url, e
            )),
        }),
    }
}

/// Generate a PT note draft from a session transcript.
///
/// Requires: ClinicalDocumentation + Create (only Providers and up can generate notes).
///
/// `note_type` should be "progress_note" or "initial_eval".
/// `patient_context` provides optional demographics, prior note summary, and diagnosis.
#[tauri::command]
pub async fn generate_note_draft(
    transcript: String,
    note_type: String,
    patient_context: Option<PatientContext>,
    _template_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<NoteDraftResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    // Validate inputs
    if transcript.trim().is_empty() {
        return Err(AppError::Validation(
            "Transcript cannot be empty".to_string(),
        ));
    }

    let valid_types = ["progress_note", "initial_eval"];
    if !valid_types.contains(&note_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid note_type '{}'. Must be one of: {}",
            note_type,
            valid_types.join(", ")
        )));
    }

    let patient_id_for_audit = patient_context
        .as_ref()
        .and_then(|ctx| ctx.patient_id.clone());

    let start = std::time::Instant::now();

    // Read settings, sample notes, and clinical context from DB (synchronous)
    let (settings, ollama_url, style_reference, clinical_context) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let url = s.ollama_url.clone().unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());

        // Load sample notes for style matching
        let samples = load_sample_notes(&conn, &note_type);
        let style_ref = build_style_reference(&samples);

        // Load patient clinical context if patient_id is provided
        let clin_ctx = patient_context
            .as_ref()
            .and_then(|ctx| ctx.patient_id.as_deref())
            .map(|pid| load_patient_clinical_context(&conn, pid))
            .unwrap_or_default();

        (s, url, style_ref, clin_ctx)
    };

    // Build system prompt with style reference appended
    let base_system_prompt = match note_type.as_str() {
        "progress_note" => PROGRESS_NOTE_SYSTEM_PROMPT,
        "initial_eval" => INITIAL_EVAL_SYSTEM_PROMPT,
        _ => unreachable!(),
    };
    let system_prompt = if style_reference.is_empty() {
        base_system_prompt.to_string()
    } else {
        format!("{}{}", base_system_prompt, style_reference)
    };

    // Select user prompt with clinical context
    let user_prompt = match note_type.as_str() {
        "progress_note" => build_progress_note_prompt(&transcript, &patient_context, &clinical_context),
        "initial_eval" => build_initial_eval_prompt(&transcript, &patient_context, &clinical_context),
        _ => unreachable!(),
    };

    // Pre-read credentials (synchronously) so we don't hold the DB lock across awaits.
    let (bedrock_creds, claude_creds) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        (
            read_bedrock_credentials(&conn).ok(),
            read_claude_credentials(&conn).ok(),
        )
    };

    // Route to the configured provider
    let (raw_response, model_used) = if settings.provider == "claude" {
        // User explicitly configured Claude API
        let creds = claude_creds.ok_or_else(|| {
            AppError::Validation(
                "Claude API key not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;
        call_claude_generate(&creds, &system_prompt, &user_prompt).await?
    } else if settings.provider == "bedrock" {
        // User explicitly configured Bedrock
        let creds = bedrock_creds.ok_or_else(|| {
            AppError::Validation(
                "AWS Bedrock credentials not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;
        call_bedrock_generate(&creds, &system_prompt, &user_prompt).await?
    } else {
        // Try Ollama (default)
        let status = {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
                .build()
                .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

            let url = build_ollama_tags_url(&ollama_url);
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let tags: OllamaTagsResponse = resp.json().await.unwrap_or(OllamaTagsResponse { models: None });
                    let models: Vec<String> = tags.models.unwrap_or_default().into_iter().map(|m| m.name).collect();
                    Some(models)
                }
                _ => None,
            }
        };

        match status {
            Some(available_models) => {
                let model = select_model(&settings, &available_models);
                let response =
                    call_ollama_generate(&ollama_url, &model, &system_prompt, &user_prompt)
                        .await?;
                (response, model)
            }
            None => {
                // Ollama unavailable — try Claude fallback, then Bedrock fallback
                if let Some(cc) = claude_creds {
                    if let Ok(result) = call_claude_generate(&cc, &system_prompt, &user_prompt).await {
                        return {
                            let elapsed_ms = start.elapsed().as_millis() as u64;
                            let (fields, confidence) = parse_note_draft_response(&result.0)?;
                            let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
                            write_audit_entry(&conn, AuditEntryInput {
                                user_id: sess.user_id.clone(),
                                action: format!("llm.generate_note.{}", note_type),
                                resource_type: "LlmNoteDraft".to_string(),
                                resource_id: None,
                                patient_id: patient_id_for_audit.clone(),
                                device_id: device_id.id().to_string(),
                                success: true,
                                details: Some(format!("model={}, time_ms={}", result.1, elapsed_ms)),
                            })?;
                            Ok(NoteDraftResult { note_type, fields, confidence, model_used: result.1, generation_time_ms: elapsed_ms })
                        };
                    }
                }
                match bedrock_creds {
                    Some(creds) => {
                        match call_bedrock_generate(&creds, &system_prompt, &user_prompt).await {
                            Ok(result) => result,
                            Err(_bedrock_err) => {
                                return Err(AppError::Serialization(
                                    "LLM unavailable. Ollama is not running and cloud AI requests failed. \
                                     Please start Ollama (ollama serve) or check your AI configuration."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                    None => {
                        return Err(AppError::Serialization(
                            "LLM unavailable. Ollama is not running and no cloud AI provider is configured. \
                             Please start Ollama (ollama serve) or configure Claude/Bedrock in Settings > AI/LLM."
                                .to_string(),
                        ));
                    }
                }
            }
        }
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Parse the response
    let (fields, confidence) = parse_note_draft_response(&raw_response)?;

    // Audit log — do NOT log transcript text, only action + patient_id
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;

        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: format!("llm.generate_note.{}", note_type),
                resource_type: "LlmNoteDraft".to_string(),
                resource_id: None,
                patient_id: patient_id_for_audit.clone(),
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!("model={}, time_ms={}", model_used, elapsed_ms)),
            },
        )?;
    }

    Ok(NoteDraftResult {
        note_type,
        fields,
        confidence,
        model_used,
        generation_time_ms: elapsed_ms,
    })
}

/// Suggest CPT codes based on a note's text content.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn suggest_cpt_codes(
    note_text: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<CptSuggestion>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    if note_text.trim().is_empty() {
        return Err(AppError::Validation(
            "Note text cannot be empty".to_string(),
        ));
    }

    let (settings, ollama_url) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let url = s.ollama_url.clone().unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        (s, url)
    };

    // Try Ollama
    let raw_response = {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

        let url = build_ollama_tags_url(&ollama_url);
        let available_models = match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let tags: OllamaTagsResponse = resp.json().await.unwrap_or(OllamaTagsResponse { models: None });
                tags.models.unwrap_or_default().into_iter().map(|m| m.name).collect::<Vec<_>>()
            }
            _ => vec![],
        };

        let model = select_model(&settings, &available_models);
        call_ollama_generate(
            &ollama_url,
            &model,
            CPT_SUGGESTION_SYSTEM_PROMPT,
            &format!("PT Note:\n{}", note_text),
        )
        .await?
    };

    let suggestions = parse_cpt_suggestions(&raw_response)?;

    // Audit log
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "llm.suggest_cpt_codes".to_string(),
                resource_type: "LlmCptSuggestion".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.id().to_string(),
                success: true,
                details: Some(format!("suggestions_count={}", suggestions.len())),
            },
        )?;
    }

    Ok(suggestions)
}

/// Extract objective data (ROM, pain, MMT) from a session transcript.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn extract_objective_data(
    transcript: String,
    patient_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ExtractedObjectiveData, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    if transcript.trim().is_empty() {
        return Err(AppError::Validation(
            "Transcript cannot be empty".to_string(),
        ));
    }

    let (settings, ollama_url) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let url = s.ollama_url.clone().unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        (s, url)
    };

    let raw_response = {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Serialization(format!("HTTP client error: {}", e)))?;

        let url = build_ollama_tags_url(&ollama_url);
        let available_models = match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let tags: OllamaTagsResponse = resp.json().await.unwrap_or(OllamaTagsResponse { models: None });
                tags.models.unwrap_or_default().into_iter().map(|m| m.name).collect::<Vec<_>>()
            }
            _ => vec![],
        };

        let model = select_model(&settings, &available_models);
        call_ollama_generate(
            &ollama_url,
            &model,
            EXTRACT_OBJECTIVE_SYSTEM_PROMPT,
            &format!("Session transcript:\n{}", transcript),
        )
        .await?
    };

    let data = parse_extracted_objective_data(&raw_response)?;

    // Audit log — transcript is ePHI, do NOT log it
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: sess.user_id.clone(),
                action: "llm.extract_objective_data".to_string(),
                resource_type: "LlmObjectiveExtraction".to_string(),
                resource_id: None,
                patient_id: patient_id.clone(),
                device_id: device_id.id().to_string(),
                success: true,
                details: None,
            },
        )?;
    }

    Ok(data)
}

/// Configure LLM provider, model, and credentials.
///
/// Requires: SystemAdmin (only admins can change LLM settings).
#[tauri::command]
pub async fn configure_llm_settings(
    input: LlmSettingsInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<LlmSettings, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::UserManagement, Action::Update)?;

    // Validate provider
    let valid_providers = ["ollama", "claude", "bedrock"];
    if !valid_providers.contains(&input.provider.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid provider '{}'. Must be one of: 'ollama', 'claude', or 'bedrock'.",
            input.provider
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Save provider
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_provider', ?1)",
        rusqlite::params![input.provider],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(ref model) = input.model {
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_model', ?1)",
            rusqlite::params![model],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    if let Some(ref url) = input.ollama_url {
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_ollama_url', ?1)",
            rusqlite::params![url],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    if input.provider == "claude" {
        // Route credentials to Claude-specific keys
        // Skip empty strings — empty means "keep existing value"
        if let Some(ref key) = input.api_key {
            if !key.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_claude_api_key', ?1)",
                    rusqlite::params![key],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
        // Save Claude model from the dedicated claude_model field
        if let Some(ref model) = input.claude_model {
            if !model.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_claude_model', ?1)",
                    rusqlite::params![model],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
    } else if input.provider == "bedrock" {
        // Route credentials to Bedrock-specific keys
        // Skip empty strings — empty means "keep existing value"
        if let Some(ref key) = input.api_key {
            if !key.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_access_key', ?1)",
                    rusqlite::params![key],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
        if let Some(ref secret) = input.api_secret {
            if !secret.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_secret_key', ?1)",
                    rusqlite::params![secret],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
        // Save Bedrock region
        if let Some(ref region) = input.bedrock_region {
            if !region.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_region', ?1)",
                    rusqlite::params![region],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
        // Save Bedrock model override
        if let Some(ref model) = input.bedrock_model {
            if !model.trim().is_empty() {
                conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_model', ?1)",
                    rusqlite::params![model],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
    }

    // Audit log
    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "llm.configure_settings".to_string(),
            resource_type: "LlmSettings".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "provider={}, model={}",
                input.provider,
                input.model.as_deref().unwrap_or("default")
            )),
        },
    )?;

    Ok(read_llm_settings(&conn))
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: Ollama URL construction ──────────────────────────────────

    #[test]
    fn test_build_ollama_tags_url_default() {
        let url = build_ollama_tags_url(DEFAULT_OLLAMA_URL);
        assert_eq!(url, "http://localhost:11434/api/tags");
    }

    #[test]
    fn test_build_ollama_tags_url_custom() {
        let url = build_ollama_tags_url("http://192.168.1.100:11434");
        assert_eq!(url, "http://192.168.1.100:11434/api/tags");
    }

    #[test]
    fn test_build_ollama_tags_url_trailing_slash() {
        let url = build_ollama_tags_url("http://localhost:11434/");
        assert_eq!(url, "http://localhost:11434/api/tags");
    }

    #[test]
    fn test_build_ollama_generate_url() {
        let url = build_ollama_generate_url(DEFAULT_OLLAMA_URL);
        assert_eq!(url, "http://localhost:11434/api/generate");
    }

    // ── Test 2: Prompt template formatting ──────────────────────────────

    #[test]
    fn test_progress_note_prompt_with_full_context() {
        let ctx = Some(PatientContext {
            patient_id: Some("pat-123".to_string()),
            demographics: Some("65yo male".to_string()),
            prior_note_summary: Some("Improving ROM in R shoulder".to_string()),
            diagnosis: Some("R shoulder impingement".to_string()),
        });

        let prompt = build_progress_note_prompt("Patient reports pain at 4/10.", &ctx, "");

        assert!(prompt.contains("Patient demographics: 65yo male"));
        assert!(prompt.contains("Diagnosis: R shoulder impingement"));
        assert!(prompt.contains("Prior note summary: Improving ROM in R shoulder"));
        assert!(prompt.contains("Session transcript:"));
        assert!(prompt.contains("Patient reports pain at 4/10."));
    }

    #[test]
    fn test_progress_note_prompt_without_context() {
        let prompt = build_progress_note_prompt("Patient doing well.", &None, "");

        assert!(!prompt.contains("Patient demographics:"));
        assert!(prompt.contains("Session transcript:"));
        assert!(prompt.contains("Patient doing well."));
    }

    #[test]
    fn test_initial_eval_prompt_with_context() {
        let ctx = Some(PatientContext {
            patient_id: Some("pat-456".to_string()),
            demographics: Some("42yo female".to_string()),
            prior_note_summary: None,
            diagnosis: Some("Low back pain".to_string()),
        });

        let prompt = build_initial_eval_prompt("New patient with LBP x 3 months.", &ctx, "");

        assert!(prompt.contains("Patient demographics: 42yo female"));
        assert!(prompt.contains("Referral diagnosis: Low back pain"));
        assert!(prompt.contains("Initial evaluation transcript:"));
        assert!(prompt.contains("New patient with LBP x 3 months."));
    }

    #[test]
    fn test_system_prompts_contain_json_structure() {
        // Verify progress note prompt requests JSON with correct top-level keys
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"subjective\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"objective\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"assessment\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"plan\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"pain_nrs\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"cpt_code\""));
        assert!(PROGRESS_NOTE_SYSTEM_PROMPT.contains("\"confidence\""));

        // Verify initial eval prompt
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"history_of_present_illness\""));
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"rom_measurements\""));
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"mmt_grades\""));
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"eval_complexity\""));
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"short_term_goals\""));
        assert!(INITIAL_EVAL_SYSTEM_PROMPT.contains("\"long_term_goals\""));
    }

    // ── Test 3: CPT code suggestion parsing ─────────────────────────────

    #[test]
    fn test_parse_cpt_suggestions_valid() {
        let response = r#"[
            {"code": "97110", "description": "Therapeutic exercise", "minutes": 15, "confidence": "high"},
            {"code": "97140", "description": "Manual therapy", "minutes": 15, "confidence": "medium"}
        ]"#;

        let suggestions = parse_cpt_suggestions(response).unwrap();
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].code, "97110");
        assert_eq!(suggestions[0].description, "Therapeutic exercise");
        assert_eq!(suggestions[0].minutes, 15);
        assert_eq!(suggestions[0].confidence, "high");
        assert_eq!(suggestions[1].code, "97140");
        assert_eq!(suggestions[1].confidence, "medium");
    }

    #[test]
    fn test_parse_cpt_suggestions_with_code_fences() {
        let response = "```json\n[\n  {\"code\": \"97110\", \"description\": \"Therapeutic exercise\", \"minutes\": 15, \"confidence\": \"high\"}\n]\n```";

        let suggestions = parse_cpt_suggestions(response).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].code, "97110");
    }

    #[test]
    fn test_parse_cpt_suggestions_invalid_json() {
        let response = "This is not JSON";
        let result = parse_cpt_suggestions(response);
        assert!(result.is_err());
    }

    // ── Test 4: Confidence level / note draft parsing ────────────────────

    #[test]
    fn test_parse_note_draft_response_with_confidence() {
        let response = r#"{
            "fields": {
                "subjective": {
                    "patient_report": "Patient reports improvement",
                    "pain_nrs": 4,
                    "hep_compliance": "yes",
                    "barriers": null
                },
                "objective": {
                    "treatments": [{"cpt_code": "97110", "minutes": 15}],
                    "exercises": "Shoulder flexion, IR/ER"
                },
                "assessment": {
                    "progress_status": "progressing",
                    "narrative": "Good progress with ROM improvements"
                },
                "plan": {
                    "next_session": "Continue current program",
                    "hep_updates": "Add wall slides"
                }
            },
            "confidence": {
                "subjective.patient_report": "high",
                "subjective.pain_nrs": "high",
                "subjective.hep_compliance": "medium",
                "subjective.barriers": "low",
                "objective.treatments": "high",
                "objective.exercises": "high",
                "assessment.progress_status": "medium",
                "assessment.narrative": "medium",
                "plan.next_session": "medium",
                "plan.hep_updates": "low"
            }
        }"#;

        let (fields, confidence) = parse_note_draft_response(response).unwrap();

        // Verify fields structure
        assert!(fields.get("subjective").is_some());
        assert!(fields.get("objective").is_some());
        assert!(fields.get("assessment").is_some());
        assert!(fields.get("plan").is_some());

        let pain = fields["subjective"]["pain_nrs"].as_u64();
        assert_eq!(pain, Some(4));

        // Verify confidence levels
        assert_eq!(confidence.get("subjective.patient_report").unwrap(), "high");
        assert_eq!(confidence.get("subjective.hep_compliance").unwrap(), "medium");
        assert_eq!(confidence.get("subjective.barriers").unwrap(), "low");
        assert_eq!(confidence.get("plan.hep_updates").unwrap(), "low");
    }

    #[test]
    fn test_parse_note_draft_response_without_confidence() {
        let response = r#"{
            "fields": {"subjective": {"patient_report": "Doing well"}}
        }"#;

        let (fields, confidence) = parse_note_draft_response(response).unwrap();
        assert!(fields.get("subjective").is_some());
        assert!(confidence.is_empty());
    }

    #[test]
    fn test_parse_note_draft_bare_json_no_wrapper() {
        // When LLM returns just the fields without the wrapper
        let response = r#"{"subjective": {"patient_report": "Pain improving"}}"#;

        let (fields, confidence) = parse_note_draft_response(response).unwrap();
        assert!(fields.get("subjective").is_some());
        assert!(confidence.is_empty());
    }

    // ── Test 5: Extracted objective data parsing ─────────────────────────

    #[test]
    fn test_parse_extracted_objective_data() {
        let response = r#"{
            "rom_values": {"shoulder_flexion": "160 degrees", "knee_extension": "0 degrees"},
            "pain_scores": {"current": 4, "with_activity": 6},
            "mmt_grades": {"deltoid": "4/5", "quadriceps": "5/5"}
        }"#;

        let data = parse_extracted_objective_data(response).unwrap();

        let rom = data.rom_values.unwrap();
        assert_eq!(rom.len(), 2);
        assert!(rom.contains_key("shoulder_flexion"));

        let pain = data.pain_scores.unwrap();
        assert_eq!(pain.len(), 2);

        let mmt = data.mmt_grades.unwrap();
        assert_eq!(mmt.get("deltoid").unwrap(), "4/5");
    }

    #[test]
    fn test_parse_extracted_objective_empty_categories() {
        let response = r#"{
            "rom_values": {},
            "pain_scores": {},
            "mmt_grades": {}
        }"#;

        let data = parse_extracted_objective_data(response).unwrap();
        assert!(data.rom_values.unwrap().is_empty());
        assert!(data.pain_scores.unwrap().is_empty());
        assert!(data.mmt_grades.unwrap().is_empty());
    }

    // ── Test 6: Code fence stripping ─────────────────────────────────────

    #[test]
    fn test_strip_code_fences_json() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_code_fences(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_strip_code_fences_plain() {
        let input = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_code_fences(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_strip_code_fences_none() {
        let input = "{\"key\": \"value\"}";
        assert_eq!(strip_code_fences(input), "{\"key\": \"value\"}");
    }

    // ── Test 7: Model selection logic ────────────────────────────────────

    #[test]
    fn test_select_model_configured_available() {
        let settings = LlmSettings {
            provider: "ollama".to_string(),
            model: Some("mistral:7b".to_string()),
            ollama_url: None,
        };
        let available = vec!["mistral:7b".to_string(), "llama3.1:8b".to_string()];
        assert_eq!(select_model(&settings, &available), "mistral:7b");
    }

    #[test]
    fn test_select_model_default_when_no_config() {
        let settings = LlmSettings {
            provider: "ollama".to_string(),
            model: None,
            ollama_url: None,
        };
        let available = vec![
            "llama3.1:8b".to_string(),
            "phi3:mini".to_string(),
        ];
        assert_eq!(select_model(&settings, &available), DEFAULT_MODEL);
    }

    #[test]
    fn test_select_model_fallback_when_default_unavailable() {
        let settings = LlmSettings {
            provider: "ollama".to_string(),
            model: None,
            ollama_url: None,
        };
        let available = vec!["phi3:mini".to_string()];
        assert_eq!(select_model(&settings, &available), FALLBACK_MODEL);
    }

    #[test]
    fn test_select_model_first_available_when_neither_default() {
        let settings = LlmSettings {
            provider: "ollama".to_string(),
            model: None,
            ollama_url: None,
        };
        let available = vec!["codellama:7b".to_string()];
        assert_eq!(select_model(&settings, &available), "codellama:7b");
    }

    #[test]
    fn test_select_model_returns_default_when_empty_list() {
        let settings = LlmSettings {
            provider: "ollama".to_string(),
            model: None,
            ollama_url: None,
        };
        let available: Vec<String> = vec![];
        assert_eq!(select_model(&settings, &available), DEFAULT_MODEL);
    }
}
