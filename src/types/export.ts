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
