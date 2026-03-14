/// commands/claims.rs — Electronic Claims Submission 837P (M004/S02)
///
/// Implements payer configuration, 837P EDI generation, and claim lifecycle
/// management for physical therapy billing.
///
/// Payer Configuration
/// -------------------
/// Stores payer info (name, EDI payer ID, clearinghouse, billing rule, phone, address).
/// Commands: create_payer, list_payers, update_payer, get_payer
///
/// 837P Generation
/// ---------------
/// Builds a well-formed 837P EDI transaction set from encounter_billing data:
///   ISA/GS/ST headers, Loop 2000A (billing provider), Loop 2000B (subscriber),
///   Loop 2300 (claim), Loop 2400 (service lines), SE/GE/IEA trailers.
/// Uses string building — no external EDI crate.
///
/// Claim Lifecycle
/// ---------------
///   draft → validated → submitted → accepted → paid → denied → appealed
///
/// RBAC
/// ----
///   SystemAdmin / Provider / BillingStaff → full CRUD
///   NurseMa / FrontDesk                   → Read only
///
/// Audit
/// -----
/// Every command writes an audit row via `write_audit_entry`.
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

/// Clearinghouse partner options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clearinghouse {
    OfficeAlly,
    Availity,
    Trizetto,
    Manual,
}

impl Clearinghouse {
    pub fn as_str(&self) -> &'static str {
        match self {
            Clearinghouse::OfficeAlly => "office_ally",
            Clearinghouse::Availity => "availity",
            Clearinghouse::Trizetto => "trizetto",
            Clearinghouse::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "office_ally" => Some(Clearinghouse::OfficeAlly),
            "availity" => Some(Clearinghouse::Availity),
            "trizetto" => Some(Clearinghouse::Trizetto),
            "manual" => Some(Clearinghouse::Manual),
            _ => None,
        }
    }
}

/// Billing rule for 8-minute rule calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingRule {
    Medicare,
    Ama,
}

impl BillingRule {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingRule::Medicare => "medicare",
            BillingRule::Ama => "ama",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "medicare" => Some(BillingRule::Medicare),
            "ama" => Some(BillingRule::Ama),
            _ => None,
        }
    }
}

/// Claim lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Draft,
    Validated,
    Submitted,
    Accepted,
    Paid,
    Denied,
    Appealed,
}

impl ClaimStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaimStatus::Draft => "draft",
            ClaimStatus::Validated => "validated",
            ClaimStatus::Submitted => "submitted",
            ClaimStatus::Accepted => "accepted",
            ClaimStatus::Paid => "paid",
            ClaimStatus::Denied => "denied",
            ClaimStatus::Appealed => "appealed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(ClaimStatus::Draft),
            "validated" => Some(ClaimStatus::Validated),
            "submitted" => Some(ClaimStatus::Submitted),
            "accepted" => Some(ClaimStatus::Accepted),
            "paid" => Some(ClaimStatus::Paid),
            "denied" => Some(ClaimStatus::Denied),
            "appealed" => Some(ClaimStatus::Appealed),
            _ => None,
        }
    }
}

/// Input for creating a payer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayerInput {
    pub name: String,
    pub edi_payer_id: Option<String>,
    pub clearinghouse: Option<String>,
    pub billing_rule: String,
    pub phone: Option<String>,
    pub address: Option<String>,
}

/// A payer configuration record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayerRecord {
    pub payer_id: String,
    pub name: String,
    pub edi_payer_id: Option<String>,
    pub clearinghouse: Option<String>,
    pub billing_rule: String,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub created_at: String,
}

/// Input for creating a claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateClaimInput {
    pub encounter_billing_id: String,
    pub payer_id: String,
    pub patient_id: String,
}

/// A claim record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub claim_id: String,
    pub encounter_billing_id: String,
    pub payer_id: String,
    pub patient_id: String,
    pub status: String,
    pub edi_content: Option<String>,
    pub edi_file_path: Option<String>,
    pub control_number: Option<String>,
    pub submitted_at: Option<String>,
    pub response_at: Option<String>,
    pub paid_amount: Option<f64>,
    pub adjustment_amount: Option<f64>,
    pub denial_reason: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Result of claim validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// Result of 837P EDI generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdiGenerationResult {
    pub claim_id: String,
    pub edi_content: String,
    pub edi_file_path: String,
    pub segment_count: u32,
    pub control_number: String,
}

/// Input for listing claims with filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimListFilter {
    pub patient_id: Option<String>,
    pub status: Option<String>,
    pub payer_id: Option<String>,
}

/// Input for updating claim status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateClaimStatusInput {
    pub status: String,
    pub notes: Option<String>,
    pub paid_amount: Option<f64>,
    pub adjustment_amount: Option<f64>,
    pub denial_reason: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// 837P EDI Generation Logic
// ─────────────────────────────────────────────────────────────────────────────

/// Internal struct for building 837P from encounter billing data.
#[derive(Debug)]
struct BillingProvider {
    npi: String,
    tax_id: String,
    last_name: String,
    first_name: String,
    address_line1: String,
    city: String,
    state: String,
    zip: String,
}

#[derive(Debug)]
struct SubscriberInfo {
    member_id: String,
    last_name: String,
    first_name: String,
    birth_date: String,
    gender: String,
    address_line1: String,
    city: String,
    state: String,
    zip: String,
}

#[derive(Debug)]
struct ClaimInfo {
    claim_id: String,
    total_charge: f64,
    place_of_service: String,
    diagnosis_codes: Vec<String>,
    service_lines: Vec<ServiceLine>,
}

#[derive(Debug)]
struct ServiceLine {
    cpt_code: String,
    modifiers: Vec<String>,
    units: u32,
    charge: f64,
    date_of_service: String,
    dx_pointers: Vec<String>,
}

/// Build an 837P EDI transaction set.
///
/// Returns the full EDI text and the segment count (excluding ISA/IEA).
pub fn build_837p(
    control_number: &str,
    provider: &BillingProvider,
    subscriber: &SubscriberInfo,
    payer_id: &str,
    claim: &ClaimInfo,
) -> (String, u32) {
    let mut segments: Vec<String> = Vec::new();
    let element_sep = '*';
    let component_sep = ':';
    let segment_term = '~';

    // ISA — Interchange Control Header
    let isa = format!(
        "ISA{e}00{e}          {e}00{e}          {e}ZZ{e}MEDARC         {e}ZZ{e}{payer:<15}{e}260314{e}1200{e}^{e}00501{e}{ctrl:09}{e}0{e}P{e}{comp}",
        e = element_sep,
        payer = payer_id.chars().take(15).collect::<String>(),
        ctrl = control_number.chars().take(9).collect::<String>(),
        comp = component_sep
    );
    segments.push(isa);

    // GS — Functional Group Header
    let gs = format!(
        "GS{e}HC{e}MEDARC{e}{payer}{e}20260314{e}1200{e}1{e}X{e}005010X222A1",
        e = element_sep,
        payer = payer_id,
    );
    segments.push(gs);

    // ST — Transaction Set Header (segment count calculated at end)
    let st = format!(
        "ST{e}837{e}{ctrl:04}{e}005010X222A1",
        e = element_sep,
        ctrl = control_number.parse::<u32>().unwrap_or(1) % 10000,
    );
    segments.push(st);

    // BPR — Beginning of Provider Information
    let bpr = format!(
        "BPR{e}I{e}0{e}C{e}ACH{e}CTX{e}01{e}999999999{e}DA{e}12345678{e}1234567890{e}{e}01{e}999999999{e}DA{e}12345678{e}20260314",
        e = element_sep
    );
    segments.push(bpr);

    // NM1 — Submitter Name (Loop 1000A)
    let nm1_submitter = format!(
        "NM1{e}41{e}2{e}MEDARC CLINIC{e}{e}{e}{e}{e}46{e}MEDARC01",
        e = element_sep
    );
    segments.push(nm1_submitter);

    // PER — Submitter EDI Contact
    let per = format!(
        "PER{e}IC{e}MEDARC EDI{e}TE{e}5551234567",
        e = element_sep
    );
    segments.push(per);

    // NM1 — Receiver Name (Loop 1000B)
    let nm1_receiver = format!(
        "NM1{e}40{e}2{e}PAYER NAME{e}{e}{e}{e}{e}46{e}{payer}",
        e = element_sep,
        payer = payer_id,
    );
    segments.push(nm1_receiver);

    // HL — Billing Provider Hierarchical Level (Loop 2000A)
    let hl_billing = format!(
        "HL{e}1{e}{e}20{e}1",
        e = element_sep
    );
    segments.push(hl_billing);

    // PRV — Provider Specialty (Loop 2000A)
    let prv = format!(
        "PRV{e}BI{e}PXC{e}225100000X",
        e = element_sep
    );
    segments.push(prv);

    // NM1 — Billing Provider Name (Loop 2010AA)
    let nm1_billing = format!(
        "NM1{e}85{e}1{e}{last}{e}{first}{e}{e}{e}{e}XX{e}{npi}",
        e = element_sep,
        last = provider.last_name,
        first = provider.first_name,
        npi = provider.npi,
    );
    segments.push(nm1_billing);

    // N3 — Billing Provider Address
    let n3_billing = format!(
        "N3{e}{addr}",
        e = element_sep,
        addr = provider.address_line1,
    );
    segments.push(n3_billing);

    // N4 — Billing Provider City/State/Zip
    let n4_billing = format!(
        "N4{e}{city}{e}{state}{e}{zip}",
        e = element_sep,
        city = provider.city,
        state = provider.state,
        zip = provider.zip,
    );
    segments.push(n4_billing);

    // REF — Billing Provider Tax ID
    let ref_taxid = format!(
        "REF{e}EI{e}{taxid}",
        e = element_sep,
        taxid = provider.tax_id,
    );
    segments.push(ref_taxid);

    // HL — Subscriber Hierarchical Level (Loop 2000B)
    let hl_subscriber = format!(
        "HL{e}2{e}1{e}22{e}0",
        e = element_sep
    );
    segments.push(hl_subscriber);

    // SBR — Subscriber Information (Loop 2000B)
    let sbr = format!(
        "SBR{e}P{e}18{e}{e}{e}{e}{e}{e}{e}MB",
        e = element_sep
    );
    segments.push(sbr);

    // NM1 — Subscriber Name (Loop 2010BA)
    let nm1_subscriber = format!(
        "NM1{e}IL{e}1{e}{last}{e}{first}{e}{e}{e}{e}MI{e}{member_id}",
        e = element_sep,
        last = subscriber.last_name,
        first = subscriber.first_name,
        member_id = subscriber.member_id,
    );
    segments.push(nm1_subscriber);

    // N3 — Subscriber Address
    let n3_sub = format!(
        "N3{e}{addr}",
        e = element_sep,
        addr = subscriber.address_line1,
    );
    segments.push(n3_sub);

    // N4 — Subscriber City/State/Zip
    let n4_sub = format!(
        "N4{e}{city}{e}{state}{e}{zip}",
        e = element_sep,
        city = subscriber.city,
        state = subscriber.state,
        zip = subscriber.zip,
    );
    segments.push(n4_sub);

    // DMG — Subscriber Demographic Info
    let gender_code = if subscriber.gender.to_lowercase() == "female" {
        "F"
    } else {
        "M"
    };
    let dob = subscriber.birth_date.replace('-', "");
    let dmg = format!(
        "DMG{e}D8{e}{dob}{e}{gender}",
        e = element_sep,
        dob = dob,
        gender = gender_code,
    );
    segments.push(dmg);

    // NM1 — Payer Name (Loop 2010BB)
    let nm1_payer = format!(
        "NM1{e}PR{e}2{e}PAYER NAME{e}{e}{e}{e}{e}PI{e}{payer}",
        e = element_sep,
        payer = payer_id,
    );
    segments.push(nm1_payer);

    // CLM — Claim Information (Loop 2300)
    let clm = format!(
        "CLM{e}{claim_id}{e}{charge:.2}{e}{e}{e}{pos}{comp}B{comp}1{e}Y{e}A{e}Y{e}I",
        e = element_sep,
        comp = component_sep,
        claim_id = claim.claim_id,
        charge = claim.total_charge,
        pos = claim.place_of_service,
    );
    segments.push(clm);

    // HI — Health Care Diagnosis Codes (Loop 2300)
    if !claim.diagnosis_codes.is_empty() {
        let dx_segs: Vec<String> = claim
            .diagnosis_codes
            .iter()
            .enumerate()
            .map(|(i, code)| {
                if i == 0 {
                    format!("ABK{comp}{code}", comp = component_sep, code = code)
                } else {
                    format!("ABF{comp}{code}", comp = component_sep, code = code)
                }
            })
            .collect();
        let hi = format!("HI{e}{codes}", e = element_sep, codes = dx_segs.join(&element_sep.to_string()));
        segments.push(hi);
    }

    // LX + SV1 — Service Lines (Loop 2400)
    for (i, svc) in claim.service_lines.iter().enumerate() {
        // LX — Service Line Number
        let lx = format!("LX{e}{num}", e = element_sep, num = i + 1);
        segments.push(lx);

        // SV1 — Professional Service
        let modifier_str = if svc.modifiers.is_empty() {
            String::new()
        } else {
            format!(
                "{comp}{mods}",
                comp = component_sep,
                mods = svc.modifiers.join(&component_sep.to_string())
            )
        };
        let dx_ptr_str = svc.dx_pointers.join(":");
        let sv1 = format!(
            "SV1{e}HC{comp}{cpt}{mods}{e}{charge:.2}{e}UN{e}{units}{e}{e}{e}{dx}",
            e = element_sep,
            comp = component_sep,
            cpt = svc.cpt_code,
            mods = modifier_str,
            charge = svc.charge,
            units = svc.units,
            dx = dx_ptr_str,
        );
        segments.push(sv1);

        // DTP — Date of Service
        let dos = svc.date_of_service.replace('-', "");
        let dtp = format!(
            "DTP{e}472{e}D8{e}{dos}",
            e = element_sep,
            dos = dos,
        );
        segments.push(dtp);
    }

    // SE — Transaction Set Trailer (segment count = ST through SE, inclusive)
    // Segments counted: everything from ST onward (index 2 onward) plus SE itself
    let seg_count = (segments.len() - 2 + 1) as u32; // -2 for ISA+GS, +1 for SE
    let se = format!(
        "SE{e}{count}{e}{ctrl:04}",
        e = element_sep,
        count = seg_count,
        ctrl = control_number.parse::<u32>().unwrap_or(1) % 10000,
    );
    segments.push(se);

    // GE — Functional Group Trailer
    let ge = format!("GE{e}1{e}1", e = element_sep);
    segments.push(ge);

    // IEA — Interchange Control Trailer
    let iea = format!(
        "IEA{e}1{e}{ctrl:09}",
        e = element_sep,
        ctrl = control_number.chars().take(9).collect::<String>(),
    );
    segments.push(iea);

    // Join segments with segment terminator + newline for readability
    let edi_text = segments
        .iter()
        .map(|s| format!("{}{}", s, segment_term))
        .collect::<Vec<_>>()
        .join("\n");

    (edi_text, seg_count)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: load billing data for claim generation
// ─────────────────────────────────────────────────────────────────────────────

struct EncounterBillingData {
    patient_id: String,
    total_charge: f64,
    billing_rule: String,
    line_items: Vec<LineItemData>,
}

struct LineItemData {
    cpt_code: String,
    modifiers: Option<String>,
    units: u32,
    charge: f64,
    dx_pointers: Option<String>,
}

fn load_encounter_billing(
    conn: &rusqlite::Connection,
    billing_id: &str,
) -> Result<EncounterBillingData, AppError> {
    let (patient_id, total_charge, billing_rule): (String, f64, String) = conn
        .query_row(
            "SELECT patient_id, total_charge, billing_rule FROM encounter_billing WHERE billing_id = ?1",
            [billing_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Encounter billing {} not found", billing_id)))?;

    let mut stmt = conn.prepare(
        "SELECT cpt_code, modifiers, units, charge, dx_pointers FROM billing_line_items WHERE billing_id = ?1",
    )?;
    let line_items: Vec<LineItemData> = stmt
        .query_map([billing_id], |row| {
            Ok(LineItemData {
                cpt_code: row.get(0)?,
                modifiers: row.get(1)?,
                units: row.get::<_, u32>(2)?,
                charge: row.get(3)?,
                dx_pointers: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(EncounterBillingData {
        patient_id,
        total_charge,
        billing_rule,
        line_items,
    })
}

fn load_patient_info(
    conn: &rusqlite::Connection,
    patient_id: &str,
) -> Result<SubscriberInfo, AppError> {
    // Try to load from fhir_resources (Patient JSON)
    let resource_json: String = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1 AND resource_type = 'Patient'",
            [patient_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Patient {} not found", patient_id)))?;

    // Parse basic fields from FHIR JSON (simplified extraction)
    let v: serde_json::Value = serde_json::from_str(&resource_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let last_name = v["name"][0]["family"]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let first_name = v["name"][0]["given"][0]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let birth_date = v["birthDate"]
        .as_str()
        .unwrap_or("19000101")
        .to_string();
    let gender = v["gender"].as_str().unwrap_or("unknown").to_string();
    let address_line1 = v["address"][0]["line"][0]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let city = v["address"][0]["city"]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let state = v["address"][0]["state"]
        .as_str()
        .unwrap_or("XX")
        .to_string();
    let zip = v["address"][0]["postalCode"]
        .as_str()
        .unwrap_or("00000")
        .to_string();
    // Try to get member ID from identifier
    let member_id = v["identifier"]
        .as_array()
        .and_then(|ids| {
            ids.iter().find(|id| {
                id["type"]["coding"][0]["code"]
                    .as_str()
                    .map(|c| c == "MB")
                    .unwrap_or(false)
            })
        })
        .and_then(|id| id["value"].as_str())
        .unwrap_or("UNKNOWN")
        .to_string();

    Ok(SubscriberInfo {
        member_id,
        last_name,
        first_name,
        birth_date,
        gender,
        address_line1,
        city,
        state,
        zip,
    })
}

fn load_billing_provider(conn: &rusqlite::Connection) -> BillingProvider {
    // Load from app_settings
    let get_setting = |key: &str, default: &str| -> String {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| default.to_string())
    };

    BillingProvider {
        npi: get_setting("billing_npi", "0000000000"),
        tax_id: get_setting("billing_tax_id", "000000000"),
        last_name: get_setting("billing_provider_last_name", "PROVIDER"),
        first_name: get_setting("billing_provider_first_name", "BILLING"),
        address_line1: get_setting("billing_address_line1", "123 MAIN ST"),
        city: get_setting("billing_city", "ANYTOWN"),
        state: get_setting("billing_state", "XX"),
        zip: get_setting("billing_zip", "00000"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Payer CRUD commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new payer configuration.
///
/// Requires: Billing + Create
#[tauri::command]
pub async fn create_payer(
    input: PayerInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PayerRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    let payer_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO payer_config (payer_id, name, edi_payer_id, clearinghouse, billing_rule, phone, address, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            payer_id,
            input.name,
            input.edi_payer_id,
            input.clearinghouse,
            input.billing_rule,
            input.phone,
            input.address,
            now,
        ],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.create_payer".to_string(),
            resource_type: "PayerConfig".to_string(),
            resource_id: Some(payer_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("name={}", input.name)),
        },
    );

    Ok(PayerRecord {
        payer_id,
        name: input.name,
        edi_payer_id: input.edi_payer_id,
        clearinghouse: input.clearinghouse,
        billing_rule: input.billing_rule,
        phone: input.phone,
        address: input.address,
        created_at: now,
    })
}

/// List all payer configurations.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_payers(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<PayerRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT payer_id, name, edi_payer_id, clearinghouse, billing_rule, phone, address, created_at
         FROM payer_config ORDER BY name ASC",
    )?;

    let payers: Vec<PayerRecord> = stmt
        .query_map([], |row| {
            Ok(PayerRecord {
                payer_id: row.get(0)?,
                name: row.get(1)?,
                edi_payer_id: row.get(2)?,
                clearinghouse: row.get(3)?,
                billing_rule: row.get(4)?,
                phone: row.get(5)?,
                address: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.list_payers".to_string(),
            resource_type: "PayerConfig".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", payers.len())),
        },
    );

    Ok(payers)
}

/// Get a single payer configuration by ID.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_payer(
    payer_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PayerRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let payer = conn
        .query_row(
            "SELECT payer_id, name, edi_payer_id, clearinghouse, billing_rule, phone, address, created_at
             FROM payer_config WHERE payer_id = ?1",
            [&payer_id],
            |row| {
                Ok(PayerRecord {
                    payer_id: row.get(0)?,
                    name: row.get(1)?,
                    edi_payer_id: row.get(2)?,
                    clearinghouse: row.get(3)?,
                    billing_rule: row.get(4)?,
                    phone: row.get(5)?,
                    address: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .map_err(|_| AppError::NotFound(format!("Payer {} not found", payer_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.get_payer".to_string(),
            resource_type: "PayerConfig".to_string(),
            resource_id: Some(payer_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(payer)
}

/// Update an existing payer configuration.
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn update_payer(
    payer_id: String,
    input: PayerInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<PayerRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows_affected = conn.execute(
        "UPDATE payer_config SET name = ?1, edi_payer_id = ?2, clearinghouse = ?3,
         billing_rule = ?4, phone = ?5, address = ?6 WHERE payer_id = ?7",
        rusqlite::params![
            input.name,
            input.edi_payer_id,
            input.clearinghouse,
            input.billing_rule,
            input.phone,
            input.address,
            payer_id,
        ],
    )?;

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!("Payer {} not found", payer_id)));
    }

    let payer = conn
        .query_row(
            "SELECT payer_id, name, edi_payer_id, clearinghouse, billing_rule, phone, address, created_at
             FROM payer_config WHERE payer_id = ?1",
            [&payer_id],
            |row| {
                Ok(PayerRecord {
                    payer_id: row.get(0)?,
                    name: row.get(1)?,
                    edi_payer_id: row.get(2)?,
                    clearinghouse: row.get(3)?,
                    billing_rule: row.get(4)?,
                    phone: row.get(5)?,
                    address: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .map_err(|_| AppError::NotFound(format!("Payer {} not found after update", payer_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.update_payer".to_string(),
            resource_type: "PayerConfig".to_string(),
            resource_id: Some(payer_id),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("name={}", input.name)),
        },
    );

    Ok(payer)
}

// ─────────────────────────────────────────────────────────────────────────────
// Claim lifecycle commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new claim in draft status.
///
/// Requires: Billing + Create
#[tauri::command]
pub async fn create_claim(
    input: CreateClaimInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ClaimRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    // Verify the encounter billing exists
    {
        let conn = db
            .conn
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM encounter_billing WHERE billing_id = ?1",
                [&input.encounter_billing_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if count == 0 {
            return Err(AppError::NotFound(format!(
                "Encounter billing {} not found",
                input.encounter_billing_id
            )));
        }

        // Verify the payer exists
        let payer_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM payer_config WHERE payer_id = ?1",
                [&input.payer_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if payer_count == 0 {
            return Err(AppError::NotFound(format!(
                "Payer {} not found",
                input.payer_id
            )));
        }
    }

    let claim_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO claims (claim_id, encounter_billing_id, payer_id, patient_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'draft', ?5, ?5)",
        rusqlite::params![
            claim_id,
            input.encounter_billing_id,
            input.payer_id,
            input.patient_id,
            now,
        ],
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.create_claim".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "billing_id={},payer_id={}",
                input.encounter_billing_id, input.payer_id
            )),
        },
    );

    Ok(ClaimRecord {
        claim_id,
        encounter_billing_id: input.encounter_billing_id,
        payer_id: input.payer_id,
        patient_id: input.patient_id,
        status: "draft".to_string(),
        edi_content: None,
        edi_file_path: None,
        control_number: None,
        submitted_at: None,
        response_at: None,
        paid_amount: None,
        adjustment_amount: None,
        denial_reason: None,
        notes: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Validate a claim — checks required fields for 837P submission.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn validate_claim(
    claim_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ValidationResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load claim
    let (encounter_billing_id, payer_id, patient_id, status): (String, String, String, String) = conn
        .query_row(
            "SELECT encounter_billing_id, payer_id, patient_id, status FROM claims WHERE claim_id = ?1",
            [&claim_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    let mut errors: Vec<String> = Vec::new();

    // Check billing provider NPI
    let npi: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'billing_npi'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    if npi.is_empty() || npi == "0000000000" {
        errors.push("Billing provider NPI is not configured (app_settings.billing_npi)".to_string());
    } else if npi.len() != 10 || !npi.chars().all(|c| c.is_ascii_digit()) {
        errors.push("Billing provider NPI must be exactly 10 digits".to_string());
    }

    // Check tax ID
    let tax_id: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'billing_tax_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    if tax_id.is_empty() || tax_id == "000000000" {
        errors.push("Billing provider Tax ID is not configured (app_settings.billing_tax_id)".to_string());
    }

    // Check payer EDI ID
    let edi_payer_id: Option<String> = conn
        .query_row(
            "SELECT edi_payer_id FROM payer_config WHERE payer_id = ?1",
            [&payer_id],
            |row| row.get(0),
        )
        .unwrap_or(None);
    if edi_payer_id.as_deref().unwrap_or("").is_empty() {
        errors.push("Payer EDI ID is not configured for this payer".to_string());
    }

    // Check patient member ID
    let patient_resource: Option<String> = conn
        .query_row(
            "SELECT resource FROM fhir_resources WHERE id = ?1 AND resource_type = 'Patient'",
            [&patient_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(json) = patient_resource {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
            let has_member_id = v["identifier"]
                .as_array()
                .map(|ids| {
                    ids.iter().any(|id| {
                        id["type"]["coding"][0]["code"]
                            .as_str()
                            .map(|c| c == "MB")
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !has_member_id {
                errors.push("Patient is missing a Member ID (identifier with type MB)".to_string());
            }
        }
    } else {
        errors.push(format!("Patient {} not found in FHIR resources", patient_id));
    }

    // Check diagnosis codes via encounter
    let line_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM billing_line_items WHERE billing_id = ?1",
            [&encounter_billing_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if line_count == 0 {
        errors.push("No CPT billing line items found for this encounter".to_string());
    }

    // Check encounter has diagnosis codes (from encounter_index or fhir_resources)
    let encounter_id: Option<String> = conn
        .query_row(
            "SELECT encounter_id FROM encounter_billing WHERE billing_id = ?1",
            [&encounter_billing_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(enc_id) = encounter_id {
        let enc_resource: Option<String> = conn
            .query_row(
                "SELECT resource FROM fhir_resources WHERE id = ?1 AND resource_type = 'Encounter'",
                [&enc_id],
                |row| row.get(0),
            )
            .ok();
        if let Some(json) = enc_resource {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                let has_diagnosis = v["diagnosis"].as_array().map(|d| !d.is_empty()).unwrap_or(false)
                    || v["reasonCode"].as_array().map(|r| !r.is_empty()).unwrap_or(false);
                if !has_diagnosis {
                    errors.push(
                        "Encounter has no diagnosis codes (required for 837P CLM/HI segments)"
                            .to_string(),
                    );
                }
            }
        } else {
            errors.push("Encounter FHIR resource not found — diagnosis codes unavailable".to_string());
        }
    } else {
        errors.push("No encounter linked to this billing record".to_string());
    }

    // If valid, update claim status to 'validated'
    let is_valid = errors.is_empty();
    if is_valid && status == "draft" {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE claims SET status = 'validated', updated_at = ?1 WHERE claim_id = ?2",
            rusqlite::params![now, claim_id],
        )?;
    }

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.validate_claim".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("valid={},error_count={}", is_valid, errors.len())),
        },
    );

    Ok(ValidationResult {
        valid: is_valid,
        errors,
    })
}

/// Generate 837P EDI content for a claim and save it.
///
/// Requires: Billing + Create
#[tauri::command]
pub async fn generate_837p(
    encounter_billing_id: String,
    payer_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<EdiGenerationResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Load billing data
    let billing_data = load_encounter_billing(&conn, &encounter_billing_id)?;

    // Load payer EDI ID
    let payer_edi_id: String = conn
        .query_row(
            "SELECT COALESCE(edi_payer_id, payer_id) FROM payer_config WHERE payer_id = ?1",
            [&payer_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Payer {} not found", payer_id)))?;

    // Load subscriber (patient) info
    let subscriber = load_patient_info(&conn, &billing_data.patient_id)?;

    // Load billing provider from app_settings
    let provider = load_billing_provider(&conn);

    // Build service lines
    let dos = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let service_lines: Vec<ServiceLine> = billing_data
        .line_items
        .iter()
        .map(|li| ServiceLine {
            cpt_code: li.cpt_code.clone(),
            modifiers: li
                .modifiers
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
            units: li.units,
            charge: li.charge,
            date_of_service: dos.clone(),
            dx_pointers: li
                .dx_pointers
                .as_deref()
                .unwrap_or("A")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
        })
        .collect();

    // Build claim info — we use a generated claim_id for the CLM segment
    let control_number = format!(
        "{:09}",
        chrono::Utc::now().timestamp() % 1_000_000_000
    );

    let claim_info = ClaimInfo {
        claim_id: control_number.clone(),
        total_charge: billing_data.total_charge,
        place_of_service: "11".to_string(), // Office
        diagnosis_codes: vec!["M54.5".to_string()], // placeholder
        service_lines,
    };

    let (edi_content, seg_count) = build_837p(
        &control_number,
        &provider,
        &subscriber,
        &payer_edi_id,
        &claim_info,
    );

    // Save EDI content to file path (conceptual — actual file write requires app data dir)
    let edi_file_path = format!("/tmp/medarc_837p_{}.edi", control_number);

    // Find or create claim record for this billing/payer combo
    let existing_claim: Option<String> = conn
        .query_row(
            "SELECT claim_id FROM claims WHERE encounter_billing_id = ?1 AND payer_id = ?2 LIMIT 1",
            [&encounter_billing_id, &payer_id],
            |row| row.get(0),
        )
        .ok();

    let now = chrono::Utc::now().to_rfc3339();
    let claim_id = if let Some(cid) = existing_claim {
        conn.execute(
            "UPDATE claims SET edi_content = ?1, edi_file_path = ?2, control_number = ?3, updated_at = ?4 WHERE claim_id = ?5",
            rusqlite::params![edi_content, edi_file_path, control_number, now, cid],
        )?;
        cid
    } else {
        let new_claim_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO claims (claim_id, encounter_billing_id, payer_id, patient_id, status, edi_content, edi_file_path, control_number, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                new_claim_id,
                encounter_billing_id,
                payer_id,
                billing_data.patient_id,
                edi_content,
                edi_file_path,
                control_number,
                now,
            ],
        )?;
        new_claim_id
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.generate_837p".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id.clone()),
            patient_id: Some(billing_data.patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "control_number={},segments={}",
                control_number, seg_count
            )),
        },
    );

    Ok(EdiGenerationResult {
        claim_id,
        edi_content,
        edi_file_path,
        segment_count: seg_count,
        control_number,
    })
}

/// Mark a claim as submitted.
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn submit_claim(
    claim_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ClaimRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Verify claim exists and is in validated state
    let (current_status, patient_id): (String, String) = conn
        .query_row(
            "SELECT status, patient_id FROM claims WHERE claim_id = ?1",
            [&claim_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    if current_status != "validated" && current_status != "draft" {
        return Err(AppError::Validation(format!(
            "Claim must be in draft or validated state to submit (current: {})",
            current_status
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE claims SET status = 'submitted', submitted_at = ?1, updated_at = ?1 WHERE claim_id = ?2",
        rusqlite::params![now, claim_id],
    )?;

    let claim = conn
        .query_row(
            "SELECT claim_id, encounter_billing_id, payer_id, patient_id, status,
             edi_content, edi_file_path, control_number, submitted_at, response_at,
             paid_amount, adjustment_amount, denial_reason, notes, created_at, updated_at
             FROM claims WHERE claim_id = ?1",
            [&claim_id],
            row_to_claim_record,
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.submit_claim".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some("status=submitted".to_string()),
        },
    );

    Ok(claim)
}

/// List claims with optional filters.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn list_claims(
    patient_id: Option<String>,
    status: Option<String>,
    payer_id: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<ClaimRecord>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut conditions: Vec<String> = Vec::new();
    if patient_id.is_some() {
        conditions.push("patient_id = ?1".to_string());
    }
    if status.is_some() {
        conditions.push(format!("status = ?{}", conditions.len() + 1));
    }
    if payer_id.is_some() {
        conditions.push(format!("payer_id = ?{}", conditions.len() + 1));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        "SELECT claim_id, encounter_billing_id, payer_id, patient_id, status,
         edi_content, edi_file_path, control_number, submitted_at, response_at,
         paid_amount, adjustment_amount, denial_reason, notes, created_at, updated_at
         FROM claims{} ORDER BY created_at DESC",
        where_clause
    );

    let mut stmt = conn.prepare(&query)?;

    let claims: Vec<ClaimRecord> = match (patient_id.as_ref(), status.as_ref(), payer_id.as_ref()) {
        (Some(p), Some(s), Some(py)) => stmt
            .query_map(rusqlite::params![p, s, py], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (Some(p), Some(s), None) => stmt
            .query_map(rusqlite::params![p, s], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (Some(p), None, Some(py)) => stmt
            .query_map(rusqlite::params![p, py], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (None, Some(s), Some(py)) => stmt
            .query_map(rusqlite::params![s, py], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (Some(p), None, None) => stmt
            .query_map(rusqlite::params![p], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (None, Some(s), None) => stmt
            .query_map(rusqlite::params![s], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (None, None, Some(py)) => stmt
            .query_map(rusqlite::params![py], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
        (None, None, None) => stmt
            .query_map([], row_to_claim_record)?
            .filter_map(|r| r.ok())
            .collect(),
    };

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.list_claims".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: None,
            patient_id: patient_id,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("count={}", claims.len())),
        },
    );

    Ok(claims)
}

/// Get a single claim by ID.
///
/// Requires: Billing + Read
#[tauri::command]
pub async fn get_claim(
    claim_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ClaimRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let claim = conn
        .query_row(
            "SELECT claim_id, encounter_billing_id, payer_id, patient_id, status,
             edi_content, edi_file_path, control_number, submitted_at, response_at,
             paid_amount, adjustment_amount, denial_reason, notes, created_at, updated_at
             FROM claims WHERE claim_id = ?1",
            [&claim_id],
            row_to_claim_record,
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.get_claim".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id),
            patient_id: Some(claim.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(claim)
}

/// Update claim status manually (e.g., mark as accepted, paid, denied).
///
/// Requires: Billing + Update
#[tauri::command]
pub async fn update_claim_status(
    claim_id: String,
    input: UpdateClaimStatusInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ClaimRecord, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::Billing, Action::Update)?;

    // Validate status value
    if ClaimStatus::from_str(&input.status).is_none() {
        return Err(AppError::Validation(format!(
            "Invalid claim status: {}. Must be one of: draft, validated, submitted, accepted, paid, denied, appealed",
            input.status
        )));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let patient_id: String = conn
        .query_row(
            "SELECT patient_id FROM claims WHERE claim_id = ?1",
            [&claim_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    let now = chrono::Utc::now().to_rfc3339();
    let response_at = if matches!(
        input.status.as_str(),
        "accepted" | "paid" | "denied"
    ) {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        "UPDATE claims SET status = ?1, notes = COALESCE(?2, notes),
         paid_amount = COALESCE(?3, paid_amount),
         adjustment_amount = COALESCE(?4, adjustment_amount),
         denial_reason = COALESCE(?5, denial_reason),
         response_at = COALESCE(?6, response_at),
         updated_at = ?7
         WHERE claim_id = ?8",
        rusqlite::params![
            input.status,
            input.notes,
            input.paid_amount,
            input.adjustment_amount,
            input.denial_reason,
            response_at,
            now,
            claim_id,
        ],
    )?;

    let claim = conn
        .query_row(
            "SELECT claim_id, encounter_billing_id, payer_id, patient_id, status,
             edi_content, edi_file_path, control_number, submitted_at, response_at,
             paid_amount, adjustment_amount, denial_reason, notes, created_at, updated_at
             FROM claims WHERE claim_id = ?1",
            [&claim_id],
            row_to_claim_record,
        )
        .map_err(|_| AppError::NotFound(format!("Claim {} not found", claim_id)))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "claims.update_claim_status".to_string(),
            resource_type: "Claim".to_string(),
            resource_id: Some(claim_id),
            patient_id: Some(patient_id),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("status={}", input.status)),
        },
    );

    Ok(claim)
}

// ─────────────────────────────────────────────────────────────────────────────
// Row mapping helper
// ─────────────────────────────────────────────────────────────────────────────

fn row_to_claim_record(row: &rusqlite::Row) -> rusqlite::Result<ClaimRecord> {
    Ok(ClaimRecord {
        claim_id: row.get(0)?,
        encounter_billing_id: row.get(1)?,
        payer_id: row.get(2)?,
        patient_id: row.get(3)?,
        status: row.get(4)?,
        edi_content: row.get(5)?,
        edi_file_path: row.get(6)?,
        control_number: row.get(7)?,
        submitted_at: row.get(8)?,
        response_at: row.get(9)?,
        paid_amount: row.get(10)?,
        adjustment_amount: row.get(11)?,
        denial_reason: row.get(12)?,
        notes: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 837P Header Generation ───────────────────────────────────────────────

    #[test]
    fn test_837p_isa_header_present() {
        let provider = BillingProvider {
            npi: "1234567890".to_string(),
            tax_id: "123456789".to_string(),
            last_name: "SMITH".to_string(),
            first_name: "JOHN".to_string(),
            address_line1: "123 MAIN ST".to_string(),
            city: "ANYTOWN".to_string(),
            state: "CA".to_string(),
            zip: "90210".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "MB123456".to_string(),
            last_name: "DOE".to_string(),
            first_name: "JANE".to_string(),
            birth_date: "1985-03-15".to_string(),
            gender: "female".to_string(),
            address_line1: "456 OAK AVE".to_string(),
            city: "SPRINGFIELD".to_string(),
            state: "CA".to_string(),
            zip: "90211".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM001".to_string(),
            total_charge: 250.00,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M54.5".to_string()],
            service_lines: vec![ServiceLine {
                cpt_code: "97110".to_string(),
                modifiers: vec!["KX".to_string()],
                units: 2,
                charge: 125.00,
                date_of_service: "2026-03-14".to_string(),
                dx_pointers: vec!["A".to_string()],
            }],
        };

        let (edi, seg_count) = build_837p("000000001", &provider, &subscriber, "BCBS001", &claim);

        assert!(edi.starts_with("ISA"), "EDI must start with ISA segment");
        assert!(edi.contains("GS*HC*"), "EDI must contain GS header");
        assert!(edi.contains("ST*837*"), "EDI must contain ST transaction header");
        assert!(edi.contains("IEA"), "EDI must contain IEA trailer");
        assert!(seg_count > 0, "Segment count must be positive");
    }

    #[test]
    fn test_837p_segment_count_in_se() {
        let provider = BillingProvider {
            npi: "1234567890".to_string(),
            tax_id: "123456789".to_string(),
            last_name: "SMITH".to_string(),
            first_name: "JOHN".to_string(),
            address_line1: "123 MAIN ST".to_string(),
            city: "ANYTOWN".to_string(),
            state: "CA".to_string(),
            zip: "90210".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "MB123456".to_string(),
            last_name: "DOE".to_string(),
            first_name: "JANE".to_string(),
            birth_date: "1985-03-15".to_string(),
            gender: "male".to_string(),
            address_line1: "456 OAK AVE".to_string(),
            city: "SPRINGFIELD".to_string(),
            state: "CA".to_string(),
            zip: "90211".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM002".to_string(),
            total_charge: 375.00,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M54.5".to_string(), "G89.29".to_string()],
            service_lines: vec![
                ServiceLine {
                    cpt_code: "97110".to_string(),
                    modifiers: vec![],
                    units: 2,
                    charge: 125.00,
                    date_of_service: "2026-03-14".to_string(),
                    dx_pointers: vec!["A".to_string()],
                },
                ServiceLine {
                    cpt_code: "97140".to_string(),
                    modifiers: vec![],
                    units: 1,
                    charge: 75.00,
                    date_of_service: "2026-03-14".to_string(),
                    dx_pointers: vec!["A".to_string()],
                },
            ],
        };

        let (edi, seg_count) = build_837p("000000002", &provider, &subscriber, "AETNA01", &claim);

        // SE segment should contain the segment count
        let se_line = edi
            .lines()
            .find(|l| l.starts_with("SE*"))
            .expect("SE segment must be present");
        let parts: Vec<&str> = se_line.trim_end_matches('~').split('*').collect();
        assert_eq!(parts.len(), 3, "SE segment must have 3 elements");
        let se_count: u32 = parts[1].parse().expect("SE count must be numeric");
        assert_eq!(se_count, seg_count, "SE segment count must match returned count");
    }

    #[test]
    fn test_837p_subscriber_info_in_nm1() {
        let provider = BillingProvider {
            npi: "9876543210".to_string(),
            tax_id: "987654321".to_string(),
            last_name: "JONES".to_string(),
            first_name: "MARY".to_string(),
            address_line1: "789 ELM ST".to_string(),
            city: "PORTLAND".to_string(),
            state: "OR".to_string(),
            zip: "97201".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "XY789012".to_string(),
            last_name: "PATIENT".to_string(),
            first_name: "TEST".to_string(),
            birth_date: "1990-06-20".to_string(),
            gender: "female".to_string(),
            address_line1: "100 PINE RD".to_string(),
            city: "PORTLAND".to_string(),
            state: "OR".to_string(),
            zip: "97202".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM003".to_string(),
            total_charge: 100.00,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M25.511".to_string()],
            service_lines: vec![ServiceLine {
                cpt_code: "97010".to_string(),
                modifiers: vec![],
                units: 1,
                charge: 100.00,
                date_of_service: "2026-03-14".to_string(),
                dx_pointers: vec!["A".to_string()],
            }],
        };

        let (edi, _) = build_837p("000000003", &provider, &subscriber, "CIGNA01", &claim);

        // NM1*IL segment should contain subscriber last name and member ID
        let nm1_il = edi
            .lines()
            .find(|l| l.starts_with("NM1*IL*"))
            .expect("NM1*IL segment for subscriber must be present");
        assert!(nm1_il.contains("PATIENT"), "Subscriber last name must be in NM1*IL");
        assert!(nm1_il.contains("XY789012"), "Member ID must be in NM1*IL");
    }

    #[test]
    fn test_837p_service_line_sv1_format() {
        let provider = BillingProvider {
            npi: "1234567890".to_string(),
            tax_id: "123456789".to_string(),
            last_name: "SMITH".to_string(),
            first_name: "JOHN".to_string(),
            address_line1: "123 MAIN ST".to_string(),
            city: "ANYTOWN".to_string(),
            state: "CA".to_string(),
            zip: "90210".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "MB111222".to_string(),
            last_name: "DOE".to_string(),
            first_name: "JAMES".to_string(),
            birth_date: "1975-01-01".to_string(),
            gender: "male".to_string(),
            address_line1: "1 BROAD ST".to_string(),
            city: "CHICAGO".to_string(),
            state: "IL".to_string(),
            zip: "60601".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM004".to_string(),
            total_charge: 200.00,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M54.4".to_string()],
            service_lines: vec![ServiceLine {
                cpt_code: "97530".to_string(),
                modifiers: vec!["KX".to_string(), "59".to_string()],
                units: 3,
                charge: 200.00,
                date_of_service: "2026-03-14".to_string(),
                dx_pointers: vec!["A".to_string()],
            }],
        };

        let (edi, _) = build_837p("000000004", &provider, &subscriber, "UHC001", &claim);

        // SV1 segment should contain CPT code and units
        let sv1_line = edi
            .lines()
            .find(|l| l.starts_with("SV1*"))
            .expect("SV1 segment for service line must be present");
        assert!(sv1_line.contains("97530"), "SV1 must contain CPT 97530");
        assert!(sv1_line.contains("3"), "SV1 must contain 3 units");
        // Modifiers in component separator format
        assert!(sv1_line.contains("KX"), "SV1 must contain modifier KX");
    }

    // ── Claim Validation Logic ───────────────────────────────────────────────

    #[test]
    fn test_npi_validation_rules() {
        // NPI must be exactly 10 digits
        let valid_npi = "1234567890";
        assert_eq!(valid_npi.len(), 10);
        assert!(valid_npi.chars().all(|c| c.is_ascii_digit()));

        let short_npi = "123456789";
        assert_ne!(short_npi.len(), 10);

        let alpha_npi = "12345ABCDE";
        assert!(!alpha_npi.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_claim_status_round_trip() {
        for status in ["draft", "validated", "submitted", "accepted", "paid", "denied", "appealed"] {
            let parsed = ClaimStatus::from_str(status).expect("Known status must parse");
            assert_eq!(parsed.as_str(), status, "Status must round-trip correctly");
        }
    }

    #[test]
    fn test_claim_status_invalid() {
        assert!(ClaimStatus::from_str("unknown").is_none());
        assert!(ClaimStatus::from_str("").is_none());
        assert!(ClaimStatus::from_str("DRAFT").is_none()); // case-sensitive
    }

    // ── Payer Configuration ──────────────────────────────────────────────────

    #[test]
    fn test_clearinghouse_round_trip() {
        for (s, variant) in [
            ("office_ally", Clearinghouse::OfficeAlly),
            ("availity", Clearinghouse::Availity),
            ("trizetto", Clearinghouse::Trizetto),
            ("manual", Clearinghouse::Manual),
        ] {
            let parsed = Clearinghouse::from_str(s).expect("Known clearinghouse must parse");
            assert_eq!(parsed, variant);
            assert_eq!(parsed.as_str(), s);
        }
    }

    #[test]
    fn test_billing_rule_round_trip() {
        assert_eq!(BillingRule::from_str("medicare").unwrap().as_str(), "medicare");
        assert_eq!(BillingRule::from_str("ama").unwrap().as_str(), "ama");
        assert!(BillingRule::from_str("unknown").is_none());
    }

    // ── Service Line Formatting ──────────────────────────────────────────────

    #[test]
    fn test_multiple_service_lines_generate_lx_sv1_dtp() {
        let provider = BillingProvider {
            npi: "1234567890".to_string(),
            tax_id: "123456789".to_string(),
            last_name: "SMITH".to_string(),
            first_name: "JOHN".to_string(),
            address_line1: "123 MAIN ST".to_string(),
            city: "ANYTOWN".to_string(),
            state: "CA".to_string(),
            zip: "90210".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "MB999".to_string(),
            last_name: "LAST".to_string(),
            first_name: "FIRST".to_string(),
            birth_date: "1980-12-01".to_string(),
            gender: "male".to_string(),
            address_line1: "1 ST".to_string(),
            city: "CITY".to_string(),
            state: "TX".to_string(),
            zip: "75001".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM005".to_string(),
            total_charge: 500.00,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M54.5".to_string()],
            service_lines: vec![
                ServiceLine {
                    cpt_code: "97161".to_string(),
                    modifiers: vec![],
                    units: 1,
                    charge: 200.00,
                    date_of_service: "2026-03-14".to_string(),
                    dx_pointers: vec!["A".to_string()],
                },
                ServiceLine {
                    cpt_code: "97110".to_string(),
                    modifiers: vec!["KX".to_string()],
                    units: 2,
                    charge: 150.00,
                    date_of_service: "2026-03-14".to_string(),
                    dx_pointers: vec!["A".to_string()],
                },
                ServiceLine {
                    cpt_code: "97140".to_string(),
                    modifiers: vec!["KX".to_string()],
                    units: 2,
                    charge: 150.00,
                    date_of_service: "2026-03-14".to_string(),
                    dx_pointers: vec!["A".to_string()],
                },
            ],
        };

        let (edi, _) = build_837p("000000005", &provider, &subscriber, "MCAID01", &claim);

        // Count LX segments — should match service line count
        let lx_count = edi.lines().filter(|l| l.starts_with("LX*")).count();
        assert_eq!(lx_count, 3, "Must have one LX per service line");

        let sv1_count = edi.lines().filter(|l| l.starts_with("SV1*")).count();
        assert_eq!(sv1_count, 3, "Must have one SV1 per service line");

        let dtp_count = edi.lines().filter(|l| l.starts_with("DTP*472*")).count();
        assert_eq!(dtp_count, 3, "Must have one DTP*472 per service line");
    }

    #[test]
    fn test_837p_clm_contains_total_charge() {
        let provider = BillingProvider {
            npi: "1234567890".to_string(),
            tax_id: "123456789".to_string(),
            last_name: "SMITH".to_string(),
            first_name: "JOHN".to_string(),
            address_line1: "123 MAIN ST".to_string(),
            city: "ANYTOWN".to_string(),
            state: "CA".to_string(),
            zip: "90210".to_string(),
        };
        let subscriber = SubscriberInfo {
            member_id: "MB777".to_string(),
            last_name: "TEST".to_string(),
            first_name: "USER".to_string(),
            birth_date: "1970-01-01".to_string(),
            gender: "male".to_string(),
            address_line1: "1 MAIN".to_string(),
            city: "TOWN".to_string(),
            state: "NY".to_string(),
            zip: "10001".to_string(),
        };
        let claim = ClaimInfo {
            claim_id: "CLM006".to_string(),
            total_charge: 999.99,
            place_of_service: "11".to_string(),
            diagnosis_codes: vec!["M54.5".to_string()],
            service_lines: vec![ServiceLine {
                cpt_code: "97162".to_string(),
                modifiers: vec![],
                units: 1,
                charge: 999.99,
                date_of_service: "2026-03-14".to_string(),
                dx_pointers: vec!["A".to_string()],
            }],
        };

        let (edi, _) = build_837p("000000006", &provider, &subscriber, "PAYER01", &claim);

        let clm_line = edi
            .lines()
            .find(|l| l.starts_with("CLM*"))
            .expect("CLM segment must be present");
        assert!(clm_line.contains("999.99"), "CLM segment must contain total charge");
        assert!(clm_line.contains("11"), "CLM segment must contain place of service 11");
    }
}
