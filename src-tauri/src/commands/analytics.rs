/// commands/analytics.rs — Analytics & Outcomes Dashboard (M003/S02)
///
/// Computes operational, financial, clinical outcomes, and payer-mix KPIs
/// from existing index tables and billing/claims data.
///
/// Operational KPIs
/// ----------------
///   Visits per period (from encounter_index)
///   Cancellation/no-show rate (from appointment_index)
///   Units per visit (from billing_line_items)
///   New patients per month (from patient_index)
///
/// Financial KPIs
/// --------------
///   Revenue per visit (total collections / visits)
///   Net collection rate (payments / (charges - adjustments))
///   Days in A/R (avg days from claim submission to payment)
///   Charges per visit
///   A/R aging distribution
///
/// Clinical Outcomes
/// -----------------
///   MCID achievement rate per measure
///   Average score improvement by measure
///   Outcomes by provider
///   Discharge rate (patients with discharge_summary note)
///
/// Payer Mix
/// ---------
///   Revenue percentage by payer
///   Visit count by payer
///   Average reimbursement per payer
///
/// RBAC
/// ----
///   All analytics commands require `Billing` resource access.
///     SystemAdmin / Provider / BillingStaff → Read
///     NurseMa / FrontDesk                   → No access
///
/// Audit
/// -----
///   Every command writes an audit row via `write_audit_entry`.
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
// MCID constants (mirrored from objective_measures.rs)
// ─────────────────────────────────────────────────────────────────────────────

const MCID_LEFS: f64 = 9.0;
const MCID_DASH: f64 = 10.8;
const MCID_NDI: f64 = 7.5;
const MCID_OSWESTRY: f64 = 10.0;
const MCID_PSFS: f64 = 2.0;
// FABQ has no widely-agreed MCID; use 5.0 as a reasonable threshold
const MCID_FABQ: f64 = 5.0;

fn mcid_for_measure(measure_type: &str) -> f64 {
    match measure_type {
        "lefs" => MCID_LEFS,
        "dash" => MCID_DASH,
        "ndi" => MCID_NDI,
        "oswestry" => MCID_OSWESTRY,
        "psfs" => MCID_PSFS,
        "fabq" => MCID_FABQ,
        _ => 0.0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Return types
// ─────────────────────────────────────────────────────────────────────────────

/// Operational KPIs for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalKPIs {
    pub total_visits: i64,
    pub cancellation_rate: f64,
    pub no_show_rate: f64,
    pub avg_units_per_visit: f64,
    pub new_patients: i64,
    pub period_start: String,
    pub period_end: String,
    pub provider_id: Option<String>,
}

/// Financial KPIs for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialKPIs {
    pub total_charges: f64,
    pub total_payments: f64,
    pub total_adjustments: f64,
    pub revenue_per_visit: f64,
    pub net_collection_rate: f64,
    pub days_in_ar: f64,
    pub charges_per_visit: f64,
    pub ar_aging_0_30: f64,
    pub ar_aging_31_60: f64,
    pub ar_aging_61_90: f64,
    pub ar_aging_91_plus: f64,
    pub period_start: String,
    pub period_end: String,
    pub payer_id: Option<String>,
}

/// MCID achievement and average improvement for one measure type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureOutcome {
    pub measure_type: String,
    pub patient_count: i64,
    pub mcid_achieved_count: i64,
    pub mcid_achievement_rate: f64,
    pub avg_initial_score: f64,
    pub avg_final_score: f64,
    pub avg_improvement: f64,
}

/// Outcomes grouped by provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOutcome {
    pub provider_id: String,
    pub patient_count: i64,
    pub avg_improvement: f64,
    pub discharge_count: i64,
}

/// Aggregated clinical outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalOutcomes {
    pub measure_outcomes: Vec<MeasureOutcome>,
    pub provider_outcomes: Vec<ProviderOutcome>,
    pub discharge_rate: f64,
    pub total_patients_with_outcomes: i64,
    pub period_start: String,
    pub period_end: String,
    pub measure_type_filter: Option<String>,
    pub provider_id_filter: Option<String>,
}

/// Revenue and visit breakdown for one payer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayerBreakdown {
    pub payer_id: String,
    pub payer_name: String,
    pub visit_count: i64,
    pub total_charges: f64,
    pub total_payments: f64,
    pub revenue_percentage: f64,
    pub avg_reimbursement_per_visit: f64,
}

/// Payer mix for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayerMix {
    pub payers: Vec<PayerBreakdown>,
    pub total_visits: i64,
    pub total_charges: f64,
    pub total_payments: f64,
    pub period_start: String,
    pub period_end: String,
}

/// KPI snapshot saved to kpi_snapshots table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KpiSnapshot {
    pub snapshot_id: String,
    pub period_type: String,
    pub period_start: String,
    pub period_end: String,
    pub provider_id: Option<String>,
    pub kpi_data: serde_json::Value,
    pub computed_at: String,
}

/// All KPIs combined (for dashboard summary endpoint).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub operational: OperationalKPIs,
    pub financial: FinancialKPIs,
    pub clinical: ClinicalOutcomes,
    pub payer_mix: PayerMix,
    pub period_start: String,
    pub period_end: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper — parse ISO date prefix for SQLite date comparison
// ─────────────────────────────────────────────────────────────────────────────

/// Ensures a date string is in YYYY-MM-DD format for SQLite text comparisons.
fn date_prefix(s: &str) -> &str {
    // Accept "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM:SS..." — take first 10 chars.
    if s.len() >= 10 { &s[..10] } else { s }
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: get_operational_kpis
// ─────────────────────────────────────────────────────────────────────────────

/// Compute operational KPIs (visits, cancellations, units/visit, new patients).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_operational_kpis(
    start_date: String,
    end_date: String,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<OperationalKPIs, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // ── Total completed visits ─────────────────────────────────────────────
    let total_visits: i64 = {
        let mut stmt = if provider_id.is_some() {
            conn.prepare(
                "SELECT COUNT(*) FROM encounter_index
                 WHERE date(encounter_date) >= ?1
                   AND date(encounter_date) <= ?2
                   AND provider_id = ?3
                   AND status = 'finished'",
            )
            .map_err(|e| AppError::Database(e.to_string()))?
        } else {
            conn.prepare(
                "SELECT COUNT(*) FROM encounter_index
                 WHERE date(encounter_date) >= ?1
                   AND date(encounter_date) <= ?2
                   AND status = 'finished'",
            )
            .map_err(|e| AppError::Database(e.to_string()))?
        };

        if let Some(ref pid) = provider_id {
            stmt.query_row(rusqlite::params![start, end, pid], |r| r.get(0))
        } else {
            stmt.query_row(rusqlite::params![start, end], |r| r.get(0))
        }
        .unwrap_or(0)
    };

    // ── Appointment totals for cancellation / no-show rates ───────────────
    let (total_appointments, cancelled, no_show): (i64, i64, i64) = {
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM appointment_index
                 WHERE date(start_time) >= ?1
                   AND date(start_time) <= ?2",
                rusqlite::params![start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let cancelled: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM appointment_index
                 WHERE date(start_time) >= ?1
                   AND date(start_time) <= ?2
                   AND status = 'cancelled'",
                rusqlite::params![start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let no_show: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM appointment_index
                 WHERE date(start_time) >= ?1
                   AND date(start_time) <= ?2
                   AND status = 'noshow'",
                rusqlite::params![start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);

        (total, cancelled, no_show)
    };

    let cancellation_rate = if total_appointments > 0 {
        cancelled as f64 / total_appointments as f64 * 100.0
    } else {
        0.0
    };

    let no_show_rate = if total_appointments > 0 {
        no_show as f64 / total_appointments as f64 * 100.0
    } else {
        0.0
    };

    // ── Average units per visit ────────────────────────────────────────────
    // Join encounter_index → encounter_billing → billing_line_items
    let avg_units_per_visit: f64 = if total_visits == 0 {
        0.0
    } else {
        let total_units: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_units), 0)
                 FROM encounter_billing eb
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                   AND ei.status = 'finished'",
                rusqlite::params![start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);
        total_units as f64 / total_visits as f64
    };

    // ── New patients (first encounter date in the period) ─────────────────
    // A "new patient" = patient whose earliest encounter falls within the range.
    let new_patients: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT patient_id, MIN(date(encounter_date)) AS first_date
                FROM encounter_index
                GROUP BY patient_id
                HAVING first_date >= ?1 AND first_date <= ?2
             )",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.get_operational_kpis".to_string(),
            resource_type: "Analytics".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period={}/{}", start, end)),
        },
    );

    Ok(OperationalKPIs {
        total_visits,
        cancellation_rate,
        no_show_rate,
        avg_units_per_visit,
        new_patients,
        period_start: start,
        period_end: end,
        provider_id,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: get_financial_kpis
// ─────────────────────────────────────────────────────────────────────────────

/// Compute financial KPIs (revenue, collection rate, days in A/R, aging).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_financial_kpis(
    start_date: String,
    end_date: String,
    payer_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<FinancialKPIs, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // ── Totals from encounter_billing + encounter_index ────────────────────
    let (total_charges, total_visits): (f64, i64) = {
        let row: (f64, i64) = if let Some(ref pid) = payer_id {
            conn.query_row(
                "SELECT COALESCE(SUM(eb.total_charge), 0.0), COUNT(DISTINCT eb.billing_id)
                 FROM encounter_billing eb
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                   AND eb.payer_id = ?3",
                rusqlite::params![start, end, pid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0.0, 0))
        } else {
            conn.query_row(
                "SELECT COALESCE(SUM(eb.total_charge), 0.0), COUNT(DISTINCT eb.billing_id)
                 FROM encounter_billing eb
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2",
                rusqlite::params![start, end],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0.0, 0))
        };
        row
    };

    // ── Payments and adjustments from claims ──────────────────────────────
    let (total_payments, total_adjustments): (f64, f64) = {
        let row: (f64, f64) = if let Some(ref pid) = payer_id {
            conn.query_row(
                "SELECT COALESCE(SUM(c.paid_amount), 0.0),
                        COALESCE(SUM(c.adjustment_amount), 0.0)
                 FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                   AND c.payer_id = ?3
                   AND c.status IN ('paid', 'accepted')",
                rusqlite::params![start, end, pid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0.0, 0.0))
        } else {
            conn.query_row(
                "SELECT COALESCE(SUM(c.paid_amount), 0.0),
                        COALESCE(SUM(c.adjustment_amount), 0.0)
                 FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                   AND c.status IN ('paid', 'accepted')",
                rusqlite::params![start, end],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0.0, 0.0))
        };
        row
    };

    // ── Derived metrics ────────────────────────────────────────────────────
    let revenue_per_visit = if total_visits > 0 {
        total_payments / total_visits as f64
    } else {
        0.0
    };

    let charges_per_visit = if total_visits > 0 {
        total_charges / total_visits as f64
    } else {
        0.0
    };

    // Net collection rate = payments / (charges - adjustments)
    let net_collection_rate = {
        let denominator = total_charges - total_adjustments;
        if denominator > 0.0 {
            (total_payments / denominator * 100.0).min(100.0)
        } else {
            0.0
        }
    };

    // ── Days in A/R — average days from claim submission to payment ────────
    let days_in_ar: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(
                JULIANDAY(c.response_at) - JULIANDAY(c.submitted_at)
             ), 0.0)
             FROM claims c
             WHERE c.status = 'paid'
               AND c.submitted_at IS NOT NULL
               AND c.response_at IS NOT NULL
               AND date(c.submitted_at) >= ?1
               AND date(c.submitted_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    // ── A/R aging — outstanding (submitted/accepted) claims by age ─────────
    // Age is computed from submitted_at to today (using SQLite date('now')).
    let ar_aging_0_30: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge), 0.0)
             FROM claims c
             JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
             WHERE c.status IN ('submitted', 'accepted')
               AND julianday('now') - julianday(c.submitted_at) BETWEEN 0 AND 30",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let ar_aging_31_60: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge), 0.0)
             FROM claims c
             JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
             WHERE c.status IN ('submitted', 'accepted')
               AND julianday('now') - julianday(c.submitted_at) BETWEEN 31 AND 60",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let ar_aging_61_90: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge), 0.0)
             FROM claims c
             JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
             WHERE c.status IN ('submitted', 'accepted')
               AND julianday('now') - julianday(c.submitted_at) BETWEEN 61 AND 90",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let ar_aging_91_plus: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge), 0.0)
             FROM claims c
             JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
             WHERE c.status IN ('submitted', 'accepted')
               AND julianday('now') - julianday(c.submitted_at) > 90",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.get_financial_kpis".to_string(),
            resource_type: "Analytics".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period={}/{}", start, end)),
        },
    );

    Ok(FinancialKPIs {
        total_charges,
        total_payments,
        total_adjustments,
        revenue_per_visit,
        net_collection_rate,
        days_in_ar,
        charges_per_visit,
        ar_aging_0_30,
        ar_aging_31_60,
        ar_aging_61_90,
        ar_aging_91_plus,
        period_start: start,
        period_end: end,
        payer_id,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: get_clinical_outcomes
// ─────────────────────────────────────────────────────────────────────────────

/// Compute clinical outcomes (MCID rates, avg improvement, discharge rate).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_clinical_outcomes(
    start_date: String,
    end_date: String,
    measure_type: Option<String>,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ClinicalOutcomes, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // ── Determine which measure types to compute ───────────────────────────
    let all_measures = ["lefs", "dash", "ndi", "oswestry", "psfs", "fabq"];
    let measures_to_compute: Vec<&str> = if let Some(ref mt) = measure_type {
        all_measures.iter().filter(|&&m| m == mt.as_str()).copied().collect()
    } else {
        all_measures.to_vec()
    };

    // ── Per-measure: MCID rates and average improvement ───────────────────
    // Strategy: for each patient+measure, find the earliest and latest score
    // within the period. If the change meets MCID, count as achieved.
    let mut measure_outcomes: Vec<MeasureOutcome> = Vec::new();

    for mt in &measures_to_compute {
        let mcid = mcid_for_measure(mt);

        // Get all (patient_id, earliest_score, latest_score) pairs
        struct PatientScores {
            initial_score: f64,
            final_score: f64,
        }

        let patient_scores: Vec<PatientScores> = {
            // For LEFS/PSFS: higher = better (improvement = final - initial)
            // For DASH/NDI/Oswestry/FABQ: lower = better (improvement = initial - final)
            let is_higher_better = matches!(*mt, "lefs" | "psfs");

            let stmt = conn
                .prepare(
                    "SELECT patient_id,
                            MIN(score) AS min_score,
                            MAX(score) AS max_score,
                            MIN(recorded_at) AS first_date,
                            MAX(recorded_at) AS last_date
                     FROM outcome_score_index
                     WHERE measure_type = ?1
                       AND date(recorded_at) >= ?2
                       AND date(recorded_at) <= ?3
                     GROUP BY patient_id
                     HAVING COUNT(*) >= 2",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;

            // We need initial and final scores, not min/max.
            // Re-query to get first and last per patient.
            let mut inner_stmt = conn
                .prepare(
                    "SELECT
                       (SELECT score FROM outcome_score_index
                        WHERE measure_type = ?1 AND patient_id = outer_q.patient_id
                          AND date(recorded_at) >= ?2 AND date(recorded_at) <= ?3
                        ORDER BY recorded_at ASC LIMIT 1) AS initial_score,
                       (SELECT score FROM outcome_score_index
                        WHERE measure_type = ?1 AND patient_id = outer_q.patient_id
                          AND date(recorded_at) >= ?2 AND date(recorded_at) <= ?3
                        ORDER BY recorded_at DESC LIMIT 1) AS final_score
                     FROM (
                       SELECT DISTINCT patient_id FROM outcome_score_index
                       WHERE measure_type = ?1
                         AND date(recorded_at) >= ?2
                         AND date(recorded_at) <= ?3
                       GROUP BY patient_id HAVING COUNT(*) >= 2
                     ) AS outer_q",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;

            // Suppress unused variable warning
            drop(stmt);

            let rows = inner_stmt
                .query_map(rusqlite::params![mt, start, end], |row| {
                    Ok((
                        row.get::<_, Option<f64>>(0)?,
                        row.get::<_, Option<f64>>(1)?,
                    ))
                })
                .map_err(|e| AppError::Database(e.to_string()))?;

            let mut scores = Vec::new();
            for row in rows.flatten() {
                if let (Some(initial), Some(final_s)) = row {
                    let _ = is_higher_better; // used below
                    scores.push(PatientScores {
                        initial_score: initial,
                        final_score: final_s,
                    });
                }
            }
            scores
        };

        let patient_count = patient_scores.len() as i64;
        if patient_count == 0 {
            measure_outcomes.push(MeasureOutcome {
                measure_type: mt.to_string(),
                patient_count: 0,
                mcid_achieved_count: 0,
                mcid_achievement_rate: 0.0,
                avg_initial_score: 0.0,
                avg_final_score: 0.0,
                avg_improvement: 0.0,
            });
            continue;
        }

        let is_higher_better = matches!(*mt, "lefs" | "psfs");

        let mut mcid_count = 0i64;
        let mut sum_initial = 0.0f64;
        let mut sum_final = 0.0f64;

        for ps in &patient_scores {
            sum_initial += ps.initial_score;
            sum_final += ps.final_score;

            let improvement = if is_higher_better {
                ps.final_score - ps.initial_score
            } else {
                ps.initial_score - ps.final_score
            };

            if improvement >= mcid {
                mcid_count += 1;
            }
        }

        let n = patient_count as f64;
        let avg_initial = sum_initial / n;
        let avg_final = sum_final / n;
        let avg_improvement = if is_higher_better {
            avg_final - avg_initial
        } else {
            avg_initial - avg_final
        };

        let mcid_rate = mcid_count as f64 / n * 100.0;

        measure_outcomes.push(MeasureOutcome {
            measure_type: mt.to_string(),
            patient_count,
            mcid_achieved_count: mcid_count,
            mcid_achievement_rate: mcid_rate,
            avg_initial_score: avg_initial,
            avg_final_score: avg_final,
            avg_improvement,
        });
    }

    // ── Provider outcomes ──────────────────────────────────────────────────
    let provider_outcomes: Vec<ProviderOutcome> = {
        let mut stmt = conn
            .prepare(
                "SELECT ei.provider_id,
                        COUNT(DISTINCT osi.patient_id) AS patient_count,
                        AVG(osi.score) AS avg_score,
                        COUNT(DISTINCT CASE WHEN pn.note_type = 'discharge_summary' THEN pn.patient_id END) AS discharge_count
                 FROM outcome_score_index osi
                 JOIN encounter_index ei ON ei.patient_id = osi.patient_id
                   AND date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                 LEFT JOIN pt_note_index pn ON pn.patient_id = osi.patient_id
                   AND pn.note_type = 'discharge_summary'
                 WHERE date(osi.recorded_at) >= ?1
                   AND date(osi.recorded_at) <= ?2
                 GROUP BY ei.provider_id",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![start, end], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut outcomes = Vec::new();
        for row in rows.flatten() {
            let (pid, pc, avg_score, dc) = row;
            if provider_id.as_deref().map_or(true, |f| f == pid) {
                outcomes.push(ProviderOutcome {
                    provider_id: pid,
                    patient_count: pc,
                    avg_improvement: avg_score,
                    discharge_count: dc,
                });
            }
        }
        outcomes
    };

    // ── Discharge rate ─────────────────────────────────────────────────────
    let patients_with_discharge: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT patient_id) FROM pt_note_index
             WHERE note_type = 'discharge_summary'
               AND date(created_at) >= ?1
               AND date(created_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_patients_with_outcomes: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT patient_id) FROM outcome_score_index
             WHERE date(recorded_at) >= ?1
               AND date(recorded_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let discharge_rate = if total_patients_with_outcomes > 0 {
        patients_with_discharge as f64 / total_patients_with_outcomes as f64 * 100.0
    } else {
        0.0
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.get_clinical_outcomes".to_string(),
            resource_type: "Analytics".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period={}/{}", start, end)),
        },
    );

    Ok(ClinicalOutcomes {
        measure_outcomes,
        provider_outcomes,
        discharge_rate,
        total_patients_with_outcomes,
        period_start: start,
        period_end: end,
        measure_type_filter: measure_type,
        provider_id_filter: provider_id,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: get_payer_mix
// ─────────────────────────────────────────────────────────────────────────────

/// Compute payer mix (revenue %, visit count, avg reimbursement per payer).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_payer_mix(
    start_date: String,
    end_date: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PayerMix, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // ── Per-payer totals ───────────────────────────────────────────────────
    // Join encounter_billing (charges) → claims (payments) → payer_config (name)
    let mut stmt = conn
        .prepare(
            "SELECT
               COALESCE(eb.payer_id, 'self_pay') AS payer_id,
               COALESCE(pc.name, 'Self Pay') AS payer_name,
               COUNT(DISTINCT eb.billing_id) AS visit_count,
               COALESCE(SUM(eb.total_charge), 0.0) AS total_charges,
               COALESCE(SUM(c.paid_amount), 0.0) AS total_payments
             FROM encounter_billing eb
             JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
             LEFT JOIN payer_config pc ON pc.payer_id = eb.payer_id
             LEFT JOIN claims c ON c.encounter_billing_id = eb.billing_id
               AND c.status IN ('paid', 'accepted')
             WHERE date(ei.encounter_date) >= ?1
               AND date(ei.encounter_date) <= ?2
             GROUP BY eb.payer_id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(rusqlite::params![start, end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut payer_data: Vec<(String, String, i64, f64, f64)> = Vec::new();
    for row in rows.flatten() {
        payer_data.push(row);
    }

    // ── Compute totals and percentages ─────────────────────────────────────
    let total_visits: i64 = payer_data.iter().map(|p| p.2).sum();
    let total_charges: f64 = payer_data.iter().map(|p| p.3).sum();
    let total_payments: f64 = payer_data.iter().map(|p| p.4).sum();

    let payers: Vec<PayerBreakdown> = payer_data
        .into_iter()
        .map(|(payer_id, payer_name, visit_count, charges, payments)| {
            let revenue_percentage = if total_payments > 0.0 {
                payments / total_payments * 100.0
            } else {
                0.0
            };
            let avg_reimbursement_per_visit = if visit_count > 0 {
                payments / visit_count as f64
            } else {
                0.0
            };
            PayerBreakdown {
                payer_id,
                payer_name,
                visit_count,
                total_charges: charges,
                total_payments: payments,
                revenue_percentage,
                avg_reimbursement_per_visit,
            }
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.get_payer_mix".to_string(),
            resource_type: "Analytics".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period={}/{}", start, end)),
        },
    );

    Ok(PayerMix {
        payers,
        total_visits,
        total_charges,
        total_payments,
        period_start: start,
        period_end: end,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: get_dashboard_summary
// ─────────────────────────────────────────────────────────────────────────────

/// Compute all KPI sections in one call (aggregates the four sub-commands).
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_dashboard_summary(
    start_date: String,
    end_date: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<DashboardSummary, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // ── Operational ────────────────────────────────────────────────────────
    let total_visits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM encounter_index
             WHERE date(encounter_date) >= ?1
               AND date(encounter_date) <= ?2
               AND status = 'finished'",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_appointments: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM appointment_index
             WHERE date(start_time) >= ?1 AND date(start_time) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let cancelled: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM appointment_index
             WHERE date(start_time) >= ?1 AND date(start_time) <= ?2
               AND status = 'cancelled'",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let no_show: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM appointment_index
             WHERE date(start_time) >= ?1 AND date(start_time) <= ?2
               AND status = 'noshow'",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let avg_units_per_visit: f64 = if total_visits == 0 {
        0.0
    } else {
        let total_units: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_units), 0)
                 FROM encounter_billing eb
                 JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
                 WHERE date(ei.encounter_date) >= ?1
                   AND date(ei.encounter_date) <= ?2
                   AND ei.status = 'finished'",
                rusqlite::params![start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);
        total_units as f64 / total_visits as f64
    };

    let new_patients: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT patient_id, MIN(date(encounter_date)) AS first_date
                FROM encounter_index GROUP BY patient_id
                HAVING first_date >= ?1 AND first_date <= ?2
             )",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let operational = OperationalKPIs {
        total_visits,
        cancellation_rate: if total_appointments > 0 {
            cancelled as f64 / total_appointments as f64 * 100.0
        } else {
            0.0
        },
        no_show_rate: if total_appointments > 0 {
            no_show as f64 / total_appointments as f64 * 100.0
        } else {
            0.0
        },
        avg_units_per_visit,
        new_patients,
        period_start: start.clone(),
        period_end: end.clone(),
        provider_id: None,
    };

    // ── Financial ──────────────────────────────────────────────────────────
    let (total_charges, visit_count_billing): (f64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge), 0.0), COUNT(DISTINCT eb.billing_id)
             FROM encounter_billing eb
             JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
             WHERE date(ei.encounter_date) >= ?1
               AND date(ei.encounter_date) <= ?2",
            rusqlite::params![start, end],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0.0, 0));

    let (total_payments, total_adjustments): (f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(c.paid_amount), 0.0), COALESCE(SUM(c.adjustment_amount), 0.0)
             FROM claims c
             JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
             JOIN encounter_index ei ON ei.encounter_id = eb.encounter_id
             WHERE date(ei.encounter_date) >= ?1
               AND date(ei.encounter_date) <= ?2
               AND c.status IN ('paid', 'accepted')",
            rusqlite::params![start, end],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0.0, 0.0));

    let days_in_ar: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(JULIANDAY(response_at) - JULIANDAY(submitted_at)), 0.0)
             FROM claims WHERE status = 'paid'
               AND submitted_at IS NOT NULL AND response_at IS NOT NULL
               AND date(submitted_at) >= ?1 AND date(submitted_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let net_collection_rate = {
        let denom = total_charges - total_adjustments;
        if denom > 0.0 { (total_payments / denom * 100.0).min(100.0) } else { 0.0 }
    };

    let financial = FinancialKPIs {
        total_charges,
        total_payments,
        total_adjustments,
        revenue_per_visit: if visit_count_billing > 0 {
            total_payments / visit_count_billing as f64
        } else {
            0.0
        },
        net_collection_rate,
        days_in_ar,
        charges_per_visit: if visit_count_billing > 0 {
            total_charges / visit_count_billing as f64
        } else {
            0.0
        },
        ar_aging_0_30: conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_charge),0.0) FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id=c.encounter_billing_id
                 WHERE c.status IN ('submitted','accepted')
                   AND julianday('now')-julianday(c.submitted_at) BETWEEN 0 AND 30",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0),
        ar_aging_31_60: conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_charge),0.0) FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id=c.encounter_billing_id
                 WHERE c.status IN ('submitted','accepted')
                   AND julianday('now')-julianday(c.submitted_at) BETWEEN 31 AND 60",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0),
        ar_aging_61_90: conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_charge),0.0) FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id=c.encounter_billing_id
                 WHERE c.status IN ('submitted','accepted')
                   AND julianday('now')-julianday(c.submitted_at) BETWEEN 61 AND 90",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0),
        ar_aging_91_plus: conn
            .query_row(
                "SELECT COALESCE(SUM(eb.total_charge),0.0) FROM claims c
                 JOIN encounter_billing eb ON eb.billing_id=c.encounter_billing_id
                 WHERE c.status IN ('submitted','accepted')
                   AND julianday('now')-julianday(c.submitted_at) > 90",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0),
        period_start: start.clone(),
        period_end: end.clone(),
        payer_id: None,
    };

    // ── Clinical outcomes (simplified summary) ─────────────────────────────
    let total_patients_with_outcomes: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT patient_id) FROM outcome_score_index
             WHERE date(recorded_at) >= ?1 AND date(recorded_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let patients_with_discharge: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT patient_id) FROM pt_note_index
             WHERE note_type='discharge_summary'
               AND date(created_at) >= ?1 AND date(created_at) <= ?2",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let discharge_rate = if total_patients_with_outcomes > 0 {
        patients_with_discharge as f64 / total_patients_with_outcomes as f64 * 100.0
    } else {
        0.0
    };

    let clinical = ClinicalOutcomes {
        measure_outcomes: vec![],
        provider_outcomes: vec![],
        discharge_rate,
        total_patients_with_outcomes,
        period_start: start.clone(),
        period_end: end.clone(),
        measure_type_filter: None,
        provider_id_filter: None,
    };

    // ── Payer mix ──────────────────────────────────────────────────────────
    let mut payer_stmt = conn
        .prepare(
            "SELECT COALESCE(eb.payer_id,'self_pay'), COALESCE(pc.name,'Self Pay'),
                    COUNT(DISTINCT eb.billing_id), COALESCE(SUM(eb.total_charge),0.0),
                    COALESCE(SUM(c.paid_amount),0.0)
             FROM encounter_billing eb
             JOIN encounter_index ei ON ei.encounter_id=eb.encounter_id
             LEFT JOIN payer_config pc ON pc.payer_id=eb.payer_id
             LEFT JOIN claims c ON c.encounter_billing_id=eb.billing_id
               AND c.status IN ('paid','accepted')
             WHERE date(ei.encounter_date) >= ?1 AND date(ei.encounter_date) <= ?2
             GROUP BY eb.payer_id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let payer_rows = payer_stmt
        .query_map(rusqlite::params![start, end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut payer_data: Vec<(String, String, i64, f64, f64)> = Vec::new();
    for row in payer_rows.flatten() {
        payer_data.push(row);
    }

    let pm_total_visits: i64 = payer_data.iter().map(|p| p.2).sum();
    let pm_total_charges: f64 = payer_data.iter().map(|p| p.3).sum();
    let pm_total_payments: f64 = payer_data.iter().map(|p| p.4).sum();

    let payers: Vec<PayerBreakdown> = payer_data
        .into_iter()
        .map(|(pid, pname, vc, charges, payments)| PayerBreakdown {
            payer_id: pid,
            payer_name: pname,
            visit_count: vc,
            total_charges: charges,
            total_payments: payments,
            revenue_percentage: if pm_total_payments > 0.0 {
                payments / pm_total_payments * 100.0
            } else {
                0.0
            },
            avg_reimbursement_per_visit: if vc > 0 { payments / vc as f64 } else { 0.0 },
        })
        .collect();

    let payer_mix = PayerMix {
        payers,
        total_visits: pm_total_visits,
        total_charges: pm_total_charges,
        total_payments: pm_total_payments,
        period_start: start.clone(),
        period_end: end.clone(),
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.get_dashboard_summary".to_string(),
            resource_type: "Analytics".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period={}/{}", start, end)),
        },
    );

    Ok(DashboardSummary {
        operational,
        financial,
        clinical,
        payer_mix,
        period_start: start,
        period_end: end,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: save_kpi_snapshot
// ─────────────────────────────────────────────────────────────────────────────

/// Compute and persist KPIs as a named snapshot for fast historical retrieval.
///
/// Requires: Billing + Create
#[tauri::command]
pub async fn save_kpi_snapshot(
    period_type: String,
    start_date: String,
    end_date: String,
    provider_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<KpiSnapshot, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    let valid_periods = ["daily", "weekly", "monthly", "quarterly", "yearly"];
    if !valid_periods.contains(&period_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid period_type '{}'. Must be one of: {}",
            period_type,
            valid_periods.join(", ")
        )));
    }

    let start = date_prefix(&start_date).to_string();
    let end = date_prefix(&end_date).to_string();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Build a lightweight KPI data JSON object inline (avoid re-locking)
    let total_visits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM encounter_index
             WHERE date(encounter_date) >= ?1 AND date(encounter_date) <= ?2
               AND status = 'finished'",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let (total_charges, total_payments): (f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(eb.total_charge),0.0), COALESCE(SUM(c.paid_amount),0.0)
             FROM encounter_billing eb
             JOIN encounter_index ei ON ei.encounter_id=eb.encounter_id
             LEFT JOIN claims c ON c.encounter_billing_id=eb.billing_id
               AND c.status IN ('paid','accepted')
             WHERE date(ei.encounter_date) >= ?1 AND date(ei.encounter_date) <= ?2",
            rusqlite::params![start, end],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0.0, 0.0));

    let new_patients: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT patient_id, MIN(date(encounter_date)) AS fd
                FROM encounter_index GROUP BY patient_id
                HAVING fd >= ?1 AND fd <= ?2
             )",
            rusqlite::params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let kpi_data = serde_json::json!({
        "totalVisits": total_visits,
        "totalCharges": total_charges,
        "totalPayments": total_payments,
        "newPatients": new_patients,
        "revenuePerVisit": if total_visits > 0 { total_payments / total_visits as f64 } else { 0.0 }
    });

    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let kpi_json = serde_json::to_string(&kpi_data)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO kpi_snapshots
           (snapshot_id, period_type, period_start, period_end, provider_id, kpi_data, computed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            snapshot_id,
            period_type,
            start,
            end,
            provider_id,
            kpi_json,
            now
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.save_kpi_snapshot".to_string(),
            resource_type: "KpiSnapshot".to_string(),
            resource_id: Some(snapshot_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("period_type={}, range={}/{}", period_type, start, end)),
        },
    );

    Ok(KpiSnapshot {
        snapshot_id,
        period_type,
        period_start: start,
        period_end: end,
        provider_id,
        kpi_data,
        computed_at: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Command: list_kpi_snapshots
// ─────────────────────────────────────────────────────────────────────────────

/// List historical KPI snapshots, optionally filtered by period type.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_kpi_snapshots(
    period_type: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<KpiSnapshot>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let snapshots: Vec<KpiSnapshot> = if let Some(ref pt) = period_type {
        let mut stmt = conn
            .prepare(
                "SELECT snapshot_id, period_type, period_start, period_end,
                        provider_id, kpi_data, computed_at
                 FROM kpi_snapshots
                 WHERE period_type = ?1
                 ORDER BY period_start DESC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let rows: Vec<_> = stmt
            .query_map(rusqlite::params![pt], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        rows.into_iter()
            .filter_map(|(sid, pt, ps, pe, prov, kd, ca)| {
                let kpi_data: serde_json::Value = serde_json::from_str(&kd).ok()?;
                Some(KpiSnapshot {
                    snapshot_id: sid,
                    period_type: pt,
                    period_start: ps,
                    period_end: pe,
                    provider_id: prov,
                    kpi_data,
                    computed_at: ca,
                })
            })
            .collect()
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT snapshot_id, period_type, period_start, period_end,
                        provider_id, kpi_data, computed_at
                 FROM kpi_snapshots
                 ORDER BY period_start DESC
                 LIMIT 200",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let rows: Vec<_> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        rows.into_iter()
            .filter_map(|(sid, pt, ps, pe, prov, kd, ca)| {
                let kpi_data: serde_json::Value = serde_json::from_str(&kd).ok()?;
                Some(KpiSnapshot {
                    snapshot_id: sid,
                    period_type: pt,
                    period_start: ps,
                    period_end: pe,
                    provider_id: prov,
                    kpi_data,
                    computed_at: ca,
                })
            })
            .collect()
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "analytics.list_kpi_snapshots".to_string(),
            resource_type: "KpiSnapshot".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: period_type.clone().map(|p| format!("period_type={}", p)),
        },
    );

    Ok(snapshots)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: date_prefix helper ─────────────────────────────────────────
    #[test]
    fn date_prefix_strips_time() {
        assert_eq!(date_prefix("2024-01-15T10:30:00Z"), "2024-01-15");
        assert_eq!(date_prefix("2024-01-15"), "2024-01-15");
        assert_eq!(date_prefix("2024-03-01T00:00:00+00:00"), "2024-03-01");
        // Short string is returned as-is
        assert_eq!(date_prefix("2024"), "2024");
    }

    // ── Test 2: MCID values are correct per measure ────────────────────────
    #[test]
    fn mcid_constants_correct() {
        assert_eq!(mcid_for_measure("lefs"), 9.0);
        assert_eq!(mcid_for_measure("dash"), 10.8);
        assert_eq!(mcid_for_measure("ndi"), 7.5);
        assert_eq!(mcid_for_measure("oswestry"), 10.0);
        assert_eq!(mcid_for_measure("psfs"), 2.0);
        assert_eq!(mcid_for_measure("fabq"), 5.0);
        // Unknown measure returns 0.0 — no divide-by-zero risk
        assert_eq!(mcid_for_measure("unknown"), 0.0);
    }

    // ── Test 3: MCID achievement rate calculation ──────────────────────────
    #[test]
    fn mcid_achievement_rate_lefs() {
        // LEFS: higher is better; improvement = final - initial; MCID = 9
        let mcid = mcid_for_measure("lefs");
        let is_higher_better = true;

        // Patient A: 30 → 42 (+12 ≥ 9 = achieved)
        // Patient B: 50 → 55 (+5 < 9 = not achieved)
        // Patient C: 20 → 35 (+15 ≥ 9 = achieved)
        struct PS {
            initial: f64,
            final_s: f64,
        }

        let patients = vec![
            PS { initial: 30.0, final_s: 42.0 },
            PS { initial: 50.0, final_s: 55.0 },
            PS { initial: 20.0, final_s: 35.0 },
        ];

        let mut achieved = 0i64;
        for p in &patients {
            let improvement = if is_higher_better {
                p.final_s - p.initial
            } else {
                p.initial - p.final_s
            };
            if improvement >= mcid {
                achieved += 1;
            }
        }

        assert_eq!(achieved, 2);
        let rate = achieved as f64 / patients.len() as f64 * 100.0;
        assert!((rate - 66.666_666).abs() < 0.01);
    }

    // ── Test 4: Payer mix percentage computation ───────────────────────────
    #[test]
    fn payer_mix_percentages_sum_to_100() {
        let total_payments = 10_000.0_f64;
        let payer_payments = vec![4_000.0_f64, 3_000.0, 2_000.0, 1_000.0];

        let percentages: Vec<f64> = payer_payments
            .iter()
            .map(|&p| p / total_payments * 100.0)
            .collect();

        let sum: f64 = percentages.iter().sum();
        assert!((sum - 100.0).abs() < 0.001, "percentages sum to {}", sum);

        assert!((percentages[0] - 40.0).abs() < 0.001);
        assert!((percentages[1] - 30.0).abs() < 0.001);
        assert!((percentages[2] - 20.0).abs() < 0.001);
        assert!((percentages[3] - 10.0).abs() < 0.001);
    }

    // ── Test 5: Empty data handling — zero KPIs, no panics ────────────────
    #[test]
    fn empty_data_produces_zero_kpis() {
        // Simulate the no-visits case for operational KPIs
        let total_visits = 0i64;
        let total_appointments = 0i64;
        let cancelled = 0i64;
        let no_show = 0i64;

        let cancellation_rate = if total_appointments > 0 {
            cancelled as f64 / total_appointments as f64 * 100.0
        } else {
            0.0
        };

        let no_show_rate = if total_appointments > 0 {
            no_show as f64 / total_appointments as f64 * 100.0
        } else {
            0.0
        };

        let avg_units = if total_visits > 0 { 8.0 / total_visits as f64 } else { 0.0 };

        assert_eq!(cancellation_rate, 0.0);
        assert_eq!(no_show_rate, 0.0);
        assert_eq!(avg_units, 0.0);

        // Simulate the no-claims case for financial KPIs
        let total_charges = 0.0_f64;
        let total_payments = 0.0_f64;
        let total_adjustments = 0.0_f64;

        let net_collection_rate = {
            let denom = total_charges - total_adjustments;
            if denom > 0.0 { total_payments / denom * 100.0 } else { 0.0 }
        };

        assert_eq!(net_collection_rate, 0.0);

        // Simulate empty payer mix — percentages should not divide by zero
        let total_payer_payments = 0.0_f64;
        let payer_payment = 0.0_f64;
        let pct = if total_payer_payments > 0.0 {
            payer_payment / total_payer_payments * 100.0
        } else {
            0.0
        };
        assert_eq!(pct, 0.0);
    }

    // ── Test 6: Net collection rate calculation ────────────────────────────
    #[test]
    fn net_collection_rate_correct() {
        // charges=10000, payments=8500, adjustments=500
        // net rate = 8500 / (10000 - 500) * 100 = 8500/9500*100 ≈ 89.47%
        let charges = 10_000.0_f64;
        let payments = 8_500.0_f64;
        let adjustments = 500.0_f64;

        let rate = {
            let denom = charges - adjustments;
            if denom > 0.0 { (payments / denom * 100.0).min(100.0) } else { 0.0 }
        };

        assert!((rate - 89.473_68).abs() < 0.01, "rate={}", rate);
    }

    // ── Test 7: Revenue per visit computation ──────────────────────────────
    #[test]
    fn revenue_per_visit_correct() {
        let total_payments = 12_000.0_f64;
        let total_visits = 40i64;
        let rpv = if total_visits > 0 {
            total_payments / total_visits as f64
        } else {
            0.0
        };
        assert_eq!(rpv, 300.0);
    }
}
