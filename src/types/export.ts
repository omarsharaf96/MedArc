/**
 * TypeScript types for PDF Export & Report Generation (M003/S05).
 */

/** Result of a PDF generation command. */
export interface PdfExportResult {
  exportId: string;
  filePath: string;
  fileSizeBytes: number;
  completedAt: string;
  pageCount: number;
}

/** Export log entry for tracking generated PDFs. */
export interface ExportLogEntry {
  exportId: string;
  patientId: string;
  exportType: string;
  filePath: string;
  generatedAt: string;
  generatedBy: string;
}

/** Export settings for letterhead and signature configuration. */
export interface ExportSettings {
  /** Practice name for letterhead. */
  practiceName: string | null;
  /** Practice address for letterhead. */
  practiceAddress: string | null;
  /** Practice phone number for letterhead. */
  practicePhone: string | null;
  /** Practice logo as base64-encoded image data. */
  practiceLogoBase64: string | null;
  /** Logo display width in pixels (50–500, default 200). */
  logoWidthPx: number | null;
  /** Provider signature image as base64-encoded image data. */
  signatureImageBase64: string | null;
  /** Provider name/credentials line (e.g. "Omar Safwat Sharaf, PT, DPT"). */
  providerNameCredentials: string | null;
  /** Provider license number. */
  licenseNumber: string | null;
}

/** Input for the fax-encounter-note workflow. */
export interface FaxEncounterNoteInput {
  /** The encounter ID whose note to fax. */
  encounterId: string;
  /** Recipient fax number (E.164 format). */
  recipientFax: string;
  /** Recipient name for the fax log. */
  recipientName: string;
  /** Optional patient ID (derived from encounter if not provided). */
  patientId: string | null;
}

/** Schedule print settings for configuring schedule PDF output. */
export interface SchedulePrintSettings {
  /** Whether to include calendar events in the printout. */
  includeCalendarEvents: boolean | null;
  /** Whether to include cancelled appointments. */
  includeCancelled: boolean | null;
  /** Date display format: "MM/DD/YYYY", "DD/MM/YYYY", or "YYYY-MM-DD". */
  dateFormat: string | null;
  /** Whether to show patient date of birth. */
  showPatientDob: boolean | null;
  /** Whether to show the appointment type column. */
  showAppointmentType: boolean | null;
  /** Whether to show the appointment status column. */
  showAppointmentStatus: boolean | null;
  /** Clinic name override for letterhead. */
  clinicName: string | null;
  /** Clinic address override for letterhead. */
  clinicAddress: string | null;
  /** Clinic phone override for letterhead. */
  clinicPhone: string | null;
  /** Whether to include the clinic logo in the letterhead. */
  includeClinicLogo: boolean | null;
  /** Document format: "letter" or "a4". */
  documentFormat: string | null;
  /** Page orientation: "portrait" or "landscape". */
  orientation: string | null;
  /** Whether to show the provider name in the header. */
  showProviderName: boolean | null;
}

/** Result of the fax-encounter-note workflow. */
export interface FaxEncounterNoteResult {
  /** Path to the generated PDF that was faxed. */
  pdfPath: string;
  /** The fax log entry ID. */
  faxId: string;
  /** Fax status after queueing. */
  status: string;
}
