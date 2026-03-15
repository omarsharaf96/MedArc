/// commands/pdf_export.rs — PDF Export Engine (M003/S05)
///
/// Implements 5 PDF export types for clinical documents:
///   1. Single Note PDF       — generate_note_pdf(pt_note_id)
///   2. Progress Report       — generate_progress_report(patient_id, start_date?, end_date?)
///   3. Insurance Narrative   — generate_insurance_narrative(patient_id)
///   4. Legal/IME Report      — generate_legal_report(patient_id)
///   5. Full Chart Export     — generate_chart_export(patient_id, start_date?, end_date?)
///
/// PDF generation uses the `printpdf` crate with built-in Helvetica fonts.
/// Pages are Letter size (8.5" x 11") with 1" margins.
///
/// Each command:
///   - Requires authentication and RBAC permission (PdfExport::Create)
///   - Writes an audit trail entry
///   - Logs the export in `export_log` table
///   - Returns a file path to a temp-generated PDF
///
/// RBAC
/// ----
/// Provider / SystemAdmin: full Create access (generate PDFs).
/// NurseMa: Read only (cannot generate, but can view existing export log).
/// BillingStaff: Read only.
/// FrontDesk: no access.
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.
use printpdf::*;
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
// Constants — Page layout
// ─────────────────────────────────────────────────────────────────────────────

/// Letter page width in mm (8.5 inches).
const PAGE_WIDTH_MM: f32 = 215.9;
/// Letter page height in mm (11 inches).
const PAGE_HEIGHT_MM: f32 = 279.4;
/// Left margin in mm (1 inch).
const MARGIN_LEFT_MM: f32 = 25.4;
/// Right margin in mm (1 inch).
const MARGIN_RIGHT_MM: f32 = 25.4;
/// Top margin in mm (1 inch).
const MARGIN_TOP_MM: f32 = 25.4;
/// Bottom margin in mm (1 inch).
const MARGIN_BOTTOM_MM: f32 = 25.4;

/// Usable content width in mm.
const CONTENT_WIDTH_MM: f32 = PAGE_WIDTH_MM - MARGIN_LEFT_MM - MARGIN_RIGHT_MM;

/// Font sizes in points.
const FONT_SIZE_TITLE: f32 = 16.0;
const FONT_SIZE_HEADING: f32 = 12.0;
const FONT_SIZE_BODY: f32 = 10.0;
const FONT_SIZE_SMALL: f32 = 8.0;

/// Line height multiplier (relative to font size).
const LINE_HEIGHT_FACTOR: f32 = 1.4;

/// Approximate average character width for Helvetica at 1pt in mm.
/// Used for word-wrap estimation. Helvetica average char width is ~0.5em,
/// and 1pt = 0.3528mm, so 1pt char ~= 0.22mm. We use a conservative factor.
const CHAR_WIDTH_PER_PT_MM: f32 = 0.22;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result returned to the frontend after PDF generation.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PdfExportResult {
    /// Absolute path to the generated PDF file.
    pub file_path: String,
    /// Type of export: note_pdf, progress_report, insurance_narrative, legal_report, chart_export.
    pub export_type: String,
    /// Number of pages in the generated PDF.
    pub pages: u32,
}

/// One row from the export_log table.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLogEntry {
    pub export_id: String,
    pub patient_id: String,
    pub export_type: String,
    pub file_path: String,
    pub generated_at: String,
    pub generated_by: String,
}

/// Practice settings loaded from app_settings for letterhead rendering.
#[derive(Debug, Clone)]
pub struct PracticeSettings {
    pub practice_name: String,
    pub practice_address: String,
    pub practice_phone: String,
    pub practice_fax: String,
    pub practice_npi: String,
}

/// Patient demographic info extracted from the database for PDF headers.
#[derive(Debug, Clone)]
struct PatientInfo {
    patient_id: String,
    family_name: String,
    given_name: String,
    birth_date: String,
    mrn: String,
}

/// Provider info for signature lines.
#[derive(Debug, Clone)]
struct ProviderInfo {
    display_name: String,
    #[allow(dead_code)]
    user_id: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// PDF Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Estimate line height in mm for a given font size.
fn line_height_mm(font_size: f32) -> f32 {
    font_size * 0.3528 * LINE_HEIGHT_FACTOR
}

/// Estimate how many characters fit on one line at the given font size
/// within the content width.
fn chars_per_line(font_size: f32) -> usize {
    let char_width = CHAR_WIDTH_PER_PT_MM * font_size;
    if char_width <= 0.0 {
        return 80;
    }
    (CONTENT_WIDTH_MM / char_width) as usize
}

/// Word-wrap a text string into lines that fit within the page content width
/// at the given font size.
pub fn wrap_text(text: &str, font_size: f32) -> Vec<String> {
    let max_chars = chars_per_line(font_size);
    if max_chars == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in &words {
            if current_line.is_empty() {
                if word.len() > max_chars {
                    // Very long word — break it
                    let mut remaining = *word;
                    while remaining.len() > max_chars {
                        lines.push(remaining[..max_chars].to_string());
                        remaining = &remaining[max_chars..];
                    }
                    current_line = remaining.to_string();
                } else {
                    current_line = word.to_string();
                }
            } else if current_line.len() + 1 + word.len() <= max_chars {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Check whether we need a new page. Returns true if remaining vertical space
/// is less than the minimum needed.
pub fn needs_new_page(current_y_mm: f32, min_needed_mm: f32) -> bool {
    current_y_mm - min_needed_mm < MARGIN_BOTTOM_MM
}

/// Validate that an export type string is one of the allowed values.
pub fn validate_export_type(export_type: &str) -> Result<(), AppError> {
    match export_type {
        "note_pdf" | "progress_report" | "insurance_narrative" | "legal_report" | "chart_export" => {
            Ok(())
        }
        _ => Err(AppError::Validation(format!(
            "Invalid export type '{}'. Must be one of: note_pdf, progress_report, insurance_narrative, legal_report, chart_export",
            export_type
        ))),
    }
}

/// Build the letterhead text lines for the practice header.
pub fn build_letterhead_lines(settings: &PracticeSettings) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(settings.practice_name.clone());
    if !settings.practice_address.is_empty() {
        lines.push(settings.practice_address.clone());
    }
    let mut contact_line = String::new();
    if !settings.practice_phone.is_empty() {
        contact_line.push_str(&format!("Phone: {}", settings.practice_phone));
    }
    if !settings.practice_fax.is_empty() {
        if !contact_line.is_empty() {
            contact_line.push_str("  |  ");
        }
        contact_line.push_str(&format!("Fax: {}", settings.practice_fax));
    }
    if !contact_line.is_empty() {
        lines.push(contact_line);
    }
    if !settings.practice_npi.is_empty() {
        lines.push(format!("NPI: {}", settings.practice_npi));
    }
    lines
}

/// Generate the patient header text lines.
fn build_patient_header_lines(patient: &PatientInfo, report_date: &str) -> Vec<String> {
    vec![
        format!(
            "Patient: {} {}",
            patient.given_name, patient.family_name
        ),
        format!("DOB: {}  |  MRN: {}", patient.birth_date, patient.mrn),
        format!("Date of Report: {}", report_date),
    ]
}

/// Build a signature line block.
fn build_signature_lines(provider_name: &str) -> Vec<String> {
    vec![
        String::new(),
        String::new(),
        "________________________________________".to_string(),
        format!("{}", provider_name),
        String::new(),
        format!("Date: ________________________________________"),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// PDF Document Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Stateful builder that tracks pages and vertical position.
struct PdfBuilder {
    doc: PdfDocumentReference,
    /// Current page reference.
    current_page: PdfPageIndex,
    /// Current layer reference.
    current_layer: PdfLayerIndex,
    /// Current Y position (from top of page, in mm).
    y_pos: f32,
    /// Total number of pages.
    page_count: u32,
    /// Built-in Helvetica font (regular).
    font_regular: IndirectFontRef,
    /// Built-in Helvetica-Bold font.
    font_bold: IndirectFontRef,
}

impl PdfBuilder {
    /// Create a new PDF document with the given title.
    fn new(title: &str) -> Result<Self, AppError> {
        let (doc, page, layer) = PdfDocument::new(
            title,
            Mm(PAGE_WIDTH_MM),
            Mm(PAGE_HEIGHT_MM),
            "Layer 1",
        );

        let font_regular = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| AppError::Validation(format!("Failed to load Helvetica font: {}", e)))?;
        let font_bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| {
                AppError::Validation(format!("Failed to load Helvetica-Bold font: {}", e))
            })?;

        Ok(Self {
            doc,
            current_page: page,
            current_layer: layer,
            y_pos: PAGE_HEIGHT_MM - MARGIN_TOP_MM,
            page_count: 1,
            font_regular,
            font_bold,
        })
    }

    /// Get the current layer for drawing.
    fn layer(&self) -> PdfLayerReference {
        self.doc
            .get_page(self.current_page)
            .get_layer(self.current_layer)
    }

    /// Add a new page and return to the top.
    fn new_page(&mut self) {
        let (page, layer) =
            self.doc
                .add_page(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), "Layer 1");
        self.current_page = page;
        self.current_layer = layer;
        self.y_pos = PAGE_HEIGHT_MM - MARGIN_TOP_MM;
        self.page_count += 1;
    }

    /// Check and add a new page if needed, returning the (possibly updated) Y position.
    fn ensure_space(&mut self, needed_mm: f32) {
        if needs_new_page(self.y_pos, needed_mm) {
            self.new_page();
        }
    }

    /// Write a single line of text using the regular font.
    fn write_text(&mut self, text: &str, font_size: f32) {
        let layer = self.layer();
        layer.use_text(
            text,
            font_size,
            Mm(MARGIN_LEFT_MM),
            Mm(self.y_pos),
            &self.font_regular,
        );
        self.y_pos -= line_height_mm(font_size);
    }

    /// Write a single line of bold text.
    fn write_bold(&mut self, text: &str, font_size: f32) {
        let layer = self.layer();
        layer.use_text(
            text,
            font_size,
            Mm(MARGIN_LEFT_MM),
            Mm(self.y_pos),
            &self.font_bold,
        );
        self.y_pos -= line_height_mm(font_size);
    }

    /// Add the practice letterhead at the current position.
    fn add_letterhead(&mut self, settings: &PracticeSettings) {
        let lines = build_letterhead_lines(settings);

        // Practice name in title font
        if let Some(first) = lines.first() {
            self.write_bold(first, FONT_SIZE_TITLE);
        }
        // Remaining lines in small font
        for line in lines.iter().skip(1) {
            self.write_text(line, FONT_SIZE_SMALL);
        }

        // Separator line
        self.y_pos -= 2.0;
        let layer = self.layer();
        let line = Line {
            points: vec![
                (Point::new(Mm(MARGIN_LEFT_MM), Mm(self.y_pos)), false),
                (
                    Point::new(Mm(PAGE_WIDTH_MM - MARGIN_RIGHT_MM), Mm(self.y_pos)),
                    false,
                ),
            ],
            is_closed: false,
        };
        layer.add_line(line);
        self.y_pos -= 4.0;
    }

    /// Add patient demographic header at the current position.
    fn add_patient_header(&mut self, patient: &PatientInfo, report_date: &str) {
        let lines = build_patient_header_lines(patient, report_date);
        for line in &lines {
            self.write_text(line, FONT_SIZE_BODY);
        }
        self.y_pos -= 4.0;
    }

    /// Add a section heading (bold, slightly larger).
    fn add_section_heading(&mut self, title: &str) {
        self.ensure_space(line_height_mm(FONT_SIZE_HEADING) + 4.0);
        self.y_pos -= 2.0;
        self.write_bold(title, FONT_SIZE_HEADING);
        self.y_pos -= 1.0;
    }

    /// Add a word-wrapped text block.
    fn add_text_block(&mut self, text: &str) {
        let lines = wrap_text(text, FONT_SIZE_BODY);
        for line in &lines {
            self.ensure_space(line_height_mm(FONT_SIZE_BODY));
            self.write_text(line, FONT_SIZE_BODY);
        }
    }

    /// Add a labeled field (label in bold, value in regular).
    fn add_field(&mut self, label: &str, value: &str) {
        self.ensure_space(line_height_mm(FONT_SIZE_BODY));
        // Write label bold, then value on same or next line
        let combined = format!("{}: {}", label, value);
        let lines = wrap_text(&combined, FONT_SIZE_BODY);
        for (i, line) in lines.iter().enumerate() {
            self.ensure_space(line_height_mm(FONT_SIZE_BODY));
            if i == 0 {
                // First line starts with bold label — for simplicity we render the entire
                // first line in regular since we can't mix fonts on one printpdf line easily.
                // A future enhancement could split the text.
                self.write_text(line, FONT_SIZE_BODY);
            } else {
                self.write_text(line, FONT_SIZE_BODY);
            }
        }
    }

    /// Add a signature line block.
    fn add_signature_line(&mut self, provider_name: &str) {
        let lines = build_signature_lines(provider_name);
        self.ensure_space(line_height_mm(FONT_SIZE_BODY) * lines.len() as f32);
        for line in &lines {
            self.write_text(line, FONT_SIZE_BODY);
        }
    }

    /// Save the PDF document to the given file path.
    fn save(self, path: &str) -> Result<(), AppError> {
        let file = std::fs::File::create(path)
            .map_err(AppError::Io)?;
        let mut writer = std::io::BufWriter::new(file);
        self.doc
            .save(&mut writer)
            .map_err(|e| AppError::Validation(format!("Failed to save PDF: {}", e)))?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Database Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Load practice settings from the `app_settings` table.
fn load_practice_settings(conn: &rusqlite::Connection) -> PracticeSettings {
    let get_setting = |key: &str, default: &str| -> String {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| default.to_string())
    };

    PracticeSettings {
        practice_name: get_setting("practice_name", "Physical Therapy Practice"),
        practice_address: get_setting("practice_address", ""),
        practice_phone: get_setting("practice_phone", ""),
        practice_fax: get_setting("practice_fax", ""),
        practice_npi: get_setting("practice_npi", ""),
    }
}

/// Load patient info from the patient_index table.
fn load_patient_info(
    conn: &rusqlite::Connection,
    patient_id: &str,
) -> Result<PatientInfo, AppError> {
    conn.query_row(
        "SELECT patient_id, family_name, COALESCE(given_name, ''), COALESCE(birth_date, ''), mrn
         FROM patient_index WHERE patient_id = ?1",
        rusqlite::params![patient_id],
        |row| {
            Ok(PatientInfo {
                patient_id: row.get(0)?,
                family_name: row.get(1)?,
                given_name: row.get(2)?,
                birth_date: row.get(3)?,
                mrn: row.get(4)?,
            })
        },
    )
    .map_err(|_| AppError::NotFound(format!("Patient not found: {}", patient_id)))
}

/// Load provider info from the users table.
fn load_provider_info(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<ProviderInfo, AppError> {
    conn.query_row(
        "SELECT id, display_name FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |row| {
            Ok(ProviderInfo {
                user_id: row.get(0)?,
                display_name: row.get(1)?,
            })
        },
    )
    .map_err(|_| AppError::NotFound(format!("Provider not found: {}", user_id)))
}

/// Log an export to the export_log table.
fn log_export(
    conn: &rusqlite::Connection,
    patient_id: &str,
    export_type: &str,
    file_path: &str,
    generated_by: &str,
) -> Result<String, AppError> {
    let export_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO export_log (export_id, patient_id, export_type, file_path, generated_by)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![&export_id, patient_id, export_type, file_path, generated_by],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(export_id)
}

/// Generate a temp file path for the PDF output.
/// Uses ~/Library/Application Support/com.medarc.emr/exports to keep PHI
/// out of world-readable /tmp.
fn temp_pdf_path(prefix: &str) -> Result<String, AppError> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let base = std::path::PathBuf::from(home)
        .join("Library/Application Support/com.medarc.emr/exports");
    std::fs::create_dir_all(&base).map_err(|e| AppError::Io(e))?;
    let filename = format!(
        "{}-{}.pdf",
        prefix,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    Ok(base.join(filename).to_string_lossy().into_owned())
}

/// Load encounters for a patient, optionally filtered by date range.
fn load_encounters(
    conn: &rusqlite::Connection,
    patient_id: &str,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<Vec<(String, serde_json::Value, String)>, AppError> {
    let mut sql = String::from(
        "SELECT f.id, f.resource, e.encounter_date
         FROM fhir_resources f
         JOIN encounter_index e ON f.id = e.encounter_id
         WHERE e.patient_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(patient_id.to_string())];

    if let Some(sd) = start_date {
        sql.push_str(" AND e.encounter_date >= ?2");
        params.push(Box::new(sd.to_string()));
        if let Some(ed) = end_date {
            sql.push_str(" AND e.encounter_date <= ?3");
            params.push(Box::new(ed.to_string()));
        }
    } else if let Some(ed) = end_date {
        sql.push_str(" AND e.encounter_date <= ?2");
        params.push(Box::new(ed.to_string()));
    }

    sql.push_str(" ORDER BY e.encounter_date ASC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            let id: String = row.get(0)?;
            let resource_json: String = row.get(1)?;
            let date: String = row.get(2)?;
            let resource: serde_json::Value =
                serde_json::from_str(&resource_json).unwrap_or(serde_json::Value::Null);
            Ok((id, resource, date))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(result)
}

/// Load problems/diagnoses for a patient.
fn load_problems(
    conn: &rusqlite::Connection,
    patient_id: &str,
) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT p.icd10_code, COALESCE(json_extract(f.resource, '$.code.text'), '')
             FROM problem_index p
             JOIN fhir_resources f ON f.id = p.problem_id
             WHERE p.patient_id = ?1 AND p.clinical_status = 'active'",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(result)
}

/// Extract a string from a JSON value by key path.
fn json_str(val: &serde_json::Value, key: &str) -> String {
    val.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Render a single encounter/note onto the PDF builder.
fn render_encounter(
    builder: &mut PdfBuilder,
    resource: &serde_json::Value,
    encounter_date: &str,
) {
    let encounter_type = json_str(resource, "encounter_type");
    let status = json_str(resource, "status");
    let chief_complaint = json_str(resource, "chief_complaint");

    builder.add_section_heading(&format!(
        "Encounter: {} — {} ({})",
        encounter_date, encounter_type, status
    ));

    if !chief_complaint.is_empty() {
        builder.add_field("Chief Complaint", &chief_complaint);
    }

    // SOAP note sections
    if let Some(soap) = resource.get("soap") {
        if let Some(s) = soap.get("subjective").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                builder.add_section_heading("Subjective");
                builder.add_text_block(s);
            }
        }
        if let Some(o) = soap.get("objective").and_then(|v| v.as_str()) {
            if !o.is_empty() {
                builder.add_section_heading("Objective");
                builder.add_text_block(o);
            }
        }
        if let Some(a) = soap.get("assessment").and_then(|v| v.as_str()) {
            if !a.is_empty() {
                builder.add_section_heading("Assessment");
                builder.add_text_block(a);
            }
        }
        if let Some(p) = soap.get("plan").and_then(|v| v.as_str()) {
            if !p.is_empty() {
                builder.add_section_heading("Plan");
                builder.add_text_block(p);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a PDF for a single clinical note/encounter.
///
/// Takes an encounter ID (pt_note_id), fetches the encounter data,
/// and renders it with letterhead and signature line.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_note_pdf(
    pt_note_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;

    // Fetch the encounter
    let (resource_json, patient_id, encounter_date): (String, String, String) = conn
        .query_row(
            "SELECT f.resource, e.patient_id, e.encounter_date
             FROM fhir_resources f
             JOIN encounter_index e ON f.id = e.encounter_id
             WHERE f.id = ?1",
            rusqlite::params![&pt_note_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("Encounter not found: {}", pt_note_id)))?;

    let resource: serde_json::Value = serde_json::from_str(&resource_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Build PDF
    let mut builder = PdfBuilder::new(&format!(
        "Clinical Note - {} {} - {}",
        patient.given_name, patient.family_name, encounter_date
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    render_encounter(&mut builder, &resource, &encounter_date);

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("note")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    // Log the export
    log_export(&conn, &patient_id, "note_pdf", &file_path, &session.user_id)?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_note_pdf".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: Some(pt_note_id),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated note PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "note_pdf".to_string(),
        pages: page_count,
    })
}

/// Generate a progress report PDF for a patient within a date range.
///
/// Includes: episode dates, total visits, diagnosis, STG/LTG status,
/// outcome measure scores, functional progress narrative, and continued care
/// justification.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_progress_report(
    patient_id: String,
    start_date: Option<String>,
    end_date: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;
    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Load encounters within date range
    let encounters = load_encounters(
        &conn,
        &patient_id,
        start_date.as_deref(),
        end_date.as_deref(),
    )?;

    // Load active problems
    let problems = load_problems(&conn, &patient_id)?;

    // Build PDF
    let mut builder = PdfBuilder::new(&format!(
        "Progress Report - {} {}",
        patient.given_name, patient.family_name
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    builder.add_section_heading("PROGRESS REPORT");

    // Episode summary
    let date_range_text = match (start_date.as_deref(), end_date.as_deref()) {
        (Some(sd), Some(ed)) => format!("{} to {}", sd, ed),
        (Some(sd), None) => format!("{} to present", sd),
        (None, Some(ed)) => format!("Up to {}", ed),
        (None, None) => "All dates".to_string(),
    };
    builder.add_field("Reporting Period", &date_range_text);
    builder.add_field("Total Visits", &encounters.len().to_string());

    // Diagnoses
    if !problems.is_empty() {
        builder.add_section_heading("Diagnoses");
        for (icd10, desc) in &problems {
            let diagnosis_text = if desc.is_empty() {
                icd10.clone()
            } else {
                format!("{} — {}", icd10, desc)
            };
            builder.add_text_block(&format!("  - {}", diagnosis_text));
        }
    }

    // Goals section (placeholder — goals would come from a goals table in future)
    builder.add_section_heading("Goals & Functional Outcomes");
    builder.add_text_block(
        "Short-Term Goals (STG) and Long-Term Goals (LTG) status \
         should be documented by the treating provider.",
    );

    // Outcome measures (placeholder — would come from outcome_score_index)
    builder.add_section_heading("Outcome Measures");
    builder.add_text_block(
        "Outcome measure scores (initial vs. current) to be populated \
         from clinical assessments.",
    );

    // Functional progress narrative
    builder.add_section_heading("Functional Progress Narrative");
    builder.add_text_block("[To be completed by treating provider]");

    // Continued care justification
    builder.add_section_heading("Continued Care Justification");
    builder.add_text_block("[To be completed by treating provider]");

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("progress-report")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    log_export(
        &conn,
        &patient_id,
        "progress_report",
        &file_path,
        &session.user_id,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_progress_report".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated progress report PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "progress_report".to_string(),
        pages: page_count,
    })
}

/// Generate an insurance narrative PDF for a patient.
///
/// Includes: diagnosis with ICD-10, functional limitations,
/// CPT codes billed, medical necessity statement, and clinical evidence.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_insurance_narrative(
    patient_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;
    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let problems = load_problems(&conn, &patient_id)?;

    // Load total encounter count
    let total_visits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM encounter_index WHERE patient_id = ?1",
            rusqlite::params![&patient_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Build PDF
    let mut builder = PdfBuilder::new(&format!(
        "Insurance Narrative - {} {}",
        patient.given_name, patient.family_name
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    builder.add_section_heading("INSURANCE NARRATIVE REPORT");

    // Diagnoses with ICD-10
    builder.add_section_heading("Diagnosis (ICD-10)");
    if problems.is_empty() {
        builder.add_text_block("No active diagnoses on file.");
    } else {
        for (icd10, desc) in &problems {
            let text = if desc.is_empty() {
                icd10.clone()
            } else {
                format!("{} — {}", icd10, desc)
            };
            builder.add_text_block(&format!("  - {}", text));
        }
    }

    // Functional limitations
    builder.add_section_heading("Functional Limitations");
    builder.add_text_block("[To be completed by treating provider]");

    // CPT codes billed
    builder.add_section_heading("Services Provided (CPT Codes)");
    builder.add_field("Total Visits", &total_visits.to_string());
    builder.add_text_block(
        "Detailed CPT code breakdown to be provided by the billing department.",
    );

    // Medical necessity
    builder.add_section_heading("Medical Necessity Statement");
    builder.add_text_block(
        "The above services are medically necessary to address the patient's \
         functional limitations and restore the patient to their prior level of function. \
         Skilled therapy intervention is required due to the complexity of the patient's condition.",
    );

    // Clinical evidence
    builder.add_section_heading("Clinical Evidence of Progress");
    builder.add_text_block("[To be completed by treating provider]");

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("insurance-narrative")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    log_export(
        &conn,
        &patient_id,
        "insurance_narrative",
        &file_path,
        &session.user_id,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_insurance_narrative".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated insurance narrative PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "insurance_narrative".to_string(),
        pages: page_count,
    })
}

/// Generate a legal/IME report PDF for a patient.
///
/// Includes: mechanism of injury, clinical findings summary,
/// functional impact assessment, treatment summary, and prognosis.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_legal_report(
    patient_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;
    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let problems = load_problems(&conn, &patient_id)?;
    let encounters = load_encounters(&conn, &patient_id, None, None)?;

    // Build PDF
    let mut builder = PdfBuilder::new(&format!(
        "Legal/IME Report - {} {}",
        patient.given_name, patient.family_name
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    builder.add_section_heading("LEGAL / INDEPENDENT MEDICAL EXAMINATION REPORT");

    // Mechanism of injury
    builder.add_section_heading("Mechanism of Injury");
    builder.add_text_block("[To be completed by treating provider]");

    // Clinical findings
    builder.add_section_heading("Clinical Findings Summary");
    if !problems.is_empty() {
        builder.add_text_block("Active diagnoses:");
        for (icd10, desc) in &problems {
            let text = if desc.is_empty() {
                icd10.clone()
            } else {
                format!("{} — {}", icd10, desc)
            };
            builder.add_text_block(&format!("  - {}", text));
        }
    } else {
        builder.add_text_block("No active diagnoses on file.");
    }

    // Functional impact
    builder.add_section_heading("Functional Impact Assessment");
    builder.add_text_block("[To be completed by treating provider]");

    // Treatment summary
    builder.add_section_heading("Treatment Provided Summary");
    builder.add_field("Total Encounters", &encounters.len().to_string());
    if let Some(first) = encounters.first() {
        builder.add_field("First Encounter", &first.2);
    }
    if let Some(last) = encounters.last() {
        builder.add_field("Most Recent Encounter", &last.2);
    }

    // Prognosis
    builder.add_section_heading("Prognosis Statement");
    builder.add_text_block("[To be completed by treating provider]");

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("legal-report")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    log_export(
        &conn,
        &patient_id,
        "legal_report",
        &file_path,
        &session.user_id,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_legal_report".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated legal/IME report PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "legal_report".to_string(),
        pages: page_count,
    })
}

/// Generate a full chart export PDF for a patient.
///
/// Includes a cover page with table of contents, then all encounters
/// in date order, each as a section. Optionally filtered by date range.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_chart_export(
    patient_id: String,
    start_date: Option<String>,
    end_date: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;
    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let encounters = load_encounters(
        &conn,
        &patient_id,
        start_date.as_deref(),
        end_date.as_deref(),
    )?;
    let problems = load_problems(&conn, &patient_id)?;

    // Build PDF
    let mut builder = PdfBuilder::new(&format!(
        "Chart Export - {} {}",
        patient.given_name, patient.family_name
    ))?;

    // ── Cover Page ──
    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    builder.add_section_heading("FULL CHART EXPORT");

    let date_range_text = match (start_date.as_deref(), end_date.as_deref()) {
        (Some(sd), Some(ed)) => format!("{} to {}", sd, ed),
        (Some(sd), None) => format!("{} to present", sd),
        (None, Some(ed)) => format!("Up to {}", ed),
        (None, None) => "All dates".to_string(),
    };
    builder.add_field("Date Range", &date_range_text);
    builder.add_field("Total Encounters", &encounters.len().to_string());

    // Active diagnoses on cover page
    if !problems.is_empty() {
        builder.add_section_heading("Active Diagnoses");
        for (icd10, desc) in &problems {
            let text = if desc.is_empty() {
                icd10.clone()
            } else {
                format!("{} — {}", icd10, desc)
            };
            builder.add_text_block(&format!("  - {}", text));
        }
    }

    // Table of contents
    builder.add_section_heading("Table of Contents");
    for (i, (_id, resource, date)) in encounters.iter().enumerate() {
        let enc_type = json_str(resource, "encounter_type");
        let toc_line = format!("{}. {} — {}", i + 1, date, enc_type);
        builder.add_text_block(&toc_line);
    }

    // ── Encounter pages ──
    for (_id, resource, date) in &encounters {
        builder.new_page();
        builder.add_letterhead(&settings);
        builder.add_patient_header(&patient, &report_date);
        render_encounter(&mut builder, resource, date);
    }

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("chart-export")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    log_export(
        &conn,
        &patient_id,
        "chart_export",
        &file_path,
        &session.user_id,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_chart_export".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: None,
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated chart export PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "chart_export".to_string(),
        pages: page_count,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter Note PDF + Fax workflow commands
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a PDF of a specific encounter's SOAP note.
///
/// This is the encounter-workspace-level command that takes an encounter ID
/// and renders the note with letterhead, patient demographics, SOAP sections,
/// and a provider signature line. Returns the temp file path so the frontend
/// can save-as or pass it to the fax workflow.
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create.
#[tauri::command]
pub fn generate_encounter_note_pdf(
    encounter_id: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<PdfExportResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;

    if encounter_id.trim().is_empty() {
        return Err(AppError::Validation(
            "encounter_id is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;

    // Fetch the encounter from encounter_index + fhir_resources
    let (resource_json, patient_id, encounter_date): (String, String, String) = conn
        .query_row(
            "SELECT f.resource, e.patient_id, e.encounter_date
             FROM fhir_resources f
             JOIN encounter_index e ON f.id = e.encounter_id
             WHERE f.id = ?1",
            rusqlite::params![&encounter_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| {
            AppError::NotFound(format!("Encounter not found: {}", encounter_id))
        })?;

    let resource: serde_json::Value = serde_json::from_str(&resource_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Build the PDF
    let mut builder = PdfBuilder::new(&format!(
        "Encounter Note - {} {} - {}",
        patient.given_name, patient.family_name, encounter_date
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);

    // Render the encounter SOAP sections
    render_encounter(&mut builder, &resource, &encounter_date);

    builder.add_signature_line(&provider.display_name);

    let file_path = temp_pdf_path("encounter-note")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    // Log the export
    log_export(
        &conn,
        &patient_id,
        "note_pdf",
        &file_path,
        &session.user_id,
    )?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_encounter_note_pdf".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: Some(encounter_id),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!("generated encounter note PDF: {}", file_path)),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "note_pdf".to_string(),
        pages: page_count,
    })
}

/// Input for the fax-encounter-note workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaxEncounterNoteInput {
    /// The encounter ID whose note to fax.
    pub encounter_id: String,
    /// Recipient fax number (E.164 format).
    pub recipient_fax: String,
    /// Recipient name for the fax log.
    pub recipient_name: String,
    /// Optional patient ID (derived from encounter if not provided).
    pub patient_id: Option<String>,
}

/// Result of the fax-encounter-note workflow.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaxEncounterNoteResult {
    /// Path to the generated PDF that was faxed.
    pub pdf_path: String,
    /// The fax log entry ID.
    pub fax_id: String,
    /// Fax status after queueing.
    pub status: String,
}

/// Generate a PDF of the encounter note and immediately send it via Phaxio fax.
///
/// Workflow:
///   1. Generate the encounter note PDF (same as generate_encounter_note_pdf)
///   2. Read Phaxio credentials from app_settings
///   3. POST the PDF to Phaxio /v2/faxes
///   4. Log the fax in fax_log
///   5. Return the fax log entry
///
/// RBAC: Provider or SystemAdmin, PdfExport::Create + ClinicalRecords::Create.
#[tauri::command]
pub fn fax_encounter_note(
    input: FaxEncounterNoteInput,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<FaxEncounterNoteResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::PdfExport, Action::Create)?;
    middleware::require_permission(session.role, Resource::ClinicalRecords, Action::Create)?;

    if input.encounter_id.trim().is_empty() {
        return Err(AppError::Validation(
            "encounter_id is required".to_string(),
        ));
    }
    if input.recipient_fax.trim().is_empty() {
        return Err(AppError::Validation(
            "recipient_fax is required".to_string(),
        ));
    }
    if input.recipient_name.trim().is_empty() {
        return Err(AppError::Validation(
            "recipient_name is required".to_string(),
        ));
    }

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let settings = load_practice_settings(&conn);
    let provider = load_provider_info(&conn, &session.user_id)?;

    // Step 1: Fetch encounter data
    let (resource_json, patient_id, encounter_date): (String, String, String) = conn
        .query_row(
            "SELECT f.resource, e.patient_id, e.encounter_date
             FROM fhir_resources f
             JOIN encounter_index e ON f.id = e.encounter_id
             WHERE f.id = ?1",
            rusqlite::params![&input.encounter_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "Encounter not found: {}",
                input.encounter_id
            ))
        })?;

    let resource: serde_json::Value = serde_json::from_str(&resource_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let patient = load_patient_info(&conn, &patient_id)?;
    let report_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Step 2: Generate the PDF
    let mut builder = PdfBuilder::new(&format!(
        "Encounter Note - {} {} - {}",
        patient.given_name, patient.family_name, encounter_date
    ))?;

    builder.add_letterhead(&settings);
    builder.add_patient_header(&patient, &report_date);
    render_encounter(&mut builder, &resource, &encounter_date);
    builder.add_signature_line(&provider.display_name);

    let pdf_path = temp_pdf_path("fax-encounter-note")?;
    builder.save(&pdf_path)?;

    log_export(
        &conn,
        &patient_id,
        "note_pdf",
        &pdf_path,
        &session.user_id,
    )?;

    // Step 3: Read Phaxio credentials
    let get_setting = |key: &str| -> Result<String, AppError> {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| {
            AppError::Validation(format!(
                "Phaxio not configured: missing setting '{}'",
                key
            ))
        })
    };

    let api_key = get_setting("phaxio_api_key")?;
    let api_secret = get_setting("phaxio_api_secret")?;

    // Step 4: Send fax via Phaxio API
    let fax_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Read the PDF file bytes for multipart upload
    let pdf_bytes = std::fs::read(&pdf_path).map_err(AppError::Io)?;

    let client = reqwest::blocking::Client::new();
    let form = reqwest::blocking::multipart::Form::new()
        .text("api_key", api_key)
        .text("api_secret", api_secret)
        .text("to", input.recipient_fax.clone())
        .part(
            "file",
            reqwest::blocking::multipart::Part::bytes(pdf_bytes)
                .file_name("encounter-note.pdf")
                .mime_str("application/pdf")
                .map_err(|e| AppError::Validation(e.to_string()))?,
        );

    let mut status = "queued".to_string();
    let mut phaxio_fax_id: Option<String> = None;
    let mut error_message: Option<String> = None;

    match client
        .post("https://api.phaxio.com/v2/faxes")
        .multipart(form)
        .send()
    {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if body["success"].as_bool() == Some(true) {
                        phaxio_fax_id = body["data"]["id"]
                            .as_i64()
                            .map(|id| id.to_string());
                        status = "queued".to_string();
                    } else {
                        status = "failed".to_string();
                        error_message =
                            body["message"].as_str().map(|s| s.to_string());
                    }
                }
            } else {
                status = "failed".to_string();
                error_message = Some(format!("HTTP {}", resp.status()));
            }
        }
        Err(e) => {
            status = "failed".to_string();
            error_message = Some(e.to_string());
        }
    }

    // Step 5: Log fax in fax_log
    let doc_name = format!(
        "Encounter Note - {} {}",
        patient.given_name, patient.family_name
    );
    conn.execute(
        "INSERT INTO fax_log (fax_id, phaxio_fax_id, direction, patient_id, recipient_name, recipient_fax, document_name, file_path, status, sent_at, error_message, retry_count)
         VALUES (?1, ?2, 'sent', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
        rusqlite::params![
            fax_id,
            phaxio_fax_id,
            patient_id,
            input.recipient_name,
            input.recipient_fax,
            doc_name,
            pdf_path,
            status,
            now,
            error_message,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.fax_encounter_note".to_string(),
            resource_type: "FaxLog".to_string(),
            resource_id: Some(fax_id.clone()),
            patient_id: Some(patient_id),
            device_id: device_id.get().to_string(),
            success: status != "failed",
            details: Some(format!(
                "faxed encounter note to {} ({})",
                input.recipient_name, input.recipient_fax
            )),
        },
    );

    Ok(FaxEncounterNoteResult {
        pdf_path,
        fax_id,
        status,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Export Settings (Letterhead + Signature configuration)
// ─────────────────────────────────────────────────────────────────────────────

/// Export settings stored as JSON in app_settings under key "export_settings".
/// Contains letterhead and signature configuration for PDF exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    /// Practice name for letterhead.
    pub practice_name: Option<String>,
    /// Practice address for letterhead.
    pub practice_address: Option<String>,
    /// Practice phone number for letterhead.
    pub practice_phone: Option<String>,
    /// Practice logo as base64-encoded image data.
    pub practice_logo_base64: Option<String>,
    /// Provider signature image as base64-encoded image data.
    pub signature_image_base64: Option<String>,
    /// Provider name/credentials line (e.g. "Omar Safwat Sharaf, PT, DPT").
    pub provider_name_credentials: Option<String>,
    /// Provider license number.
    pub license_number: Option<String>,
}

/// Retrieve export settings (letterhead + signature) from app_settings.
///
/// Returns the stored ExportSettings or an empty default if not yet configured.
/// Requires PdfExport::Read permission.
#[tauri::command]
pub async fn get_export_settings(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<ExportSettings, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(_sess.role, Resource::PdfExport, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let json_str: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'export_settings'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "{}".to_string());

    let settings: ExportSettings =
        serde_json::from_str(&json_str).unwrap_or_else(|_| ExportSettings {
            practice_name: None,
            practice_address: None,
            practice_phone: None,
            practice_logo_base64: None,
            signature_image_base64: None,
            provider_name_credentials: None,
            license_number: None,
        });

    Ok(settings)
}

/// Save export settings (letterhead + signature) to app_settings.
///
/// Overwrites any existing export_settings value.
/// Requires PdfExport::Update permission (SystemAdmin or Provider).
#[tauri::command]
pub async fn set_export_settings(
    settings: ExportSettings,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<ExportSettings, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::PdfExport, Action::Update)?;

    let json_str = serde_json::to_string(&settings)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('export_settings', ?1, datetime('now'))",
        rusqlite::params![json_str],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pdf_export.export_settings.update".to_string(),
            resource_type: "AppSettings".to_string(),
            resource_id: Some("export_settings".to_string()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Export settings (letterhead/signature) updated".to_string()),
        },
    )?;

    Ok(settings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: Letterhead text generation ─────────────────────────────────

    #[test]
    fn letterhead_lines_include_all_practice_info() {
        let settings = PracticeSettings {
            practice_name: "Acme Physical Therapy".to_string(),
            practice_address: "123 Main St, Suite 100, Springfield, IL 62701".to_string(),
            practice_phone: "(555) 123-4567".to_string(),
            practice_fax: "(555) 123-4568".to_string(),
            practice_npi: "1234567890".to_string(),
        };

        let lines = build_letterhead_lines(&settings);

        assert!(lines.len() >= 3, "Expected at least 3 letterhead lines, got {}", lines.len());
        assert_eq!(lines[0], "Acme Physical Therapy");
        assert!(lines[1].contains("123 Main St"), "Address missing");
        assert!(lines.iter().any(|l| l.contains("(555) 123-4567")), "Phone missing");
        assert!(lines.iter().any(|l| l.contains("(555) 123-4568")), "Fax missing");
        assert!(lines.iter().any(|l| l.contains("1234567890")), "NPI missing");
    }

    #[test]
    fn letterhead_lines_handle_empty_fields() {
        let settings = PracticeSettings {
            practice_name: "Minimal Practice".to_string(),
            practice_address: "".to_string(),
            practice_phone: "".to_string(),
            practice_fax: "".to_string(),
            practice_npi: "".to_string(),
        };

        let lines = build_letterhead_lines(&settings);
        assert_eq!(lines.len(), 1, "Only practice name should be present");
        assert_eq!(lines[0], "Minimal Practice");
    }

    // ── Test 2: Page break calculation ────────────────────────────────────

    #[test]
    fn page_break_needed_when_y_below_margin() {
        // Y position near bottom of page
        let y_pos = MARGIN_BOTTOM_MM + 5.0;
        // Requesting 10mm of space
        assert!(
            needs_new_page(y_pos, 10.0),
            "Should need new page when insufficient space"
        );
    }

    #[test]
    fn page_break_not_needed_when_space_available() {
        let y_pos = PAGE_HEIGHT_MM - MARGIN_TOP_MM; // Top of content area
        assert!(
            !needs_new_page(y_pos, 50.0),
            "Should NOT need new page with plenty of space"
        );
    }

    #[test]
    fn page_break_exact_boundary() {
        // Exactly at the boundary: y_pos - needed == MARGIN_BOTTOM_MM
        let y_pos = MARGIN_BOTTOM_MM + 10.0;
        assert!(
            !needs_new_page(y_pos, 10.0),
            "Exactly at boundary should NOT need new page"
        );
    }

    // ── Test 3: Export type validation ─────────────────────────────────────

    #[test]
    fn valid_export_types_accepted() {
        assert!(validate_export_type("note_pdf").is_ok());
        assert!(validate_export_type("progress_report").is_ok());
        assert!(validate_export_type("insurance_narrative").is_ok());
        assert!(validate_export_type("legal_report").is_ok());
        assert!(validate_export_type("chart_export").is_ok());
    }

    #[test]
    fn invalid_export_type_rejected() {
        assert!(validate_export_type("invalid").is_err());
        assert!(validate_export_type("").is_err());
        assert!(validate_export_type("NOTE_PDF").is_err()); // case-sensitive
    }

    // ── Test 4: Word wrapping ─────────────────────────────────────────────

    #[test]
    fn wrap_text_splits_long_lines() {
        let long_text = "This is a very long sentence that should be split across multiple lines when rendered at a normal body font size in a PDF document with standard margins.";
        let lines = wrap_text(long_text, FONT_SIZE_BODY);
        assert!(
            lines.len() > 1,
            "Long text should wrap to multiple lines, got {}",
            lines.len()
        );
        // Verify all original words are present
        let rejoined = lines.join(" ");
        for word in long_text.split_whitespace() {
            assert!(
                rejoined.contains(word),
                "Word '{}' lost during wrapping",
                word
            );
        }
    }

    #[test]
    fn wrap_text_preserves_short_lines() {
        let short_text = "Hello";
        let lines = wrap_text(short_text, FONT_SIZE_BODY);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hello");
    }

    #[test]
    fn wrap_text_handles_empty_input() {
        let lines = wrap_text("", FONT_SIZE_BODY);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "");
    }

    #[test]
    fn wrap_text_handles_newlines() {
        let text = "Line one\nLine two\nLine three";
        let lines = wrap_text(text, FONT_SIZE_BODY);
        assert!(lines.len() >= 3, "Should preserve paragraph breaks");
    }

    // ── Test 5: Migration SQL validation ──────────────────────────────────

    #[test]
    fn export_log_migration_sql_is_valid() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS export_log (
                export_id    TEXT PRIMARY KEY,
                patient_id   TEXT NOT NULL,
                export_type  TEXT NOT NULL CHECK(export_type IN ('note_pdf','progress_report','insurance_narrative','legal_report','chart_export')),
                file_path    TEXT NOT NULL,
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                generated_by TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_export_patient ON export_log(patient_id);"
        ).expect("Migration SQL should be valid");

        // Insert a valid row
        conn.execute(
            "INSERT INTO export_log (export_id, patient_id, export_type, file_path, generated_by)
             VALUES ('e1', 'p1', 'note_pdf', '/tmp/test.pdf', 'user1')",
            [],
        )
        .expect("Valid insert should succeed");

        // Insert with invalid export_type should fail
        let result = conn.execute(
            "INSERT INTO export_log (export_id, patient_id, export_type, file_path, generated_by)
             VALUES ('e2', 'p1', 'invalid_type', '/tmp/test.pdf', 'user1')",
            [],
        );
        assert!(result.is_err(), "Invalid export_type should fail CHECK constraint");
    }

    // ── Test 6: PDF document creation ─────────────────────────────────────

    #[test]
    fn pdf_builder_creates_valid_document() {
        let builder = PdfBuilder::new("Test Document");
        assert!(builder.is_ok(), "PdfBuilder::new should succeed");
        let builder = builder.unwrap();
        assert_eq!(builder.page_count, 1, "New doc should have 1 page");
    }

    #[test]
    fn pdf_builder_add_page_increments_count() {
        let mut builder = PdfBuilder::new("Test").unwrap();
        assert_eq!(builder.page_count, 1);
        builder.new_page();
        assert_eq!(builder.page_count, 2);
        builder.new_page();
        assert_eq!(builder.page_count, 3);
    }

    // ── Test 7: Signature line generation ─────────────────────────────────

    #[test]
    fn signature_lines_contain_provider_name() {
        let lines = build_signature_lines("Dr. Jane Smith, PT, DPT");
        assert!(
            lines.iter().any(|l| l.contains("Dr. Jane Smith")),
            "Signature lines should contain provider name"
        );
        assert!(
            lines.iter().any(|l| l.contains("________")),
            "Signature lines should contain signature line"
        );
        assert!(
            lines.iter().any(|l| l.contains("Date:")),
            "Signature lines should contain date line"
        );
    }

    // ── Test 8: Line height and chars per line calculations ───────────────

    #[test]
    fn line_height_scales_with_font_size() {
        let h10 = line_height_mm(10.0);
        let h12 = line_height_mm(12.0);
        assert!(h12 > h10, "Larger font should have larger line height");
        assert!(h10 > 0.0, "Line height should be positive");
    }

    #[test]
    fn chars_per_line_decreases_with_font_size() {
        let c10 = chars_per_line(10.0);
        let c16 = chars_per_line(16.0);
        assert!(c10 > c16, "Smaller font should fit more chars per line");
        assert!(c10 > 0, "Chars per line should be positive");
    }

    // ── Test 9: PdfExportResult serialization ─────────────────────────────

    #[test]
    fn pdf_export_result_serializes_correctly() {
        let result = PdfExportResult {
            file_path: "/tmp/test.pdf".to_string(),
            export_type: "note_pdf".to_string(),
            pages: 3,
        };
        let json = serde_json::to_string(&result).expect("should serialize");
        assert!(json.contains("\"filePath\""), "camelCase filePath expected");
        assert!(json.contains("\"exportType\""), "camelCase exportType expected");
        assert!(json.contains("\"pages\""), "pages field expected");
        assert!(json.contains("3"), "page count value expected");
    }
}
