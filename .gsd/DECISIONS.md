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
