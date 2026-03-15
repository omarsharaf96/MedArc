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

/** Result of the fax-encounter-note workflow. */
export interface FaxEncounterNoteResult {
  /** Path to the generated PDF that was faxed. */
  pdfPath: string;
  /** The fax log entry ID. */
  faxId: string;
  /** Fax status after queueing. */
  status: string;
}
