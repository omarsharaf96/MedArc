# Phase 3: Audit Logging - Research

**Researched:** 2026-03-11
**Domain:** HIPAA-compliant tamper-evident audit logging in Rust/SQLite (Tauri 2 desktop EMR)
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AUDT-01 | Every ePHI access is logged with timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure | Schema design, HIPAA field requirements, device UID crate |
| AUDT-02 | Audit logs use tamper-proof storage with cryptographic hash chains (each entry includes hash of previous entry) | SHA-256 hash chain pattern, SQLite trigger immutability |
| AUDT-03 | Audit logs are retained for minimum 6 years and cannot be deleted or modified by any user role | SQLite BEFORE DELETE/UPDATE triggers with RAISE(ABORT), retention logic |
| AUDT-04 | Provider can view their own audit log entries | RBAC `AuditLogs:Read` already in roles.rs, Tauri command + React table |
| AUDT-05 | System Admin can view all audit log entries | RBAC `AuditLogs:Read` already in roles.rs for SystemAdmin, same command with role filter |
</phase_requirements>

---

## Summary

Phase 3 builds a HIPAA Security Rule-compliant audit logging system on top of the SQLCipher-encrypted SQLite database and RBAC layer established in Phases 1 and 2. The work has three distinct technical pillars: (1) a new `audit_logs` SQLite table with an immutability layer enforced by database triggers, (2) SHA-256 hash-chaining where each row's hash input includes the previous row's stored hash, and (3) Tauri command integration that automatically writes an audit entry at every ePHI access point (the existing FHIR command handlers) and exposes role-scoped read queries to the React frontend.

The RBAC matrix already accounts for audit log access: `SystemAdmin` has full access, `Provider` has read-only access to their own entries, and all other roles have no audit access. This means Phase 3 is purely additive — no RBAC changes are needed.

The most critical design decision is where audit writes happen. They must be call-site injected into the existing FHIR commands (create_resource, get_resource, list_resources, update_resource, delete_resource) and break_glass commands, not in a separate background thread. Writing in the same transaction as the FHIR operation would be ideal for atomicity, but since the `Database` struct wraps a single `Mutex<Connection>`, the lock must be acquired only once per command to avoid deadlock.

**Primary recommendation:** Add a `write_audit_entry(conn: &Connection, entry: AuditEntry) -> Result<(), AppError>` function that takes the already-held connection reference and writes the audit row within the same lock scope as the FHIR operation, ensuring atomicity without a second lock acquisition.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| sha2 | 0.10.9 | SHA-256 hash chain computation | RustCrypto project, 393M+ downloads, pure Rust, no unsafe, MIT/Apache-2.0 |
| hex | 0.4 (already in Cargo.toml) | Encode SHA-256 bytes to lowercase hex string for storage | Already a project dependency |
| chrono | 0.4 (already in Cargo.toml) | UTC timestamps in RFC3339 | Already a project dependency |
| uuid | 1 (already in Cargo.toml) | Unique audit entry IDs | Already a project dependency |
| machine-uid | 0.5.4 | Get platform-native machine identifier on macOS (via ioreg) without root | Lightweight, macOS-native, no permissions required |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rusqlite triggers | built-in | `BEFORE UPDATE/DELETE` triggers using `RAISE(ABORT)` to enforce immutability | Declared in migration SQL, not a separate crate |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| machine-uid | machineid-rs | machineid-rs adds encryption overhead not needed here; machine-uid is simpler |
| machine-uid | Tauri OS plugin hostname | Hostname is user-configurable and not unique; machine UID is hardware-stable |
| SHA-256 | SHA-512 | Both are fine; SHA-256 is the standard choice and produces a 64-char hex string that is compact for storage |
| SQLite triggers | Application-layer enforcement | Triggers fire regardless of how the connection is accessed; application-layer enforcement can be bypassed if connection is opened outside the app |

**Installation (new dependencies only):**
```bash
# In src-tauri/Cargo.toml [dependencies]
sha2 = "0.10"
machine-uid = "0.5"
```

---

## Architecture Patterns

### Recommended Module Structure

```
src-tauri/src/
├── audit/
│   ├── mod.rs          # pub mod declarations
│   ├── entry.rs        # AuditEntry struct, ActionType enum, write_audit_entry()
│   └── query.rs        # read_audit_entries() with role-scoped filtering
├── commands/
│   ├── audit.rs        # Tauri commands: get_audit_log (AUDT-04, AUDT-05)
│   ├── fhir.rs         # EXISTING -- add audit write calls here (AUDT-01)
│   └── break_glass.rs  # EXISTING -- already logs, may need audit table integration
└── db/
    └── migrations.rs   # EXISTING -- add Migration 8: audit_logs table + triggers
```

### Pattern 1: AuditEntry Struct and ActionType Enum

**What:** A typed Rust struct representing one audit log row, with an `ActionType` enum covering all ePHI operations.

**When to use:** At every FHIR command call site to capture the operation before or after execution.

```rust
// Source: designed from HIPAA field requirements + project patterns
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    Create,
    Read,
    List,
    Update,
    Delete,
    Login,
    Logout,
    LockSession,
    UnlockSession,
    BreakGlassActivate,
    BreakGlassDeactivate,
    MfaSetup,
    MfaDisable,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Create => "CREATE",
            ActionType::Read => "READ",
            ActionType::List => "LIST",
            ActionType::Update => "UPDATE",
            ActionType::Delete => "DELETE",
            ActionType::Login => "LOGIN",
            ActionType::Logout => "LOGOUT",
            ActionType::LockSession => "LOCK_SESSION",
            ActionType::UnlockSession => "UNLOCK_SESSION",
            ActionType::BreakGlassActivate => "BREAK_GLASS_ACTIVATE",
            ActionType::BreakGlassDeactivate => "BREAK_GLASS_DEACTIVATE",
            ActionType::MfaSetup => "MFA_SETUP",
            ActionType::MfaDisable => "MFA_DISABLE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: String,              // UUID v4
    pub timestamp: String,       // RFC3339 UTC
    pub user_id: String,         // From session
    pub action_type: ActionType, // What operation
    pub resource_type: Option<String>, // e.g. "Patient", "Encounter"
    pub resource_id: Option<String>,   // The FHIR resource ID or patient ID
    pub device_id: String,       // machine-uid value (cached at startup)
    pub success: bool,           // Did the operation succeed?
    pub error_message: Option<String>, // If !success, why
    pub previous_hash: String,   // Hash of the prior entry ("GENESIS" for first)
    pub entry_hash: String,      // SHA-256 of (previous_hash + all other fields)
}
```

### Pattern 2: Hash Chain Computation

**What:** Each entry's `entry_hash` is computed by SHA-256 hashing the canonical string of all entry fields concatenated with `previous_hash`. This is the standard hash chain pattern used in tamper-evident logging.

**When to use:** Called inside `write_audit_entry()` before the INSERT.

```rust
// Source: derived from https://dev.to/veritaschain/building-a-tamper-evident-audit-log
// and sha2 0.10.9 docs at https://docs.rs/sha2/0.10.9/sha2/
use sha2::{Sha256, Digest};
use hex;

fn compute_entry_hash(entry: &AuditEntry) -> String {
    // Canonical serialization: deterministic field ordering
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        entry.previous_hash,
        entry.id,
        entry.timestamp,
        entry.user_id,
        entry.action_type.as_str(),
        entry.resource_type.as_deref().unwrap_or(""),
        entry.resource_id.as_deref().unwrap_or(""),
        entry.device_id,
        if entry.success { "1" } else { "0" },
        entry.error_message.as_deref().unwrap_or(""),
    );
    let hash_bytes = Sha256::digest(canonical.as_bytes());
    hex::encode(hash_bytes)
}
```

### Pattern 3: Fetching the Previous Hash Before INSERT

**What:** The `previous_hash` for a new entry is the `entry_hash` of the most recently inserted row. For the genesis (first) entry, use the string literal `"GENESIS"`.

**Critical:** This query and the INSERT must happen within the same Mutex lock hold to prevent a race condition where two concurrent writes both read the same "previous" hash.

```rust
// Source: project pattern (see db/connection.rs -- conn is &Connection under Mutex lock)
fn get_last_hash(conn: &rusqlite::Connection) -> Result<String, AppError> {
    let result: Result<String, _> = conn.query_row(
        "SELECT entry_hash FROM audit_logs ORDER BY rowid DESC LIMIT 1",
        [],
        |row| row.get(0),
    );
    match result {
        Ok(hash) => Ok(hash),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok("GENESIS".to_string()),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}
```

### Pattern 4: write_audit_entry() Takes &Connection, Not &Database

**What:** To avoid double-locking the Mutex (which deadlocks), `write_audit_entry` accepts a `&rusqlite::Connection` reference that the caller already holds, not a `&Database`.

**When to use:** All FHIR commands follow the pattern: acquire lock, do FHIR work, call `write_audit_entry` with the same conn, release lock. This is one atomic operation.

```rust
// Source: project pattern (see commands/fhir.rs -- conn is from db.conn.lock())
pub fn write_audit_entry(
    conn: &rusqlite::Connection,
    user_id: &str,
    action_type: ActionType,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    device_id: &str,
    success: bool,
    error_message: Option<&str>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let previous_hash = get_last_hash(conn)?;

    let mut entry = AuditEntry {
        id: id.clone(),
        timestamp: timestamp.clone(),
        user_id: user_id.to_string(),
        action_type,
        resource_type: resource_type.map(|s| s.to_string()),
        resource_id: resource_id.map(|s| s.to_string()),
        device_id: device_id.to_string(),
        success,
        error_message: error_message.map(|s| s.to_string()),
        previous_hash: previous_hash.clone(),
        entry_hash: String::new(), // filled below
    };
    entry.entry_hash = compute_entry_hash(&entry);

    conn.execute(
        "INSERT INTO audit_logs (
            id, timestamp, user_id, action_type, resource_type, resource_id,
            device_id, success, error_message, previous_hash, entry_hash
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            entry.id, entry.timestamp, entry.user_id,
            entry.action_type.as_str(),
            entry.resource_type, entry.resource_id,
            entry.device_id,
            if entry.success { 1i64 } else { 0i64 },
            entry.error_message, entry.previous_hash, entry.entry_hash
        ],
    )?;
    Ok(())
}
```

### Pattern 5: SQLite Trigger-Based Immutability

**What:** Two `BEFORE` triggers raise an abort error if any code attempts to UPDATE or DELETE rows in `audit_logs`. This enforces immutability at the database layer, not just the application layer.

**When to use:** Declared in Migration 8 alongside the `audit_logs` table DDL.

```sql
-- Source: SQLite trigger pattern for RAISE(ABORT)
-- See https://www.sqlitetutorial.net/sqlite-trigger/
CREATE TRIGGER audit_logs_prevent_update
BEFORE UPDATE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'Audit log entries are immutable and cannot be modified');
END;

CREATE TRIGGER audit_logs_prevent_delete
BEFORE DELETE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'Audit log entries are immutable and cannot be deleted');
END;
```

### Pattern 6: Device ID Caching at Startup

**What:** `machine_uid::get()` makes a system call (ioreg on macOS). Cache the result once at app startup in `app.manage()` as a `DeviceId(String)` newtype, then pass it to commands via `State<'_, DeviceId>`.

**When to use:** In `lib.rs` `setup()` closure.

```rust
// Source: Tauri 2 state management pattern (same as SessionManager, Database)
pub struct DeviceId(pub String);

// In setup:
let device_id = machine_uid::get()
    .unwrap_or_else(|_| "unknown-device".to_string());
app.manage(DeviceId(device_id));
```

### Pattern 7: Role-Scoped Audit Query (AUDT-04, AUDT-05)

**What:** A single Tauri command `get_audit_log` that returns entries filtered by user role. Provider sees only their own entries; SystemAdmin sees all.

```rust
// Source: project pattern (see commands/fhir.rs for list query shape)
#[tauri::command]
pub fn get_audit_log(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<AuditLogEntry>, AppError> {
    let (user_id, role) =
        middleware::check_permission(&session, Resource::AuditLogs, Action::Read)?;

    let conn = db.conn.lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (query, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) =
        match role {
            Role::SystemAdmin => (
                "SELECT ... FROM audit_logs ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
                vec![Box::new(limit.unwrap_or(100)), Box::new(offset.unwrap_or(0))],
            ),
            Role::Provider => (
                "SELECT ... FROM audit_logs WHERE user_id = ?1 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
                vec![Box::new(user_id), Box::new(limit.unwrap_or(100)), Box::new(offset.unwrap_or(0))],
            ),
            _ => return Err(AppError::Unauthorized("No audit log access".to_string())),
        };
    // ... execute and map rows
}
```

### Recommended Schema (Migration 8)

```sql
CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp TEXT NOT NULL,              -- RFC3339 UTC (indexed for time-range queries)
    user_id TEXT NOT NULL,                -- REFERENCES users(id) -- no FK to allow log-even-if-user-deleted
    action_type TEXT NOT NULL,            -- 'CREATE', 'READ', 'UPDATE', 'DELETE', 'LOGIN', etc.
    resource_type TEXT,                   -- FHIR resource type or NULL for non-FHIR actions
    resource_id TEXT,                     -- FHIR resource ID or patient ID or NULL
    device_id TEXT NOT NULL,              -- machine-uid value
    success INTEGER NOT NULL DEFAULT 1,   -- 1 = success, 0 = failure
    error_message TEXT,                   -- NULL if success
    previous_hash TEXT NOT NULL,          -- entry_hash of prior row, or 'GENESIS'
    entry_hash TEXT NOT NULL UNIQUE,      -- SHA-256 of all fields incl. previous_hash
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource_id ON audit_logs(resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action_type ON audit_logs(action_type);
```

**Note on user_id foreign key:** Intentionally NOT a hard FK reference. HIPAA requires audit logs be retained for 6 years; if a user account is deleted the audit history must survive. Store user_id as TEXT with no constraint.

### Anti-Patterns to Avoid

- **Double-locking Database Mutex:** Never call `db.conn.lock()` twice in the same command. Acquire once, do all work (FHIR op + audit write) within that scope, release.
- **Writing audit entries in a background thread:** The hash chain requires strict sequential ordering. Async or background writes can produce hash chain gaps if two entries race.
- **Using `SystemTime::now()` for timestamps:** Always use `chrono::Utc::now().to_rfc3339()` for UTC compliance. SystemTime is not timezone-aware.
- **Not logging failures:** HIPAA requires logging both successful and failed access attempts. Every FHIR command must write an audit entry for the failure case too, not just the success path.
- **Omitting audit entries on permission denial:** When `middleware::check_permission()` returns `Err(Unauthorized)`, that denied access attempt is also an ePHI access event and must be logged.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SHA-256 hashing | Custom digest implementation | sha2 0.10.9 (RustCrypto) | 393M downloads, formally reviewed, constant-time |
| Machine identifier | Read `/etc/machine-id` or call ioreg manually | machine-uid 0.5.4 | Handles macOS ioreg, Linux dbus, Windows registry correctly |
| Hex encoding | `format!("{:02x}", byte)` loop | hex 0.4 (already in Cargo.toml) | Battle-tested, zero unsafe, already a dependency |
| Immutability enforcement | Application-layer `if` checks | SQLite `BEFORE DELETE/UPDATE` triggers with `RAISE(ABORT)` | Triggers fire at DB layer regardless of code path |

**Key insight:** The hash chain logic is ~50 lines of code; the real complexity is in the wiring: calling audit writes at every command call site without double-locking, and ensuring failure paths also produce audit entries.

---

## Common Pitfalls

### Pitfall 1: Mutex Deadlock from Double Lock Acquisition
**What goes wrong:** A command acquires `db.conn.lock()` for the FHIR operation, then calls a function that also acquires `db.conn.lock()` for the audit write. The second `lock()` call blocks forever on the same thread.
**Why it happens:** `std::sync::Mutex` is not reentrant in Rust.
**How to avoid:** `write_audit_entry` must accept `&rusqlite::Connection` (already unlocked), not `&Database`. See Pattern 4.
**Warning signs:** Application hangs on first FHIR operation after audit integration.

### Pitfall 2: Missing Audit Entry on Failure Path
**What goes wrong:** FHIR commands return early with `Err(...)` before the audit write, so failed access attempts are never logged. HIPAA requires logging failures.
**Why it happens:** Natural early-return Rust error handling skips code after the `?` operator.
**How to avoid:** Use a helper that always writes the audit entry regardless of operation outcome. Consider a RAII-style audit writer, or restructure: attempt operation, capture result, write audit entry with `success = result.is_ok()`, then return result.
**Warning signs:** No audit entries visible for unauthorized access attempts in testing.

### Pitfall 3: Hash Chain Race Condition
**What goes wrong:** Two concurrent FHIR commands both call `get_last_hash()`, get the same previous hash, and both compute the same `entry_hash`. The `UNIQUE` constraint on `entry_hash` then rejects one of the INSERTs.
**Why it happens:** `get_last_hash()` and the INSERT are two separate statements; another thread can insert between them.
**How to avoid:** The existing `Mutex<Connection>` on `Database` already serializes all DB access. As long as `get_last_hash()` and the INSERT happen within the same Mutex lock hold (which Pattern 4 ensures), this race cannot occur.
**Warning signs:** Sporadic INSERT failures with "UNIQUE constraint failed: audit_logs.entry_hash".

### Pitfall 4: Logging Audit Log Reads (Infinite Loop)
**What goes wrong:** The `get_audit_log` command itself is an ePHI-adjacent operation. If you add an audit entry for every audit log read, each audit read generates another audit entry, which could cause unbounded growth and a logical loop.
**Why it happens:** Over-application of the "log everything" rule.
**How to avoid:** Audit log reads (AUDT-04, AUDT-05) do NOT themselves generate audit entries. Only ePHI (patient/clinical data) access is logged. This is consistent with HIPAA intent.

### Pitfall 5: Break-Glass vs. Audit Table Overlap
**What goes wrong:** Break-glass events are currently logged in `break_glass_log` (Migration 6). Adding audit logging for break-glass actions risks duplicating the record.
**Why it happens:** Two separate logging mechanisms for the same event.
**How to avoid:** Keep `break_glass_log` for break-glass lifecycle data (reason, duration, actions taken). Additionally write an `audit_logs` entry with `action_type = BREAK_GLASS_ACTIVATE` to include the event in the hash chain. Both logs serve different purposes: operational (break_glass_log) vs. integrity chain (audit_logs).

### Pitfall 6: Device ID Unavailable
**What goes wrong:** `machine_uid::get()` can fail (sandboxed environment, permissions issue). If you panic, the app crashes at startup.
**Why it happens:** System calls can fail.
**How to avoid:** Use `.unwrap_or_else(|_| "unknown-device".to_string())` as a fallback. Log a warning but do not panic. HIPAA does not require device ID to be globally unique, just device-correlated; "unknown-device" is acceptable if the hardware doesn't expose one.

---

## Code Examples

### Computing SHA-256 Entry Hash

```rust
// Source: sha2 0.10.9 docs at https://docs.rs/sha2/0.10.9/sha2/
// hex crate already in Cargo.toml (project dependency)
use sha2::{Sha256, Digest};

fn compute_entry_hash(
    previous_hash: &str,
    id: &str,
    timestamp: &str,
    user_id: &str,
    action_type: &str,
    resource_type: &str,
    resource_id: &str,
    device_id: &str,
    success: bool,
    error_message: &str,
) -> String {
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        previous_hash, id, timestamp, user_id, action_type,
        resource_type, resource_id, device_id,
        if success { "1" } else { "0" },
        error_message,
    );
    let hash = Sha256::digest(canonical.as_bytes());
    hex::encode(hash)
}
```

### Migration 8 SQL (complete)

```sql
-- Source: project migration pattern (see db/migrations.rs) + SQLite trigger docs
CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp TEXT NOT NULL,
    user_id TEXT NOT NULL,
    action_type TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    device_id TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    previous_hash TEXT NOT NULL,
    entry_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource_id ON audit_logs(resource_id);

CREATE TRIGGER audit_logs_prevent_update
BEFORE UPDATE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'Audit log entries are immutable');
END;

CREATE TRIGGER audit_logs_prevent_delete
BEFORE DELETE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'Audit log entries cannot be deleted');
END;
```

### Adding Audit Write to Existing FHIR Command

```rust
// Source: adapted from commands/fhir.rs pattern + audit::entry module
#[tauri::command]
pub fn get_resource(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id_state: State<'_, DeviceId>,
    id: String,
) -> Result<FhirResource, AppError> {
    // 1. Permission check (does NOT acquire db lock)
    let (user_id, role) =
        middleware::check_permission(&session, Resource::ClinicalRecords, Action::Read)?;

    // 2. Acquire DB lock ONCE
    let conn = db.conn.lock()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // 3. Attempt the FHIR operation
    let result = query_fhir_resource(&conn, &id);

    // 4. Write audit entry with success/failure -- within same lock scope
    let success = result.is_ok();
    let err_msg = result.as_ref().err().map(|e| e.to_string());
    audit::entry::write_audit_entry(
        &conn,
        &user_id,
        ActionType::Read,
        Some("FhirResource"),
        Some(&id),
        &device_id_state.0,
        success,
        err_msg.as_deref(),
    )?;

    // 5. Apply field filtering and return (lock drops here)
    let mut resource = result?;
    let allowed_fields = roles::visible_fields(role, &resource.resource_type);
    resource.resource = field_filter::filter_resource(&resource.resource, &allowed_fields);
    Ok(resource)
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Separate audit log file (syslog) | Database-embedded audit table with hash chain | Healthcare industry shift 2018-2024 | Single encrypted store; no risk of log file deletion |
| Application-layer immutability | SQLite trigger-based RAISE(ABORT) | Always available in SQLite | Cannot be bypassed by direct DB access |
| MD5/SHA-1 for audit integrity | SHA-256 (SHA-2 family) | ~2010, formally deprecated SHA-1 for security 2017 | SHA-256 is the current HIPAA-acceptable standard |
| Plaintext audit logs | Audit inside SQLCipher-encrypted DB | Tauri + SQLCipher pattern | PHI in audit entries is encrypted at rest without extra work |

**Deprecated/outdated:**
- Separate syslog-based audit: creates a separate unencrypted file outside the encrypted DB; unacceptable for ePHI.
- SHA-1 for integrity hashing: deprecated for security applications; use SHA-256.
- Audit writes in separate async task: introduces hash chain race conditions; synchronous inline writes are required.

---

## Integration Points With Existing Code

### Commands That Need Audit Injection

All of these must write to `audit_logs` after Phase 3:

| Command | Module | ActionType | resource_type | resource_id |
|---------|--------|-----------|--------------|-------------|
| `create_resource` | commands/fhir.rs | Create | input.resource_type | returned id |
| `get_resource` | commands/fhir.rs | Read | resource.resource_type | id param |
| `list_resources` | commands/fhir.rs | List | resource_type param or "ALL" | None |
| `update_resource` | commands/fhir.rs | Update | resource.resource_type | input.id |
| `delete_resource` | commands/fhir.rs | Delete | None (not returned) | id param |
| `login` | commands/auth.rs | Login | None | None |
| `logout` | commands/auth.rs | Logout | None | None |
| `activate_break_glass` | commands/break_glass.rs | BreakGlassActivate | None | patient_id |
| `deactivate_break_glass` | commands/break_glass.rs | BreakGlassDeactivate | None | None |

### RBAC: No Changes Required

`roles.rs` already defines:
- `(Provider, AuditLogs, Read) => true` — Provider reads own logs (AUDT-04)
- `(SystemAdmin, AuditLogs, _) => true` — SystemAdmin reads all (AUDT-05)
- All other roles: `false` for AuditLogs

The `middleware::check_permission()` function already handles enforcement.

### lib.rs: New State to Manage

```rust
// Add to setup() in lib.rs:
let device_id = machine_uid::get()
    .unwrap_or_else(|_| "unknown-device".to_string());
app.manage(DeviceId(device_id));
```

---

## Open Questions

1. **Audit log retention enforcement (AUDT-03: 6 years)**
   - What we know: The triggers prevent DELETE. But HIPAA requires logs be kept for 6 years.
   - What's unclear: The current schema has no `created_at`-based retention gating. The requirement says "cannot be deleted" — the trigger satisfies this. There is no explicit requirement to auto-expire entries after 6 years (only to RETAIN for 6 years).
   - Recommendation: The immutability trigger is sufficient for AUDT-03. Document that the 6-year retention window means "keep for at least 6 years"; since deletion is prohibited entirely, this requirement is satisfied by default. If storage growth is a concern, this is a Phase 3 deferred item.

2. **Should login/logout events be logged (AUDT-01 scope)?**
   - What we know: AUDT-01 specifies "every ePHI access." Login/logout are not ePHI access per se.
   - What's unclear: HIPAA audit controls (45 CFR 164.312(b)) include system logins as auditable events.
   - Recommendation: Include Login/Logout/LockSession/UnlockSession in `audit_logs` as `ActionType` variants. This goes slightly beyond AUDT-01's letter but satisfies the spirit and is standard practice. The planner can scope these as a distinct sub-task.

3. **Hash chain verification command**
   - What we know: AUDT-02 requires tamper-proof storage with hash chains. The chain is written correctly, but there is no verification command to detect if someone has manipulated the raw SQLite file outside the app.
   - What's unclear: Whether AUDT-02 requires a verification UI or just that the chain structure exists.
   - Recommendation: Implement a `verify_audit_chain()` Tauri command accessible to SystemAdmin only. O(n) walk of the audit_logs table, recomputing each hash and checking linkage. This is a moderate-sized task; include it in Phase 3.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` unit tests (same as all existing modules) |
| Config file | none -- Cargo runs tests natively |
| Quick run command | `cargo test -p medarc --lib -- audit` |
| Full suite command | `cargo test -p medarc` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUDT-01 | `write_audit_entry` stores all 9 required fields | unit | `cargo test -p medarc --lib -- audit::entry::tests` | Wave 0 |
| AUDT-01 | Failure path audit entry has `success = false` | unit | `cargo test -p medarc --lib -- audit::entry::tests::failure_path_logged` | Wave 0 |
| AUDT-02 | `entry_hash` changes if any field changes | unit | `cargo test -p medarc --lib -- audit::entry::tests::hash_chain_integrity` | Wave 0 |
| AUDT-02 | `previous_hash` of entry N equals `entry_hash` of entry N-1 | unit | `cargo test -p medarc --lib -- audit::entry::tests::chain_linkage` | Wave 0 |
| AUDT-02 | `verify_audit_chain` returns Ok for untampered chain | unit | `cargo test -p medarc --lib -- audit::query::tests::verify_clean_chain` | Wave 0 |
| AUDT-02 | SQLite UPDATE trigger aborts with RAISE(ABORT) | unit | `cargo test -p medarc --lib -- audit::entry::tests::immutability_update_rejected` | Wave 0 |
| AUDT-02 | SQLite DELETE trigger aborts with RAISE(ABORT) | unit | `cargo test -p medarc --lib -- audit::entry::tests::immutability_delete_rejected` | Wave 0 |
| AUDT-03 | No DELETE succeeds from any role (tested via trigger) | unit | covered by AUDT-02 trigger tests | Wave 0 |
| AUDT-04 | Provider `get_audit_log` returns only their own entries | unit | `cargo test -p medarc --lib -- audit::query::tests::provider_sees_own_only` | Wave 0 |
| AUDT-05 | SystemAdmin `get_audit_log` returns all entries | unit | `cargo test -p medarc --lib -- audit::query::tests::system_admin_sees_all` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p medarc --lib -- audit`
- **Per wave merge:** `cargo test -p medarc`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src-tauri/src/audit/entry.rs` — covers AUDT-01, AUDT-02
- [ ] `src-tauri/src/audit/query.rs` — covers AUDT-04, AUDT-05
- [ ] `src-tauri/src/audit/mod.rs` — module declarations
- [ ] `src-tauri/src/commands/audit.rs` — Tauri command for get_audit_log and verify_audit_chain
- [ ] In-memory DB test helper (pattern: open `:memory:` SQLite, run migrations, test) — shared by all audit tests

---

## Sources

### Primary (HIGH confidence)
- [sha2 0.10.9 docs (docs.rs)](https://docs.rs/sha2/0.10.9/sha2/) — SHA-256 API, imports, usage verified
- [machine-uid 0.5.4 docs (docs.rs)](https://docs.rs/machine-uid/latest/machine_uid/) — macOS ioreg machine ID, version verified
- Project source: `src-tauri/src/rbac/roles.rs` — RBAC matrix confirms AuditLogs permissions already defined
- Project source: `src-tauri/src/db/migrations.rs` — Migration pattern and SQLite schema conventions
- Project source: `src-tauri/src/db/connection.rs` — Mutex<Connection> pattern, single lock strategy
- Project source: `src-tauri/src/commands/fhir.rs` — Command structure to replicate for audit injection

### Secondary (MEDIUM confidence)
- [HIPAA Audit Log Requirements 2025 (kiteworks.com)](https://www.kiteworks.com/hipaa-compliance/hipaa-audit-log-requirements/) — 6-year retention, required fields, immutability guidance
- [HIPAA audit log fields developer guide (pangea.cloud)](https://pangea.cloud/blog/hipaa-audit-log-requirements/) — Field schema example verified against 45 CFR 164.312(b)
- [Hash chain tamper-evident audit log (dev.to/veritaschain)](https://dev.to/veritaschain/building-a-tamper-evident-audit-log-with-sha-256-hash-chains-zero-dependencies-h0b) — Algorithm description verified against sha2 docs
- [SQLite trigger RAISE(ABORT) pattern (sqlitetutorial.net)](https://www.sqlitetutorial.net/sqlite-trigger/) — BEFORE DELETE/UPDATE trigger syntax verified

### Tertiary (LOW confidence)
- [HIPAA audit log retention 6 years (schellman.com)](https://www.schellman.com/blog/healthcare-compliance/hipaa-audit-log-retention-policy) — Notes ongoing debate about whether audit logs fall under the 6-year documentation rule; erring on conservative side (6 years) is the correct approach

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — sha2 is the project standard for hashing; hex already in Cargo.toml; machine-uid verified at docs.rs
- Architecture (hash chain algorithm): HIGH — derived from sha2 docs + established pattern; project mutex model confirmed by reading connection.rs
- RBAC integration: HIGH — read roles.rs directly; AuditLogs permissions already exist
- HIPAA field requirements: MEDIUM — multiple concordant sources but not a single official HHS schema specification
- Pitfalls: HIGH — mutex deadlock and failure-path omission are Rust-specific verified patterns

**Research date:** 2026-03-11
**Valid until:** 2026-06-11 (sha2, machine-uid are stable crates; HIPAA requirements are stable law)
