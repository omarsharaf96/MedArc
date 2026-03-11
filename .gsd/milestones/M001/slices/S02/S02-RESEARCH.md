# Phase 2: Authentication & Access Control - Research

**Researched:** 2026-03-11
**Domain:** User authentication (Argon2 password hashing, TOTP MFA, Touch ID biometrics), session management (inactivity auto-lock), RBAC with field-level access control, emergency break-glass access
**Confidence:** HIGH

## Summary

Phase 2 builds the full authentication and authorization layer on top of the Phase 1 encrypted database foundation. This involves six distinct technical domains: (1) user account creation with Argon2id password hashing, (2) session management with configurable inactivity auto-lock, (3) Touch ID biometric authentication via macOS LocalAuthentication framework, (4) TOTP-based multi-factor authentication, (5) role-based access control with 5 roles and field-level permissions, and (6) emergency break-glass access with full audit logging.

The Rust ecosystem has mature, well-maintained crates for every component. The `argon2` crate (v0.5.3) from RustCrypto provides pure-Rust Argon2id hashing, and the higher-level `password-auth` crate (v1.0.0) wraps it with a simple two-function API (`generate_hash`/`verify_password`). For TOTP, the `totp-rs` crate (v5.7.1) is RFC 6238-compliant with optional QR code generation. Touch ID integration requires either the community `tauri-plugin-biometry` (which supports macOS) or direct use of `objc2-local-authentication` (v0.3.2), since the official Tauri biometric plugin only supports iOS and Android. Session auto-lock is a frontend concern implemented with JavaScript activity detection (mouse/keyboard events) and a Rust-side session state machine.

The RBAC system is application-specific and should be implemented as a Rust middleware layer around Tauri commands. The Day0 requirements define a clear 5-role permission matrix covering Clinical Records, Scheduling, Billing, Prescriptions, and Audit Logs. Field-level access control means filtering JSON fields from FHIR resources based on the caller's role before returning data to the frontend. The break-glass pattern follows the HIPAA standard: pre-staged emergency accounts with elevated but scoped permissions, time-limited sessions, and mandatory audit logging of all actions taken during break-glass access.

**Primary recommendation:** Use `password-auth` v1.0.0 (wraps `argon2`) for password hashing, `totp-rs` v5.7.1 with `qr` and `gen_secret` features for TOTP, `tauri-plugin-biometry` for Touch ID, implement session auto-lock via frontend activity tracking with Rust-side session state, and build a custom RBAC middleware that checks role permissions before executing Tauri commands.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AUTH-01 | User can create account with unique user ID (no shared accounts per HIPAA) | `users` table migration with UNIQUE constraint on username; UUID primary key; `password-auth` for credential storage |
| AUTH-02 | User can log in with password hashed via bcrypt/Argon2 (minimum 12 characters) | `password-auth` v1.0.0 uses Argon2id by default with OWASP-recommended params; 12-char minimum enforced in validation layer |
| AUTH-03 | User session auto-locks after 10-15 minutes of inactivity (configurable) | Frontend JS activity tracking (mousemove/keydown/click) resetting a timer; Rust-side session state with `locked` flag; configurable via `app_settings` table |
| AUTH-04 | User can authenticate via Touch ID on supported hardware | `tauri-plugin-biometry` provides cross-platform biometric auth including macOS Touch ID; falls back to device credential if Touch ID unavailable |
| AUTH-05 | User can enable TOTP-based MFA for their account | `totp-rs` v5.7.1 with `qr`, `otpauth`, `gen_secret` features; QR code generation for authenticator app enrollment; TOTP secret stored encrypted in database |
| AUTH-06 | System enforces RBAC with 5 roles: System Admin, Provider, Nurse/MA, Billing Staff, Front Desk | Enum-based role system in Rust; `user_roles` table; permission check middleware wrapping Tauri commands |
| AUTH-07 | Each role has field-level access control per RBAC matrix | JSON field filtering on FHIR resources before returning to frontend; role-permission matrix defined as static Rust data structure |
| AUTH-08 | Emergency "break-glass" access is time-limited, tightly scoped, and fully logged | Break-glass session with `expires_at` timestamp; elevated permissions scoped to clinical read-only; all actions logged with break-glass flag |
</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| password-auth | 1.0.0 | Password hashing (Argon2id) | RustCrypto's high-level wrapper; uses OWASP-recommended Argon2id parameters by default; two-function API eliminates misconfiguration |
| argon2 | 0.5.3 | Underlying Argon2 implementation | Pure Rust, no C dependencies; supports Argon2d/Argon2i/Argon2id; used internally by password-auth |
| totp-rs | 5.7.1 | TOTP token generation/verification | RFC 6238-compliant; QR code generation; otpauth URL parsing; secure secret zeroing on drop |
| tauri-plugin-biometry | 0.2.x | Touch ID / biometric auth | Community plugin supporting macOS Touch ID (official plugin is mobile-only); Tauri v2 compatible |
| uuid | 1.x | User IDs | Already in project; cryptographically random UUIDs for user accounts |
| chrono | 0.4.x | Session timestamps | Already in project; session expiry and break-glass timeout tracking |

### Frontend

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| react | 18.x | UI framework | Already in project |
| @tauri-apps/api | 2.x | IPC for auth commands | Already in project |
| qrcode.react | 4.x | TOTP QR code display | Renders QR codes for authenticator app enrollment in React |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde | 1.x | Serialization | Already in project; serialize auth structs for Tauri commands |
| rand | 0.8.x | Random generation | Session token generation, TOTP secret generation fallback |
| base32 | 0.5.x | TOTP secret encoding | Encode TOTP secrets for authenticator app compatibility (may be pulled in by totp-rs) |
| objc2-local-authentication | 0.3.2 | Direct macOS LocalAuthentication | Fallback if tauri-plugin-biometry proves insufficient; direct LAContext access |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| password-auth | argon2 directly | argon2 requires manual salt generation, parameter tuning; password-auth handles this with OWASP defaults |
| password-auth | bcrypt crate | Requirements say "bcrypt/Argon2"; Argon2id is the OWASP-recommended choice over bcrypt (memory-hard, configurable) |
| tauri-plugin-biometry | objc2-local-authentication directly | Direct FFI gives more control but requires manual Objective-C bridging; plugin provides clean Tauri integration |
| tauri-plugin-biometry | security-framework crate | security-framework doesn't wrap LocalAuthentication; it covers Keychain/certificates |
| totp-rs | totp_rfc6238 | totp_rfc6238 is lower-level; totp-rs adds QR code generation, otpauth URL support, and secret generation |
| Custom RBAC | Apache Casbin (Rust) | Casbin is powerful but overkill for 5 fixed roles; a static permission matrix is simpler and has zero runtime overhead |

**Installation (Rust - additions to Cargo.toml):**
```toml
[dependencies]
# Authentication
password-auth = { version = "1", features = ["argon2"] }
totp-rs = { version = "5", features = ["qr", "otpauth", "gen_secret", "serde_support"] }
rand = "0.8"
base32 = "0.5"

# Biometric (Touch ID)
tauri-plugin-biometry = "0.2"
```

**Installation (Frontend):**
```bash
npm install qrcode.react
npm install @anthropic-ai/tauri-plugin-biometry-api  # or @choochmeque/tauri-plugin-biometry-api
```

## Architecture Patterns

### Recommended Project Structure (additions to Phase 1)
```
src-tauri/src/
├── auth/                    # NEW: Authentication module
│   ├── mod.rs               # Module declarations
│   ├── password.rs          # Password hashing/verification using password-auth
│   ├── session.rs           # Session state machine (active/locked/expired)
│   ├── totp.rs              # TOTP setup, verification, secret management
│   └── biometric.rs         # Touch ID integration (if not using plugin directly)
├── rbac/                    # NEW: Role-based access control
│   ├── mod.rs               # Module declarations
│   ├── roles.rs             # Role enum, permission matrix
│   ├── middleware.rs         # Permission check before command execution
│   └── field_filter.rs      # JSON field filtering per role
├── commands/
│   ├── auth.rs              # NEW: Login, logout, register, unlock commands
│   ├── session.rs           # NEW: Session management commands
│   ├── fhir.rs              # EXISTING: Now wrapped with RBAC checks
│   └── health.rs            # EXISTING
├── db/
│   ├── migrations.rs        # EXISTING: New migrations for users, sessions, roles tables
│   └── models/
│       ├── fhir.rs           # EXISTING
│       └── user.rs           # NEW: User, UserRole, Session models
src/
├── components/
│   ├── auth/                # NEW: Auth UI components
│   │   ├── LoginForm.tsx    # Username/password login
│   │   ├── RegisterForm.tsx # Account creation
│   │   ├── LockScreen.tsx   # Session lock overlay
│   │   ├── MfaSetup.tsx     # TOTP enrollment with QR code
│   │   └── MfaPrompt.tsx    # TOTP code entry on login
│   └── ...existing...
├── hooks/
│   ├── useAuth.ts           # NEW: Auth state management
│   ├── useSession.ts        # NEW: Inactivity detection
│   └── useIdleTimer.ts      # NEW: Idle timeout logic
├── lib/
│   └── tauri.ts             # EXISTING: Add auth command wrappers
└── types/
    └── auth.ts              # NEW: Auth-related TypeScript types
```

### Pattern 1: User Account Creation with Argon2id

**What:** Create user accounts with unique IDs and Argon2id-hashed passwords.
**When to use:** AUTH-01, AUTH-02 implementation.

```rust
// src-tauri/src/auth/password.rs
use password_auth::{generate_hash, verify_password};
use crate::error::AppError;

/// Hash a password using Argon2id with OWASP-recommended parameters.
/// password-auth handles salt generation and parameter selection.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    if password.len() < 12 {
        return Err(AppError::Validation("Password must be at least 12 characters".into()));
    }
    let hash = generate_hash(password);
    Ok(hash)
}

/// Verify a password against a stored hash.
pub fn verify(password: &str, hash: &str) -> Result<(), AppError> {
    verify_password(password, hash)
        .map_err(|_| AppError::Authentication("Invalid password".into()))
}
```

```rust
// src-tauri/src/commands/auth.rs
use tauri::State;
use crate::db::connection::Database;
use crate::auth::password;
use crate::error::AppError;

#[tauri::command]
pub fn register_user(
    db: State<'_, Database>,
    username: String,
    password: String,
    display_name: String,
    role: String,
) -> Result<UserResponse, AppError> {
    // Validate password length
    if password.len() < 12 {
        return Err(AppError::Validation(
            "Password must be at least 12 characters".into()
        ));
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Check username uniqueness
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE username = ?1)",
        rusqlite::params![username],
        |row| row.get(0),
    )?;
    if exists {
        return Err(AppError::Validation("Username already exists".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let password_hash = password::hash_password(&password)?;
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO users (id, username, password_hash, display_name, role, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, username, password_hash, display_name, role, now, now],
    )?;

    Ok(UserResponse { id, username, display_name, role })
}
```

### Pattern 2: Session State Machine

**What:** Track authentication state with configurable inactivity timeout.
**When to use:** AUTH-03 implementation.

```rust
// src-tauri/src/auth/session.rs
use std::sync::Mutex;
use chrono::{DateTime, Utc, Duration};

#[derive(Debug, Clone)]
pub enum SessionState {
    /// No user logged in
    Unauthenticated,
    /// User logged in and active
    Active {
        user_id: String,
        role: String,
        last_activity: DateTime<Utc>,
        session_id: String,
    },
    /// Session locked due to inactivity
    Locked {
        user_id: String,
        role: String,
        locked_at: DateTime<Utc>,
        session_id: String,
    },
    /// Break-glass emergency session
    BreakGlass {
        user_id: String,
        original_role: String,
        elevated_permissions: Vec<String>,
        expires_at: DateTime<Utc>,
        session_id: String,
    },
}

pub struct SessionManager {
    state: Mutex<SessionState>,
    timeout_minutes: Mutex<u32>, // Configurable: 10-15 minutes
}

impl SessionManager {
    pub fn new(timeout_minutes: u32) -> Self {
        SessionManager {
            state: Mutex::new(SessionState::Unauthenticated),
            timeout_minutes: Mutex::new(timeout_minutes),
        }
    }

    pub fn check_timeout(&self) -> bool {
        let state = self.state.lock().unwrap();
        let timeout = *self.timeout_minutes.lock().unwrap();
        match &*state {
            SessionState::Active { last_activity, .. } => {
                Utc::now() - *last_activity > Duration::minutes(timeout as i64)
            }
            _ => false,
        }
    }

    pub fn refresh_activity(&self) {
        let mut state = self.state.lock().unwrap();
        if let SessionState::Active { last_activity, .. } = &mut *state {
            *last_activity = Utc::now();
        }
    }

    pub fn lock(&self) {
        let mut state = self.state.lock().unwrap();
        if let SessionState::Active { user_id, role, session_id, .. } = state.clone() {
            *state = SessionState::Locked {
                user_id,
                role,
                locked_at: Utc::now(),
                session_id,
            };
        }
    }
}
```

### Pattern 3: TOTP MFA Enrollment and Verification

**What:** Enable TOTP-based MFA with QR code enrollment.
**When to use:** AUTH-05 implementation.

```rust
// src-tauri/src/auth/totp.rs
use totp_rs::{Algorithm, TOTP, Secret};
use crate::error::AppError;

/// Generate a new TOTP secret and return setup data for QR code.
pub fn generate_totp_setup(
    username: &str,
) -> Result<(String, String, String), AppError> {
    // Generate a random secret
    let secret = Secret::generate_secret();
    let secret_base32 = secret.to_encoded().to_string();

    let totp = TOTP::new(
        Algorithm::SHA1, // SHA1 for maximum authenticator app compatibility
        6,               // 6 digits
        1,               // 1 step skew for clock drift tolerance
        30,              // 30-second period
        secret.to_bytes().map_err(|e| AppError::Authentication(e.to_string()))?,
        Some("MedArc".to_string()),
        username.to_string(),
    ).map_err(|e| AppError::Authentication(e.to_string()))?;

    let otpauth_url = totp.get_url();
    let qr_base64 = totp.get_qr_base64()
        .map_err(|e| AppError::Authentication(e.to_string()))?;

    Ok((secret_base32, otpauth_url, qr_base64))
}

/// Verify a TOTP code against a stored secret.
pub fn verify_totp(secret_base32: &str, code: &str) -> Result<bool, AppError> {
    let secret = Secret::Encoded(secret_base32.to_string());
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(|e| AppError::Authentication(e.to_string()))?,
        Some("MedArc".to_string()),
        String::new(),
    ).map_err(|e| AppError::Authentication(e.to_string()))?;

    Ok(totp.check_current(code)
        .map_err(|e| AppError::Authentication(e.to_string()))?)
}
```

### Pattern 4: RBAC Permission Matrix

**What:** Static role-permission matrix enforced at the Tauri command layer.
**When to use:** AUTH-06, AUTH-07 implementation.

```rust
// src-tauri/src/rbac/roles.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    SystemAdmin,
    Provider,
    NurseMa,
    BillingStaff,
    FrontDesk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    ClinicalRecords,
    Scheduling,
    Billing,
    Prescriptions,
    AuditLogs,
    UserManagement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Read,
    Update,
    Delete,
}

/// Check if a role has permission for an action on a resource.
/// Based on the RBAC matrix from Day0 requirements.
pub fn has_permission(role: Role, resource: Resource, action: Action) -> bool {
    use Role::*;
    use Resource::*;
    use Action::*;

    match (role, resource, action) {
        // System Admin: Full clinical (troubleshooting), Full scheduling,
        // Full billing, No prescriptions, Read all audit logs
        (SystemAdmin, Prescriptions, _) => false,
        (SystemAdmin, _, _) => true,

        // Provider: Full CRUD clinical, R/W own scheduling,
        // Read billing, Full CRUD prescriptions, Read own audit
        (Provider, ClinicalRecords, _) => true,
        (Provider, Scheduling, Create | Read | Update) => true,
        (Provider, Scheduling, Delete) => false,
        (Provider, Billing, Read) => true,
        (Provider, Billing, _) => false,
        (Provider, Prescriptions, _) => true,
        (Provider, AuditLogs, Read) => true, // own only - enforced at query level
        (Provider, AuditLogs, _) => false,

        // Nurse/MA: Read + Update vitals (clinical), R/W scheduling,
        // No billing, Read-only prescriptions, No audit
        (NurseMa, ClinicalRecords, Read | Update) => true,
        (NurseMa, ClinicalRecords, _) => false,
        (NurseMa, Scheduling, Create | Read | Update) => true,
        (NurseMa, Scheduling, Delete) => false,
        (NurseMa, Billing, _) => false,
        (NurseMa, Prescriptions, Read) => true,
        (NurseMa, Prescriptions, _) => false,
        (NurseMa, AuditLogs, _) => false,

        // Billing Staff: Demographics + codes only (clinical),
        // Read scheduling, Full CRUD billing, No prescriptions, No audit
        (BillingStaff, ClinicalRecords, Read) => true, // demographics + codes only - filtered at field level
        (BillingStaff, ClinicalRecords, _) => false,
        (BillingStaff, Scheduling, Read) => true,
        (BillingStaff, Scheduling, _) => false,
        (BillingStaff, Billing, _) => true,
        (BillingStaff, Prescriptions, _) => false,
        (BillingStaff, AuditLogs, _) => false,

        // Front Desk: Demographics only (clinical), Full CRUD scheduling,
        // Limited read billing, No prescriptions, No audit
        (FrontDesk, ClinicalRecords, Read) => true, // demographics only - filtered at field level
        (FrontDesk, ClinicalRecords, _) => false,
        (FrontDesk, Scheduling, _) => true,
        (FrontDesk, Billing, Read) => true, // limited
        (FrontDesk, Billing, _) => false,
        (FrontDesk, Prescriptions, _) => false,
        (FrontDesk, AuditLogs, _) => false,

        // Default deny
        (_, UserManagement, _) => matches!(role, SystemAdmin),
    }
}

/// Define which FHIR resource JSON fields are visible per role.
/// Used for field-level access control (AUTH-07).
pub fn visible_fields(role: Role, resource_type: &str) -> Vec<&'static str> {
    match (role, resource_type) {
        // Billing Staff sees demographics + billing codes only
        (Role::BillingStaff, "Patient") => vec![
            "id", "name", "birthDate", "gender", "address",
            "telecom", "identifier",
        ],
        (Role::BillingStaff, "Encounter") => vec![
            "id", "status", "class", "type", "subject", "period",
            // Billing-relevant fields
        ],

        // Front Desk sees demographics only
        (Role::FrontDesk, "Patient") => vec![
            "id", "name", "birthDate", "gender", "address",
            "telecom", "identifier", "contact",
        ],

        // Provider, SystemAdmin, Nurse/MA see all fields
        _ => vec!["*"],
    }
}
```

### Pattern 5: Field-Level JSON Filtering

**What:** Strip JSON fields from FHIR resources based on role permissions before returning to frontend.
**When to use:** AUTH-07 implementation -- wrap all FHIR resource read operations.

```rust
// src-tauri/src/rbac/field_filter.rs
use serde_json::Value;

/// Filter a FHIR resource JSON to include only allowed fields.
/// If allowed_fields contains "*", the resource is returned unmodified.
pub fn filter_resource(resource: &Value, allowed_fields: &[&str]) -> Value {
    if allowed_fields.contains(&"*") {
        return resource.clone();
    }

    match resource {
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(key, _)| allowed_fields.contains(&key.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Value::Object(filtered)
        }
        other => other.clone(),
    }
}
```

### Pattern 6: Break-Glass Emergency Access

**What:** Time-limited elevated access with comprehensive audit logging.
**When to use:** AUTH-08 implementation.

```rust
// Conceptual pattern for break-glass activation
// The actual implementation requires the audit logging from Phase 3,
// but the session and permission elevation are Phase 2 concerns.

#[tauri::command]
pub fn activate_break_glass(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    reason: String,
    patient_id: Option<String>,
) -> Result<BreakGlassResponse, AppError> {
    // 1. Require authenticated session
    let current_user = session.get_current_user()?;

    // 2. Validate reason is provided (mandatory)
    if reason.trim().is_empty() {
        return Err(AppError::Validation("Break-glass reason is required".into()));
    }

    // 3. Create time-limited break-glass session (30 minutes max)
    let expires_at = Utc::now() + Duration::minutes(30);

    // 4. Log break-glass activation (audit record)
    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let log_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO break_glass_log (id, user_id, reason, patient_id, activated_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![log_id, current_user.id, reason, patient_id,
                          Utc::now().to_rfc3339(), expires_at.to_rfc3339()],
    )?;

    // 5. Elevate session permissions
    session.activate_break_glass(
        current_user.id.clone(),
        current_user.role.clone(),
        vec!["clinical_records:read".into()], // Scoped to read-only clinical
        expires_at,
    )?;

    Ok(BreakGlassResponse {
        session_id: log_id,
        expires_at: expires_at.to_rfc3339(),
    })
}
```

### Pattern 7: Frontend Inactivity Detection

**What:** Track user activity and trigger session lock after configurable timeout.
**When to use:** AUTH-03 implementation on the frontend.

```typescript
// src/hooks/useIdleTimer.ts
import { useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export function useIdleTimer(timeoutMinutes: number) {
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  const resetTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(async () => {
      // Lock the session via Rust backend
      await invoke('lock_session');
    }, timeoutMinutes * 60 * 1000);
  }, [timeoutMinutes]);

  useEffect(() => {
    const events = ['mousemove', 'keydown', 'click', 'scroll', 'touchstart'];

    const handleActivity = () => {
      resetTimer();
      // Notify Rust to update last_activity timestamp
      invoke('refresh_session').catch(() => {});
    };

    events.forEach(event =>
      document.addEventListener(event, handleActivity, { passive: true })
    );

    resetTimer(); // Start the timer

    return () => {
      events.forEach(event =>
        document.removeEventListener(event, handleActivity)
      );
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [resetTimer]);
}
```

### Anti-Patterns to Avoid

- **Storing passwords in plaintext or with weak hashing:** Always use `password-auth` which defaults to Argon2id with OWASP parameters. Never use MD5, SHA-256, or unsalted hashing for passwords.
- **Storing TOTP secrets unencrypted:** TOTP secrets in the database must be encrypted. Since the database is already SQLCipher-encrypted, this provides one layer, but consider an additional application-level encryption for TOTP secrets.
- **Implementing RBAC checks only in the frontend:** Frontend role checks are for UX only (hiding buttons). All permission enforcement MUST happen in Rust Tauri commands. The frontend is untrusted.
- **Using SHA-256 or SHA-512 for TOTP:** Most authenticator apps (Google Authenticator, Authy) only reliably support SHA-1 for TOTP. They may silently revert to SHA-1 even when SHA-256 is specified, causing verification failures. Use SHA-1 explicitly.
- **Hardcoding timeout values:** The inactivity timeout must be configurable (10-15 minutes per HIPAA). Store in `app_settings` table, not as a constant.
- **Skipping break-glass logging:** Every action during a break-glass session must be logged. The break-glass log is the primary audit trail for HIPAA compliance and must include reason, timestamp, user, patient accessed, and actions taken.
- **Allowing break-glass without reason:** HIPAA requires documented justification. The reason field is mandatory and must be non-empty.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Password hashing | Custom Argon2 parameter tuning | `password-auth` v1.0.0 | OWASP-recommended params built-in; handles salt generation, encoding, verification |
| TOTP implementation | Custom HMAC-based OTP | `totp-rs` v5.7.1 | RFC 6238-compliant; handles time drift, code padding, secret encoding; QR code generation |
| Touch ID integration | Direct Objective-C FFI calls | `tauri-plugin-biometry` | Handles LAContext lifecycle, error handling, platform detection |
| Password validation | Regex-based password rules | Dedicated validation function | Length check (min 12) is the primary requirement; complexity rules are NIST-discouraged |
| Session tokens | Custom random string generation | `uuid::Uuid::new_v4()` | Cryptographically random, sufficient entropy for session IDs |
| QR code rendering | Canvas-based QR drawing | `qrcode.react` (frontend) or `totp-rs` QR feature (backend) | Well-tested libraries; totp-rs can generate base64 PNG directly |

**Key insight:** Authentication is the highest-risk security surface in a HIPAA-compliant application. Every hand-rolled component is a potential vulnerability. Use battle-tested libraries for every cryptographic operation.

## Common Pitfalls

### Pitfall 1: Argon2 Memory/Time Parameters Too Low
**What goes wrong:** Password hashes are crackable with commodity hardware.
**Why it happens:** Developer reduces Argon2 memory/time cost for faster tests or development.
**How to avoid:** Use `password-auth` which applies OWASP-recommended defaults automatically. Never override parameters for development convenience. If testing is slow, mock the password-auth calls in tests.
**Warning signs:** Argon2 hashing completing in under 100ms on modern hardware.

### Pitfall 2: TOTP Clock Drift Failures
**What goes wrong:** Valid TOTP codes rejected because server and authenticator app clocks are out of sync.
**Why it happens:** System clock drift, timezone issues, or NTP sync delays.
**How to avoid:** Use `totp-rs` with `skew = 1` which accepts codes from the previous, current, and next 30-second window. This provides a 90-second acceptance window.
**Warning signs:** Users reporting "code expired" even when entering codes immediately.

### Pitfall 3: Frontend-Only Session Lock
**What goes wrong:** User can bypass the lock screen by directly invoking Tauri commands from the browser console.
**Why it happens:** Session lock only implemented as a React overlay without backend validation.
**How to avoid:** Every Tauri command must check session state in the Rust backend. If the session is locked, return an `Unauthorized` error regardless of what the frontend shows.
**Warning signs:** Tauri commands succeeding while the lock screen is displayed.

### Pitfall 4: Role Elevation Without Re-authentication
**What goes wrong:** Break-glass access granted without verifying the user's identity.
**Why it happens:** Developer assumes the current session implies identity verification.
**How to avoid:** Require password re-entry (and MFA if enabled) before activating break-glass access. The break-glass flow should be: request -> re-authenticate -> provide reason -> activate.
**Warning signs:** Break-glass activation with a single button click.

### Pitfall 5: TOTP Secret Leakage in API Responses
**What goes wrong:** TOTP secret is returned to the frontend after initial setup.
**Why it happens:** The user model includes the totp_secret field and it's serialized in all responses.
**How to avoid:** Never include `totp_secret` in user response structs. Create a separate `TotpSetupResponse` that includes the secret/QR code only during initial enrollment. After enrollment, the secret should only be accessible to the Rust backend for verification.
**Warning signs:** TOTP secret visible in frontend network inspection after login.

### Pitfall 6: Missing Unique Constraint on Username
**What goes wrong:** Duplicate user accounts created, violating HIPAA unique user ID requirement.
**Why it happens:** Race condition between uniqueness check and insert, or missing database constraint.
**How to avoid:** Add a UNIQUE constraint on `users.username` at the database level. The application-level check is a UX improvement, but the database constraint is the actual enforcement.
**Warning signs:** SQLite constraint violation errors in production.

### Pitfall 7: Touch ID Bypassing Password Auth
**What goes wrong:** Touch ID used as the sole authentication method, bypassing password entirely.
**Why it happens:** Treating Touch ID as a primary auth method rather than a convenience unlock.
**How to avoid:** Touch ID should only be available for session unlock (AUTH-04), not initial login. The user must authenticate with password (+ MFA if enabled) on first login. Touch ID is for re-authentication after session lock.
**Warning signs:** User can open the app and access data with only Touch ID, never having entered a password.

## Database Schema (Migrations)

New tables required for Phase 2:

```sql
-- Migration 4: Users table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,           -- UUID
    username TEXT NOT NULL UNIQUE,          -- Unique user identifier (HIPAA)
    password_hash TEXT NOT NULL,            -- Argon2id hash from password-auth
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('system_admin', 'provider', 'nurse_ma', 'billing_staff', 'front_desk')),
    totp_secret TEXT,                       -- Base32-encoded TOTP secret (NULL = MFA not enabled)
    totp_enabled INTEGER NOT NULL DEFAULT 0,
    touch_id_enabled INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    failed_login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,                      -- Account lockout timestamp
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- Migration 5: Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,           -- UUID session token
    user_id TEXT NOT NULL REFERENCES users(id),
    state TEXT NOT NULL CHECK(state IN ('active', 'locked', 'expired')),
    last_activity TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);

-- Migration 6: Break-glass audit log
CREATE TABLE IF NOT EXISTS break_glass_log (
    id TEXT PRIMARY KEY NOT NULL,           -- UUID
    user_id TEXT NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL,                   -- Mandatory justification
    patient_id TEXT,                        -- Scoped to specific patient if applicable
    activated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    deactivated_at TEXT,                    -- NULL until session ends
    actions_taken TEXT,                     -- JSON array of actions performed
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_break_glass_user ON break_glass_log(user_id);
CREATE INDEX IF NOT EXISTS idx_break_glass_activated ON break_glass_log(activated_at);

-- Migration 7: App settings (for configurable timeout)
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('session_timeout_minutes', '15');
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('max_failed_logins', '5');
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('lockout_duration_minutes', '30');
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| bcrypt for password hashing | Argon2id (OWASP recommendation) | 2019+ | Memory-hard; configurable parallelism; resistant to GPU/ASIC attacks; bcrypt limited to 72-byte passwords |
| Custom TOTP implementations | RFC 6238-compliant libraries | Ongoing | Standardized; QR code enrollment; otpauth:// URL format universal across authenticator apps |
| NIST password complexity rules | NIST SP 800-63B (length > complexity) | 2017 | Minimum length (12+ chars) more effective than requiring special characters; composition rules discouraged |
| Session cookies (web apps) | Managed state session tokens (desktop) | N/A | Desktop apps use in-memory session state; no cookie management needed; Tauri managed state is the session store |
| OAuth2/OIDC for auth | Local password + MFA (desktop app) | N/A | Desktop EMR is local-first; no external identity provider needed in Phase 1; OAuth would add unnecessary complexity |

**Deprecated/outdated:**
- **bcrypt for new applications:** While still acceptable, OWASP recommends Argon2id for new implementations. bcrypt's 72-byte password limit and lack of memory-hardness are disadvantages.
- **SMS-based MFA:** NIST downgraded SMS to "restricted" authenticator in SP 800-63B. TOTP is the recommended approach.
- **Password complexity rules (uppercase/lowercase/special):** NIST SP 800-63B explicitly discourages composition rules. Length (minimum 12 characters) is the primary strength factor.

## Open Questions

1. **Touch ID Plugin Maturity**
   - What we know: The official Tauri biometric plugin does NOT support macOS (mobile-only). The community `tauri-plugin-biometry` (v0.2) claims macOS support. Alternatively, `objc2-local-authentication` (v0.3.2) provides direct Rust bindings to macOS LocalAuthentication framework.
   - What's unclear: The maturity and reliability of `tauri-plugin-biometry` for production use; whether it handles edge cases (Touch ID not enrolled, hardware without Touch ID, password fallback).
   - Recommendation: Start with `tauri-plugin-biometry`. If it proves unreliable, fall back to direct `objc2-local-authentication` usage. Touch ID should gracefully degrade to password re-entry if biometrics are unavailable.

2. **Break-Glass Audit Logging Before Phase 3**
   - What we know: Phase 3 implements the full audit logging system (AUDT-01 through AUDT-05). Phase 2 requires break-glass actions to be "fully logged" (AUTH-08).
   - What's unclear: Whether to implement a minimal break-glass-specific audit log in Phase 2 or defer full logging to Phase 3.
   - Recommendation: Implement a `break_glass_log` table in Phase 2 that captures break-glass-specific events. Phase 3's full audit system will extend this with the tamper-proof hash chain. This avoids blocking AUTH-08 on Phase 3.

3. **Account Lockout Policy**
   - What we know: HIPAA requires access controls but doesn't specify exact lockout thresholds.
   - What's unclear: The optimal number of failed login attempts before lockout and the lockout duration.
   - Recommendation: Default to 5 failed attempts with 30-minute lockout (stored in `app_settings` for configurability). This balances security with usability in a clinical setting where locked-out providers cannot treat patients.

4. **Initial Admin Account Bootstrap**
   - What we know: The first user must be a System Admin to manage other accounts. There's no existing user to grant admin privileges.
   - What's unclear: Whether to auto-create a default admin on first launch or require a setup wizard.
   - Recommendation: Implement a first-run setup flow: if the `users` table is empty, the registration form creates a System Admin account. After the first admin exists, only admins can create new accounts.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework + cargo test |
| Config file | None needed (Cargo.toml `[dev-dependencies]`) |
| Quick run command | `cd src-tauri && cargo test` |
| Full suite command | `cd src-tauri && cargo test --all-features` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUTH-01 | Create user with unique username | unit | `cd src-tauri && cargo test auth::tests::test_create_user -x` | No -- Wave 0 |
| AUTH-01 | Reject duplicate username | unit | `cd src-tauri && cargo test auth::tests::test_duplicate_username -x` | No -- Wave 0 |
| AUTH-02 | Hash password with Argon2id | unit | `cd src-tauri && cargo test auth::password::tests::test_hash_verify -x` | No -- Wave 0 |
| AUTH-02 | Reject passwords < 12 chars | unit | `cd src-tauri && cargo test auth::password::tests::test_min_length -x` | No -- Wave 0 |
| AUTH-03 | Session locks after timeout | unit | `cd src-tauri && cargo test auth::session::tests::test_timeout_lock -x` | No -- Wave 0 |
| AUTH-04 | Touch ID availability check | manual-only | N/A (requires hardware) | N/A |
| AUTH-05 | TOTP secret generation | unit | `cd src-tauri && cargo test auth::totp::tests::test_generate_setup -x` | No -- Wave 0 |
| AUTH-05 | TOTP code verification | unit | `cd src-tauri && cargo test auth::totp::tests::test_verify_code -x` | No -- Wave 0 |
| AUTH-06 | Permission matrix correctness | unit | `cd src-tauri && cargo test rbac::roles::tests::test_permissions -x` | No -- Wave 0 |
| AUTH-07 | Field filtering by role | unit | `cd src-tauri && cargo test rbac::field_filter::tests::test_filter -x` | No -- Wave 0 |
| AUTH-08 | Break-glass activation with reason | unit | `cd src-tauri && cargo test auth::tests::test_break_glass -x` | No -- Wave 0 |
| AUTH-08 | Break-glass expiry enforcement | unit | `cd src-tauri && cargo test auth::tests::test_break_glass_expiry -x` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cd src-tauri && cargo test`
- **Per wave merge:** `cd src-tauri && cargo test --all-features`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src-tauri/src/auth/mod.rs` -- module declarations
- [ ] `src-tauri/src/auth/password.rs` -- password hash/verify with tests
- [ ] `src-tauri/src/auth/session.rs` -- session state machine with tests
- [ ] `src-tauri/src/auth/totp.rs` -- TOTP setup/verify with tests
- [ ] `src-tauri/src/rbac/roles.rs` -- role enum and permission matrix with tests
- [ ] `src-tauri/src/rbac/field_filter.rs` -- JSON field filtering with tests
- [ ] Test helpers for creating in-memory SQLCipher databases for unit tests

## Sources

### Primary (HIGH confidence)
- [RustCrypto argon2 crate](https://docs.rs/argon2/0.5.3) - Pure Rust Argon2id implementation, v0.5.3
- [RustCrypto password-auth crate](https://docs.rs/password-auth/1.0.0) - High-level password hashing API with OWASP defaults, v1.0.0
- [totp-rs crate](https://docs.rs/totp-rs/5.7.1) - RFC 6238 TOTP implementation, v5.7.1, QR code generation
- [Tauri 2 Biometric Plugin docs](https://v2.tauri.app/plugin/biometric/) - Official plugin: Android/iOS only, macOS NOT supported
- [Apple LAContext documentation](https://developer.apple.com/documentation/localauthentication/lacontext) - macOS Touch ID via LocalAuthentication framework
- [objc2-local-authentication crate](https://docs.rs/objc2-local-authentication/0.3.2) - Rust bindings for Apple LocalAuthentication, v0.3.2
- [HIPAA Security Rule technical safeguards](https://hipaa.yale.edu/security/break-glass-procedure-granting-emergency-access-critical-ephi-systems) - Break-glass access requirements
- [MedArc Day0 Requirements RBAC Matrix](requirments/Day0.md) - 5-role permission matrix from project requirements

### Secondary (MEDIUM confidence)
- [tauri-plugin-biometry](https://github.com/Choochmeque/tauri-plugin-biometry) - Community plugin with macOS Touch ID support, v0.2
- [NIST SP 800-63B](https://pages.nist.gov/800-63-3/sp800-63b.html) - Digital Identity Guidelines: password requirements, MFA recommendations
- [OpenEMR Access Controls Listing](https://www.open-emr.org/wiki/index.php/Access_Controls_Listing) - Reference EMR ACL structure for comparison
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html) - Argon2id recommended parameters

### Tertiary (LOW confidence)
- [tauri-plugin-biometry maturity](https://crates.io/crates/tauri-plugin-biometry) - Community plugin; production readiness unverified
- [Tauri biometric macOS issue #263](https://github.com/tauri-apps/plugins-workspace/issues/263) - Feature request still open; confirms official plugin gap

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All crates are stable releases from well-maintained projects (RustCrypto, constantoine/totp-rs)
- Architecture: HIGH - Patterns follow established Tauri 2 state management and command patterns from Phase 1
- Password hashing: HIGH - `password-auth` is RustCrypto's official high-level API; OWASP parameters built-in
- TOTP: HIGH - `totp-rs` v5.7.1 is mature, RFC-compliant, widely used
- Touch ID: MEDIUM - Official Tauri plugin lacks macOS support; community plugin exists but production readiness unverified
- RBAC: HIGH - Permission matrix directly from project requirements (Day0.md); implementation pattern is straightforward static dispatch
- Break-glass: MEDIUM - Pattern well-documented in HIPAA guidance; audit logging coupling with Phase 3 needs resolution
- Session management: HIGH - Standard inactivity timer pattern; Tauri managed state for backend enforcement

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (30 days -- all core technologies are stable releases; Touch ID plugin may evolve faster)