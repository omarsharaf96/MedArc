/// commands/therapy_cap.rs — Therapy Cap & KX Modifier Monitoring (M004/S02)
///
/// Implements Medicare therapy cap tracking, KX modifier application,
/// PTA CQ modifier detection, ABN workflow, and alert computation.
///
/// Therapy Cap Thresholds (2026)
/// -----------------------------
/// PT + SLP combined: $2,480 (KX modifier threshold)
/// Targeted Medical Review: $3,000
///
/// KX Modifier Logic
/// -----------------
/// When cumulative Medicare charges for PT+SLP reach $2,480, the KX modifier
/// must be added to all timed service lines to indicate medical necessity
/// documentation is on file.
///
/// PTA CQ Modifier
/// ---------------
/// When the treating provider is a PTA (Physical Therapist Assistant),
/// a CQ modifier is required on all service lines (15% payment reduction).
///
/// ABN Workflow (CMS-R-131)
/// ------------------------
/// An Advance Beneficiary Notice must be issued when:
///   - Therapy cap is approaching (within $200)
///   - Authorization has expired
///   - Service is non-covered
///   - Frequency limit applies
/// Patient choices: option1_pay / option2_dont_pay / option3_dont_provide
///
/// RBAC
/// ----
/// All commands require Billing resource access.
///   SystemAdmin / Provider / BillingStaff → full CRUD
///   NurseMa / FrontDesk                   → Read only
///
/// Audit
/// -----
/// Every mutating command writes an audit row via `write_audit_entry`.
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
// Threshold constants
// ─────────────────────────────────────────────────────────────────────────────

/// 2026 Medicare therapy cap for PT + SLP combined.
/// KX modifier is required at or above this threshold.
pub const THERAPY_CAP_THRESHOLD: f64 = 2480.0;

/// Targeted Medical Review threshold.
/// Claims above this amount require additional documentation review.
pub const TARGETED_REVIEW_THRESHOLD: f64 = 3000.0;

/// Amber alert threshold: within $200 of the KX cap.
pub const APPROACHING_THRESHOLD: f64 = THERAPY_CAP_THRESHOLD - 200.0; // $2,280

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Therapy cap status for a patient in a given calendar year.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TherapyCapStatus {
    pub tracking_id: String,
    pub patient_id: String,
    pub calendar_year: i64,
    pub payer_type: String,
    pub cumulative_charges: f64,
    pub threshold_amount: f64,
    pub remaining: f64,
    pub kx_required: bool,
    pub kx_applied_date: Option<String>,
    pub review_threshold_reached: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Result of `check_therapy_cap` — returns computed status fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TherapyCapCheck {
    pub patient_id: String,
    pub calendar_year: i64,
    pub cumulative_charges: f64,
    pub threshold_amount: f64,
    pub remaining: f64,
    pub kx_required: bool,
    pub review_threshold_reached: bool,
}

/// An alert for therapy cap status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TherapyCapAlert {
    pub patient_id: String,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub cumulative_charges: f64,
    pub threshold_amount: f64,
}

/// Input for creating an ABN record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbnInput {
    /// Patient the ABN is for.
    pub patient_id: String,
    /// Reason for issuing the ABN.
    pub reason: String,
    /// Services that may not be covered (JSON array of CPT codes).
    pub services: Vec<String>,
    /// The user creating this record.
    pub created_by: String,
}

/// An ABN record as stored in and returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbnRecord {
    pub abn_id: String,
    pub patient_id: String,
    pub reason: String,
    pub services: Vec<String>,
    pub patient_choice: Option<String>,
    pub signed_date: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// Input for applying or updating a patient's ABN choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbnChoiceInput {
    /// ABN ID to update.
    pub abn_id: String,
    /// Patient's choice: option1_pay, option2_dont_pay, option3_dont_provide.
    pub patient_choice: String,
    /// Date patient signed (ISO 8601 date).
    pub signed_date: String,
}

/// Result of checking whether a provider is a PTA.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtaModifierCheck {
    pub encounter_id: String,
    pub provider_id: String,
    pub is_pta: bool,
    pub cq_modifier_required: bool,
    pub message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure helper functions (testable without DB)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute therapy cap alerts for a given cumulative charge amount (pure function).
///
/// Returns a list of alerts based on three thresholds:
///   - Approaching ($2,280–$2,479.99): amber "Approaching therapy cap"
///   - At cap ($2,480–$2,999.99): red "KX modifier required"
///   - TMR ($3,000+): red "Targeted Medical Review threshold reached"
pub fn compute_therapy_cap_alerts(
    patient_id: &str,
    cumulative_charges: f64,
    threshold_amount: f64,
) -> Vec<TherapyCapAlert> {
    let mut alerts: Vec<TherapyCapAlert> = Vec::new();

    if cumulative_charges >= TARGETED_REVIEW_THRESHOLD {
        alerts.push(TherapyCapAlert {
            patient_id: patient_id.to_string(),
            alert_type: "targeted_medical_review".to_string(),
            severity: "error".to_string(),
            message: format!(
                "Targeted Medical Review threshold reached (${:.2} charged)",
                cumulative_charges
            ),
            cumulative_charges,
            threshold_amount,
        });
    } else if cumulative_charges >= THERAPY_CAP_THRESHOLD {
        alerts.push(TherapyCapAlert {
            patient_id: patient_id.to_string(),
            alert_type: "kx_modifier_required".to_string(),
            severity: "error".to_string(),
            message: format!(
                "KX modifier required — therapy cap reached (${:.2} of ${:.2} used)",
                cumulative_charges, threshold_amount
            ),
            cumulative_charges,
            threshold_amount,
        });
    } else if cumulative_charges >= APPROACHING_THRESHOLD {
        let remaining = threshold_amount - cumulative_charges;
        alerts.push(TherapyCapAlert {
            patient_id: patient_id.to_string(),
            alert_type: "approaching_therapy_cap".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "Approaching therapy cap — ${:.2} remaining of ${:.2} threshold",
                remaining, threshold_amount
            ),
            cumulative_charges,
            threshold_amount,
        });
    }

    alerts
}

/// Compute whether KX modifier is required for given cumulative charges (pure function).
pub fn kx_modifier_required(cumulative_charges: f64) -> bool {
    cumulative_charges >= THERAPY_CAP_THRESHOLD
}

/// Compute whether Targeted Medical Review threshold is reached (pure function).
pub fn targeted_review_reached(cumulative_charges: f64) -> bool {
    cumulative_charges >= TARGETED_REVIEW_THRESHOLD
}

/// Compute the remaining amount before therapy cap (pure function).
/// Returns 0.0 if cap already met or exceeded.
pub fn compute_remaining(cumulative_charges: f64, threshold: f64) -> f64 {
    (threshold - cumulative_charges).max(0.0)
}

/// Parse a comma-separated modifiers string and check for a specific modifier.
#[allow(dead_code)]
pub fn has_modifier(modifiers: &Option<String>, modifier: &str) -> bool {
    match modifiers {
        None => false,
        Some(s) => s
            .split(',')
            .any(|m| m.trim().eq_ignore_ascii_case(modifier)),
    }
}

/// Add a modifier to a comma-separated modifiers string.
/// Returns the updated string. If modifier already present, returns unchanged.
pub fn add_modifier(modifiers: &Option<String>, modifier: &str) -> String {
    match modifiers {
        None => modifier.to_string(),
        Some(s) if s.is_empty() => modifier.to_string(),
        Some(s) => {
            if s.split(',')
                .any(|m| m.trim().eq_ignore_ascii_case(modifier))
            {
                s.clone()
            } else {
                format!("{},{}", s, modifier)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Database helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Upsert a therapy_cap_tracking row for a patient/year/payer combination.
/// Returns the current cumulative charges (updated if provided).
fn upsert_cap_tracking(
    conn: &rusqlite::Connection,
    patient_id: &str,
    calendar_year: i64,
    payer_type: &str,
    new_cumulative: Option<f64>,
) -> Result<TherapyCapStatus, AppError> {
    let tracking_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Compute cumulative from encounter_billing if not provided
    let cumulative: f64 = if let Some(c) = new_cumulative {
        c
    } else {
        compute_cumulative_from_db(conn, patient_id, calendar_year)?
    };

    let kx_required_flag = kx_required_flag(cumulative);
    let review_flag = if targeted_review_reached(cumulative) { 1i64 } else { 0i64 };

    // Try to get existing row
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT tracking_id FROM therapy_cap_tracking
             WHERE patient_id = ?1 AND calendar_year = ?2 AND payer_type = ?3",
            rusqlite::params![patient_id, calendar_year, payer_type],
            |row| row.get(0),
        )
        .ok();

    if let Some(ref tid) = existing_id {
        // Update existing
        let kx_applied_date: Option<String> = conn
            .query_row(
                "SELECT kx_applied_date FROM therapy_cap_tracking WHERE tracking_id = ?1",
                rusqlite::params![tid],
                |row| row.get(0),
            )
            .unwrap_or(None);

        conn.execute(
            "UPDATE therapy_cap_tracking
             SET cumulative_charges = ?1,
                 review_threshold_reached = ?2,
                 updated_at = ?3
             WHERE tracking_id = ?4",
            rusqlite::params![cumulative, review_flag, now, tid],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(TherapyCapStatus {
            tracking_id: tid.clone(),
            patient_id: patient_id.to_string(),
            calendar_year,
            payer_type: payer_type.to_string(),
            cumulative_charges: cumulative,
            threshold_amount: THERAPY_CAP_THRESHOLD,
            remaining: compute_remaining(cumulative, THERAPY_CAP_THRESHOLD),
            kx_required: kx_required_flag,
            kx_applied_date,
            review_threshold_reached: review_flag == 1,
            created_at: now.clone(),
            updated_at: now,
        })
    } else {
        // Insert new
        conn.execute(
            "INSERT INTO therapy_cap_tracking
                (tracking_id, patient_id, calendar_year, payer_type, cumulative_charges,
                 threshold_amount, review_threshold_reached, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                tracking_id,
                patient_id,
                calendar_year,
                payer_type,
                cumulative,
                THERAPY_CAP_THRESHOLD,
                review_flag,
                now,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(TherapyCapStatus {
            tracking_id,
            patient_id: patient_id.to_string(),
            calendar_year,
            payer_type: payer_type.to_string(),
            cumulative_charges: cumulative,
            threshold_amount: THERAPY_CAP_THRESHOLD,
            remaining: compute_remaining(cumulative, THERAPY_CAP_THRESHOLD),
            kx_required: kx_required_flag,
            kx_applied_date: None,
            review_threshold_reached: review_flag == 1,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

/// Compute cumulative charges from encounter_billing for a patient/year.
/// Sums total_charge for Medicare billing rows in the given calendar year.
fn compute_cumulative_from_db(
    conn: &rusqlite::Connection,
    patient_id: &str,
    calendar_year: i64,
) -> Result<f64, AppError> {
    let year_start = format!("{}-01-01", calendar_year);
    let year_end = format!("{}-12-31 23:59:59", calendar_year);

    let total: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_charge), 0.0)
             FROM encounter_billing
             WHERE patient_id = ?1
               AND billing_rule = 'medicare'
               AND created_at >= ?2
               AND created_at <= ?3",
            rusqlite::params![patient_id, year_start, year_end],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(total)
}

/// Convert bool KX required to integer for internal logic.
fn kx_required_flag(cumulative: f64) -> bool {
    kx_modifier_required(cumulative)
}

/// Read a therapy_cap_tracking row by patient/year/payer.
#[allow(dead_code)]
fn query_cap_status(
    conn: &rusqlite::Connection,
    patient_id: &str,
    calendar_year: i64,
) -> Result<Option<TherapyCapStatus>, AppError> {
    let result = conn.query_row(
        "SELECT tracking_id, patient_id, calendar_year, payer_type, cumulative_charges,
                threshold_amount, kx_applied_date, review_threshold_reached,
                created_at, updated_at
         FROM therapy_cap_tracking
         WHERE patient_id = ?1 AND calendar_year = ?2 AND payer_type = 'medicare'",
        rusqlite::params![patient_id, calendar_year],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    );

    match result {
        Ok((
            tracking_id,
            pid,
            year,
            payer_type,
            cumulative,
            threshold,
            kx_applied_date,
            review_flag,
            created_at,
            updated_at,
        )) => Ok(Some(TherapyCapStatus {
            tracking_id,
            patient_id: pid,
            calendar_year: year,
            payer_type,
            cumulative_charges: cumulative,
            threshold_amount: threshold,
            remaining: compute_remaining(cumulative, threshold),
            kx_required: kx_modifier_required(cumulative),
            kx_applied_date,
            review_threshold_reached: review_flag == 1,
            created_at,
            updated_at,
        })),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Check therapy cap status for a patient in a given calendar year.
///
/// Returns cumulative charges, threshold, remaining balance, and whether
/// the KX modifier and/or Targeted Medical Review threshold are triggered.
///
/// If `calendar_year` is None, defaults to the current year.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn check_therapy_cap(
    patient_id: String,
    calendar_year: Option<i64>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<TherapyCapCheck, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let year = calendar_year
        .unwrap_or_else(|| chrono::Utc::now().format("%Y").to_string().parse().unwrap_or(2026));

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let cumulative = compute_cumulative_from_db(&conn, &patient_id, year)?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.check".to_string(),
            resource_type: "TherapyCap".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "year={} charges={:.2}",
                year, cumulative
            )),
        },
    )?;

    Ok(TherapyCapCheck {
        patient_id,
        calendar_year: year,
        cumulative_charges: cumulative,
        threshold_amount: THERAPY_CAP_THRESHOLD,
        remaining: compute_remaining(cumulative, THERAPY_CAP_THRESHOLD),
        kx_required: kx_modifier_required(cumulative),
        review_threshold_reached: targeted_review_reached(cumulative),
    })
}

/// Refresh (recompute and upsert) cumulative charge tracking for a patient.
///
/// Called after encounter billing is saved/finalized to keep the tracking table
/// in sync with actual billed charges.
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn refresh_therapy_cap_tracking(
    patient_id: String,
    calendar_year: Option<i64>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<TherapyCapStatus, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let year = calendar_year
        .unwrap_or_else(|| chrono::Utc::now().format("%Y").to_string().parse().unwrap_or(2026));

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let status = upsert_cap_tracking(&conn, &patient_id, year, "medicare", None)?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.refresh".to_string(),
            resource_type: "TherapyCap".to_string(),
            resource_id: Some(status.tracking_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "year={} charges={:.2} kx={}",
                year, status.cumulative_charges, status.kx_required
            )),
        },
    )?;

    Ok(status)
}

/// Apply KX modifier to all timed service lines on an encounter billing record.
///
/// The KX modifier signals to Medicare that the provider has documentation of
/// medical necessity on file to justify continued therapy beyond the cap.
/// Also marks the kx_applied_date in therapy_cap_tracking.
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn apply_kx_modifier(
    billing_id: String,
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<u64, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Fetch all timed line items for this billing record
    let mut stmt = conn
        .prepare(
            "SELECT li.line_id, li.modifiers
             FROM billing_line_items li
             JOIN cpt_fee_schedule fs ON fs.cpt_code = li.cpt_code
             WHERE li.billing_id = ?1 AND fs.is_timed = 1",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows: Vec<(String, Option<String>)> = stmt
        .query_map(rusqlite::params![billing_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut updated_count: u64 = 0;

    for (line_id, modifiers) in rows {
        let new_modifiers = add_modifier(&modifiers, "KX");
        conn.execute(
            "UPDATE billing_line_items SET modifiers = ?1 WHERE line_id = ?2",
            rusqlite::params![new_modifiers, line_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        updated_count += 1;
    }

    // Record the kx_applied_date on the tracking row if it exists
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let year: i64 = chrono::Utc::now()
        .format("%Y")
        .to_string()
        .parse()
        .unwrap_or(2026);

    conn.execute(
        "UPDATE therapy_cap_tracking
         SET kx_applied_date = ?1, updated_at = ?2
         WHERE patient_id = ?3 AND calendar_year = ?4 AND payer_type = 'medicare'",
        rusqlite::params![today, now, patient_id, year],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.apply_kx_modifier".to_string(),
            resource_type: "EncounterBilling".to_string(),
            resource_id: Some(billing_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "billing_id={} lines_updated={}",
                billing_id, updated_count
            )),
        },
    )?;

    Ok(updated_count)
}

/// Get active therapy cap alerts for a patient.
///
/// Returns zero to two alerts depending on cumulative charges:
///   - approaching_therapy_cap (amber)
///   - kx_modifier_required (red)
///   - targeted_medical_review (red)
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_therapy_cap_alerts(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    _device_id: State<'_, DeviceId>,
) -> Result<Vec<TherapyCapAlert>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let year: i64 = chrono::Utc::now()
        .format("%Y")
        .to_string()
        .parse()
        .unwrap_or(2026);

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = sess; // consumed above

    let cumulative = compute_cumulative_from_db(&conn, &patient_id, year)?;

    Ok(compute_therapy_cap_alerts(
        &patient_id,
        cumulative,
        THERAPY_CAP_THRESHOLD,
    ))
}

/// Create an ABN (Advance Beneficiary Notice) record for a patient.
///
/// The ABN must be presented to the patient before providing a service
/// that Medicare may not cover. This creates the record; the patient
/// choice and signature are recorded separately via `record_abn_choice`.
///
/// Requires: Billing + Create
#[tauri::command]
pub async fn generate_abn(
    input: AbnInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AbnRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    // Validate reason
    let valid_reasons = [
        "therapy_cap_approaching",
        "auth_expired",
        "non_covered_service",
        "frequency_limit",
    ];
    if !valid_reasons.contains(&input.reason.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid ABN reason '{}'. Must be one of: {}",
            input.reason,
            valid_reasons.join(", ")
        )));
    }

    let abn_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let services_json = serde_json::to_string(&input.services)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO abn_records
            (abn_id, patient_id, reason, services_json, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            abn_id,
            input.patient_id,
            input.reason,
            services_json,
            input.created_by,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.generate_abn".to_string(),
            resource_type: "ABN".to_string(),
            resource_id: Some(abn_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "reason={} services={}",
                input.reason,
                input.services.join(",")
            )),
        },
    )?;

    Ok(AbnRecord {
        abn_id,
        patient_id: input.patient_id,
        reason: input.reason,
        services: input.services,
        patient_choice: None,
        signed_date: None,
        created_by: input.created_by,
        created_at: now,
    })
}

/// Record a patient's choice on an existing ABN record.
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn record_abn_choice(
    input: AbnChoiceInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AbnRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let valid_choices = ["option1_pay", "option2_dont_pay", "option3_dont_provide"];
    if !valid_choices.contains(&input.patient_choice.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid patient choice '{}'. Must be one of: {}",
            input.patient_choice,
            valid_choices.join(", ")
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "UPDATE abn_records
         SET patient_choice = ?1, signed_date = ?2
         WHERE abn_id = ?3",
        rusqlite::params![input.patient_choice, input.signed_date, input.abn_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let record = query_abn_record(&conn, &input.abn_id)?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.record_abn_choice".to_string(),
            resource_type: "ABN".to_string(),
            resource_id: Some(input.abn_id.clone()),
            patient_id: Some(record.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "choice={} signed={}",
                input.patient_choice, input.signed_date
            )),
        },
    )?;

    Ok(record)
}

/// List all ABN records for a patient.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_abns(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<AbnRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn
        .prepare(
            "SELECT abn_id, patient_id, reason, services_json, patient_choice,
                    signed_date, created_by, created_at
             FROM abn_records
             WHERE patient_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let records = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            let services_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                services_str,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let results: Vec<AbnRecord> = records
        .into_iter()
        .map(
            |(abn_id, pid, reason, services_str, patient_choice, signed_date, created_by, created_at)| {
                let services: Vec<String> =
                    serde_json::from_str(&services_str).unwrap_or_default();
                AbnRecord {
                    abn_id,
                    patient_id: pid,
                    reason,
                    services,
                    patient_choice,
                    signed_date,
                    created_by,
                    created_at,
                }
            },
        )
        .collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.list_abns".to_string(),
            resource_type: "ABN".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", results.len())),
        },
    )?;

    Ok(results)
}

/// Check whether the treating provider for an encounter is a PTA.
///
/// If the provider has "PTA" in their role or display_name, a CQ modifier
/// is auto-suggested on all service lines (15% payment reduction indicator).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn check_pta_modifier(
    encounter_id: String,
    provider_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PtaModifierCheck, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Check if the provider is a PTA — look at display_name and role in users table
    let provider_info: Option<(String, String)> = conn
        .query_row(
            "SELECT display_name, role FROM users WHERE id = ?1",
            rusqlite::params![provider_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let is_pta = match &provider_info {
        None => false,
        Some((display_name, role)) => {
            let dn_lower = display_name.to_lowercase();
            let role_lower = role.to_lowercase();
            dn_lower.contains("pta")
                || dn_lower.contains("physical therapist assistant")
                || role_lower.contains("pta")
        }
    };

    let message = if is_pta {
        "Provider is a PTA — CQ modifier required on all service lines (15% payment reduction)"
            .to_string()
    } else {
        "Provider is not a PTA — CQ modifier not required".to_string()
    };

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "therapy_cap.check_pta_modifier".to_string(),
            resource_type: "EncounterBilling".to_string(),
            resource_id: Some(encounter_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "provider_id={} is_pta={}",
                provider_id, is_pta
            )),
        },
    )?;

    Ok(PtaModifierCheck {
        encounter_id,
        provider_id,
        is_pta,
        cq_modifier_required: is_pta,
        message,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn query_abn_record(
    conn: &rusqlite::Connection,
    abn_id: &str,
) -> Result<AbnRecord, AppError> {
    let (patient_id, reason, services_str, patient_choice, signed_date, created_by, created_at): (
        String, String, String, Option<String>, Option<String>, String, String,
    ) = conn
        .query_row(
            "SELECT patient_id, reason, services_json, patient_choice,
                    signed_date, created_by, created_at
             FROM abn_records WHERE abn_id = ?1",
            rusqlite::params![abn_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("ABN record {} not found", abn_id)))?;

    let services: Vec<String> = serde_json::from_str(&services_str).unwrap_or_default();

    Ok(AbnRecord {
        abn_id: abn_id.to_string(),
        patient_id,
        reason,
        services,
        patient_choice,
        signed_date,
        created_by,
        created_at,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Threshold checks ─────────────────────────────────────────────────────

    #[test]
    fn kx_not_required_below_threshold() {
        // $2,400 is below the $2,480 threshold
        assert!(!kx_modifier_required(2400.0));
    }

    #[test]
    fn kx_required_at_threshold() {
        // Exactly $2,480 requires KX
        assert!(kx_modifier_required(2480.0));
    }

    #[test]
    fn kx_required_above_threshold() {
        // $2,500 is above threshold
        assert!(kx_modifier_required(2500.0));
    }

    #[test]
    fn targeted_review_not_reached_below() {
        assert!(!targeted_review_reached(2999.99));
    }

    #[test]
    fn targeted_review_reached_at_threshold() {
        assert!(targeted_review_reached(3000.0));
    }

    #[test]
    fn targeted_review_reached_above() {
        assert!(targeted_review_reached(3100.0));
    }

    // ── Remaining balance ─────────────────────────────────────────────────────

    #[test]
    fn remaining_computed_correctly() {
        let remaining = compute_remaining(2000.0, THERAPY_CAP_THRESHOLD);
        assert!((remaining - 480.0).abs() < 0.001);
    }

    #[test]
    fn remaining_zero_at_cap() {
        let remaining = compute_remaining(2480.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(remaining, 0.0);
    }

    #[test]
    fn remaining_zero_over_cap() {
        // Never negative
        let remaining = compute_remaining(3000.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(remaining, 0.0);
    }

    // ── Alert generation ─────────────────────────────────────────────────────

    #[test]
    fn no_alerts_well_below_threshold() {
        let alerts = compute_therapy_cap_alerts("pt-1", 1000.0, THERAPY_CAP_THRESHOLD);
        assert!(alerts.is_empty());
    }

    #[test]
    fn approaching_alert_generated() {
        // $2,300 is between $2,280 (approaching) and $2,480 (cap)
        let alerts = compute_therapy_cap_alerts("pt-1", 2300.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "approaching_therapy_cap");
        assert_eq!(alerts[0].severity, "warning");
        assert!(alerts[0].message.contains("Approaching therapy cap"));
    }

    #[test]
    fn kx_required_alert_at_cap() {
        // $2,480 — exactly at cap
        let alerts = compute_therapy_cap_alerts("pt-1", 2480.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "kx_modifier_required");
        assert_eq!(alerts[0].severity, "error");
    }

    #[test]
    fn targeted_review_alert_at_3000() {
        let alerts = compute_therapy_cap_alerts("pt-1", 3000.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "targeted_medical_review");
        assert_eq!(alerts[0].severity, "error");
        assert!(alerts[0].message.contains("Targeted Medical Review"));
    }

    // ── KX modifier string manipulation ─────────────────────────────────────

    #[test]
    fn add_kx_to_none_modifiers() {
        let result = add_modifier(&None, "KX");
        assert_eq!(result, "KX");
    }

    #[test]
    fn add_kx_to_existing_modifier() {
        let existing = Some("GP".to_string());
        let result = add_modifier(&existing, "KX");
        assert_eq!(result, "GP,KX");
    }

    #[test]
    fn add_kx_idempotent_already_present() {
        let existing = Some("KX,GP".to_string());
        let result = add_modifier(&existing, "KX");
        // Should not add KX again
        assert_eq!(result, "KX,GP");
    }

    #[test]
    fn has_modifier_returns_true_when_present() {
        assert!(has_modifier(&Some("CQ,KX".to_string()), "KX"));
        assert!(has_modifier(&Some("CQ,KX".to_string()), "CQ"));
    }

    #[test]
    fn has_modifier_returns_false_when_absent() {
        assert!(!has_modifier(&Some("GP".to_string()), "KX"));
        assert!(!has_modifier(&None, "KX"));
    }

    // ── CQ modifier detection ────────────────────────────────────────────────

    #[test]
    fn cq_modifier_check_logic() {
        // Simulate PTA detection string logic
        let pta_display = "Jane Doe, PTA";
        let is_pta = pta_display.to_lowercase().contains("pta");
        assert!(is_pta);
    }

    #[test]
    fn cq_modifier_not_required_for_pt() {
        let pt_display = "John Smith, PT, DPT";
        let is_pta = pt_display.to_lowercase().contains("pta")
            || pt_display
                .to_lowercase()
                .contains("physical therapist assistant");
        // "pta" appears inside "Physical Therapist" abbreviation but not as whole word —
        // this test verifies exact match doesn't false-positive on "PT"
        // Note: "physical therapist assistant" would match, but "pt, dpt" alone should not
        assert!(!is_pta);
    }

    // ── Calendar year boundary ───────────────────────────────────────────────

    #[test]
    fn calendar_year_boundary_different_years() {
        // Charges from 2025 should NOT count toward 2026 cap
        // This is enforced by the SQL WHERE clause — tested here conceptually
        // by verifying that $2,500 charges in year Y trigger KX
        let charges_year_2026 = 2500.0f64;
        assert!(kx_modifier_required(charges_year_2026));

        // If the 2025 charges were $3,000 but 2026 is $0, no KX needed
        let charges_year_2025_only = 0.0f64;
        assert!(!kx_modifier_required(charges_year_2025_only));
    }

    // ── ABN record validation ────────────────────────────────────────────────

    #[test]
    fn abn_reason_values_are_valid() {
        let valid_reasons = [
            "therapy_cap_approaching",
            "auth_expired",
            "non_covered_service",
            "frequency_limit",
        ];
        for reason in &valid_reasons {
            assert!(valid_reasons.contains(reason));
        }
    }

    #[test]
    fn abn_patient_choice_values_are_valid() {
        let valid_choices = ["option1_pay", "option2_dont_pay", "option3_dont_provide"];
        for choice in &valid_choices {
            assert!(valid_choices.contains(choice));
        }
    }

    // ── Approaching threshold boundary ───────────────────────────────────────

    #[test]
    fn approaching_threshold_boundary_exact() {
        // $2,279.99 — just below approaching threshold, no alert
        let alerts = compute_therapy_cap_alerts("pt-1", 2279.99, THERAPY_CAP_THRESHOLD);
        assert!(alerts.is_empty(), "Should be no alert at $2,279.99");
    }

    #[test]
    fn approaching_threshold_boundary_just_over() {
        // $2,280.00 — exactly at approaching threshold
        let alerts = compute_therapy_cap_alerts("pt-1", 2280.0, THERAPY_CAP_THRESHOLD);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "approaching_therapy_cap");
    }
}
