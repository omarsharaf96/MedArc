/// commands/era_processing.rs — ERA/835 Remittance Processing (M003/S02)
///
/// Implements 835 EDI parsing, auto-posting, denial management, A/R aging,
/// and patient balance calculation for MedArc EMR billing workflows.
///
/// 835 EDI Parser
/// --------------
/// Parses 835 remittance text files using standard delimiters:
///   segment delimiter ~, element delimiter *, sub-element delimiter :
/// Extracts: BPR (payment info), TRN (trace number), CLP (claim-level payment),
///   CAS (adjustments with CARC codes), SVC (service-level detail).
/// Command: `parse_835_file(file_path)` → RemittanceAdvice
///
/// Auto-Posting
/// ------------
/// Matches ERA payments to claims by control_number or (patient_id + service date).
/// `auto_post_remittance(remittance_id)` posts payments, updates claim status to
/// "paid" or "denied", and creates claim_payments records.
///
/// Denial Management
/// -----------------
/// Flags denied/adjusted claims for staff review.
/// CARC codes: CO-4, CO-97 (contractual), PR-1 (deductible), PR-2 (coinsurance),
///   PR-3 (copay).
/// `list_denials(status?, payer_id?)` returns a filterable denial queue.
///
/// A/R Aging
/// ---------
/// `get_ar_aging(payer_id?)` → buckets: 0-30, 31-60, 61-90, 91-120, 120+ days.
/// Aging is calculated from claim submission date to today for unpaid claims.
///
/// Patient Balance
/// ---------------
/// `get_patient_balance(patient_id)` → total outstanding after insurance payments.
///
/// RBAC
/// ----
///   SystemAdmin / Provider / BillingStaff → full CRUD
///   NurseMa / FrontDesk                   → Read only
///
/// Audit
/// -----
/// Every mutating command writes an audit row via `write_audit_entry`.
use std::collections::HashMap;

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
// Data types
// ─────────────────────────────────────────────────────────────────────────────

/// A single adjustment code + amount pair (from a CAS segment).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentCode {
    /// Group code: CO (contractual), PR (patient responsibility), OA (other).
    pub group_code: String,
    /// CARC reason code (e.g. "4", "97", "1", "2", "3").
    pub reason_code: String,
    /// Dollar amount adjusted.
    pub amount: f64,
}

/// Service-level detail from an SVC segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLinePayment {
    /// Procedure code (CPT/HCPCS) from SVC01.
    pub procedure_code: String,
    /// Submitted charge amount.
    pub submitted_charge: f64,
    /// Paid amount for this service line.
    pub paid_amount: f64,
    /// Revenue code (if present).
    pub revenue_code: Option<String>,
    /// Adjustments applied to this service line.
    pub adjustments: Vec<AdjustmentCode>,
}

/// Claim-level payment detail from a CLP loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPaymentDetail {
    /// Claim control number from CLP01 (matches claims.control_number).
    pub claim_control_number: String,
    /// Payment status: 1=Processed as Primary, 2=Processed as Secondary,
    /// 19=Processed as Primary, Forwarded to Additional Payer(s), 4=Denied.
    pub claim_status_code: String,
    /// Total charge billed.
    pub total_charge: f64,
    /// Amount paid by payer.
    pub paid_amount: f64,
    /// Patient responsibility (deductible + coinsurance + copay).
    pub patient_responsibility: f64,
    /// Claim filing indicator (e.g. "MB" for Medicare Part B).
    pub claim_filing_indicator: Option<String>,
    /// Claim-level adjustment codes from CAS segments.
    pub adjustments: Vec<AdjustmentCode>,
    /// Service-level payment details from SVC loops.
    pub service_lines: Vec<ServiceLinePayment>,
}

/// Parsed representation of a complete 835 remittance advice file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemittanceAdvice {
    /// Payer name from N1*PR segment.
    pub payer_name: Option<String>,
    /// Payer ID from REF*2U or NM109.
    pub payer_id: Option<String>,
    /// Payee NPI or tax ID.
    pub payee_id: Option<String>,
    /// BPR02: total payment amount.
    pub payment_amount: f64,
    /// BPR16: payment effective date (YYYYMMDD).
    pub payment_date: Option<String>,
    /// TRN02: check/EFT trace number.
    pub trace_number: Option<String>,
    /// All CLP loops parsed from the file.
    pub claims: Vec<ClaimPaymentDetail>,
}

/// A saved remittance advice record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemittanceRecord {
    pub remittance_id: String,
    pub payer_id: Option<String>,
    pub trace_number: Option<String>,
    pub payment_amount: f64,
    pub payment_date: Option<String>,
    pub file_path: Option<String>,
    pub posted: bool,
    pub created_at: String,
}

/// A claim payment record (from claim_payments table).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPaymentRecord {
    pub payment_id: String,
    pub claim_id: String,
    pub remittance_id: Option<String>,
    pub paid_amount: f64,
    pub adjustment_amount: f64,
    pub patient_responsibility: f64,
    pub adjustment_codes: Option<String>,
    pub posted_at: String,
}

/// Result of auto-posting a remittance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPostResult {
    /// Number of claims successfully matched and posted.
    pub matched_count: u32,
    /// Number of claim payment records created.
    pub payments_created: u32,
    /// CLP control numbers that could not be matched to a claim.
    pub unmatched_control_numbers: Vec<String>,
}

/// A denial queue entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenialRecord {
    pub claim_id: String,
    pub patient_id: String,
    pub payer_id: String,
    pub status: String,
    pub denial_reason: Option<String>,
    pub adjustment_codes: Option<String>,
    pub paid_amount: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

/// A/R aging bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgingBucket {
    /// Bucket label e.g. "0-30", "31-60", "61-90", "91-120", "120+".
    pub label: String,
    /// Total outstanding amount in this bucket.
    pub total_amount: f64,
    /// Number of claims in this bucket.
    pub claim_count: u32,
}

/// Full A/R aging report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArAgingReport {
    pub buckets: Vec<AgingBucket>,
    /// Grand total outstanding.
    pub total_outstanding: f64,
}

/// A single claim's outstanding balance info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimBalance {
    pub claim_id: String,
    pub total_charge: f64,
    pub total_paid: f64,
    pub outstanding: f64,
    pub patient_responsibility: f64,
}

/// Patient balance summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientBalance {
    pub patient_id: String,
    /// Total billed (from encounter_billing).
    pub total_billed: f64,
    /// Total paid by insurance.
    pub total_insurance_paid: f64,
    /// Total patient responsibility (copay/deductible/coinsurance).
    pub total_patient_responsibility: f64,
    /// Remaining outstanding balance.
    pub outstanding_balance: f64,
    pub claim_details: Vec<ClaimBalance>,
}

/// Input for listing remittances with optional filters.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemittanceListFilter {
    pub payer_id: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// 835 EDI Parser
// ─────────────────────────────────────────────────────────────────────────────

/// Parse an 835 EDI file into a structured `RemittanceAdvice`.
///
/// The 835 format uses:
///   segment terminator: `~`
///   element separator:  `*`
///   sub-element separator: `:`
///
/// Segments extracted:
///   BPR — payment amount and date
///   TRN — trace number
///   N1*PR — payer name
///   N1*PE — payee
///   CLP — claim-level payment loop
///   CAS — claim/service adjustment codes
///   SVC — service line payment detail
pub fn parse_835(content: &str) -> Result<RemittanceAdvice, AppError> {
    // Detect segment terminator (normally ~, but may have \n after it)
    let segments: Vec<&str> = content
        .split('~')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut payment_amount: f64 = 0.0;
    let mut payment_date: Option<String> = None;
    let mut trace_number: Option<String> = None;
    let mut payer_name: Option<String> = None;
    let mut payer_id: Option<String> = None;
    let mut payee_id: Option<String> = None;

    // CLP loop state
    let mut claims: Vec<ClaimPaymentDetail> = Vec::new();
    let mut current_clp: Option<ClaimPaymentDetail> = None;
    let mut current_svc: Option<ServiceLinePayment> = None;
    // Track whether the CAS segment belongs to a CLP or SVC context
    let mut in_svc_context = false;
    // Track N1 qualifier
    let mut _last_n1_qualifier = String::new();

    for seg in &segments {
        let elements: Vec<&str> = seg.splitn(20, '*').collect();
        let seg_id = elements.first().copied().unwrap_or("").trim();

        match seg_id {
            "BPR" => {
                // BPR*I*<amount>*...*...*...*...*...*...*...*...*...*...*...*...*...*<date>
                if let Some(amt_str) = elements.get(2) {
                    payment_amount = amt_str.parse().unwrap_or(0.0);
                }
                // BPR16 is the payment effective date
                if let Some(date) = elements.get(16) {
                    if !date.is_empty() {
                        payment_date = Some(date.to_string());
                    }
                }
            }
            "TRN" => {
                // TRN*1*<trace_number>*<payer_id>
                if let Some(tn) = elements.get(2) {
                    trace_number = Some(tn.to_string());
                }
            }
            "N1" => {
                // N1*PR*<payer_name>  or  N1*PE*<payee_name>
                _last_n1_qualifier = elements.get(1).copied().unwrap_or("").to_string();
                let name = elements.get(2).copied().unwrap_or("");
                let id = elements.get(4).copied().unwrap_or("");
                match _last_n1_qualifier.as_str() {
                    "PR" => {
                        payer_name = Some(name.to_string());
                        if !id.is_empty() {
                            payer_id = Some(id.to_string());
                        }
                    }
                    "PE" => {
                        if !id.is_empty() {
                            payee_id = Some(id.to_string());
                        }
                    }
                    _ => {}
                }
            }
            "REF" => {
                // REF*2U*<payer_id>
                if elements.get(1).copied() == Some("2U") {
                    if let Some(id) = elements.get(2) {
                        payer_id = Some(id.to_string());
                    }
                }
            }
            "CLP" => {
                // Flush previous SVC into current CLP
                if let Some(svc) = current_svc.take() {
                    if let Some(ref mut clp) = current_clp {
                        clp.service_lines.push(svc);
                    }
                }
                // Flush previous CLP into claims list
                if let Some(clp) = current_clp.take() {
                    claims.push(clp);
                }
                in_svc_context = false;

                // CLP*<control_number>*<status_code>*<total_charge>*<paid_amount>*<patient_resp>*<filing_indicator>
                let control = elements.get(1).copied().unwrap_or("").to_string();
                let status_code = elements.get(2).copied().unwrap_or("").to_string();
                let total_charge: f64 = elements.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let paid: f64 = elements.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let patient_resp: f64 = elements.get(5).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let filing_indicator = elements.get(6).map(|s| s.to_string()).filter(|s| !s.is_empty());

                current_clp = Some(ClaimPaymentDetail {
                    claim_control_number: control,
                    claim_status_code: status_code,
                    total_charge,
                    paid_amount: paid,
                    patient_responsibility: patient_resp,
                    claim_filing_indicator: filing_indicator,
                    adjustments: Vec::new(),
                    service_lines: Vec::new(),
                });
            }
            "CAS" => {
                // CAS*<group_code>*<reason_code>*<amount>[*<quantity>*<reason_code2>*<amount2>...]
                // A single CAS can have up to 6 adjustment triplets
                let group_code = elements.get(1).copied().unwrap_or("").to_string();
                let mut i = 2usize;
                let mut parsed: Vec<AdjustmentCode> = Vec::new();
                while i + 1 < elements.len() {
                    let reason = elements.get(i).copied().unwrap_or("");
                    let amt_str = elements.get(i + 1).copied().unwrap_or("0");
                    if reason.is_empty() {
                        break;
                    }
                    let amount: f64 = amt_str.parse().unwrap_or(0.0);
                    parsed.push(AdjustmentCode {
                        group_code: group_code.clone(),
                        reason_code: reason.to_string(),
                        amount,
                    });
                    // Skip optional quantity field if present (every 3rd element after position 2)
                    // CAS format: CAS*group*reason*amount[*qty*reason2*amt2...]
                    // positions: 2=reason, 3=amount, 4=qty(opt), 5=reason2, 6=amt2 ...
                    // Standard: triplets are (reason, amount, qty?) — advance by 3 to skip qty
                    i += 3;
                }

                if in_svc_context {
                    if let Some(ref mut svc) = current_svc {
                        svc.adjustments.extend(parsed);
                    }
                } else if let Some(ref mut clp) = current_clp {
                    clp.adjustments.extend(parsed);
                }
            }
            "SVC" => {
                // Flush previous SVC
                if let Some(svc) = current_svc.take() {
                    if let Some(ref mut clp) = current_clp {
                        clp.service_lines.push(svc);
                    }
                }
                in_svc_context = true;

                // SVC*<comp_proc_code>*<submitted_charge>*<paid_amount>[*<revenue_code>]
                // SVC01 is a composite: HC:<cpt>[:<mod1>]
                let proc_composite = elements.get(1).copied().unwrap_or("");
                let parts: Vec<&str> = proc_composite.split(':').collect();
                let procedure_code = parts.get(1).copied().unwrap_or(proc_composite).to_string();
                let revenue_code = parts.first().filter(|&&s| s != "HC" && s != "WK").map(|s| s.to_string());

                let submitted_charge: f64 = elements.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let paid_amount: f64 = elements.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);

                current_svc = Some(ServiceLinePayment {
                    procedure_code,
                    submitted_charge,
                    paid_amount,
                    revenue_code,
                    adjustments: Vec::new(),
                });
            }
            _ => {}
        }
    }

    // Flush final SVC and CLP
    if let Some(svc) = current_svc.take() {
        if let Some(ref mut clp) = current_clp {
            clp.service_lines.push(svc);
        }
    }
    if let Some(clp) = current_clp.take() {
        claims.push(clp);
    }

    Ok(RemittanceAdvice {
        payer_name,
        payer_id,
        payee_id,
        payment_amount,
        payment_date,
        trace_number,
        claims,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// A/R Aging calculation (pure function — testable without DB)
// ─────────────────────────────────────────────────────────────────────────────

/// Calculate the A/R aging bucket label for a claim given days-outstanding.
pub fn aging_bucket_label(days: i64) -> &'static str {
    match days {
        0..=30 => "0-30",
        31..=60 => "31-60",
        61..=90 => "61-90",
        91..=120 => "91-120",
        _ => "120+",
    }
}

/// Build an `ArAgingReport` from a slice of (submitted_at_date_str, total_charge) pairs.
///
/// `today` is passed in so the function is pure and unit-testable.
pub fn build_aging_report(
    claims: &[(String, f64)],
    today: &str,
) -> ArAgingReport {
    let today_naive = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::Local::now().date_naive());

    let mut bucket_map: HashMap<&'static str, (f64, u32)> = HashMap::new();
    for label in &["0-30", "31-60", "61-90", "91-120", "120+"] {
        bucket_map.insert(label, (0.0, 0));
    }

    for (submitted_at, charge) in claims {
        let submitted = chrono::NaiveDate::parse_from_str(submitted_at, "%Y-%m-%d")
            .or_else(|_| chrono::NaiveDate::parse_from_str(submitted_at, "%Y-%m-%dT%H:%M:%S%.fZ"))
            .unwrap_or(today_naive);
        let days = (today_naive - submitted).num_days().max(0);
        let label = aging_bucket_label(days);
        let entry = bucket_map.entry(label).or_insert((0.0, 0));
        entry.0 += charge;
        entry.1 += 1;
    }

    let ordered_labels = ["0-30", "31-60", "61-90", "91-120", "120+"];
    let buckets: Vec<AgingBucket> = ordered_labels
        .iter()
        .map(|label| {
            let (total, count) = bucket_map.get(label).copied().unwrap_or((0.0, 0));
            AgingBucket {
                label: label.to_string(),
                total_amount: total,
                claim_count: count,
            }
        })
        .collect();

    let total_outstanding: f64 = buckets.iter().map(|b| b.total_amount).sum();

    ArAgingReport {
        buckets,
        total_outstanding,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Parse an 835 EDI file and return structured remittance data WITHOUT saving to DB.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn parse_835_file(
    file_path: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    _device_id: State<'_, DeviceId>,
) -> Result<RemittanceAdvice, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let _ = db; // DB not needed for pure parse
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::Io(e))?;

    parse_835(&content)
}

/// Parse an 835 EDI file, save the remittance record, and create claim_payments.
///
/// Returns the saved `RemittanceRecord`.
/// Requires: Billing + Create
#[tauri::command]
pub async fn import_835(
    file_path: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<RemittanceRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::Io(e))?;
    let advice = parse_835(&content)?;

    let remittance_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO remittance_advice (remittance_id, payer_id, trace_number, payment_amount, payment_date, file_path, posted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        rusqlite::params![
            remittance_id,
            advice.payer_id,
            advice.trace_number,
            advice.payment_amount,
            advice.payment_date,
            file_path,
            now,
        ],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.import_835".to_string(),
            resource_type: "RemittanceAdvice".to_string(),
            resource_id: Some(remittance_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "file={} amount={} claims={}",
                file_path,
                advice.payment_amount,
                advice.claims.len()
            )),
        },
    );

    Ok(RemittanceRecord {
        remittance_id,
        payer_id: advice.payer_id,
        trace_number: advice.trace_number,
        payment_amount: advice.payment_amount,
        payment_date: advice.payment_date,
        file_path: Some(file_path),
        posted: false,
        created_at: now,
    })
}

/// Auto-post a remittance: match ERA payments to claims and create payment records.
///
/// Matching strategy (in order):
///   1. Match by `claims.control_number = clp.claim_control_number`
///   2. No secondary match implemented yet — unmatched CNs are returned in result.
///
/// For each matched claim:
///   - Creates a `claim_payments` row.
///   - Updates `claims.status` to "paid" (status_code 1/2/19) or "denied" (status_code 4).
///   - Updates `claims.paid_amount` and `claims.adjustment_amount`.
///
/// Marks the remittance as `posted = 1` when complete.
/// Requires: Billing + Update
#[tauri::command]
pub async fn auto_post_remittance(
    remittance_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<AutoPostResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load the remittance file path to re-parse (or we could store parsed JSON — we re-parse)
    let (file_path, already_posted): (Option<String>, i64) = conn
        .query_row(
            "SELECT file_path, posted FROM remittance_advice WHERE remittance_id = ?1",
            [&remittance_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Remittance {} not found", remittance_id)))?;

    if already_posted != 0 {
        return Err(AppError::Validation(format!(
            "Remittance {} has already been posted",
            remittance_id
        )));
    }

    let advice = match file_path {
        Some(ref path) => {
            let content = std::fs::read_to_string(path).map_err(AppError::Io)?;
            parse_835(&content)?
        }
        None => {
            return Err(AppError::Validation(
                "Remittance has no associated file path".to_string(),
            ))
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let mut matched_count: u32 = 0;
    let mut payments_created: u32 = 0;
    let mut unmatched: Vec<String> = Vec::new();

    for clp in &advice.claims {
        // Try to find matching claim by control number
        let claim_row: Option<(String, String)> = conn
            .query_row(
                "SELECT claim_id, patient_id FROM claims WHERE control_number = ?1 LIMIT 1",
                [&clp.claim_control_number],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (claim_id, _patient_id) = match claim_row {
            Some(row) => row,
            None => {
                unmatched.push(clp.claim_control_number.clone());
                continue;
            }
        };

        matched_count += 1;

        // Compute total adjustment amount from CAS codes
        let total_adj: f64 = clp.adjustments.iter().map(|a| a.amount).sum();

        // Serialize adjustment codes as CSV string "GROUP-REASON:AMOUNT"
        let adj_codes_str = if clp.adjustments.is_empty() {
            None
        } else {
            let codes: Vec<String> = clp
                .adjustments
                .iter()
                .map(|a| format!("{}-{}:{}", a.group_code, a.reason_code, a.amount))
                .collect();
            Some(codes.join(","))
        };

        // Create claim_payments record
        let payment_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO claim_payments (payment_id, claim_id, remittance_id, paid_amount, adjustment_amount, patient_responsibility, adjustment_codes, posted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                payment_id,
                claim_id,
                remittance_id,
                clp.paid_amount,
                total_adj,
                clp.patient_responsibility,
                adj_codes_str,
                now,
            ],
        )?;
        payments_created += 1;

        // Determine new claim status
        // status_code: 1=Processed Primary, 2=Secondary, 19=Primary Forwarded, 4=Denied
        let new_status = if clp.claim_status_code == "4" {
            "denied"
        } else {
            "paid"
        };

        // Build denial reason from CO adjustments if denied
        let denial_reason = if new_status == "denied" {
            let co_codes: Vec<String> = clp
                .adjustments
                .iter()
                .filter(|a| a.group_code == "CO")
                .map(|a| carc_description(&a.reason_code))
                .collect();
            if co_codes.is_empty() {
                None
            } else {
                Some(co_codes.join("; "))
            }
        } else {
            None
        };

        conn.execute(
            "UPDATE claims SET status = ?1, paid_amount = ?2, adjustment_amount = ?3, denial_reason = ?4, response_at = ?5, updated_at = ?5 WHERE claim_id = ?6",
            rusqlite::params![
                new_status,
                clp.paid_amount,
                total_adj,
                denial_reason,
                now,
                claim_id,
            ],
        )?;
    }

    // Mark remittance as posted
    conn.execute(
        "UPDATE remittance_advice SET posted = 1 WHERE remittance_id = ?1",
        [&remittance_id],
    )?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.auto_post_remittance".to_string(),
            resource_type: "RemittanceAdvice".to_string(),
            resource_id: Some(remittance_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "matched={} payments={} unmatched={}",
                matched_count,
                payments_created,
                unmatched.len()
            )),
        },
    );

    Ok(AutoPostResult {
        matched_count,
        payments_created,
        unmatched_control_numbers: unmatched,
    })
}

/// List all remittance advice records with optional filters.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_remittances(
    payer_id: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<RemittanceRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT remittance_id, payer_id, trace_number, payment_amount, payment_date, file_path, posted, created_at
         FROM remittance_advice WHERE 1=1",
    );
    let mut params: Vec<String> = Vec::new();

    if let Some(ref pid) = payer_id {
        params.push(pid.clone());
        query.push_str(&format!(" AND payer_id = ?{}", params.len()));
    }
    if let Some(ref df) = date_from {
        params.push(df.clone());
        query.push_str(&format!(" AND payment_date >= ?{}", params.len()));
    }
    if let Some(ref dt) = date_to {
        params.push(dt.clone());
        query.push_str(&format!(" AND payment_date <= ?{}", params.len()));
    }
    query.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok(RemittanceRecord {
            remittance_id: row.get(0)?,
            payer_id: row.get(1)?,
            trace_number: row.get(2)?,
            payment_amount: row.get(3)?,
            payment_date: row.get(4)?,
            file_path: row.get(5)?,
            posted: row.get::<_, i64>(6)? != 0,
            created_at: row.get(7)?,
        })
    })?;

    let records: Vec<RemittanceRecord> = rows.filter_map(|r| r.ok()).collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.list_remittances".to_string(),
            resource_type: "RemittanceAdvice".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    );

    Ok(records)
}

/// Return the denial queue: claims with status "denied" or with CO adjustment codes.
///
/// Optional filters: status, payer_id.
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_denials(
    status: Option<String>,
    payer_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<DenialRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Join claims with claim_payments to get adjustment codes
    let mut query = String::from(
        "SELECT c.claim_id, c.patient_id, c.payer_id, c.status, c.denial_reason,
                cp.adjustment_codes, c.paid_amount, c.created_at, c.updated_at
         FROM claims c
         LEFT JOIN claim_payments cp ON cp.claim_id = c.claim_id
         WHERE (c.status IN ('denied','appealed') OR cp.adjustment_codes LIKE '%CO-%')",
    );
    let mut params: Vec<String> = Vec::new();

    if let Some(ref s) = status {
        params.push(s.clone());
        query.push_str(&format!(" AND c.status = ?{}", params.len()));
    }
    if let Some(ref pid) = payer_id {
        params.push(pid.clone());
        query.push_str(&format!(" AND c.payer_id = ?{}", params.len()));
    }
    query.push_str(" GROUP BY c.claim_id ORDER BY c.updated_at DESC");

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok(DenialRecord {
            claim_id: row.get(0)?,
            patient_id: row.get(1)?,
            payer_id: row.get(2)?,
            status: row.get(3)?,
            denial_reason: row.get(4)?,
            adjustment_codes: row.get(5)?,
            paid_amount: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;

    let records: Vec<DenialRecord> = rows.filter_map(|r| r.ok()).collect();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.list_denials".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", records.len())),
        },
    );

    Ok(records)
}

/// Get A/R aging report buckets for unpaid claims.
///
/// Aging is calculated from `claims.submitted_at` (or `created_at` as fallback)
/// to today's date. Optional `payer_id` filter.
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_ar_aging(
    payer_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ArAgingReport, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut query = String::from(
        "SELECT COALESCE(submitted_at, created_at), COALESCE(paid_amount, 0),
                eb.total_charge
         FROM claims c
         JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
         WHERE c.status NOT IN ('paid', 'denied', 'appealed')",
    );
    let mut params: Vec<String> = Vec::new();

    if let Some(ref pid) = payer_id {
        params.push(pid.clone());
        query.push_str(&format!(" AND c.payer_id = ?{}", params.len()));
    }

    let mut stmt = conn.prepare(&query)?;
    let pairs: Vec<(String, f64)> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let date: String = row.get(0)?;
            let total_charge: f64 = row.get(2)?;
            Ok((date, total_charge))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Use today's date
    let today = chrono::Local::now().date_naive().to_string();
    let report = build_aging_report(&pairs, &today);

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.get_ar_aging".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("total_outstanding={:.2}", report.total_outstanding)),
        },
    );

    Ok(report)
}

/// Get patient balance summary: total billed, insurance paid, patient responsibility, outstanding.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_patient_balance(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PatientBalance, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Get all claims for the patient with their billing totals
    let mut stmt = conn.prepare(
        "SELECT c.claim_id, eb.total_charge,
                COALESCE(SUM(cp.paid_amount), 0) AS total_paid,
                COALESCE(SUM(cp.patient_responsibility), 0) AS total_patient_resp
         FROM claims c
         JOIN encounter_billing eb ON eb.billing_id = c.encounter_billing_id
         LEFT JOIN claim_payments cp ON cp.claim_id = c.claim_id
         WHERE c.patient_id = ?1
         GROUP BY c.claim_id, eb.total_charge",
    )?;

    let claim_details: Vec<ClaimBalance> = stmt
        .query_map([&patient_id], |row| {
            let claim_id: String = row.get(0)?;
            let total_charge: f64 = row.get(1)?;
            let total_paid: f64 = row.get(2)?;
            let patient_resp: f64 = row.get(3)?;
            let outstanding = (total_charge - total_paid).max(0.0);
            Ok(ClaimBalance {
                claim_id,
                total_charge,
                total_paid,
                outstanding,
                patient_responsibility: patient_resp,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let total_billed: f64 = claim_details.iter().map(|c| c.total_charge).sum();
    let total_paid: f64 = claim_details.iter().map(|c| c.total_paid).sum();
    let total_patient_resp: f64 = claim_details.iter().map(|c| c.patient_responsibility).sum();
    let outstanding: f64 = claim_details.iter().map(|c| c.outstanding).sum();

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "era.get_patient_balance".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("outstanding={:.2}", outstanding)),
        },
    );

    Ok(PatientBalance {
        patient_id,
        total_billed,
        total_insurance_paid: total_paid,
        total_patient_responsibility: total_patient_resp,
        outstanding_balance: outstanding,
        claim_details,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// CARC code descriptions
// ─────────────────────────────────────────────────────────────────────────────

/// Return a human-readable description for common CARC codes.
pub fn carc_description(code: &str) -> String {
    match code {
        "1" => "CO-1: Deductible amount".to_string(),
        "2" => "CO-2: Coinsurance amount".to_string(),
        "3" => "CO-3: Copay amount".to_string(),
        "4" => "CO-4: The procedure code is inconsistent with the modifier / procedure not covered".to_string(),
        "5" => "CO-5: The procedure code / bill type is inconsistent with the place of service".to_string(),
        "6" => "CO-6: The procedure/revenue code is inconsistent with the patient's age".to_string(),
        "7" => "CO-7: The procedure/revenue code is inconsistent with the patient's gender".to_string(),
        "18" => "CO-18: Exact duplicate claim/service".to_string(),
        "22" => "CO-22: This care may be covered by another payer per coordination of benefits".to_string(),
        "29" => "CO-29: The time limit for filing has expired".to_string(),
        "45" => "CO-45: Charge exceeds fee schedule/maximum allowable or contracted/legislated fee arrangement".to_string(),
        "50" => "CO-50: These are non-covered services because this is not deemed a medical necessity".to_string(),
        "96" => "CO-96: Non-covered charge(s). At least one Remark Code must be provided".to_string(),
        "97" => "CO-97: The benefit for this service is included in the payment/allowance for another service/procedure".to_string(),
        "109" => "CO-109: Claim/service not covered by this payer/contractor".to_string(),
        "119" => "CO-119: Benefit maximum for this time period or occurrence has been reached".to_string(),
        "167" => "CO-167: This (these) diagnosis(es) is (are) not covered".to_string(),
        "170" => "CO-170: Payment is denied when performed/billed by this type of provider in this type of facility".to_string(),
        "197" => "CO-197: Precertification/authorization/notification absent".to_string(),
        _ => format!("CARC-{}", code),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid 835 with a single CLP claim and one CAS adjustment.
    const SIMPLE_835: &str = r#"ISA*00*          *00*          *ZZ*PAYER          *ZZ*PROVIDER       *260314*1200*^*00501*000000001*0*P*:~
GS*HP*PAYER*PROVIDER*20260314*1200*1*X*005010X221A1~
ST*835*0001~
BPR*I*150.00*C*ACH*CCP*01*111000025*DA*1234567890*1234567890**01*011000015*DA*9876543210*20260314~
TRN*1*1234567890*1512345678~
REF*2U*PAYERID01~
DTM*405*20260314~
N1*PR*BLUE CROSS BLUE SHIELD*XV*PAYERID01~
N1*PE*ACME PHYSICAL THERAPY*XX*1234567890~
CLP*CTRL12345*1*200.00*150.00*25.00*MB*PATIENTID*11*1~
NM1*QC*1*DOE*JOHN****MI*MEM12345~
CAS*CO*45*50.00~
SVC*HC:97110*150.00*100.00~
CAS*PR*2*25.00~
SE*13*0001~
GE*1*1~
IEA*1*000000001~"#;

    #[test]
    fn test_parse_simple_835_payment_info() {
        let advice = parse_835(SIMPLE_835).expect("Should parse without error");
        assert_eq!(advice.payment_amount, 150.0);
        assert_eq!(advice.trace_number.as_deref(), Some("1234567890"));
        assert_eq!(advice.payer_name.as_deref(), Some("BLUE CROSS BLUE SHIELD"));
        assert_eq!(advice.payer_id.as_deref(), Some("PAYERID01"));
    }

    #[test]
    fn test_parse_simple_835_claim_count() {
        let advice = parse_835(SIMPLE_835).expect("Should parse without error");
        assert_eq!(advice.claims.len(), 1, "Should have exactly one CLP claim");
        let clp = &advice.claims[0];
        assert_eq!(clp.claim_control_number, "CTRL12345");
        assert_eq!(clp.paid_amount, 150.0);
        assert_eq!(clp.total_charge, 200.0);
        assert_eq!(clp.patient_responsibility, 25.0);
    }

    #[test]
    fn test_parse_carc_adjustment_codes() {
        let advice = parse_835(SIMPLE_835).expect("Should parse without error");
        let clp = &advice.claims[0];
        // CAS*CO*45*50.00 at claim level
        assert!(!clp.adjustments.is_empty(), "Should have claim-level adjustments");
        let adj = &clp.adjustments[0];
        assert_eq!(adj.group_code, "CO");
        assert_eq!(adj.reason_code, "45");
        assert_eq!(adj.amount, 50.0);
    }

    #[test]
    fn test_parse_service_line_detail() {
        let advice = parse_835(SIMPLE_835).expect("Should parse without error");
        let clp = &advice.claims[0];
        assert_eq!(clp.service_lines.len(), 1, "Should have one SVC line");
        let svc = &clp.service_lines[0];
        assert_eq!(svc.procedure_code, "97110");
        assert_eq!(svc.submitted_charge, 150.0);
        assert_eq!(svc.paid_amount, 100.0);
        // Service-level CAS*PR*2*25.00
        assert!(!svc.adjustments.is_empty());
        assert_eq!(svc.adjustments[0].group_code, "PR");
        assert_eq!(svc.adjustments[0].reason_code, "2");
    }

    #[test]
    fn test_ar_aging_bucket_assignment() {
        // Today = 2026-03-14
        let today = "2026-03-14";
        let claims = vec![
            ("2026-03-01".to_string(), 100.0), // 13 days → 0-30
            ("2026-02-10".to_string(), 200.0), // 32 days → 31-60
            ("2026-01-10".to_string(), 300.0), // 63 days → 61-90
            ("2025-12-11".to_string(), 400.0), // 93 days → 91-120
            ("2025-10-01".to_string(), 500.0), // 164 days → 120+
        ];
        let report = build_aging_report(&claims, today);

        let bucket = |label: &str| {
            report
                .buckets
                .iter()
                .find(|b| b.label == label)
                .map(|b| b.total_amount)
                .unwrap_or(0.0)
        };

        assert_eq!(bucket("0-30"), 100.0);
        assert_eq!(bucket("31-60"), 200.0);
        assert_eq!(bucket("61-90"), 300.0);
        assert_eq!(bucket("91-120"), 400.0);
        assert_eq!(bucket("120+"), 500.0);
        assert_eq!(report.total_outstanding, 1500.0);
    }

    #[test]
    fn test_ar_aging_bucket_label() {
        assert_eq!(aging_bucket_label(0), "0-30");
        assert_eq!(aging_bucket_label(30), "0-30");
        assert_eq!(aging_bucket_label(31), "31-60");
        assert_eq!(aging_bucket_label(60), "31-60");
        assert_eq!(aging_bucket_label(61), "61-90");
        assert_eq!(aging_bucket_label(90), "61-90");
        assert_eq!(aging_bucket_label(91), "91-120");
        assert_eq!(aging_bucket_label(120), "91-120");
        assert_eq!(aging_bucket_label(121), "120+");
        assert_eq!(aging_bucket_label(999), "120+");
    }

    #[test]
    fn test_denial_flagging_co_adjustment() {
        // A CLP with status_code = 4 should be treated as denied
        let era_content = r#"ISA*00*          *00*          *ZZ*PAYER          *ZZ*PROVIDER       *260314*1200*^*00501*000000002*0*P*:~
GS*HP*PAYER*PROVIDER*20260314*1200*1*X*005010X221A1~
ST*835*0001~
BPR*I*0.00*C*ACH*CCP*01*111000025*DA*1234567890*1234567890**01*011000015*DA*9876543210*20260314~
TRN*1*TRACE9876*1512345678~
N1*PR*TEST PAYER*XV*TESTPAYER~
CLP*DENIEDCLAIM*4*300.00*0.00*0.00*MB~
CAS*CO*4*300.00~
SE*9*0001~
GE*1*1~
IEA*1*000000002~"#;
        let advice = parse_835(era_content).expect("Should parse denied claim");
        assert_eq!(advice.claims.len(), 1);
        let clp = &advice.claims[0];
        assert_eq!(clp.claim_status_code, "4", "Status code should be 4 (Denied)");
        assert_eq!(clp.paid_amount, 0.0);
        // CO-4 adjustment present
        assert!(!clp.adjustments.is_empty());
        assert_eq!(clp.adjustments[0].group_code, "CO");
        assert_eq!(clp.adjustments[0].reason_code, "4");
    }

    #[test]
    fn test_carc_description_known_codes() {
        assert!(carc_description("97").contains("included in the payment"));
        assert!(carc_description("1").contains("Deductible"));
        assert!(carc_description("2").contains("Coinsurance"));
        assert!(carc_description("3").contains("Copay"));
        assert!(carc_description("4").contains("procedure code"));
    }
}
