/**
 * TypeScript types for audit log queries and results.
 *
 * Field names use camelCase to match the Rust structs' #[serde(rename_all = "camelCase")].
 */

/** A single audit log row as returned from the backend. */
export interface AuditEntry {
  id: string;
  timestamp: string;
  userId: string;
  action: string;
  resourceType: string;
  resourceId: string | null;
  patientId: string | null;
  deviceId: string;
  success: boolean;
  details: string | null;
  previousHash: string;
  entryHash: string;
}

/** Paginated result from get_audit_log. */
export interface AuditLogPage {
  entries: AuditEntry[];
  total: number;
  limit: number;
  offset: number;
}

/** Optional filters for get_audit_log. */
export interface AuditQuery {
  userId?: string | null;
  patientId?: string | null;
  action?: string | null;
  from?: string | null;
  to?: string | null;
  limit?: number | null;
  offset?: number | null;
}

/** Result from verify_audit_chain_cmd. */
export interface ChainVerificationResult {
  valid: boolean;
  rowsChecked: number;
  error: string | null;
}
