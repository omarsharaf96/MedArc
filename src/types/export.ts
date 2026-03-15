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
