/// Audit logging module — HIPAA-compliant tamper-proof access log.
///
/// # Architecture
/// - `entry`: write path — `write_audit_entry()` inserts a row and maintains
///   the SHA-256 hash chain.
/// - `query`: read path — `query_audit_log()` for paginated retrieval and
///   `verify_audit_chain()` for cryptographic integrity verification.
///
/// # Usage
/// Every ePHI-touching command must call `write_audit_entry()` with the
/// `AuditEntryInput` struct populated from its own parameters, both on
/// success **and** on failure paths. The `device_id` comes from the
/// `DeviceIdState` managed Tauri state (wired in T04).
pub mod entry;
pub mod query;

pub use entry::{write_audit_entry, AuditEntry, AuditEntryInput};
pub use query::{
    query_audit_log, verify_audit_chain, AuditLogPage, AuditQuery, ChainVerificationResult,
};
