/**
 * TypeScript types for Phaxio fax integration.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Enums / union types
// ─────────────────────────────────────────────────────────────────────────────

/** Direction of a fax: outbound ("sent") or inbound ("received"). */
export type FaxDirection = "sent" | "received";

/** Lifecycle status of a fax. */
export type FaxStatus = "queued" | "in_progress" | "success" | "failed";

/** Category for fax contacts. */
export type FaxContactType = "insurance" | "referring_md" | "attorney" | "other";

// ─────────────────────────────────────────────────────────────────────────────
// Phaxio configuration
// ─────────────────────────────────────────────────────────────────────────────

/** Input for configuring Phaxio API credentials. */
export interface PhaxioConfigInput {
  /** Phaxio API key. */
  apiKey: string;
  /** Phaxio API secret. */
  apiSecret: string;
  /** Sender fax number (E.164 format, e.g. "+15551234567"). */
  faxNumber: string;
}

/** Phaxio configuration status (secrets are never exposed to the frontend). */
export interface PhaxioConfigRecord {
  /** Whether Phaxio credentials are configured. */
  configured: boolean;
  /** The configured fax number (if any). */
  faxNumber: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fax records
// ─────────────────────────────────────────────────────────────────────────────

/** A fax log record. */
export interface FaxRecord {
  faxId: string;
  phaxioFaxId: string | null;
  direction: FaxDirection;
  patientId: string | null;
  recipientName: string | null;
  recipientFax: string | null;
  documentName: string | null;
  filePath: string | null;
  status: FaxStatus;
  sentAt: string;
  deliveredAt: string | null;
  pages: number | null;
  errorMessage: string | null;
  retryCount: number;
}

/** Input for sending a fax. */
export interface SendFaxInput {
  /** Path to the file to fax. */
  filePath: string;
  /** Recipient fax number (E.164 format). */
  recipientFax: string;
  /** Recipient name for logging. */
  recipientName: string;
  /** Optional patient ID to associate with the fax. */
  patientId: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fax contacts
// ─────────────────────────────────────────────────────────────────────────────

/** A fax contact directory entry. */
export interface FaxContact {
  contactId: string;
  name: string;
  organization: string | null;
  faxNumber: string;
  phoneNumber: string | null;
  contactType: FaxContactType;
  notes: string | null;
  createdAt: string;
}

/** Input for creating or updating a fax contact. */
export interface FaxContactInput {
  name: string;
  organization: string | null;
  faxNumber: string;
  phoneNumber: string | null;
  /** One of: "insurance", "referring_md", "attorney", "other". */
  contactType: FaxContactType;
  notes: string | null;
}
