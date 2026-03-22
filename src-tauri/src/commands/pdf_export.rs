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
use base64::Engine as _;
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

/// Practice settings loaded from app_settings for letterhead rendering.
#[derive(Debug, Clone)]
pub struct PracticeSettings {
    pub practice_name: String,
    pub practice_address: String,
    pub practice_phone: String,
    pub practice_fax: String,
    pub practice_npi: String,
    /// Base64 data-URL for the practice logo (e.g. "data:image/png;base64,…").
    pub practice_logo_base64: Option<String>,
    /// Logo display width in pixels (default 200).
    pub logo_width_px: u32,
}

/// Patient demographic info extracted from the database for PDF headers.
#[derive(Debug, Clone)]
struct PatientInfo {
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

/// Render the practice logo inline on a given page/layer (for use outside PdfBuilder).
/// Returns the rendered height in mm, or 0.0 if no logo is available.
fn render_logo_inline(
    doc: &PdfDocumentReference,
    page: PdfPageIndex,
    layer_idx: PdfLayerIndex,
    x_mm: f32,
    y_mm: f32,
    settings: &PracticeSettings,
) -> f32 {
    let logo_b64 = match &settings.practice_logo_base64 {
        Some(s) if !s.is_empty() => s,
        _ => return 0.0,
    };

    let raw_b64 = if let Some(idx) = logo_b64.find(',') {
        &logo_b64[idx + 1..]
    } else {
        logo_b64.as_str()
    };

    let bytes = match base64::engine::general_purpose::STANDARD.decode(raw_b64) {
        Ok(b) => b,
        Err(_) => return 0.0,
    };

    let dyn_image = match image_crate::load_from_memory(&bytes) {
        Ok(img) => img,
        Err(_) => return 0.0,
    };

    let img_w_px = dyn_image.width() as f32;
    let img_h_px = dyn_image.height() as f32;
    if img_w_px == 0.0 || img_h_px == 0.0 {
        return 0.0;
    }

    let target_w_mm = settings.logo_width_px as f32 * 25.4 / 96.0;
    let aspect = img_h_px / img_w_px;
    let target_h_mm = target_w_mm * aspect;

    let dpi = 72.0_f32;
    let target_w_pt = target_w_mm / 25.4 * 72.0;
    let target_h_pt = target_h_mm / 25.4 * 72.0;
    let scale_x = target_w_pt / img_w_px;
    let scale_y = target_h_pt / img_h_px;

    let pdf_image = Image::from_dynamic_image(&dyn_image);
    let layer = doc.get_page(page).get_layer(layer_idx);

    let transform = ImageTransform {
        translate_x: Some(Mm(x_mm)),
        translate_y: Some(Mm(y_mm - target_h_mm)),
        scale_x: Some(scale_x),
        scale_y: Some(scale_y),
        dpi: Some(dpi),
        ..Default::default()
    };

    pdf_image.add_to_layer(layer, transform);
    target_h_mm
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

    /// Add the practice letterhead at the current position, optionally with logo.
    fn add_letterhead(&mut self, settings: &PracticeSettings) {
        // Try to embed the logo image if provided
        let logo_height_mm = self.try_add_logo(settings);
        let text_x = if logo_height_mm > 0.0 {
            // Place text to the right of the logo; logo_width_mm + gap
            let logo_width_mm = Self::px_to_mm(settings.logo_width_px as f32);
            MARGIN_LEFT_MM + logo_width_mm + 4.0
        } else {
            MARGIN_LEFT_MM
        };

        let lines = build_letterhead_lines(settings);
        let text_start_y = self.y_pos;

        // Practice name in title font
        if let Some(first) = lines.first() {
            let layer = self.layer();
            layer.use_text(first, FONT_SIZE_TITLE, Mm(text_x), Mm(self.y_pos), &self.font_bold);
            self.y_pos -= line_height_mm(FONT_SIZE_TITLE);
        }
        // Remaining lines in small font
        for line in lines.iter().skip(1) {
            let layer = self.layer();
            layer.use_text(line, FONT_SIZE_SMALL, Mm(text_x), Mm(self.y_pos), &self.font_regular);
            self.y_pos -= line_height_mm(FONT_SIZE_SMALL);
        }

        // If the logo is taller than the text, move y_pos down to below the logo
        let text_used = text_start_y - self.y_pos;
        if logo_height_mm > text_used {
            self.y_pos = text_start_y - logo_height_mm;
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

    /// Convert pixels to millimeters at 96 DPI (standard screen resolution).
    fn px_to_mm(px: f32) -> f32 {
        px * 25.4 / 96.0
    }

    /// Try to decode and embed the practice logo. Returns the rendered height in mm,
    /// or 0.0 if no logo is available or decoding fails.
    fn try_add_logo(&self, settings: &PracticeSettings) -> f32 {
        let logo_b64 = match &settings.practice_logo_base64 {
            Some(s) if !s.is_empty() => s,
            _ => return 0.0,
        };

        // Strip data-URL prefix (e.g. "data:image/png;base64,")
        let raw_b64 = if let Some(idx) = logo_b64.find(",") {
            &logo_b64[idx + 1..]
        } else {
            logo_b64.as_str()
        };

        // Decode base64 to bytes
        let bytes = match base64::engine::general_purpose::STANDARD.decode(raw_b64) {
            Ok(b) => b,
            Err(_) => return 0.0,
        };

        // Load image using the image crate (re-exported by printpdf)
        let dyn_image = match image_crate::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(_) => return 0.0,
        };

        let img_w_px = dyn_image.width() as f32;
        let img_h_px = dyn_image.height() as f32;
        if img_w_px == 0.0 || img_h_px == 0.0 {
            return 0.0;
        }

        // Target width in mm from the user's pixel setting
        let target_w_mm = Self::px_to_mm(settings.logo_width_px as f32);
        // Maintain aspect ratio
        let aspect = img_h_px / img_w_px;
        let target_h_mm = target_w_mm * aspect;

        // printpdf Image placement:
        // At dpi D, the image natural size is (img_w_px / D * 72) pt wide.
        // We want the image to be target_w_mm wide.
        // target_w_pt = target_w_mm / 25.4 * 72
        // natural_w_pt = img_w_px / dpi * 72
        // scale_x = target_w_pt / natural_w_pt = target_w_mm / 25.4 * dpi / img_w_px
        let dpi = 72.0_f32; // Use 72 DPI so 1px = 1pt, simplifying scale calc
        let target_w_pt = target_w_mm / 25.4 * 72.0;
        let target_h_pt = target_h_mm / 25.4 * 72.0;
        let natural_w_pt = img_w_px; // at 72 DPI, 1px = 1pt
        let natural_h_pt = img_h_px;
        let scale_x = target_w_pt / natural_w_pt;
        let scale_y = target_h_pt / natural_h_pt;

        let pdf_image = Image::from_dynamic_image(&dyn_image);
        let layer = self.layer();

        let transform = ImageTransform {
            translate_x: Some(Mm(MARGIN_LEFT_MM)),
            translate_y: Some(Mm(self.y_pos - target_h_mm)),
            scale_x: Some(scale_x),
            scale_y: Some(scale_y),
            dpi: Some(dpi),
            ..Default::default()
        };

        pdf_image.add_to_layer(layer, transform);

        target_h_mm
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
///
/// First checks the `export_settings` JSON blob (written by the Settings UI),
/// then falls back to individual `practice_*` keys (written by reminders setup).
fn load_practice_settings(conn: &rusqlite::Connection) -> PracticeSettings {
    // Try to load from the export_settings JSON blob first (set via Settings > Export tab)
    let export_settings: Option<ExportSettings> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'export_settings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|json_str| serde_json::from_str(&json_str).ok());

    // Fallback: read individual keys (legacy / reminders module)
    let get_setting = |key: &str, default: &str| -> String {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| default.to_string())
    };

    if let Some(es) = export_settings {
        PracticeSettings {
            practice_name: es
                .practice_name
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| get_setting("practice_name", "Physical Therapy Practice")),
            practice_address: es
                .practice_address
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| get_setting("practice_address", "")),
            practice_phone: es
                .practice_phone
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| get_setting("practice_phone", "")),
            practice_fax: get_setting("practice_fax", ""),
            practice_npi: get_setting("practice_npi", ""),
            practice_logo_base64: es.practice_logo_base64.filter(|s| !s.is_empty()),
            logo_width_px: es.logo_width_px.unwrap_or(200),
        }
    } else {
        PracticeSettings {
            practice_name: get_setting("practice_name", "Physical Therapy Practice"),
            practice_address: get_setting("practice_address", ""),
            practice_phone: get_setting("practice_phone", ""),
            practice_fax: get_setting("practice_fax", ""),
            practice_npi: get_setting("practice_npi", ""),
            practice_logo_base64: None,
            logo_width_px: 200,
        }
    }
}

/// Load patient info from the patient_index table.
fn load_patient_info(
    conn: &rusqlite::Connection,
    patient_id: &str,
) -> Result<PatientInfo, AppError> {
    conn.query_row(
        "SELECT family_name, COALESCE(given_name, ''), COALESCE(birth_date, ''), mrn
         FROM patient_index WHERE patient_id = ?1",
        rusqlite::params![patient_id],
        |row| {
            Ok(PatientInfo {
                family_name: row.get(0)?,
                given_name: row.get(1)?,
                birth_date: row.get(2)?,
                mrn: row.get(3)?,
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

/// Get the effective provider name for signature lines.
///
/// Prefers `provider_name_credentials` from export settings (e.g. "Omar Safwat Sharaf, PT, DPT"),
/// falling back to the provider's `display_name` from the users table.
fn effective_signature_name(conn: &rusqlite::Connection, provider: &ProviderInfo) -> String {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = 'export_settings'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|json_str| serde_json::from_str::<ExportSettings>(&json_str).ok())
    .and_then(|es| es.provider_name_credentials)
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| provider.display_name.clone())
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

/// Extract encounter type label from FHIR resource.
/// Checks type[0].text, then type[0].coding[0].display, then type[0].coding[0].code,
/// then class.code, then falls back to the flat "encounter_type" key.
fn extract_encounter_type_label(resource: &serde_json::Value) -> String {
    // Check type[0].text
    if let Some(types) = resource.get("type").and_then(|v| v.as_array()) {
        if let Some(first_type) = types.first() {
            if let Some(text) = first_type.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    return text.to_string();
                }
            }
            // Check coding[0].display or coding[0].code
            if let Some(coding) = first_type.get("coding").and_then(|v| v.as_array()) {
                if let Some(first_coding) = coding.first() {
                    if let Some(display) = first_coding.get("display").and_then(|v| v.as_str()) {
                        if !display.is_empty() {
                            return display.to_string();
                        }
                    }
                    if let Some(code) = first_coding.get("code").and_then(|v| v.as_str()) {
                        if !code.is_empty() {
                            return code.replace('_', " ");
                        }
                    }
                }
            }
        }
    }
    // Fallback to class.code
    if let Some(cls) = resource.get("class") {
        if let Some(code) = cls.get("code").and_then(|v| v.as_str()) {
            if !code.is_empty() {
                return code.to_string();
            }
        }
    }
    // Legacy flat field fallback
    json_str(resource, "encounter_type")
}

/// Extract SOAP sections from the FHIR `note` array (Annotation format).
/// Each annotation has an extension with valueCode indicating the section
/// (subjective/objective/assessment/plan) and a `text` field with content.
/// Returns (subjective, objective, assessment, plan).
fn extract_soap_from_note_array(resource: &serde_json::Value) -> (String, String, String, String) {
    let mut subjective = String::new();
    let mut objective = String::new();
    let mut assessment = String::new();
    let mut plan = String::new();

    if let Some(notes) = resource.get("note").and_then(|v| v.as_array()) {
        for item in notes {
            let text = match item.get("text").and_then(|v| v.as_str()) {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };
            let extensions = match item.get("extension").and_then(|v| v.as_array()) {
                Some(e) => e,
                None => continue,
            };
            for ext in extensions {
                let url = ext.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if url == "http://medarc.local/fhir/StructureDefinition/note-section" {
                    let section_code = ext.get("valueCode").and_then(|v| v.as_str()).unwrap_or("");
                    match section_code {
                        "subjective" => subjective = text.to_string(),
                        "objective" => objective = text.to_string(),
                        "assessment" => assessment = text.to_string(),
                        "plan" => plan = text.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    (subjective, objective, assessment, plan)
}

/// Render a single encounter/note onto the PDF builder.
fn render_encounter(
    builder: &mut PdfBuilder,
    resource: &serde_json::Value,
    encounter_date: &str,
) {
    let encounter_type = extract_encounter_type_label(resource);
    let status = json_str(resource, "status");

    // Chief complaint from reasonCode[0].text (FHIR format)
    let chief_complaint = resource.get("reasonCode")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    builder.add_section_heading(&format!(
        "Encounter: {} — {} ({})",
        encounter_date, encounter_type, status
    ));

    if !chief_complaint.is_empty() {
        builder.add_field("Chief Complaint", &chief_complaint);
    }

    // Extract SOAP sections from the FHIR note array
    let (subjective, objective, assessment, plan) = extract_soap_from_note_array(resource);

    // Check if all content is in the subjective field (single-textarea mode)
    let has_only_subjective = !subjective.is_empty()
        && objective.is_empty()
        && assessment.is_empty()
        && plan.is_empty();

    if has_only_subjective {
        // Render as a single "Clinical Note" section
        builder.add_section_heading("Clinical Note");
        builder.add_text_block(&subjective);
    } else {
        // Render as separate SOAP sections
        if !subjective.is_empty() {
            builder.add_section_heading("Subjective");
            builder.add_text_block(&subjective);
        }
        if !objective.is_empty() {
            builder.add_section_heading("Objective");
            builder.add_text_block(&objective);
        }
        if !assessment.is_empty() {
            builder.add_section_heading("Assessment");
            builder.add_text_block(&assessment);
        }
        if !plan.is_empty() {
            builder.add_section_heading("Plan");
            builder.add_text_block(&plan);
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

    let file_path = temp_pdf_path("note")?;
    let page_count = builder.page_count;
    builder.save(&file_path)?;

    // Log the export
    log_export(&conn, &patient_id, "note_pdf", &file_path, &session.user_id)?;

    let _ = write_audit_entry(
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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

    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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
    builder.add_signature_line(&effective_signature_name(&conn, &provider));

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

    let _ = write_audit_entry(
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

/// Schedule print settings stored as JSON in app_settings under key "schedule_print_settings".
/// Configures how schedule PDFs are rendered.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePrintSettings {
    /// Whether to include calendar events (non-appointment items) in the printout.
    pub include_calendar_events: Option<bool>,
    /// Whether to include cancelled appointments in the printout.
    pub include_cancelled: Option<bool>,
    /// Date display format: "MM/DD/YYYY", "DD/MM/YYYY", or "YYYY-MM-DD".
    pub date_format: Option<String>,
    /// Whether to show patient date of birth in the patient column.
    pub show_patient_dob: Option<bool>,
    /// Whether to show the appointment type column.
    pub show_appointment_type: Option<bool>,
    /// Whether to show the appointment status column.
    pub show_appointment_status: Option<bool>,
    /// Clinic name override for letterhead.
    pub clinic_name: Option<String>,
    /// Clinic address override for letterhead.
    pub clinic_address: Option<String>,
    /// Clinic phone override for letterhead.
    pub clinic_phone: Option<String>,
    /// Whether to include the clinic logo in the letterhead.
    pub include_clinic_logo: Option<bool>,
    /// Document format: "letter" (8.5x11) or "a4".
    pub document_format: Option<String>,
    /// Page orientation: "portrait" or "landscape".
    pub orientation: Option<String>,
    /// Whether to show the provider name in the header.
    pub show_provider_name: Option<bool>,
}

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
    /// Logo display width in pixels (50–500, default 200).
    pub logo_width_px: Option<u32>,
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
            logo_width_px: None,
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
// Schedule PDF Generation
// ─────────────────────────────────────────────────────────────────────────────

/// Format a date string according to the configured date format.
///
/// Supports:
///   - "MM/DD/YYYY" → e.g. "04/15/2026"
///   - "DD/MM/YYYY" → e.g. "15/04/2026"
///   - "YYYY-MM-DD" → e.g. "2026-04-15" (pass-through)
///
/// Input should be "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM:SS".
fn format_date_display(date_str: &str, format: &str) -> String {
    let date_part = date_str.split('T').next().unwrap_or(date_str);
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() < 3 {
        return date_str.to_string();
    }
    match format {
        "MM/DD/YYYY" => format!("{}/{}/{}", parts[1], parts[2], parts[0]),
        "DD/MM/YYYY" => format!("{}/{}/{}", parts[2], parts[1], parts[0]),
        _ => date_part.to_string(), // YYYY-MM-DD default
    }
}

/// Generate a schedule PDF for a date range.
///
/// Renders a table of appointments for the specified date range with
/// configurable columns and letterhead. Reads `schedule_print_settings`
/// from `app_settings` to control output.
///
/// RBAC: Provider, SystemAdmin, FrontDesk, NurseMa — AppointmentScheduling::Read + PdfExport::Create.
#[tauri::command]
pub fn generate_schedule_pdf(
    start_date: String,
    end_date: String,
    patient_id: Option<String>,
    provider_id: Option<String>,
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

    // Load schedule print settings from app_settings
    let print_settings: Option<SchedulePrintSettings> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'schedule_print_settings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok());

    let include_cancelled = print_settings
        .as_ref()
        .and_then(|s| s.include_cancelled)
        .unwrap_or(false);
    let date_format = print_settings
        .as_ref()
        .and_then(|s| s.date_format.clone())
        .unwrap_or_else(|| "MM/DD/YYYY".to_string());
    let show_patient_dob = print_settings
        .as_ref()
        .and_then(|s| s.show_patient_dob)
        .unwrap_or(false);
    let show_appointment_type = print_settings
        .as_ref()
        .and_then(|s| s.show_appointment_type)
        .unwrap_or(true);
    let show_appointment_status = print_settings
        .as_ref()
        .and_then(|s| s.show_appointment_status)
        .unwrap_or(true);
    let show_provider_name = print_settings
        .as_ref()
        .and_then(|s| s.show_provider_name)
        .unwrap_or(true);
    let include_clinic_logo = print_settings
        .as_ref()
        .and_then(|s| s.include_clinic_logo)
        .unwrap_or(true);
    let document_format = print_settings
        .as_ref()
        .and_then(|s| s.document_format.clone())
        .unwrap_or_else(|| "letter".to_string());
    let orientation = print_settings
        .as_ref()
        .and_then(|s| s.orientation.clone())
        .unwrap_or_else(|| "portrait".to_string());
    let _include_calendar_events = print_settings
        .as_ref()
        .and_then(|s| s.include_calendar_events)
        .unwrap_or(true);

    // Determine page dimensions based on format and orientation
    let (page_w, page_h) = match (document_format.as_str(), orientation.as_str()) {
        ("a4", "portrait") => (210.0_f32, 297.0_f32),
        ("a4", "landscape") => (297.0_f32, 210.0_f32),
        ("letter", "landscape") => (PAGE_HEIGHT_MM, PAGE_WIDTH_MM),
        _ => (PAGE_WIDTH_MM, PAGE_HEIGHT_MM), // letter portrait default
    };

    // Build practice settings for letterhead — prefer print-settings overrides
    let practice_settings = load_practice_settings(&conn);
    let letterhead_settings = PracticeSettings {
        practice_name: print_settings
            .as_ref()
            .and_then(|s| s.clinic_name.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or(practice_settings.practice_name),
        practice_address: print_settings
            .as_ref()
            .and_then(|s| s.clinic_address.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or(practice_settings.practice_address),
        practice_phone: print_settings
            .as_ref()
            .and_then(|s| s.clinic_phone.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or(practice_settings.practice_phone),
        practice_fax: practice_settings.practice_fax,
        practice_npi: practice_settings.practice_npi,
        practice_logo_base64: practice_settings.practice_logo_base64.clone(),
        logo_width_px: practice_settings.logo_width_px,
    };

    // Query appointments
    let mut query = String::from(
        "SELECT ai.appointment_id, ai.patient_id, ai.provider_id,
                ai.start_time, ai.status, ai.appt_type,
                fr.resource
         FROM appointment_index ai
         JOIN fhir_resources fr ON fr.id = ai.appointment_id
         WHERE ai.start_time >= ?1 AND ai.start_time < ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(start_date.clone()),
        Box::new(end_date.clone()),
    ];

    if !include_cancelled {
        query.push_str(" AND ai.status != 'cancelled'");
    }

    if let Some(ref pat) = patient_id {
        query.push_str(&format!(" AND ai.patient_id = ?{}", params.len() + 1));
        params.push(Box::new(pat.clone()));
    }

    if let Some(ref prov) = provider_id {
        query.push_str(&format!(" AND ai.provider_id = ?{}", params.len() + 1));
        params.push(Box::new(prov.clone()));
    }

    query.push_str(" ORDER BY ai.start_time ASC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();

    struct ScheduleRow {
        start_time: String,
        patient_id: String,
        provider_id: String,
        status: String,
        appt_type: String,
    }

    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows: Vec<ScheduleRow> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(ScheduleRow {
                start_time: row.get(3)?,
                patient_id: row.get(1)?,
                provider_id: row.get(2)?,
                status: row.get(4)?,
                appt_type: row.get(5)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    // Resolve patient names and DOBs
    let mut patient_cache: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for row in &rows {
        if !patient_cache.contains_key(&row.patient_id) {
            let info = conn
                .query_row(
                    "SELECT COALESCE(given_name, ''), family_name, COALESCE(birth_date, '')
                     FROM patient_index WHERE patient_id = ?1",
                    rusqlite::params![&row.patient_id],
                    |r| {
                        let given: String = r.get(0)?;
                        let family: String = r.get(1)?;
                        let dob: String = r.get(2)?;
                        Ok((format!("{} {}", given, family).trim().to_string(), dob))
                    },
                )
                .unwrap_or_else(|_| (row.patient_id.clone(), String::new()));
            patient_cache.insert(row.patient_id.clone(), info);
        }
    }

    // Resolve provider names
    let mut provider_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for row in &rows {
        if !provider_cache.contains_key(&row.provider_id) {
            let name = conn
                .query_row(
                    "SELECT display_name FROM users WHERE id = ?1",
                    rusqlite::params![&row.provider_id],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_else(|_| row.provider_id.clone());
            provider_cache.insert(row.provider_id.clone(), name);
        }
    }

    // Build the PDF using custom dimensions
    let (doc, page, layer) = PdfDocument::new(
        &format!("Schedule {} to {}", start_date, end_date),
        Mm(page_w),
        Mm(page_h),
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

    let margin_left = MARGIN_LEFT_MM;
    let margin_right = MARGIN_RIGHT_MM;
    let content_width = page_w - margin_left - margin_right;
    let mut current_page = page;
    let mut current_layer = layer;
    let mut y_pos = page_h - MARGIN_TOP_MM;
    let mut page_count: u32 = 1;

    // Helper closures
    let get_layer = |doc: &PdfDocumentReference, pg: PdfPageIndex, ly: PdfLayerIndex| {
        doc.get_page(pg).get_layer(ly)
    };

    // Render letterhead (if logo setting is not disabled)
    if include_clinic_logo {
        // Try to embed logo image
        let logo_height_mm = render_logo_inline(
            &doc, current_page, current_layer, margin_left, y_pos, &letterhead_settings
        );
        let text_x = if logo_height_mm > 0.0 {
            let logo_w_mm = PdfBuilder::px_to_mm(letterhead_settings.logo_width_px as f32);
            margin_left + logo_w_mm + 4.0
        } else {
            margin_left
        };
        let text_start_y = y_pos;

        let lh_lines = build_letterhead_lines(&letterhead_settings);
        if let Some(first) = lh_lines.first() {
            let lyr = get_layer(&doc, current_page, current_layer);
            lyr.use_text(first, FONT_SIZE_TITLE, Mm(text_x), Mm(y_pos), &font_bold);
            y_pos -= line_height_mm(FONT_SIZE_TITLE);
        }
        for line in lh_lines.iter().skip(1) {
            let lyr = get_layer(&doc, current_page, current_layer);
            lyr.use_text(line, FONT_SIZE_SMALL, Mm(text_x), Mm(y_pos), &font_regular);
            y_pos -= line_height_mm(FONT_SIZE_SMALL);
        }
        // Ensure y_pos is below the logo if the logo is taller than text
        let text_used = text_start_y - y_pos;
        if logo_height_mm > text_used {
            y_pos = text_start_y - logo_height_mm;
        }
        // Separator line
        y_pos -= 2.0;
        let lyr = get_layer(&doc, current_page, current_layer);
        let line = Line {
            points: vec![
                (Point::new(Mm(margin_left), Mm(y_pos)), false),
                (Point::new(Mm(page_w - margin_right), Mm(y_pos)), false),
            ],
            is_closed: false,
        };
        lyr.add_line(line);
        y_pos -= 4.0;
    }

    // Provider name in header (if configured)
    if show_provider_name {
        if let Some(ref prov_id) = provider_id {
            if let Some(prov_name) = provider_cache.get(prov_id) {
                let lyr = get_layer(&doc, current_page, current_layer);
                lyr.use_text(
                    &format!("Provider: {}", prov_name),
                    FONT_SIZE_BODY,
                    Mm(margin_left),
                    Mm(y_pos),
                    &font_bold,
                );
                y_pos -= line_height_mm(FONT_SIZE_BODY);
            }
        }
    }

    // Schedule title
    let formatted_start = format_date_display(&start_date, &date_format);
    let formatted_end = format_date_display(&end_date, &date_format);
    {
        let lyr = get_layer(&doc, current_page, current_layer);
        lyr.use_text(
            &format!("Schedule: {} - {}", formatted_start, formatted_end),
            FONT_SIZE_HEADING,
            Mm(margin_left),
            Mm(y_pos),
            &font_bold,
        );
        y_pos -= line_height_mm(FONT_SIZE_HEADING) + 2.0;
    }

    // Table header
    {
        let lyr = get_layer(&doc, current_page, current_layer);
        let mut x = margin_left;
        let col_time = 30.0;
        let col_patient = if show_patient_dob { 55.0 } else { 50.0 };
        let col_type = if show_appointment_type { 35.0 } else { 0.0 };
        let col_status = if show_appointment_status { 25.0 } else { 0.0 };
        let col_provider = if show_provider_name && provider_id.is_none() { 40.0 } else { 0.0 };

        lyr.use_text("Time", FONT_SIZE_SMALL, Mm(x), Mm(y_pos), &font_bold);
        x += col_time;
        lyr.use_text("Patient", FONT_SIZE_SMALL, Mm(x), Mm(y_pos), &font_bold);
        x += col_patient;
        if show_appointment_type {
            lyr.use_text("Type", FONT_SIZE_SMALL, Mm(x), Mm(y_pos), &font_bold);
            x += col_type;
        }
        if show_appointment_status {
            lyr.use_text("Status", FONT_SIZE_SMALL, Mm(x), Mm(y_pos), &font_bold);
            x += col_status;
        }
        if col_provider > 0.0 {
            lyr.use_text("Provider", FONT_SIZE_SMALL, Mm(x), Mm(y_pos), &font_bold);
        }
        let _ = content_width; // used for layout reference
        y_pos -= line_height_mm(FONT_SIZE_SMALL);

        // Header underline
        let line = Line {
            points: vec![
                (Point::new(Mm(margin_left), Mm(y_pos)), false),
                (Point::new(Mm(page_w - margin_right), Mm(y_pos)), false),
            ],
            is_closed: false,
        };
        lyr.add_line(line);
        y_pos -= 2.0;
    }

    // Table rows
    for row in &rows {
        // Check if we need a new page
        if y_pos - line_height_mm(FONT_SIZE_BODY) < MARGIN_BOTTOM_MM {
            let (pg, ly) = doc.add_page(Mm(page_w), Mm(page_h), "Layer 1");
            current_page = pg;
            current_layer = ly;
            y_pos = page_h - MARGIN_TOP_MM;
            page_count += 1;
        }

        let lyr = get_layer(&doc, current_page, current_layer);
        let mut x = margin_left;
        let col_time = 30.0;
        let col_patient = if show_patient_dob { 55.0 } else { 50.0 };
        let col_type = if show_appointment_type { 35.0 } else { 0.0 };
        let col_status = if show_appointment_status { 25.0 } else { 0.0 };
        let col_provider = if show_provider_name && provider_id.is_none() { 40.0 } else { 0.0 };

        // Time column — extract time portion and format
        let time_display = {
            let time_part = row.start_time.split('T').nth(1).unwrap_or("00:00");
            let parts: Vec<&str> = time_part.split(':').collect();
            if parts.len() >= 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    let suffix = if h >= 12 { "PM" } else { "AM" };
                    let display_h = if h % 12 == 0 { 12 } else { h % 12 };
                    format!("{}:{:02} {}", display_h, m, suffix)
                } else {
                    time_part.to_string()
                }
            } else {
                time_part.to_string()
            }
        };
        lyr.use_text(&time_display, FONT_SIZE_BODY, Mm(x), Mm(y_pos), &font_regular);
        x += col_time;

        // Patient column
        let (patient_name, patient_dob) = patient_cache
            .get(&row.patient_id)
            .cloned()
            .unwrap_or_else(|| (row.patient_id.clone(), String::new()));
        let patient_text = if show_patient_dob && !patient_dob.is_empty() {
            let formatted_dob = format_date_display(&patient_dob, &date_format);
            format!("{} (DOB: {})", patient_name, formatted_dob)
        } else {
            patient_name
        };
        lyr.use_text(&patient_text, FONT_SIZE_BODY, Mm(x), Mm(y_pos), &font_regular);
        x += col_patient;

        // Type column
        if show_appointment_type {
            let type_display = row.appt_type.replace('_', " ");
            lyr.use_text(&type_display, FONT_SIZE_BODY, Mm(x), Mm(y_pos), &font_regular);
            x += col_type;
        }

        // Status column
        if show_appointment_status {
            lyr.use_text(&row.status, FONT_SIZE_BODY, Mm(x), Mm(y_pos), &font_regular);
            x += col_status;
        }

        // Provider column (only when not filtering by single provider)
        if col_provider > 0.0 {
            let prov_name = provider_cache
                .get(&row.provider_id)
                .cloned()
                .unwrap_or_else(|| row.provider_id.clone());
            lyr.use_text(&prov_name, FONT_SIZE_BODY, Mm(x), Mm(y_pos), &font_regular);
        }

        y_pos -= line_height_mm(FONT_SIZE_BODY);
    }

    // Footer — appointment count
    y_pos -= 4.0;
    if y_pos - line_height_mm(FONT_SIZE_SMALL) < MARGIN_BOTTOM_MM {
        let (pg, ly) = doc.add_page(Mm(page_w), Mm(page_h), "Layer 1");
        current_page = pg;
        current_layer = ly;
        y_pos = page_h - MARGIN_TOP_MM;
        page_count += 1;
    }
    {
        let lyr = get_layer(&doc, current_page, current_layer);
        lyr.use_text(
            &format!("Total appointments: {}", rows.len()),
            FONT_SIZE_SMALL,
            Mm(margin_left),
            Mm(y_pos),
            &font_bold,
        );
    }

    // Save the document
    let file_path = temp_pdf_path("schedule")?;
    let file = std::fs::File::create(&file_path).map_err(AppError::Io)?;
    let mut writer = std::io::BufWriter::new(file);
    doc.save(&mut writer)
        .map_err(|e| AppError::Validation(format!("Failed to save PDF: {}", e)))?;

    let _ = write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: session.user_id.clone(),
            action: "pdf_export.generate_schedule_pdf".to_string(),
            resource_type: "PdfExport".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some(format!(
                "generated schedule PDF: {} ({} appointments)",
                file_path,
                rows.len()
            )),
        },
    );

    Ok(PdfExportResult {
        file_path,
        export_type: "schedule_pdf".to_string(),
        pages: page_count,
    })
}

/// Retrieve schedule print settings from app_settings.
///
/// Returns the stored SchedulePrintSettings or defaults if not yet configured.
#[tauri::command]
pub async fn get_schedule_print_settings(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
) -> Result<SchedulePrintSettings, AppError> {
    let _sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(_sess.role, Resource::PdfExport, Action::Read)?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let json_str: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'schedule_print_settings'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "{}".to_string());

    let settings: SchedulePrintSettings =
        serde_json::from_str(&json_str).unwrap_or_else(|_| SchedulePrintSettings {
            include_calendar_events: Some(true),
            include_cancelled: Some(false),
            date_format: Some("MM/DD/YYYY".to_string()),
            show_patient_dob: Some(false),
            show_appointment_type: Some(true),
            show_appointment_status: Some(true),
            clinic_name: None,
            clinic_address: None,
            clinic_phone: None,
            include_clinic_logo: Some(true),
            document_format: Some("letter".to_string()),
            orientation: Some("portrait".to_string()),
            show_provider_name: Some(true),
        });

    Ok(settings)
}

/// Save schedule print settings to app_settings.
///
/// Overwrites any existing schedule_print_settings value.
#[tauri::command]
pub async fn save_schedule_print_settings(
    settings: SchedulePrintSettings,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<SchedulePrintSettings, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::PdfExport, Action::Update)?;

    let json_str = serde_json::to_string(&settings)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db
        .conn
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('schedule_print_settings', ?1, datetime('now'))",
        rusqlite::params![json_str],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "pdf_export.schedule_print_settings.update".to_string(),
            resource_type: "AppSettings".to_string(),
            resource_id: Some("schedule_print_settings".to_string()),
            patient_id: None,
            device_id: device_id.get().to_string(),
            success: true,
            details: Some("Schedule print settings updated".to_string()),
        },
    )?;

    Ok(settings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Open file in default application
// ─────────────────────────────────────────────────────────────────────────────

/// Open a file in the system's default application (Preview for PDF, etc.).
#[tauri::command]
pub fn open_file_in_default_app(file_path: String) -> Result<(), AppError> {
    use std::process::Command;

    if !std::path::Path::new(&file_path).exists() {
        return Err(AppError::NotFound(format!(
            "File not found: {}",
            file_path
        )));
    }

    Command::new("open")
        .arg(&file_path)
        .spawn()
        .map_err(|e| AppError::Io(e))?;

    Ok(())
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
            practice_logo_base64: None,
            logo_width_px: 200,
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
            practice_logo_base64: None,
            logo_width_px: 200,
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

    // ── Test 3: Word wrapping ─────────────────────────────────────────────

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

    // ── Test 10: format_date_display ─────────────────────────────────────

    #[test]
    fn format_date_display_mm_dd_yyyy() {
        assert_eq!(
            format_date_display("2026-04-15", "MM/DD/YYYY"),
            "04/15/2026"
        );
    }

    #[test]
    fn format_date_display_dd_mm_yyyy() {
        assert_eq!(
            format_date_display("2026-04-15", "DD/MM/YYYY"),
            "15/04/2026"
        );
    }

    #[test]
    fn format_date_display_iso_default() {
        assert_eq!(
            format_date_display("2026-04-15", "YYYY-MM-DD"),
            "2026-04-15"
        );
    }

    #[test]
    fn format_date_display_strips_time_part() {
        assert_eq!(
            format_date_display("2026-04-15T09:30:00", "MM/DD/YYYY"),
            "04/15/2026"
        );
    }

    // ── Test 11: SchedulePrintSettings serialization ─────────────────────

    #[test]
    fn schedule_print_settings_serializes_camelcase() {
        let settings = SchedulePrintSettings {
            include_calendar_events: Some(true),
            include_cancelled: Some(false),
            date_format: Some("MM/DD/YYYY".to_string()),
            show_patient_dob: Some(true),
            show_appointment_type: Some(true),
            show_appointment_status: Some(true),
            clinic_name: Some("Test Clinic".to_string()),
            clinic_address: None,
            clinic_phone: None,
            include_clinic_logo: Some(true),
            document_format: Some("letter".to_string()),
            orientation: Some("portrait".to_string()),
            show_provider_name: Some(true),
        };
        let json = serde_json::to_string(&settings).expect("should serialize");
        assert!(json.contains("\"includeCalendarEvents\""), "camelCase includeCalendarEvents expected");
        assert!(json.contains("\"includeCancelled\""), "camelCase includeCancelled expected");
        assert!(json.contains("\"dateFormat\""), "camelCase dateFormat expected");
        assert!(json.contains("\"showPatientDob\""), "camelCase showPatientDob expected");
        assert!(json.contains("\"clinicName\""), "camelCase clinicName expected");
        assert!(json.contains("\"documentFormat\""), "camelCase documentFormat expected");
        assert!(json.contains("\"orientation\""), "camelCase orientation expected");
    }

    #[test]
    fn schedule_print_settings_deserializes_from_empty_json() {
        let settings: SchedulePrintSettings = serde_json::from_str("{}").expect("should parse {}");
        assert_eq!(settings.include_cancelled, None);
        assert_eq!(settings.date_format, None);
        assert_eq!(settings.clinic_name, None);
    }
}
