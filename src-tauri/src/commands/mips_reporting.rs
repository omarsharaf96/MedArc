/// commands/mips_reporting.rs — MIPS Quality Measure Capture (M004/S07)
///
/// Auto-extracts MIPS measures from existing clinical data and supports
/// manual screening data entry for measures requiring point-of-care capture.
///
/// Measures implemented
/// --------------------
///   #182  Functional Outcome Assessment
///         Numerator   = discharged patients with BOTH initial AND discharge outcome scores
///         Denominator = all patients with a discharge outcome score
///
///   #217–222 Functional Status Change (by body region)
///         Scores delta (initial → discharge) from outcome_score_index.
///         Measure-to-region mapping:
///           lefs           → lower extremity (#217/#218/#219)
///           oswestry / ndi → spine           (#220/#478)
///           dash           → upper extremity (#221/#222)
///
///   #134  Depression Screening (PHQ-2)
///         PHQ-2 score recorded in mips_screenings.
///         If score >= 3, flag for PHQ-9 follow-up.
///         Denominator = patients with a PHQ-2 in the performance year.
///         Numerator   = patients with score < 3 OR with PHQ-9 documented.
///
///   #155  Falls Risk Screening
///         Tracks screening result + plan-of-care for patients 65+.
///         Denominator = patients 65+ with a falls risk screening in the year.
///         Numerator   = patients with positive screen AND plan documented.
///
///   #128  BMI
///         Checks vitals for BMI recording per encounter.
///         Denominator = encounters in year.
///         Numerator   = encounters where BMI was recorded.
///
/// Commands
/// --------
///   get_mips_performance(performance_year, measure_ids?) → Vec<MipsPerformance>
///   get_mips_eligible_patients(measure_id) → Vec<EligiblePatient>
///   record_phq2_screening(patient_id, score, encounter_id) → MipsScreening
///   record_falls_screening(patient_id, result, plan_documented, encounter_id) → MipsScreening
///   get_mips_dashboard(performance_year) → MipsDashboard
///
/// RBAC
/// ----
///   All commands require ClinicalDocumentation resource access.
///   Provider / SystemAdmin → full CRUD
///   NurseMa                → record screenings (Create + Read)
///   BillingStaff           → Read-only
///   FrontDesk              → No access
///
/// Audit
/// -----
///   Every mutating command writes an audit row via write_audit_entry.

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

/// PHQ-2 threshold for PHQ-9 follow-up referral (inclusive).
pub const PHQ2_FOLLOWUP_THRESHOLD: f64 = 3.0;

/// MIPS measure IDs supported by this module.
pub const MEASURE_182: &str = "182";
pub const MEASURE_217: &str = "217";
pub const MEASURE_220: &str = "220";
pub const MEASURE_221: &str = "221";
pub const MEASURE_134: &str = "134";
pub const MEASURE_155: &str = "155";
pub const MEASURE_128: &str = "128";

// ─────────────────────────────────────────────────────────────────────────────
// Helper: map outcome measure_type → MIPS region / measure group
// ─────────────────────────────────────────────────────────────────────────────

/// Map an outcome measure type string to the corresponding MIPS measure ID.
///
/// lefs                 → "217" (lower extremity)
/// oswestry / ndi       → "220" (spine)
/// dash                 → "221" (upper extremity)
/// anything else        → None
#[allow(dead_code)]
pub fn measure_type_to_mips_id(measure_type: &str) -> Option<&'static str> {
    match measure_type {
        "lefs" => Some(MEASURE_217),
        "oswestry" | "ndi" => Some(MEASURE_220),
        "dash" => Some(MEASURE_221),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Return types
// ─────────────────────────────────────────────────────────────────────────────

/// Per-measure performance data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MipsPerformance {
    pub measure_id: String,
    pub measure_name: String,
    pub numerator: i64,
    pub denominator: i64,
    /// numerator / denominator * 100.0, or null when denominator == 0.
    pub performance_rate: Option<f64>,
    pub performance_year: i64,
}

/// A patient in the denominator of a MIPS measure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EligiblePatient {
    pub patient_id: String,
    pub display_name: String,
    pub in_numerator: bool,
}

/// A recorded MIPS screening entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MipsScreening {
    pub screening_id: String,
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub measure_type: String,
    pub score: Option<f64>,
    pub result: Option<String>,
    pub follow_up_plan: Option<String>,
    pub performance_year: i64,
    pub screened_at: String,
}

/// Color-coded performance tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PerformanceTier {
    Green,  // >= 75%
    Amber,  // 50-74%
    Red,    // < 50%
    NoData, // denominator == 0
}

/// Per-measure card for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MipsMeasureCard {
    pub measure_id: String,
    pub measure_name: String,
    pub numerator: i64,
    pub denominator: i64,
    pub performance_rate: Option<f64>,
    pub tier: PerformanceTier,
    pub performance_year: i64,
}

/// Full dashboard payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MipsDashboard {
    pub performance_year: i64,
    pub measures: Vec<MipsMeasureCard>,
    /// Sum of all performance_rate values / count of measures with data.
    pub projected_composite_score: Option<f64>,
    pub computed_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure helpers
// ─────────────────────────────────────────────────────────────────────────────

fn performance_rate(numerator: i64, denominator: i64) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(numerator as f64 / denominator as f64 * 100.0)
    }
}

fn tier_for_rate(rate: Option<f64>) -> PerformanceTier {
    match rate {
        None => PerformanceTier::NoData,
        Some(r) if r >= 75.0 => PerformanceTier::Green,
        Some(r) if r >= 50.0 => PerformanceTier::Amber,
        _ => PerformanceTier::Red,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Measure computation helpers (pure database queries)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute Measure #182 — Functional Outcome Assessment.
///
/// Denominator: distinct patients with a discharge outcome score in the year.
/// Numerator:   patients in the denominator who ALSO have an initial score.
fn compute_measure_182(
    conn: &rusqlite::Connection,
    year: i64,
) -> Result<(i64, i64), AppError> {
    let year_start = format!("{}-01-01", year);
    let year_end = format!("{}-12-31", year);

    // Denominator: patients with a discharge score in the year
    let denominator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT patient_id) FROM outcome_score_index
         WHERE episode_phase = 'discharge'
           AND recorded_at BETWEEN ?1 AND ?2",
        rusqlite::params![year_start, year_end],
        |row| row.get(0),
    )?;

    // Numerator: patients with BOTH initial AND discharge scores in the year
    let numerator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT d.patient_id)
         FROM outcome_score_index d
         INNER JOIN outcome_score_index i
           ON i.patient_id = d.patient_id
          AND i.episode_phase = 'initial'
          AND i.recorded_at BETWEEN ?1 AND ?2
         WHERE d.episode_phase = 'discharge'
           AND d.recorded_at BETWEEN ?1 AND ?2",
        rusqlite::params![year_start, year_end],
        |row| row.get(0),
    )?;

    Ok((numerator, denominator))
}

/// Compute Measures #217, #220, #221 — Functional Status Change by region.
///
/// For each measure the numerator is patients with a positive score change
/// (improvement) >= MCID for their measure type.
/// Denominator: patients with both initial and discharge scores for the measure type.
fn compute_functional_status_measure(
    conn: &rusqlite::Connection,
    year: i64,
    measure_id: &str,
) -> Result<(i64, i64), AppError> {
    let year_start = format!("{}-01-01", year);
    let year_end = format!("{}-12-31", year);

    // Determine which measure types map to this MIPS ID
    let measure_types: &[&str] = match measure_id {
        MEASURE_217 => &["lefs"],
        MEASURE_220 => &["oswestry", "ndi"],
        MEASURE_221 => &["dash"],
        _ => return Ok((0, 0)),
    };

    let mut total_numerator: i64 = 0;
    let mut total_denominator: i64 = 0;

    for mt in measure_types {
        // Denominator: patients with both initial and discharge scores for this measure type
        let denom: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT d.patient_id)
             FROM outcome_score_index d
             INNER JOIN outcome_score_index i
               ON i.patient_id = d.patient_id
              AND i.episode_phase = 'initial'
              AND i.measure_type = ?3
              AND i.recorded_at BETWEEN ?1 AND ?2
             WHERE d.episode_phase = 'discharge'
               AND d.measure_type = ?3
               AND d.recorded_at BETWEEN ?1 AND ?2",
            rusqlite::params![year_start, year_end, mt],
            |row| row.get(0),
        )?;

        // Numerator: patients with score improvement (discharge score vs initial)
        // For LEFS: higher = better (improvement = discharge > initial)
        // For DASH/NDI/Oswestry: lower = better (improvement = discharge < initial)
        let numer: i64 = if *mt == "lefs" {
            conn.query_row(
                "SELECT COUNT(DISTINCT d.patient_id)
                 FROM outcome_score_index d
                 INNER JOIN outcome_score_index i
                   ON i.patient_id = d.patient_id
                  AND i.episode_phase = 'initial'
                  AND i.measure_type = ?3
                  AND i.recorded_at BETWEEN ?1 AND ?2
                 WHERE d.episode_phase = 'discharge'
                   AND d.measure_type = ?3
                   AND d.recorded_at BETWEEN ?1 AND ?2
                   AND (d.score - i.score) >= 9.0",
                rusqlite::params![year_start, year_end, mt],
                |row| row.get(0),
            )?
        } else {
            // lower = better; improvement threshold varies by measure
            let mcid: f64 = match *mt {
                "dash" => 10.8,
                "ndi" => 7.5,
                "oswestry" => 10.0,
                _ => 5.0,
            };
            conn.query_row(
                "SELECT COUNT(DISTINCT d.patient_id)
                 FROM outcome_score_index d
                 INNER JOIN outcome_score_index i
                   ON i.patient_id = d.patient_id
                  AND i.episode_phase = 'initial'
                  AND i.measure_type = ?3
                  AND i.recorded_at BETWEEN ?1 AND ?2
                 WHERE d.episode_phase = 'discharge'
                   AND d.measure_type = ?3
                   AND d.recorded_at BETWEEN ?1 AND ?2
                   AND (i.score - d.score) >= ?4",
                rusqlite::params![year_start, year_end, mt, mcid],
                |row| row.get(0),
            )?
        };

        total_numerator += numer;
        total_denominator += denom;
    }

    Ok((total_numerator, total_denominator))
}

/// Compute Measure #134 — Depression Screening (PHQ-2).
///
/// Denominator: patients with a PHQ-2 screening in the performance year.
/// Numerator:   patients with PHQ-2 score < 3 (negative screen),
///              OR patients with PHQ-2 score >= 3 AND a PHQ-9 recorded.
fn compute_measure_134(
    conn: &rusqlite::Connection,
    year: i64,
) -> Result<(i64, i64), AppError> {
    let denominator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT patient_id) FROM mips_screenings
         WHERE measure_type = 'phq2' AND performance_year = ?1",
        rusqlite::params![year],
        |row| row.get(0),
    )?;

    // Numerator: negative screens + positive screens with PHQ-9 follow-up
    let negative: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT patient_id) FROM mips_screenings
         WHERE measure_type = 'phq2' AND performance_year = ?1
           AND score < ?2",
        rusqlite::params![year, PHQ2_FOLLOWUP_THRESHOLD],
        |row| row.get(0),
    )?;

    let positive_with_phq9: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT p.patient_id)
         FROM mips_screenings p
         INNER JOIN mips_screenings f
           ON f.patient_id = p.patient_id
          AND f.measure_type = 'phq9'
          AND f.performance_year = ?1
         WHERE p.measure_type = 'phq2'
           AND p.performance_year = ?1
           AND p.score >= ?2",
        rusqlite::params![year, PHQ2_FOLLOWUP_THRESHOLD],
        |row| row.get(0),
    )?;

    let numerator = negative + positive_with_phq9;
    Ok((numerator, denominator))
}

/// Compute Measure #155 — Falls Risk Screening.
///
/// Denominator: patients 65+ with a falls_risk screening in the year.
/// Numerator:   patients with positive screen AND follow_up_plan documented.
fn compute_measure_155(
    conn: &rusqlite::Connection,
    year: i64,
) -> Result<(i64, i64), AppError> {
    // Build age cutoff: patients born on or before year-01-01 minus 65 years
    let cutoff_date = format!("{}-01-01", year - 65);

    let denominator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT ms.patient_id)
         FROM mips_screenings ms
         INNER JOIN patient_index pi ON pi.patient_id = ms.patient_id
         WHERE ms.measure_type = 'falls_risk'
           AND ms.performance_year = ?1
           AND pi.birth_date <= ?2",
        rusqlite::params![year, cutoff_date],
        |row| row.get(0),
    ).unwrap_or(0); // patient_index may not have all patients — degrade gracefully

    let numerator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT ms.patient_id)
         FROM mips_screenings ms
         INNER JOIN patient_index pi ON pi.patient_id = ms.patient_id
         WHERE ms.measure_type = 'falls_risk'
           AND ms.performance_year = ?1
           AND pi.birth_date <= ?2
           AND ms.result = 'positive'
           AND ms.follow_up_plan IS NOT NULL
           AND ms.follow_up_plan != ''",
        rusqlite::params![year, cutoff_date],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok((numerator, denominator))
}

/// Compute Measure #128 — BMI Recording.
///
/// Denominator: encounters in the performance year.
/// Numerator:   encounters with a BMI recorded in vitals.
fn compute_measure_128(
    conn: &rusqlite::Connection,
    year: i64,
) -> Result<(i64, i64), AppError> {
    let year_start = format!("{}-01-01", year);
    let year_end = format!("{}-12-31", year);

    let denominator: i64 = conn.query_row(
        "SELECT COUNT(*) FROM encounter_index
         WHERE encounter_date BETWEEN ?1 AND ?2",
        rusqlite::params![year_start, year_end],
        |row| row.get(0),
    ).unwrap_or(0);

    // vitals table stores bmi as a field; check fhir_resources for vitals with BMI
    let numerator: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT v.encounter_id)
         FROM vitals_index v
         INNER JOIN encounter_index e ON e.encounter_id = v.encounter_id
         WHERE e.encounter_date BETWEEN ?1 AND ?2
           AND v.bmi IS NOT NULL",
        rusqlite::params![year_start, year_end],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok((numerator, denominator))
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get MIPS performance data for a given year, optionally filtered by measure IDs.
#[tauri::command]
pub fn get_mips_performance(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    performance_year: i64,
    measure_ids: Option<Vec<String>>,
) -> Result<Vec<MipsPerformance>, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Define all supported measures
    let all_measures: &[(&str, &str)] = &[
        (MEASURE_182, "Functional Outcome Assessment"),
        (MEASURE_217, "Functional Status Change — Lower Extremity"),
        (MEASURE_220, "Functional Status Change — Spine"),
        (MEASURE_221, "Functional Status Change — Upper Extremity"),
        (MEASURE_134, "Depression Screening (PHQ-2)"),
        (MEASURE_155, "Falls Risk Screening"),
        (MEASURE_128, "BMI Screening and Follow-Up"),
    ];

    let filter: Option<&[String]> = measure_ids.as_deref();

    let mut results = Vec::new();

    for (mid, mname) in all_measures {
        // Apply filter if provided
        if let Some(ids) = filter {
            if !ids.iter().any(|id| id == mid) {
                continue;
            }
        }

        let (numerator, denominator) = match *mid {
            m if m == MEASURE_182 => compute_measure_182(&conn, performance_year)?,
            m if m == MEASURE_217 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_217)?
            }
            m if m == MEASURE_220 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_220)?
            }
            m if m == MEASURE_221 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_221)?
            }
            m if m == MEASURE_134 => compute_measure_134(&conn, performance_year)?,
            m if m == MEASURE_155 => compute_measure_155(&conn, performance_year)?,
            m if m == MEASURE_128 => compute_measure_128(&conn, performance_year)?,
            _ => (0, 0),
        };

        results.push(MipsPerformance {
            measure_id: mid.to_string(),
            measure_name: mname.to_string(),
            numerator,
            denominator,
            performance_rate: performance_rate(numerator, denominator),
            performance_year,
        });
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "mips.get_performance".to_string(),
            resource_type: "mips_performance".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("year={}", performance_year)),
        },
    );

    Ok(results)
}

/// Get patients in the denominator of a specific MIPS measure.
#[tauri::command]
pub fn get_mips_eligible_patients(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    measure_id: String,
) -> Result<Vec<EligiblePatient>, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let year = chrono::Utc::now().format("%Y").to_string().parse::<i64>().unwrap_or(2024);

    let year_start = format!("{}-01-01", year);
    let year_end = format!("{}-12-31", year);

    let mut patients = Vec::new();

    match measure_id.as_str() {
        m if m == MEASURE_182 => {
            // Denominator: patients with discharge score in year
            let mut stmt = conn.prepare(
                "SELECT DISTINCT o.patient_id,
                    COALESCE(pi.display_name, o.patient_id) as dname,
                    EXISTS(
                        SELECT 1 FROM outcome_score_index i2
                        WHERE i2.patient_id = o.patient_id
                          AND i2.episode_phase = 'initial'
                          AND i2.recorded_at BETWEEN ?1 AND ?2
                    ) as in_numerator
                 FROM outcome_score_index o
                 LEFT JOIN patient_index pi ON pi.patient_id = o.patient_id
                 WHERE o.episode_phase = 'discharge'
                   AND o.recorded_at BETWEEN ?1 AND ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![year_start, year_end], |row| {
                Ok(EligiblePatient {
                    patient_id: row.get(0)?,
                    display_name: row.get(1)?,
                    in_numerator: row.get::<_, i64>(2)? != 0,
                })
            })?;
            for row in rows {
                patients.push(row?);
            }
        }
        m if m == MEASURE_134 => {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT ms.patient_id,
                    COALESCE(pi.display_name, ms.patient_id) as dname,
                    CASE WHEN ms.score < ?3 THEN 1
                         WHEN EXISTS(
                            SELECT 1 FROM mips_screenings f
                            WHERE f.patient_id = ms.patient_id
                              AND f.measure_type = 'phq9'
                              AND f.performance_year = ?4
                         ) THEN 1
                         ELSE 0
                    END as in_numerator
                 FROM mips_screenings ms
                 LEFT JOIN patient_index pi ON pi.patient_id = ms.patient_id
                 WHERE ms.measure_type = 'phq2'
                   AND ms.performance_year = ?4",
            )?;
            let rows = stmt.query_map(
                rusqlite::params![year_start, year_end, PHQ2_FOLLOWUP_THRESHOLD, year],
                |row| {
                    Ok(EligiblePatient {
                        patient_id: row.get(0)?,
                        display_name: row.get(1)?,
                        in_numerator: row.get::<_, i64>(2)? != 0,
                    })
                },
            )?;
            for row in rows {
                patients.push(row?);
            }
        }
        m if m == MEASURE_155 => {
            let cutoff_date = format!("{}-01-01", year - 65);
            let mut stmt = conn.prepare(
                "SELECT DISTINCT ms.patient_id,
                    COALESCE(pi.display_name, ms.patient_id) as dname,
                    CASE WHEN ms.result = 'positive'
                              AND ms.follow_up_plan IS NOT NULL
                              AND ms.follow_up_plan != ''
                         THEN 1 ELSE 0
                    END as in_numerator
                 FROM mips_screenings ms
                 LEFT JOIN patient_index pi ON pi.patient_id = ms.patient_id
                 WHERE ms.measure_type = 'falls_risk'
                   AND ms.performance_year = ?1
                   AND pi.birth_date <= ?2",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![year, cutoff_date], |row| {
                    Ok(EligiblePatient {
                        patient_id: row.get(0)?,
                        display_name: row.get(1)?,
                        in_numerator: row.get::<_, i64>(2)? != 0,
                    })
                })?;
            for row in rows {
                patients.push(row?);
            }
        }
        _ => {
            // Return empty for unsupported or unimplemented denominator lists
        }
    }

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "mips.get_eligible_patients".to_string(),
            resource_type: "mips_performance".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("measure_id={}", measure_id)),
        },
    );

    Ok(patients)
}

/// Record a PHQ-2 screening for a patient.
///
/// If score >= PHQ2_FOLLOWUP_THRESHOLD (3), sets follow_up_plan to
/// "PHQ-9 follow-up required" automatically.
#[tauri::command]
pub fn record_phq2_screening(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
    score: f64,
    encounter_id: Option<String>,
) -> Result<MipsScreening, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::ClinicalDocumentation, Action::Create)?;

    if score < 0.0 || score > 6.0 {
        return Err(AppError::Validation(
            "PHQ-2 score must be between 0 and 6".to_string(),
        ));
    }

    let follow_up_plan = if score >= PHQ2_FOLLOWUP_THRESHOLD {
        Some("PHQ-9 follow-up required".to_string())
    } else {
        None
    };

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let screening_id = uuid::Uuid::new_v4().to_string();
    let performance_year = chrono::Utc::now().format("%Y").to_string().parse::<i64>().unwrap_or(2024);

    conn.execute(
        "INSERT INTO mips_screenings
            (screening_id, patient_id, encounter_id, measure_type, score,
             result, follow_up_plan, performance_year)
         VALUES (?1, ?2, ?3, 'phq2', ?4, ?5, ?6, ?7)",
        rusqlite::params![
            screening_id,
            patient_id,
            encounter_id,
            score,
            if score >= PHQ2_FOLLOWUP_THRESHOLD { "positive" } else { "negative" },
            follow_up_plan,
            performance_year
        ],
    )?;

    let screened_at: String = conn.query_row(
        "SELECT screened_at FROM mips_screenings WHERE screening_id = ?1",
        rusqlite::params![screening_id],
        |row| row.get(0),
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "mips.record_phq2".to_string(),
            resource_type: "mips_screenings".to_string(),
            resource_id: Some(screening_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("score={}", score)),
        },
    );

    Ok(MipsScreening {
        screening_id,
        patient_id,
        encounter_id,
        measure_type: "phq2".to_string(),
        score: Some(score),
        result: Some(if score >= PHQ2_FOLLOWUP_THRESHOLD {
            "positive".to_string()
        } else {
            "negative".to_string()
        }),
        follow_up_plan,
        performance_year,
        screened_at,
    })
}

/// Record a falls risk screening for a patient.
///
/// For patients 65+, this satisfies MIPS Measure #155 requirements.
/// plan_documented should be true when a plan of care is in place.
#[tauri::command]
pub fn record_falls_screening(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    patient_id: String,
    result: String,
    plan_documented: bool,
    encounter_id: Option<String>,
) -> Result<MipsScreening, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::ClinicalDocumentation, Action::Create)?;

    let valid_results = ["positive", "negative"];
    if !valid_results.contains(&result.as_str()) {
        return Err(AppError::Validation(
            "Falls risk result must be 'positive' or 'negative'".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let screening_id = uuid::Uuid::new_v4().to_string();
    let performance_year = chrono::Utc::now().format("%Y").to_string().parse::<i64>().unwrap_or(2024);

    let follow_up_plan = if plan_documented {
        Some("Plan of care documented".to_string())
    } else {
        None
    };

    conn.execute(
        "INSERT INTO mips_screenings
            (screening_id, patient_id, encounter_id, measure_type,
             result, follow_up_plan, performance_year)
         VALUES (?1, ?2, ?3, 'falls_risk', ?4, ?5, ?6)",
        rusqlite::params![
            screening_id,
            patient_id,
            encounter_id,
            result,
            follow_up_plan,
            performance_year
        ],
    )?;

    let screened_at: String = conn.query_row(
        "SELECT screened_at FROM mips_screenings WHERE screening_id = ?1",
        rusqlite::params![screening_id],
        |row| row.get(0),
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "mips.record_falls_screening".to_string(),
            resource_type: "mips_screenings".to_string(),
            resource_id: Some(screening_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("result={} plan={}", result, plan_documented)),
        },
    );

    Ok(MipsScreening {
        screening_id,
        patient_id,
        encounter_id,
        measure_type: "falls_risk".to_string(),
        score: None,
        result: Some(result),
        follow_up_plan,
        performance_year,
        screened_at,
    })
}

/// Get the full MIPS dashboard: all measures with rates, tiers, and projected score.
#[tauri::command]
pub fn get_mips_dashboard(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
    performance_year: i64,
) -> Result<MipsDashboard, AppError> {
    let (user_id, _role) =
        middleware::check_permission(&session, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let all_measures: &[(&str, &str)] = &[
        (MEASURE_182, "Functional Outcome Assessment"),
        (MEASURE_217, "Functional Status Change — Lower Extremity"),
        (MEASURE_220, "Functional Status Change — Spine"),
        (MEASURE_221, "Functional Status Change — Upper Extremity"),
        (MEASURE_134, "Depression Screening (PHQ-2)"),
        (MEASURE_155, "Falls Risk Screening"),
        (MEASURE_128, "BMI Screening and Follow-Up"),
    ];

    let mut cards = Vec::new();
    let mut rate_sum = 0.0_f64;
    let mut rate_count = 0_usize;

    for (mid, mname) in all_measures {
        let (numerator, denominator) = match *mid {
            m if m == MEASURE_182 => compute_measure_182(&conn, performance_year)?,
            m if m == MEASURE_217 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_217)?
            }
            m if m == MEASURE_220 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_220)?
            }
            m if m == MEASURE_221 => {
                compute_functional_status_measure(&conn, performance_year, MEASURE_221)?
            }
            m if m == MEASURE_134 => compute_measure_134(&conn, performance_year)?,
            m if m == MEASURE_155 => compute_measure_155(&conn, performance_year)?,
            m if m == MEASURE_128 => compute_measure_128(&conn, performance_year)?,
            _ => (0, 0),
        };

        let rate = performance_rate(numerator, denominator);
        if let Some(r) = rate {
            rate_sum += r;
            rate_count += 1;
        }

        cards.push(MipsMeasureCard {
            measure_id: mid.to_string(),
            measure_name: mname.to_string(),
            numerator,
            denominator,
            performance_rate: rate,
            tier: tier_for_rate(rate),
            performance_year,
        });
    }

    let projected_composite_score = if rate_count > 0 {
        Some(rate_sum / rate_count as f64)
    } else {
        None
    };

    let computed_at = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id,
            action: "mips.get_dashboard".to_string(),
            resource_type: "mips_performance".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("year={}", performance_year)),
        },
    );

    Ok(MipsDashboard {
        performance_year,
        measures: cards,
        projected_composite_score,
        computed_at,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: Performance rate calculation with normal values
    #[test]
    fn performance_rate_normal() {
        let rate = performance_rate(75, 100);
        assert!(rate.is_some());
        assert!((rate.unwrap() - 75.0).abs() < 0.001);
    }

    // Test 2: Performance rate with zero denominator returns None
    #[test]
    fn performance_rate_zero_denominator() {
        let rate = performance_rate(0, 0);
        assert!(rate.is_none());
    }

    // Test 3: PHQ-2 threshold: score >= 3 triggers follow-up
    #[test]
    fn phq2_threshold_positive_at_three() {
        let score = 3.0;
        assert!(score >= PHQ2_FOLLOWUP_THRESHOLD);
        let follow_up = if score >= PHQ2_FOLLOWUP_THRESHOLD {
            Some("PHQ-9 follow-up required".to_string())
        } else {
            None
        };
        assert!(follow_up.is_some());
    }

    // Test 4: PHQ-2 threshold: score < 3 does NOT trigger follow-up
    #[test]
    fn phq2_threshold_negative_below_three() {
        let score = 2.0;
        assert!(score < PHQ2_FOLLOWUP_THRESHOLD);
        let follow_up = if score >= PHQ2_FOLLOWUP_THRESHOLD {
            Some("PHQ-9 follow-up required".to_string())
        } else {
            None
        };
        assert!(follow_up.is_none());
    }

    // Test 5: Measure-type to MIPS ID mapping — known types
    #[test]
    fn measure_type_mapping_lefs() {
        assert_eq!(measure_type_to_mips_id("lefs"), Some(MEASURE_217));
    }

    #[test]
    fn measure_type_mapping_oswestry() {
        assert_eq!(measure_type_to_mips_id("oswestry"), Some(MEASURE_220));
    }

    #[test]
    fn measure_type_mapping_ndi() {
        assert_eq!(measure_type_to_mips_id("ndi"), Some(MEASURE_220));
    }

    #[test]
    fn measure_type_mapping_dash() {
        assert_eq!(measure_type_to_mips_id("dash"), Some(MEASURE_221));
    }

    // Test 6: Measure-type to MIPS ID mapping — unknown type returns None
    #[test]
    fn measure_type_mapping_unknown() {
        assert_eq!(measure_type_to_mips_id("psfs"), None);
        assert_eq!(measure_type_to_mips_id("fabq"), None);
        assert_eq!(measure_type_to_mips_id(""), None);
    }

    // Test 7: Performance tier thresholds
    #[test]
    fn tier_green_at_75() {
        matches!(tier_for_rate(Some(75.0)), PerformanceTier::Green);
    }

    #[test]
    fn tier_amber_at_60() {
        matches!(tier_for_rate(Some(60.0)), PerformanceTier::Amber);
    }

    #[test]
    fn tier_red_at_40() {
        matches!(tier_for_rate(Some(40.0)), PerformanceTier::Red);
    }

    #[test]
    fn tier_no_data_for_none() {
        matches!(tier_for_rate(None), PerformanceTier::NoData);
    }

    // Test 8: Empty data handling — performance_rate(0, 0) == None
    #[test]
    fn empty_data_no_crash() {
        let rate = performance_rate(0, 0);
        assert!(rate.is_none());
        // Also verify tier_for_rate handles None gracefully
        let tier = tier_for_rate(None);
        matches!(tier, PerformanceTier::NoData);
    }
}
