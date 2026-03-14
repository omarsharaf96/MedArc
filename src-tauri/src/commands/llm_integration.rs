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
    /// Provider: "ollama" or "bedrock".
    pub provider: String,
    /// Model name override (e.g. "llama3.1:8b").
    pub model: Option<String>,
    /// Custom Ollama URL (default: http://localhost:11434).
    pub ollama_url: Option<String>,
    /// AWS access key for Bedrock (stored encrypted in SQLCipher).
    pub api_key: Option<String>,
    /// AWS secret key for Bedrock (stored encrypted in SQLCipher).
    pub api_secret: Option<String>,
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
pub fn build_progress_note_prompt(transcript: &str, patient_context: &Option<PatientContext>) -> String {
    let mut prompt = String::new();

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
pub fn build_initial_eval_prompt(transcript: &str, patient_context: &Option<PatientContext>) -> String {
    let mut prompt = String::new();

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
/// This function extracts them, handling potential markdown code fences.
pub fn parse_note_draft_response(
    raw_response: &str,
) -> Result<(serde_json::Value, HashMap<String, String>), AppError> {
    // Strip markdown code fences if present
    let cleaned = strip_code_fences(raw_response);

    let parsed: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|e| AppError::Serialization(format!("Failed to parse LLM response as JSON: {}. Raw response starts with: {}", e, &raw_response[..raw_response.len().min(200)])))?;

    let fields = parsed
        .get("fields")
        .cloned()
        .unwrap_or(parsed.clone());

    let confidence: HashMap<String, String> = if let Some(conf) = parsed.get("confidence") {
        serde_json::from_value(conf.clone()).unwrap_or_default()
    } else {
        HashMap::new()
    };

    Ok((fields, confidence))
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

    let url = format!(
        "https://bedrock-runtime.{}.amazonaws.com/model/{}/invoke",
        credentials.region, model_id
    );

    // Note: In production, this should use proper AWS SigV4 signing.
    // For now, we construct a basic request. Full SigV4 would require
    // additional dependencies (aws-sigv4, aws-credential-types).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(GENERATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Serialization(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Amz-Access-Key", &credentials.access_key)
        .header("X-Amz-Secret-Key", &credentials.secret_key)
        .json(&body)
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

    // Read settings and determine provider
    let (settings, ollama_url) = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let s = read_llm_settings(&conn);
        let url = s.ollama_url.clone().unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        (s, url)
    };

    // Select system prompt and build user prompt
    let (system_prompt, user_prompt) = match note_type.as_str() {
        "progress_note" => (
            PROGRESS_NOTE_SYSTEM_PROMPT,
            build_progress_note_prompt(&transcript, &patient_context),
        ),
        "initial_eval" => (
            INITIAL_EVAL_SYSTEM_PROMPT,
            build_initial_eval_prompt(&transcript, &patient_context),
        ),
        _ => unreachable!(), // Already validated above
    };

    // Pre-read Bedrock credentials (synchronously) so we don't hold the DB lock across awaits.
    // This is a no-op if Bedrock isn't configured — we just get an Err we can handle later.
    let bedrock_creds = {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        read_bedrock_credentials(&conn).ok()
    };

    // Try Ollama first, then Bedrock fallback
    let (raw_response, model_used) = if settings.provider == "bedrock" {
        // User explicitly configured Bedrock
        let creds = bedrock_creds.ok_or_else(|| {
            AppError::Validation(
                "AWS Bedrock credentials not configured. Go to Settings > AI/LLM to configure.".to_string(),
            )
        })?;
        call_bedrock_generate(&creds, system_prompt, &user_prompt).await?
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
                    call_ollama_generate(&ollama_url, &model, system_prompt, &user_prompt)
                        .await?;
                (response, model)
            }
            None => {
                // Ollama unavailable — try Bedrock fallback
                match bedrock_creds {
                    Some(creds) => {
                        match call_bedrock_generate(&creds, system_prompt, &user_prompt).await {
                            Ok(result) => result,
                            Err(_bedrock_err) => {
                                return Err(AppError::Serialization(
                                    "LLM unavailable. Ollama is not running and AWS Bedrock request failed. \
                                     Please start Ollama (ollama serve) or check your Bedrock configuration."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                    None => {
                        return Err(AppError::Serialization(
                            "LLM unavailable. Ollama is not running and AWS Bedrock is not configured. \
                             Please start Ollama (ollama serve) or configure Bedrock in Settings > AI/LLM."
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
    if input.provider != "ollama" && input.provider != "bedrock" {
        return Err(AppError::Validation(format!(
            "Invalid provider '{}'. Must be 'ollama' or 'bedrock'.",
            input.provider
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Upsert settings
    let settings_pairs: Vec<(&str, &str)> = {
        let mut pairs = vec![("llm_provider", input.provider.as_str())];
        if input.model.is_some() {
            pairs.push(("llm_model", ""));
        }
        if input.ollama_url.is_some() {
            pairs.push(("llm_ollama_url", ""));
        }
        if input.api_key.is_some() {
            pairs.push(("llm_bedrock_access_key", ""));
        }
        if input.api_secret.is_some() {
            pairs.push(("llm_bedrock_secret_key", ""));
        }
        pairs
    };

    // We need to handle the actual values properly
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

    if let Some(ref key) = input.api_key {
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_access_key', ?1)",
            rusqlite::params![key],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    if let Some(ref secret) = input.api_secret {
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('llm_bedrock_secret_key', ?1)",
            rusqlite::params![secret],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
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

    // Suppress unused variable warning — settings_pairs was used for documentation/planning
    let _ = settings_pairs;

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

        let prompt = build_progress_note_prompt("Patient reports pain at 4/10.", &ctx);

        assert!(prompt.contains("Patient demographics: 65yo male"));
        assert!(prompt.contains("Diagnosis: R shoulder impingement"));
        assert!(prompt.contains("Prior note summary: Improving ROM in R shoulder"));
        assert!(prompt.contains("Session transcript:"));
        assert!(prompt.contains("Patient reports pain at 4/10."));
    }

    #[test]
    fn test_progress_note_prompt_without_context() {
        let prompt = build_progress_note_prompt("Patient doing well.", &None);

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

        let prompt = build_initial_eval_prompt("New patient with LBP x 3 months.", &ctx);

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
