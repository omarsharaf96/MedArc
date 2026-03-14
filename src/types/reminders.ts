/**
 * types/reminders.ts — Appointment Reminders & Waitlist Auto-Fill (M003/S02)
 *
 * Mirrors the Rust types in commands/reminders.rs.
 * All field names use camelCase (matching #[serde(rename_all = "camelCase")]).
 */

// ─── Configuration ────────────────────────────────────────────────────────────

export interface TwilioConfigInput {
  accountSid: string;
  authToken: string;
  fromNumber: string;
}

export interface SendGridConfigInput {
  apiKey: string;
  fromEmail: string;
  fromName?: string | null;
}

export interface ReminderConfigInput {
  smsEnabled: boolean;
  emailEnabled: boolean;
  reminder24hr: boolean;
  reminder2hr: boolean;
  practiceName?: string | null;
  practicePhone?: string | null;
  twilio?: TwilioConfigInput | null;
  sendgrid?: SendGridConfigInput | null;
}

/** Current reminder configuration — secrets are masked. */
export interface ReminderConfigRecord {
  smsEnabled: boolean;
  emailEnabled: boolean;
  reminder24hr: boolean;
  reminder2hr: boolean;
  practiceName: string | null;
  practicePhone: string | null;
  twilioConfigured: boolean;
  /** Masked from_number, e.g. "+1555***4567". null if not configured. */
  twilioFromNumber: string | null;
  sendgridConfigured: boolean;
  sendgridFromEmail: string | null;
}

// ─── Reminder Log ─────────────────────────────────────────────────────────────

export type ReminderType = "24hr" | "2hr" | "no_show" | "waitlist_offer" | "custom";
export type ReminderChannel = "sms" | "email";
export type ReminderStatus = "pending" | "sent" | "delivered" | "failed";

export interface ReminderLog {
  reminderId: string;
  appointmentId: string;
  patientId: string;
  reminderType: ReminderType;
  channel: ReminderChannel;
  recipient: string;
  messageBody: string;
  status: ReminderStatus;
  externalId: string | null;
  errorMessage: string | null;
  sentAt: string | null;
  createdAt: string;
}

// ─── Reminder Results ─────────────────────────────────────────────────────────

export interface ReminderResult {
  reminderId: string;
  status: ReminderStatus;
  channel: ReminderChannel;
  recipient: string;
  externalId: string | null;
  errorMessage: string | null;
}

export interface ProcessRemindersResult {
  sentCount: number;
  skippedCount: number;
  failedCount: number;
  results: ReminderResult[];
}

// ─── Waitlist Auto-Fill ───────────────────────────────────────────────────────

export interface WaitlistMatch {
  waitlistId: string;
  patientId: string;
  patientName: string;
  phone: string | null;
  email: string | null;
  offerSent: boolean;
  offerChannel: ReminderChannel | null;
}
