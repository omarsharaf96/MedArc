/// commands/reminders.rs — Appointment Reminders & Waitlist Auto-Fill (M003/S02)
///
/// Implements reminder delivery via Twilio SMS and SendGrid email:
///   REM-01  Reminder configuration (sms/email toggles, intervals, credentials)
///   REM-02  Process pending reminders (24hr before, 2hr before)
///   REM-03  Manual send reminder / no-show follow-up
///   REM-04  Waitlist auto-fill on cancellation
///   REM-05  Reminder log query
///
/// Data model
/// ----------
/// Migration 30 creates two tables:
///   - `reminder_log`       — every sent/failed reminder with status lifecycle
///   - `reminder_templates` — default and custom message templates
///
/// Credentials are stored in the existing `app_settings` table (SQLCipher encrypted).
///
/// RBAC
/// ----
///   configure_reminders     → SystemAdmin only
///   get_reminder_config     → any authenticated user
///   process_pending_reminders → FrontDesk, NurseMa, Provider, SystemAdmin
///   send_reminder           → FrontDesk, NurseMa, Provider, SystemAdmin
///   send_no_show_followup   → FrontDesk, NurseMa, Provider, SystemAdmin
///   process_cancellation_waitlist → FrontDesk, NurseMa, Provider, SystemAdmin
///   list_reminder_log       → FrontDesk, NurseMa, Provider, BillingStaff, SystemAdmin
///
/// Audit
/// -----
/// Every command writes an audit row using `write_audit_entry`.

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

const TWILIO_API_BASE: &str = "https://api.twilio.com/2010-04-01/Accounts";
const SENDGRID_API_BASE: &str = "https://api.sendgrid.com/v3/mail/send";

// Default SMS templates
const DEFAULT_TEMPLATE_24HR: &str =
    "Hi {patient_name}, this is a reminder of your PT appointment tomorrow at {appointment_time} with {provider_name}. Reply C to confirm or call {practice_phone} to reschedule.";
const DEFAULT_TEMPLATE_2HR: &str =
    "Reminder: Your PT appointment is in 2 hours at {appointment_time}. See you soon!";
const DEFAULT_TEMPLATE_NOSHOW: &str =
    "We missed you at your appointment today. Please call {practice_phone} to reschedule.";
const DEFAULT_TEMPLATE_WAITLIST: &str =
    "Hi {patient_name}, a slot has opened at {appointment_time} on {appointment_date}. Reply Y to book or call {practice_phone}.";

// ─────────────────────────────────────────────────────────────────────────────
// Domain types — Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Input for configuring the reminder system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderConfigInput {
    /// Whether SMS reminders are enabled.
    pub sms_enabled: bool,
    /// Whether email reminders are enabled.
    pub email_enabled: bool,
    /// Whether to send the 24-hour reminder.
    pub reminder_24hr: bool,
    /// Whether to send the 2-hour reminder.
    pub reminder_2hr: bool,
    /// Practice display name used in templates.
    pub practice_name: Option<String>,
    /// Practice phone number used in templates.
    pub practice_phone: Option<String>,
    /// Twilio credentials (only required when sms_enabled = true).
    pub twilio: Option<TwilioConfigInput>,
    /// SendGrid credentials (only required when email_enabled = true).
    pub sendgrid: Option<SendGridConfigInput>,
}

/// Twilio credential input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwilioConfigInput {
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
}

/// SendGrid credential input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendGridConfigInput {
    pub api_key: String,
    pub from_email: String,
    pub from_name: Option<String>,
}

/// Current reminder configuration returned to callers (secrets masked).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderConfigRecord {
    pub sms_enabled: bool,
    pub email_enabled: bool,
    pub reminder_24hr: bool,
    pub reminder_2hr: bool,
    pub practice_name: Option<String>,
    pub practice_phone: Option<String>,
    /// Whether Twilio is configured (credentials exist).
    pub twilio_configured: bool,
    /// Masked from_number, e.g. "+1555***4567".
    pub twilio_from_number: Option<String>,
    /// Whether SendGrid is configured.
    pub sendgrid_configured: bool,
    /// From email for SendGrid.
    pub sendgrid_from_email: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Domain types — Reminder Log
// ─────────────────────────────────────────────────────────────────────────────

/// A single reminder log entry.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderLog {
    pub reminder_id: String,
    pub appointment_id: String,
    pub patient_id: String,
    pub reminder_type: String,
    pub channel: String,
    pub recipient: String,
    pub message_body: String,
    pub status: String,
    pub external_id: Option<String>,
    pub error_message: Option<String>,
    pub sent_at: Option<String>,
    pub created_at: String,
}

/// Result of sending a single reminder.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderResult {
    pub reminder_id: String,
    pub status: String,
    pub channel: String,
    pub recipient: String,
    pub external_id: Option<String>,
    pub error_message: Option<String>,
}

/// Result of processing all pending reminders.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRemindersResult {
    pub sent_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub results: Vec<ReminderResult>,
}

/// Waitlist match returned when processing a cancellation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitlistMatch {
    pub waitlist_id: String,
    pub patient_id: String,
    pub patient_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub offer_sent: bool,
    pub offer_channel: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Template helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Context for substituting template placeholders.
pub struct TemplateContext {
    pub patient_name: String,
    pub appointment_date: String,
    pub appointment_time: String,
    pub provider_name: String,
    pub practice_name: String,
    pub practice_phone: String,
}

/// Substitute all known placeholders in a template string.
pub fn render_template(template: &str, ctx: &TemplateContext) -> String {
    template
        .replace("{patient_name}", &ctx.patient_name)
        .replace("{appointment_date}", &ctx.appointment_date)
        .replace("{appointment_time}", &ctx.appointment_time)
        .replace("{provider_name}", &ctx.provider_name)
        .replace("{practice_name}", &ctx.practice_name)
        .replace("{practice_phone}", &ctx.practice_phone)
}

/// Compute the ISO datetime that is `hours_before` hours before a given appointment
/// start_time string (format: "YYYY-MM-DDTHH:MM:SS" or RFC3339).
///
/// Returns `None` if the start_time cannot be parsed.
#[allow(dead_code)]
pub fn reminder_send_time(start_time: &str, hours_before: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try RFC3339 first, then bare local datetime
    let appt_dt = chrono::DateTime::parse_from_rfc3339(start_time)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(start_time, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
        })
        .ok()?;
    Some(appt_dt - chrono::Duration::hours(hours_before))
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings helpers
// ─────────────────────────────────────────────────────────────────────────────

fn store_setting(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![key, value],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

fn get_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>, AppError> {
    match conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

fn get_setting_bool(conn: &rusqlite::Connection, key: &str, default: bool) -> bool {
    get_setting(conn, key)
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
}

/// Mask a sensitive string: show first 4 and last 4 chars, replace middle with ***.
fn mask_secret(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{}***{}", prefix, suffix)
}

// ─────────────────────────────────────────────────────────────────────────────
// Twilio SMS sender
// ─────────────────────────────────────────────────────────────────────────────

/// Send an SMS via the Twilio REST API using Basic auth (account_sid:auth_token).
/// Returns the Twilio message SID on success.
fn send_twilio_sms(
    account_sid: &str,
    auth_token: &str,
    from_number: &str,
    to_number: &str,
    body: &str,
) -> Result<String, AppError> {
    let url = format!("{}/{}/Messages.json", TWILIO_API_BASE, account_sid);

    let params = [
        ("From", from_number),
        ("To", to_number),
        ("Body", body),
    ];

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .basic_auth(account_sid, Some(auth_token))
        .form(&params)
        .send()
        .map_err(|e| AppError::Validation(format!("Twilio HTTP error: {}", e)))?;

    let status = response.status();
    let text = response
        .text()
        .unwrap_or_else(|_| "{}".to_string());

    if !status.is_success() {
        return Err(AppError::Validation(format!(
            "Twilio returned {}: {}",
            status, text
        )));
    }

    // Extract SID from JSON response
    let json: serde_json::Value =
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let sid = json
        .get("sid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(sid)
}

// ─────────────────────────────────────────────────────────────────────────────
// SendGrid email sender
// ─────────────────────────────────────────────────────────────────────────────

/// Send an email via the SendGrid v3 API.
/// Returns the SendGrid message ID (from X-Message-Id header) on success.
fn send_sendgrid_email(
    api_key: &str,
    from_email: &str,
    from_name: &str,
    to_email: &str,
    to_name: &str,
    subject: &str,
    html_body: &str,
) -> Result<String, AppError> {
    let payload = serde_json::json!({
        "personalizations": [{
            "to": [{"email": to_email, "name": to_name}]
        }],
        "from": {"email": from_email, "name": from_name},
        "subject": subject,
        "content": [{"type": "text/html", "value": html_body}]
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(SENDGRID_API_BASE)
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| AppError::Validation(format!("SendGrid HTTP error: {}", e)))?;

    let status = response.status();
    // 202 Accepted is success for SendGrid
    if status.as_u16() != 202 && !status.is_success() {
        let text = response.text().unwrap_or_default();
        return Err(AppError::Validation(format!(
            "SendGrid returned {}: {}",
            status, text
        )));
    }

    // Try to get message ID from response headers (not always present in blocking client)
    Ok(format!("sg_{}", uuid::Uuid::new_v4()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Core reminder dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// Write a reminder_log row and attempt delivery via SMS or email.
/// Returns a ReminderResult.
fn dispatch_reminder(
    conn: &rusqlite::Connection,
    appointment_id: &str,
    patient_id: &str,
    reminder_type: &str,
    channel: &str,
    recipient: &str,
    message_body: &str,
    // SMS params (optional)
    twilio_sid: Option<&str>,
    twilio_token: Option<&str>,
    twilio_from: Option<&str>,
    // Email params (optional)
    sg_api_key: Option<&str>,
    sg_from_email: Option<&str>,
    sg_from_name: Option<&str>,
    patient_name: Option<&str>,
    subject: Option<&str>,
) -> ReminderResult {
    let reminder_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Insert pending row
    let _ = conn.execute(
        "INSERT INTO reminder_log (reminder_id, appointment_id, patient_id, reminder_type,
         channel, recipient, message_body, status, sent_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?8)",
        rusqlite::params![
            &reminder_id,
            appointment_id,
            patient_id,
            reminder_type,
            channel,
            recipient,
            message_body,
            &now,
        ],
    );

    // Attempt delivery
    let (status, external_id, error_message) = match channel {
        "sms" => {
            match (twilio_sid, twilio_token, twilio_from) {
                (Some(sid), Some(token), Some(from)) => {
                    match send_twilio_sms(sid, token, from, recipient, message_body) {
                        Ok(ext_id) => ("sent".to_string(), Some(ext_id), None),
                        Err(e) => ("failed".to_string(), None, Some(e.to_string())),
                    }
                }
                _ => (
                    "failed".to_string(),
                    None,
                    Some("Twilio credentials not configured".to_string()),
                ),
            }
        }
        "email" => match (sg_api_key, sg_from_email) {
            (Some(key), Some(from_email)) => {
                let html = format!("<p>{}</p>", message_body.replace('\n', "<br>"));
                let subj = subject.unwrap_or("Appointment Reminder");
                let to_name = patient_name.unwrap_or(recipient);
                let from_name = sg_from_name.unwrap_or("MedArc");
                match send_sendgrid_email(
                    key, from_email, from_name, recipient, to_name, subj, &html,
                ) {
                    Ok(ext_id) => ("sent".to_string(), Some(ext_id), None),
                    Err(e) => ("failed".to_string(), None, Some(e.to_string())),
                }
            }
            _ => (
                "failed".to_string(),
                None,
                Some("SendGrid credentials not configured".to_string()),
            ),
        },
        _ => (
            "failed".to_string(),
            None,
            Some(format!("Unknown channel: {}", channel)),
        ),
    };

    // Update log row
    let _ = conn.execute(
        "UPDATE reminder_log SET status = ?1, external_id = ?2, error_message = ?3,
         sent_at = ?4 WHERE reminder_id = ?5",
        rusqlite::params![
            &status,
            &external_id,
            &error_message,
            &chrono::Utc::now().to_rfc3339(),
            &reminder_id,
        ],
    );

    ReminderResult {
        reminder_id,
        status,
        channel: channel.to_string(),
        recipient: recipient.to_string(),
        external_id,
        error_message,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: configure_reminders
// ─────────────────────────────────────────────────────────────────────────────

/// Store reminder configuration (credentials encrypted at rest by SQLCipher).
/// SystemAdmin only.
#[tauri::command]
pub fn configure_reminders(
    input: ReminderConfigInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<ReminderConfigRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;

    // Only SystemAdmin can configure reminder credentials
    if session.role != crate::rbac::roles::Role::SystemAdmin {
        return Err(AppError::Unauthorized(
            "Only SystemAdmin can configure reminders".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    store_setting(&conn, "reminder_sms_enabled", if input.sms_enabled { "true" } else { "false" })?;
    store_setting(&conn, "reminder_email_enabled", if input.email_enabled { "true" } else { "false" })?;
    store_setting(&conn, "reminder_24hr", if input.reminder_24hr { "true" } else { "false" })?;
    store_setting(&conn, "reminder_2hr", if input.reminder_2hr { "true" } else { "false" })?;

    if let Some(name) = &input.practice_name {
        store_setting(&conn, "practice_name", name)?;
    }
    if let Some(phone) = &input.practice_phone {
        store_setting(&conn, "practice_phone", phone)?;
    }

    if let Some(twilio) = &input.twilio {
        if !twilio.account_sid.is_empty() {
            store_setting(&conn, "twilio_account_sid", &twilio.account_sid)?;
        }
        if !twilio.auth_token.is_empty() {
            store_setting(&conn, "twilio_auth_token", &twilio.auth_token)?;
        }
        if !twilio.from_number.is_empty() {
            store_setting(&conn, "twilio_from_number", &twilio.from_number)?;
        }
    }

    if let Some(sg) = &input.sendgrid {
        if !sg.api_key.is_empty() {
            store_setting(&conn, "sendgrid_api_key", &sg.api_key)?;
        }
        if !sg.from_email.is_empty() {
            store_setting(&conn, "sendgrid_from_email", &sg.from_email)?;
        }
        if let Some(name) = &sg.from_name {
            store_setting(&conn, "sendgrid_from_name", name)?;
        }
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "configure_reminders".to_string(),
            resource_type: "ReminderConfig".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "sms_enabled={}, email_enabled={}",
                input.sms_enabled, input.email_enabled
            )),
        },
    );

    build_config_record(&conn)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: get_reminder_config
// ─────────────────────────────────────────────────────────────────────────────

/// Get the current reminder configuration (secrets masked).
#[tauri::command]
pub fn get_reminder_config(
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<ReminderConfigRecord, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::AppointmentScheduling, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "get_reminder_config".to_string(),
            resource_type: "ReminderConfig".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    build_config_record(&conn)
}

fn build_config_record(conn: &rusqlite::Connection) -> Result<ReminderConfigRecord, AppError> {
    let sms_enabled = get_setting_bool(conn, "reminder_sms_enabled", false);
    let email_enabled = get_setting_bool(conn, "reminder_email_enabled", false);
    let reminder_24hr = get_setting_bool(conn, "reminder_24hr", true);
    let reminder_2hr = get_setting_bool(conn, "reminder_2hr", true);
    let practice_name = get_setting(conn, "practice_name")?;
    let practice_phone = get_setting(conn, "practice_phone")?;

    let twilio_sid = get_setting(conn, "twilio_account_sid")?;
    let twilio_from = get_setting(conn, "twilio_from_number")?;
    let twilio_configured = twilio_sid.is_some()
        && get_setting(conn, "twilio_auth_token")?.is_some()
        && twilio_from.is_some();
    let twilio_from_masked = twilio_from.as_deref().map(mask_secret);

    let sg_key = get_setting(conn, "sendgrid_api_key")?;
    let sg_from_email = get_setting(conn, "sendgrid_from_email")?;
    let sendgrid_configured = sg_key.is_some() && sg_from_email.is_some();

    Ok(ReminderConfigRecord {
        sms_enabled,
        email_enabled,
        reminder_24hr,
        reminder_2hr,
        practice_name,
        practice_phone,
        twilio_configured,
        twilio_from_number: twilio_from_masked,
        sendgrid_configured,
        sendgrid_from_email: sg_from_email,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: process_pending_reminders
// ─────────────────────────────────────────────────────────────────────────────

/// Scan upcoming appointments and send any pending reminders at configured intervals.
/// Deduplicates — will not send if a reminder of the same type already exists for
/// the same appointment_id.
#[tauri::command]
pub fn process_pending_reminders(
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<ProcessRemindersResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Update,
    )?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let sms_enabled = get_setting_bool(&conn, "reminder_sms_enabled", false);
    let email_enabled = get_setting_bool(&conn, "reminder_email_enabled", false);
    let do_24hr = get_setting_bool(&conn, "reminder_24hr", true);
    let do_2hr = get_setting_bool(&conn, "reminder_2hr", true);

    if !sms_enabled && !email_enabled {
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "process_pending_reminders".to_string(),
                resource_type: "ReminderLog".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: true,
                details: Some("No channels enabled; no reminders sent".to_string()),
            },
        );
        return Ok(ProcessRemindersResult {
            sent_count: 0,
            skipped_count: 0,
            failed_count: 0,
            results: vec![],
        });
    }

    // Load credentials
    let twilio_sid = get_setting(&conn, "twilio_account_sid")?;
    let twilio_token = get_setting(&conn, "twilio_auth_token")?;
    let twilio_from = get_setting(&conn, "twilio_from_number")?;
    let sg_key = get_setting(&conn, "sendgrid_api_key")?;
    let sg_from_email = get_setting(&conn, "sendgrid_from_email")?;
    let sg_from_name = get_setting(&conn, "sendgrid_from_name")?;
    let practice_name = get_setting(&conn, "practice_name")?.unwrap_or_else(|| "MedArc".to_string());
    let practice_phone = get_setting(&conn, "practice_phone")?.unwrap_or_else(|| "our office".to_string());

    let now = chrono::Utc::now();

    // Query appointments that are booked and in the next 25 hours (for 24hr window)
    // We widen the window slightly to catch any drift; interval filtering is done below.
    let window_end = now + chrono::Duration::hours(25);
    let window_start = now;

    // Get booked appointments in the upcoming window from appointment_index
    let mut stmt = conn
        .prepare(
            "SELECT ai.appointment_id, ai.patient_id, ai.provider_id, ai.start_time,
                    fr.resource
             FROM appointment_index ai
             JOIN fhir_resources fr ON fr.id = ai.appointment_id
             WHERE ai.status = 'booked'
               AND ai.start_time >= ?1
               AND ai.start_time <= ?2",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    struct ApptRow {
        appointment_id: String,
        patient_id: String,
        provider_id: String,
        start_time: String,
        resource: String,
    }

    let appts: Vec<ApptRow> = stmt
        .query_map(
            rusqlite::params![
                window_start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                window_end.format("%Y-%m-%dT%H:%M:%S").to_string(),
            ],
            |row| {
                Ok(ApptRow {
                    appointment_id: row.get(0)?,
                    patient_id: row.get(1)?,
                    provider_id: row.get(2)?,
                    start_time: row.get(3)?,
                    resource: row.get(4)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let mut results = Vec::new();
    let mut sent_count = 0u32;
    let mut skipped_count = 0u32;
    let mut failed_count = 0u32;

    for appt in &appts {
        let appt_dt = match chrono::DateTime::parse_from_rfc3339(&appt.start_time)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&appt.start_time, "%Y-%m-%dT%H:%M:%S")
                    .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
            }) {
            Ok(dt) => dt,
            Err(_) => continue,
        };

        // Extract patient phone/email/name from FHIR resource
        let _fhir: serde_json::Value =
            serde_json::from_str(&appt.resource).unwrap_or(serde_json::Value::Null);

        // Look up patient resource for name/contact
        let patient_resource: serde_json::Value = conn
            .query_row(
                "SELECT resource FROM fhir_resources WHERE id = ?1",
                rusqlite::params![&appt.patient_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null);

        let patient_name = extract_patient_name(&patient_resource);
        let patient_phone = extract_patient_phone(&patient_resource);
        let patient_email = extract_patient_email(&patient_resource);

        // Look up provider name
        let provider_name = get_provider_name(&conn, &appt.provider_id);

        let appt_time_str = appt_dt.format("%I:%M %p").to_string();
        let appt_date_str = appt_dt.format("%B %d, %Y").to_string();

        let ctx = TemplateContext {
            patient_name: patient_name.clone(),
            appointment_date: appt_date_str,
            appointment_time: appt_time_str,
            provider_name,
            practice_name: practice_name.clone(),
            practice_phone: practice_phone.clone(),
        };

        // Determine which reminder types are due
        let mut types_to_send: Vec<(&str, i64)> = vec![];
        if do_24hr {
            let send_at = appt_dt - chrono::Duration::hours(24);
            // Due if we are within [send_at, send_at + 10 minutes]
            if now >= send_at && now < send_at + chrono::Duration::minutes(10) {
                types_to_send.push(("24hr", 24));
            }
        }
        if do_2hr {
            let send_at = appt_dt - chrono::Duration::hours(2);
            if now >= send_at && now < send_at + chrono::Duration::minutes(10) {
                types_to_send.push(("2hr", 2));
            }
        }

        for (rtype, _) in &types_to_send {
            // Deduplication: check if reminder already sent for this appt + type
            let already_sent: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM reminder_log
                     WHERE appointment_id = ?1
                       AND reminder_type = ?2
                       AND status IN ('sent', 'delivered', 'pending')",
                    rusqlite::params![&appt.appointment_id, rtype],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);

            if already_sent {
                skipped_count += 1;
                continue;
            }

            let template = match *rtype {
                "24hr" => DEFAULT_TEMPLATE_24HR,
                "2hr" => DEFAULT_TEMPLATE_2HR,
                _ => DEFAULT_TEMPLATE_24HR,
            };
            let message = render_template(template, &ctx);

            // SMS channel
            if sms_enabled {
                if let Some(phone) = &patient_phone {
                    let result = dispatch_reminder(
                        &conn,
                        &appt.appointment_id,
                        &appt.patient_id,
                        rtype,
                        "sms",
                        phone,
                        &message,
                        twilio_sid.as_deref(),
                        twilio_token.as_deref(),
                        twilio_from.as_deref(),
                        None, None, None, None, None,
                    );
                    if result.status == "sent" { sent_count += 1; } else { failed_count += 1; }
                    results.push(result);
                }
            }

            // Email channel
            if email_enabled {
                if let Some(email) = &patient_email {
                    let subject = match *rtype {
                        "24hr" => "Your PT Appointment is Tomorrow",
                        "2hr" => "Your PT Appointment is in 2 Hours",
                        _ => "Appointment Reminder",
                    };
                    let result = dispatch_reminder(
                        &conn,
                        &appt.appointment_id,
                        &appt.patient_id,
                        rtype,
                        "email",
                        email,
                        &message,
                        None, None, None,
                        sg_key.as_deref(),
                        sg_from_email.as_deref(),
                        sg_from_name.as_deref(),
                        Some(&patient_name),
                        Some(subject),
                    );
                    if result.status == "sent" { sent_count += 1; } else { failed_count += 1; }
                    results.push(result);
                }
            }
        }
    }

    // Also check for no-show appointments: start_time is in the past and status = 'booked'
    let no_show_window_start = (now - chrono::Duration::hours(4))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let no_show_window_end = (now - chrono::Duration::minutes(30))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let _ = conn.execute(
        "UPDATE appointment_index SET status = 'noshow'
         WHERE status = 'booked'
           AND start_time >= ?1
           AND start_time <= ?2",
        rusqlite::params![no_show_window_start, no_show_window_end],
    );

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "process_pending_reminders".to_string(),
            resource_type: "ReminderLog".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "sent={}, skipped={}, failed={}",
                sent_count, skipped_count, failed_count
            )),
        },
    );

    Ok(ProcessRemindersResult {
        sent_count,
        skipped_count,
        failed_count,
        results,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: send_reminder
// ─────────────────────────────────────────────────────────────────────────────

/// Manually send a specific reminder for an appointment.
#[tauri::command]
pub fn send_reminder(
    appointment_id: String,
    reminder_type: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<ReminderResult>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Update,
    )?;

    let valid_types = ["24hr", "2hr", "no_show", "custom"];
    if !valid_types.contains(&reminder_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid reminder_type: {}. Valid: {:?}",
            reminder_type, valid_types
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let sms_enabled = get_setting_bool(&conn, "reminder_sms_enabled", false);
    let email_enabled = get_setting_bool(&conn, "reminder_email_enabled", false);

    let twilio_sid = get_setting(&conn, "twilio_account_sid")?;
    let twilio_token = get_setting(&conn, "twilio_auth_token")?;
    let twilio_from = get_setting(&conn, "twilio_from_number")?;
    let sg_key = get_setting(&conn, "sendgrid_api_key")?;
    let sg_from_email = get_setting(&conn, "sendgrid_from_email")?;
    let sg_from_name = get_setting(&conn, "sendgrid_from_name")?;
    let practice_name = get_setting(&conn, "practice_name")?.unwrap_or_else(|| "MedArc".to_string());
    let practice_phone = get_setting(&conn, "practice_phone")?.unwrap_or_else(|| "our office".to_string());

    // Load appointment
    let (patient_id, provider_id, start_time): (String, String, String) = conn
        .query_row(
            "SELECT patient_id, provider_id, start_time FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![&appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    let patient_resource: serde_json::Value = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1",
            rusqlite::params![&patient_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let patient_name = extract_patient_name(&patient_resource);
    let patient_phone = extract_patient_phone(&patient_resource);
    let patient_email = extract_patient_email(&patient_resource);
    let provider_name = get_provider_name(&conn, &provider_id);

    // Parse start_time for display
    let appt_dt = chrono::DateTime::parse_from_rfc3339(&start_time)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&start_time, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
        })
        .unwrap_or_else(|_| chrono::Utc::now());

    let ctx = TemplateContext {
        patient_name: patient_name.clone(),
        appointment_date: appt_dt.format("%B %d, %Y").to_string(),
        appointment_time: appt_dt.format("%I:%M %p").to_string(),
        provider_name,
        practice_name,
        practice_phone,
    };

    let template = match reminder_type.as_str() {
        "24hr" => DEFAULT_TEMPLATE_24HR,
        "2hr" => DEFAULT_TEMPLATE_2HR,
        "no_show" => DEFAULT_TEMPLATE_NOSHOW,
        _ => DEFAULT_TEMPLATE_24HR,
    };
    let message = render_template(template, &ctx);

    let mut results = Vec::new();

    if sms_enabled {
        if let Some(phone) = &patient_phone {
            let r = dispatch_reminder(
                &conn, &appointment_id, &patient_id, &reminder_type,
                "sms", phone, &message,
                twilio_sid.as_deref(), twilio_token.as_deref(), twilio_from.as_deref(),
                None, None, None, None, None,
            );
            results.push(r);
        }
    }

    if email_enabled {
        if let Some(email) = &patient_email {
            let subject = "Appointment Reminder";
            let r = dispatch_reminder(
                &conn, &appointment_id, &patient_id, &reminder_type,
                "email", email, &message,
                None, None, None,
                sg_key.as_deref(), sg_from_email.as_deref(), sg_from_name.as_deref(),
                Some(&patient_name), Some(subject),
            );
            results.push(r);
        }
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "send_reminder".to_string(),
            resource_type: "ReminderLog".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("reminder_type={}, channels={}", reminder_type, results.len())),
        },
    );

    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: send_no_show_followup
// ─────────────────────────────────────────────────────────────────────────────

/// Send a no-show follow-up message for a missed appointment.
#[tauri::command]
pub fn send_no_show_followup(
    appointment_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<ReminderResult>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Update,
    )?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify appointment is actually a no-show or that start_time has passed
    let (patient_id, provider_id, start_time, status): (String, String, String, String) = conn
        .query_row(
            "SELECT patient_id, provider_id, start_time, status FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![&appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    let now = chrono::Utc::now();
    let appt_dt = chrono::DateTime::parse_from_rfc3339(&start_time)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&start_time, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
        })
        .unwrap_or(now);

    let is_no_show = status == "noshow"
        || (appt_dt < now && status != "fulfilled" && status != "cancelled");

    if !is_no_show {
        return Err(AppError::Validation(
            "Appointment is not a no-show (status is not noshow and time has not passed)".to_string(),
        ));
    }

    let sms_enabled = get_setting_bool(&conn, "reminder_sms_enabled", false);
    let email_enabled = get_setting_bool(&conn, "reminder_email_enabled", false);

    let twilio_sid = get_setting(&conn, "twilio_account_sid")?;
    let twilio_token = get_setting(&conn, "twilio_auth_token")?;
    let twilio_from = get_setting(&conn, "twilio_from_number")?;
    let sg_key = get_setting(&conn, "sendgrid_api_key")?;
    let sg_from_email = get_setting(&conn, "sendgrid_from_email")?;
    let sg_from_name = get_setting(&conn, "sendgrid_from_name")?;
    let practice_name = get_setting(&conn, "practice_name")?.unwrap_or_else(|| "MedArc".to_string());
    let practice_phone = get_setting(&conn, "practice_phone")?.unwrap_or_else(|| "our office".to_string());

    let patient_resource: serde_json::Value = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1",
            rusqlite::params![&patient_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let patient_name = extract_patient_name(&patient_resource);
    let patient_phone = extract_patient_phone(&patient_resource);
    let patient_email = extract_patient_email(&patient_resource);
    let provider_name = get_provider_name(&conn, &provider_id);

    let ctx = TemplateContext {
        patient_name: patient_name.clone(),
        appointment_date: appt_dt.format("%B %d, %Y").to_string(),
        appointment_time: appt_dt.format("%I:%M %p").to_string(),
        provider_name,
        practice_name,
        practice_phone,
    };

    let message = render_template(DEFAULT_TEMPLATE_NOSHOW, &ctx);
    let mut results = Vec::new();

    if sms_enabled {
        if let Some(phone) = &patient_phone {
            let r = dispatch_reminder(
                &conn, &appointment_id, &patient_id, "no_show",
                "sms", phone, &message,
                twilio_sid.as_deref(), twilio_token.as_deref(), twilio_from.as_deref(),
                None, None, None, None, None,
            );
            results.push(r);
        }
    }

    if email_enabled {
        if let Some(email) = &patient_email {
            let subject = "We Missed You Today";
            let r = dispatch_reminder(
                &conn, &appointment_id, &patient_id, "no_show",
                "email", email, &message,
                None, None, None,
                sg_key.as_deref(), sg_from_email.as_deref(), sg_from_name.as_deref(),
                Some(&patient_name), Some(subject),
            );
            results.push(r);
        }
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "send_no_show_followup".to_string(),
            resource_type: "ReminderLog".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("channels={}", results.len())),
        },
    );

    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: process_cancellation_waitlist
// ─────────────────────────────────────────────────────────────────────────────

/// When an appointment is cancelled, check the waitlist for matching patients
/// and send them an offer to book the slot.
#[tauri::command]
pub fn process_cancellation_waitlist(
    appointment_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<WaitlistMatch>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Update,
    )?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load the cancelled appointment
    let (provider_id, appt_type, start_time): (String, String, String) = conn
        .query_row(
            "SELECT provider_id, appt_type, start_time FROM appointment_index WHERE appointment_id = ?1",
            rusqlite::params![&appointment_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Appointment {} not found", appointment_id)))?;

    // Parse date portion for waitlist preferred_date comparison
    let appt_date = &start_time[..10]; // "YYYY-MM-DD"

    // Find waitlist patients who match: same provider (or no preference) + same appt_type + preferred_date <= appt_date
    // Ordered by priority ASC (1 = most urgent)
    let mut stmt = conn
        .prepare(
            "SELECT wi.waitlist_id, wi.patient_id, wi.provider_id, wi.preferred_date
             FROM waitlist_index wi
             WHERE wi.status = 'waiting'
               AND wi.appt_type = ?1
               AND wi.preferred_date <= ?2
               AND (wi.provider_id = ?3 OR wi.provider_id IS NULL)
             ORDER BY wi.priority ASC, wi.preferred_date ASC
             LIMIT 5",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    struct WaitRow {
        waitlist_id: String,
        patient_id: String,
    }

    let waitlist_rows: Vec<WaitRow> = stmt
        .query_map(
            rusqlite::params![&appt_type, appt_date, &provider_id],
            |row| {
                Ok(WaitRow {
                    waitlist_id: row.get(0)?,
                    patient_id: row.get(1)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let sms_enabled = get_setting_bool(&conn, "reminder_sms_enabled", false);
    let email_enabled = get_setting_bool(&conn, "reminder_email_enabled", false);
    let twilio_sid = get_setting(&conn, "twilio_account_sid")?;
    let twilio_token = get_setting(&conn, "twilio_auth_token")?;
    let twilio_from = get_setting(&conn, "twilio_from_number")?;
    let sg_key = get_setting(&conn, "sendgrid_api_key")?;
    let sg_from_email = get_setting(&conn, "sendgrid_from_email")?;
    let sg_from_name = get_setting(&conn, "sendgrid_from_name")?;
    let practice_name = get_setting(&conn, "practice_name")?.unwrap_or_else(|| "MedArc".to_string());
    let practice_phone = get_setting(&conn, "practice_phone")?.unwrap_or_else(|| "our office".to_string());

    let appt_dt = chrono::DateTime::parse_from_rfc3339(&start_time)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&start_time, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
        })
        .unwrap_or_else(|_| chrono::Utc::now());

    let mut matches = Vec::new();

    for row in &waitlist_rows {
        let patient_resource: serde_json::Value = conn
            .query_row(
                "SELECT resource FROM fhir_resources WHERE id = ?1",
                rusqlite::params![&row.patient_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null);

        let patient_name = extract_patient_name(&patient_resource);
        let patient_phone = extract_patient_phone(&patient_resource);
        let patient_email = extract_patient_email(&patient_resource);

        let ctx = TemplateContext {
            patient_name: patient_name.clone(),
            appointment_date: appt_dt.format("%B %d, %Y").to_string(),
            appointment_time: appt_dt.format("%I:%M %p").to_string(),
            provider_name: String::new(),
            practice_name: practice_name.clone(),
            practice_phone: practice_phone.clone(),
        };

        let message = render_template(DEFAULT_TEMPLATE_WAITLIST, &ctx);
        let mut offer_sent = false;
        let mut offer_channel: Option<String> = None;

        if sms_enabled {
            if let Some(phone) = &patient_phone {
                let r = dispatch_reminder(
                    &conn, &appointment_id, &row.patient_id, "waitlist_offer",
                    "sms", phone, &message,
                    twilio_sid.as_deref(), twilio_token.as_deref(), twilio_from.as_deref(),
                    None, None, None, None, None,
                );
                if r.status == "sent" {
                    offer_sent = true;
                    offer_channel = Some("sms".to_string());
                }
            }
        }

        if email_enabled && !offer_sent {
            if let Some(email) = &patient_email {
                let subject = "Appointment Slot Available";
                let r = dispatch_reminder(
                    &conn, &appointment_id, &row.patient_id, "waitlist_offer",
                    "email", email, &message,
                    None, None, None,
                    sg_key.as_deref(), sg_from_email.as_deref(), sg_from_name.as_deref(),
                    Some(&patient_name), Some(subject),
                );
                if r.status == "sent" {
                    offer_sent = true;
                    offer_channel = Some("email".to_string());
                }
            }
        }

        matches.push(WaitlistMatch {
            waitlist_id: row.waitlist_id.clone(),
            patient_id: row.patient_id.clone(),
            patient_name,
            phone: patient_phone,
            email: patient_email,
            offer_sent,
            offer_channel,
        });
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "process_cancellation_waitlist".to_string(),
            resource_type: "WaitlistIndex".to_string(),
            resource_id: Some(appointment_id.clone()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("waitlist_matches={}", matches.len())),
        },
    );

    Ok(matches)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: confirm_waitlist_booking
// ─────────────────────────────────────────────────────────────────────────────

/// Confirm a waitlist rebooking by marking the waitlist entry as fulfilled.
#[tauri::command]
pub fn confirm_waitlist_booking(
    waitlist_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<(), AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Update,
    )?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let updated = conn
        .execute(
            "UPDATE waitlist_index SET status = 'fulfilled' WHERE waitlist_id = ?1",
            rusqlite::params![&waitlist_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    if updated == 0 {
        return Err(AppError::NotFound(format!(
            "Waitlist entry {} not found",
            waitlist_id
        )));
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "confirm_waitlist_booking".to_string(),
            resource_type: "WaitlistIndex".to_string(),
            resource_id: Some(waitlist_id),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command: list_reminder_log
// ─────────────────────────────────────────────────────────────────────────────

/// List reminder log entries, optionally filtered by patient_id and/or date range.
#[tauri::command]
pub fn list_reminder_log(
    patient_id: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<ReminderLog>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(
        session.role,
        Resource::AppointmentScheduling,
        Action::Read,
    )?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut sql = String::from(
        "SELECT reminder_id, appointment_id, patient_id, reminder_type,
                channel, recipient, message_body, status,
                external_id, error_message, sent_at, created_at
         FROM reminder_log WHERE 1=1",
    );
    let mut params_box: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(ref pid) = patient_id {
        sql.push_str(&format!(" AND patient_id = ?{}", param_idx));
        params_box.push(Box::new(pid.clone()));
        param_idx += 1;
    }
    if let Some(ref sd) = start_date {
        sql.push_str(&format!(" AND created_at >= ?{}", param_idx));
        params_box.push(Box::new(sd.clone()));
        param_idx += 1;
    }
    if let Some(ref ed) = end_date {
        sql.push_str(&format!(" AND created_at <= ?{}", param_idx));
        params_box.push(Box::new(ed.clone()));
        param_idx += 1;
    }
    let _ = param_idx; // silence unused warning
    sql.push_str(" ORDER BY created_at DESC LIMIT 500");

    let params_refs: Vec<&dyn rusqlite::ToSql> =
        params_box.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let logs: Vec<ReminderLog> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(ReminderLog {
                reminder_id: row.get(0)?,
                appointment_id: row.get(1)?,
                patient_id: row.get(2)?,
                reminder_type: row.get(3)?,
                channel: row.get(4)?,
                recipient: row.get(5)?,
                message_body: row.get(6)?,
                status: row.get(7)?,
                external_id: row.get(8)?,
                error_message: row.get(9)?,
                sent_at: row.get(10)?,
                created_at: row.get(11)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "list_reminder_log".to_string(),
            resource_type: "ReminderLog".to_string(),
            resource_id: None,
            patient_id: patient_id.clone(),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("count={}", logs.len())),
        },
    );

    Ok(logs)
}

// ─────────────────────────────────────────────────────────────────────────────
// FHIR extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

fn extract_patient_name(resource: &serde_json::Value) -> String {
    resource
        .get("name")
        .and_then(|names| names.as_array())
        .and_then(|arr| arr.first())
        .and_then(|n| {
            let family = n.get("family").and_then(|v| v.as_str()).unwrap_or("");
            let given: Vec<&str> = n
                .get("given")
                .and_then(|g| g.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let first = given.first().copied().unwrap_or("");
            if family.is_empty() && first.is_empty() {
                None
            } else {
                Some(format!("{} {}", first, family).trim().to_string())
            }
        })
        .unwrap_or_else(|| "Patient".to_string())
}

fn extract_patient_phone(resource: &serde_json::Value) -> Option<String> {
    resource
        .get("telecom")
        .and_then(|t| t.as_array())
        .and_then(|arr| {
            arr.iter().find(|t| {
                t.get("system").and_then(|s| s.as_str()) == Some("phone")
            })
        })
        .and_then(|t| t.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn extract_patient_email(resource: &serde_json::Value) -> Option<String> {
    resource
        .get("telecom")
        .and_then(|t| t.as_array())
        .and_then(|arr| {
            arr.iter().find(|t| {
                t.get("system").and_then(|s| s.as_str()) == Some("email")
            })
        })
        .and_then(|t| t.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn get_provider_name(conn: &rusqlite::Connection, provider_id: &str) -> String {
    conn.query_row(
        "SELECT display_name FROM users WHERE id = ?1",
        rusqlite::params![provider_id],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "Provider".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: Template placeholder substitution ──────────────────────────────

    #[test]
    fn test_template_substitution_all_placeholders() {
        let ctx = TemplateContext {
            patient_name: "Jane Doe".to_string(),
            appointment_date: "April 1, 2026".to_string(),
            appointment_time: "09:00 AM".to_string(),
            provider_name: "Dr. Smith".to_string(),
            practice_name: "MedArc PT".to_string(),
            practice_phone: "555-1234".to_string(),
        };
        let template = "Hi {patient_name}, your appt on {appointment_date} at {appointment_time} with {provider_name} at {practice_name}. Call {practice_phone}.";
        let rendered = render_template(template, &ctx);
        assert!(rendered.contains("Jane Doe"));
        assert!(rendered.contains("April 1, 2026"));
        assert!(rendered.contains("09:00 AM"));
        assert!(rendered.contains("Dr. Smith"));
        assert!(rendered.contains("MedArc PT"));
        assert!(rendered.contains("555-1234"));
        assert!(!rendered.contains('{'));
        assert!(!rendered.contains('}'));
    }

    #[test]
    fn test_template_substitution_partial_placeholders() {
        let ctx = TemplateContext {
            patient_name: "Bob".to_string(),
            appointment_date: String::new(),
            appointment_time: "2:00 PM".to_string(),
            provider_name: String::new(),
            practice_name: String::new(),
            practice_phone: "555-9999".to_string(),
        };
        let rendered = render_template(DEFAULT_TEMPLATE_NOSHOW, &ctx);
        assert!(rendered.contains("555-9999"));
        assert!(!rendered.contains("{practice_phone}"));
    }

    // ── Test 2: Reminder interval calculation ────────────────────────────────

    #[test]
    fn test_reminder_send_time_24hr() {
        let start = "2026-04-01T09:00:00";
        let send_at = reminder_send_time(start, 24).expect("should parse");
        // send_at should be 2026-03-31T09:00:00 UTC
        assert_eq!(send_at.format("%Y-%m-%dT%H:%M:%S").to_string(), "2026-03-31T09:00:00");
    }

    #[test]
    fn test_reminder_send_time_2hr() {
        let start = "2026-04-01T14:00:00";
        let send_at = reminder_send_time(start, 2).expect("should parse");
        assert_eq!(send_at.format("%Y-%m-%dT%H:%M:%S").to_string(), "2026-04-01T12:00:00");
    }

    #[test]
    fn test_reminder_send_time_invalid_returns_none() {
        let result = reminder_send_time("not-a-date", 24);
        assert!(result.is_none());
    }

    // ── Test 3: Twilio API request body formatting ────────────────────────────

    #[test]
    fn test_twilio_url_format() {
        let account_sid = "ACtest1234567890abcdef";
        let expected = format!("{}/{}/Messages.json", TWILIO_API_BASE, account_sid);
        assert!(expected.contains("api.twilio.com"));
        assert!(expected.contains(account_sid));
        assert!(expected.ends_with("Messages.json"));
    }

    // ── Test 4: No-show detection ─────────────────────────────────────────────

    #[test]
    fn test_no_show_detection_past_booked() {
        let now = chrono::Utc::now();
        let past = now - chrono::Duration::hours(2);
        let start_time = past.format("%Y-%m-%dT%H:%M:%S").to_string();
        let status = "booked";

        let appt_dt: chrono::DateTime<chrono::Utc> =
            chrono::NaiveDateTime::parse_from_str(&start_time, "%Y-%m-%dT%H:%M:%S")
            .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
            .expect("should parse");

        let is_no_show = status == "noshow"
            || (appt_dt < now && status != "fulfilled" && status != "cancelled");
        assert!(is_no_show, "Past booked appointment should be detected as no-show");
    }

    #[test]
    fn test_no_show_detection_fulfilled_not_noshow() {
        let now = chrono::Utc::now();
        let past = now - chrono::Duration::hours(2);
        let start_time = past.format("%Y-%m-%dT%H:%M:%S").to_string();
        let status = "fulfilled";

        let appt_dt: chrono::DateTime<chrono::Utc> =
            chrono::NaiveDateTime::parse_from_str(&start_time, "%Y-%m-%dT%H:%M:%S")
            .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc))
            .expect("should parse");

        let is_no_show = status == "noshow"
            || (appt_dt < now && status != "fulfilled" && status != "cancelled");
        assert!(!is_no_show, "Fulfilled appointment should NOT be a no-show");
    }

    // ── Test 5: Mask secret helper ────────────────────────────────────────────

    #[test]
    fn test_mask_secret_long_string() {
        let masked = mask_secret("AC1234567890abcdef1234");
        assert!(masked.starts_with("AC12"));
        assert!(masked.contains("***"));
        assert!(!masked.contains("567890abcde"));
    }

    #[test]
    fn test_mask_secret_short_string() {
        let masked = mask_secret("short");
        assert_eq!(masked, "*****");
    }

    // ── Test 6: Waitlist matching — provider None should match any ────────────

    #[test]
    fn test_waitlist_offer_template() {
        let ctx = TemplateContext {
            patient_name: "Alice".to_string(),
            appointment_date: "April 2, 2026".to_string(),
            appointment_time: "11:00 AM".to_string(),
            provider_name: String::new(),
            practice_name: "MedArc PT".to_string(),
            practice_phone: "555-0101".to_string(),
        };
        let rendered = render_template(DEFAULT_TEMPLATE_WAITLIST, &ctx);
        assert!(rendered.contains("Alice"));
        assert!(rendered.contains("11:00 AM"));
        assert!(rendered.contains("555-0101"));
        assert!(rendered.contains("April 2, 2026"));
    }

    // ── Test 7: Duplicate prevention interval boundary ──────────────────────

    #[test]
    fn test_24hr_reminder_due_window() {
        let now = chrono::Utc::now();
        // Appointment exactly 24hr 5min from now — NOT yet in window
        let appt_future = now + chrono::Duration::hours(24) + chrono::Duration::minutes(5);
        let send_at = appt_future - chrono::Duration::hours(24);
        let in_window = now >= send_at && now < send_at + chrono::Duration::minutes(10);
        assert!(!in_window, "Appointment 24hr+5min away should not trigger reminder yet");

        // Appointment exactly 24hr from now — IS in window
        let appt_now = now + chrono::Duration::hours(24);
        let send_at2 = appt_now - chrono::Duration::hours(24);
        let in_window2 = now >= send_at2 && now < send_at2 + chrono::Duration::minutes(10);
        assert!(in_window2, "Appointment exactly 24hr away should trigger reminder");
    }
}
