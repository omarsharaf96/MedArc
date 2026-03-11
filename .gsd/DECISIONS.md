# Decisions

Architectural and implementation decisions extracted from completed work.

## S01 — Desktop Shell & Encrypted Database

- Used `[lib] name = 'app_lib'` in Cargo.toml for clear separation between binary and library crate
- Used rusqlite 0.32 (bundled-sqlcipher) instead of 0.38 due to version compatibility with rusqlite_migration 1.x
- Used getrandom 0.2 API for key generation (compatible with rusqlite dependency tree)
- Used `std::sync::LazyLock` for static migrations instead of lazy_static crate (standard library since Rust 1.80)
- Used raw hex key format (`x'...'`) for SQLCipher to skip PBKDF2 and eliminate startup latency
- Used `#[serde(rename_all = 'camelCase')]` on Rust structs for consistent Tauri 2 frontend serialization
- Added `NotFound` variant to AppError for resource-not-found error handling in CRUD commands
- Used json_extract approach for Patient lookups rather than virtual generated columns (SQLite ALTER TABLE limitations)
- Passed resource_type as snake_case in invoke() params — Tauri 2 uses Rust parameter names for deserialization
- Extracted UI into DatabaseStatus and FhirExplorer components for clean separation of concerns
- Used Tailwind utility classes for all styling (no CSS modules or styled-components)

## S02 — Auth & Access Control

- SessionInfo struct lives in auth::session module (single source of truth for session state)
- password-auth crate wraps Argon2id with safe defaults (no hand-rolled crypto)
- First user registration uses bootstrap pattern (no auth required when 0 users exist)
- Account lockout reads max_failed_logins and lockout_duration_minutes from app_settings table
- SessionManager initialized from app_settings timeout value at app startup
- Added Validation error variant alongside Authentication/Unauthorized for input validation
- Used match-based static dispatch for RBAC matrix (zero runtime overhead, exhaustive pattern matching)
- Break-glass scoped to `clinicalrecords:read` permission key format for middleware consistency
- Field filtering returns `Vec<&'static str>` with wildcard `'*'` for full access roles
- Used totp-rs crate with SHA-1 algorithm for maximum authenticator app compatibility
- TOTP secret not stored until user verifies with valid code (verify-before-store pattern)
- Touch ID implemented as stub (returns unavailable) without tauri-plugin-biometry — graceful degradation
- Password re-entry required for disabling TOTP and enabling Touch ID (sensitive operations)
- Used base64 img tag for QR code display instead of qrcode.react (backend already returns qrBase64)
- break_glass invoke wrapper includes password param matching actual Rust command signature
- useIdleTimer debounces refreshSession IPC to once per 30 seconds to avoid excessive backend calls
- LockScreen renders as overlay on top of content (preserves React state while obscuring UI)
- Two-phase login pattern: `login` returns `mfa_required: true + pending_user_id` when TOTP enabled; `complete_login` verifies TOTP then atomically creates the session — session creation is never attempted before TOTP is confirmed
- useAuth stores pendingMfaUserId in-memory only (not persisted) — MFA flow is intentionally lost on page refresh, requiring re-login (correct security behavior)
- All login failure paths return `AppError::Authentication("Invalid credentials")` regardless of failure reason — prevents username enumeration (HIPAA-aligned)

## S03 — Audit Logging

- Used SHA-256 (sha2 0.10 crate) for the hash chain — FIPS-140 compliant, no custom crypto
- Hash pre-image format: `{prev_hash}|{id}|{timestamp}|{user_id}|{action}|{resource_type}|{resource_id}|{patient_id}|{device_id}|{success}|{details}` with `|` separator (unambiguous: not present in UUIDs or RFC-3339 timestamps)
- Chain origin sentinel is the string `"GENESIS"` (not an empty string or NULL) — makes the genesis condition explicit and testable
- Immutability enforced by SQLite BEFORE UPDATE / BEFORE DELETE triggers at migration time — triggers fire before any row change, ensuring no code path (including future SystemAdmin commands) can bypass the lock
- `write_audit_entry()` takes a raw `&Connection` (not `&Database`) so callers that already hold the Mutex lock can call it without re-entrant deadlock — callers are responsible for acquiring the lock
- `success` stored as INTEGER (0/1) in SQLite with a CHECK constraint; mapped to/from Rust `bool` — avoids TEXT enum drift
- `details` field is free-text but never should contain raw PHI — convention enforced by documentation, not schema
- Migration 8 added as append-only migration (index 8 in the vector) — backward compatible with existing databases from prior slices
- `DeviceId` managed state introduced as a `device_id.rs` stub (returns `"DEVICE_PENDING"`) ahead of T04 which will wire machine-uid; this lets T02 commands compile and emit audit rows with a safe placeholder rather than blocking on T04
- `extract_patient_id()` helper extracts FHIR patient references from resource JSON: `Patient.id`, `subject.reference`, or `patient.reference` in priority order — audit metadata only, never used for clinical logic
- `audit_denied()` helper writes failure audit rows when permission check fails before the DB lock is acquired; it acquires its own transient lock to avoid leaving denied requests unlogged
- The pattern `let _ = write_audit_entry(...)` is intentional: audit write failures are swallowed rather than propagating — a failed audit write must never block the primary operation (HIPAA requires best-effort logging, not atomicity with the audited action)
- Login failure paths (inactive account, locked account, wrong password, MFA pending, invalid MFA code) all produce audit rows with `success = false` and a safe non-enumerable detail string
- `complete_login` (MFA step 2) also carries `device_id: State<'_, DeviceId>` so the MFA-verified login row is attributed to the same device as the initial auth attempt
- `DeviceId::from_machine_uid()` replaces the `DeviceId::placeholder()` stub in lib.rs — uses the `machine-uid 0.5` crate which reads `/etc/machine-id` on Linux, `IOPlatformUUID` via ioreg on macOS, and `MachineGuid` registry on Windows, without requiring elevated privileges; falls back gracefully to "DEVICE_UNKNOWN" with a startup warning log if the OS cannot supply an ID
