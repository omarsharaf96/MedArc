# Phase 1: Desktop Shell & Encrypted Database - Research

**Researched:** 2026-03-10
**Domain:** Tauri 2.x desktop shell, SQLCipher encrypted database, macOS Keychain integration, FHIR R4 data modeling, Rust-native CRUD
**Confidence:** HIGH

## Summary

Phase 1 establishes the application foundation: a Tauri 2.x desktop shell rendering a React/TypeScript frontend in macOS WKWebView, backed by a SQLCipher-encrypted SQLite database with encryption keys stored in macOS Keychain. All data operations run through Rust-native Tauri commands with no Python runtime dependency.

The technology stack is mature and well-documented. Tauri 2.x reached stable in October 2024 and is now at v2.10.3 (March 2026). rusqlite 0.38.0 provides first-class `bundled-sqlcipher` support that statically compiles SQLCipher (currently SQLite 3.51.1) into the binary. The `keyring` crate v3.6.3 with `apple-native` feature provides clean macOS Keychain access. FHIR R4 resources map naturally to JSON columns in SQLite with generated virtual columns and B-tree indexes for frequently queried fields.

The main architectural tension is between requirement FOUN-05 (which names Alembic, a Python migration tool) and FOUN-06 (which mandates no Python dependency). The roadmap decision "Rust owns all CRUD" resolves this: use a Rust-native migration tool (`rusqlite_migration` or `refinery`) instead of Alembic. The `render_as_batch=True` concern from Alembic is a SQLAlchemy-specific workaround for SQLite's limited ALTER TABLE support -- Rust migration tools handle this natively by writing explicit SQL. Additionally, the Secure Enclave cannot directly store symmetric keys (only EC P-256 keys); however, Keychain items stored with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` are themselves encrypted by the Secure Enclave's AES-256-GCM keys, providing hardware-backed protection for the SQLCipher passphrase.

**Primary recommendation:** Use rusqlite with `bundled-sqlcipher` for the encrypted database, `keyring` with `apple-native` for Keychain access, `rusqlite_migration` for schema migrations, and standard Tauri 2 commands with managed state for the CRUD layer. Store the FHIR JSON resources as TEXT columns with virtual generated columns and indexes for searchable fields.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FOUN-01 | Application launches as a macOS desktop app via Tauri 2.x shell with WKWebView rendering React frontend | Tauri 2.10.3 stable, `create-tauri-app` scaffolding with React/TS, WKWebView is the default on macOS |
| FOUN-02 | All data stored in SQLCipher-encrypted SQLite database with AES-256-CBC and per-page HMAC tamper detection | rusqlite 0.38.0 with `bundled-sqlcipher` feature, SQLCipher defaults to AES-256-CBC with per-page HMAC-SHA512 |
| FOUN-03 | Database encryption key stored exclusively in macOS Keychain (Secure Enclave-backed on Apple Silicon) | `keyring` 3.6.3 with `apple-native` feature; Keychain items are hardware-protected by Secure Enclave AES-256-GCM |
| FOUN-04 | Data modeled as FHIR R4 resources stored as JSON columns with indexed lookup tables | SQLite JSON1 extension + virtual generated columns with `json_extract()` + B-tree indexes |
| FOUN-05 | Alembic schema migrations with render_as_batch=True for SQLite compatibility | **Reinterpreted:** Use `rusqlite_migration` (Rust-native) instead of Alembic (Python). No Python dependency per FOUN-06. Explicit SQL migrations handle SQLite ALTER TABLE limitations natively. |
| FOUN-06 | Rust-native Tauri commands handle all database CRUD and file system operations | Tauri 2 command system with `#[tauri::command]`, managed state via `State<T>`, serde JSON serialization |
</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tauri | 2.10.3 | Desktop application framework | Stable since Oct 2024; Rust backend + WKWebView on macOS; 3-10 MB bundle vs 150-300 MB Electron |
| rusqlite | 0.38.0 | SQLite/SQLCipher database driver | First-class `bundled-sqlcipher` feature; bundles SQLite 3.51.1; `serde_json` feature for JSON columns |
| keyring | 3.6.3 | macOS Keychain access | `apple-native` feature wraps Security.framework; simple get/set/delete API for generic passwords |
| rusqlite_migration | latest | Schema migration management | Uses SQLite `user_version` pragma (no migration table overhead); ideal for embedded databases |
| serde / serde_json | 1.x | Serialization | Required for Tauri command arguments/returns and JSON column storage |
| thiserror | 1.x / 2.x | Error types | Standard for Rust error handling; integrates with Tauri command error serialization |

### Frontend

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| react | 18.x+ | UI framework | Project constraint from PROJECT.md |
| typescript | 5.x | Type safety | Project constraint |
| vite | 5.x+ | Build tool / dev server | Default bundler for Tauri 2 React template; HMR support |
| @tauri-apps/api | 2.x | Frontend IPC | Official Tauri JS API for invoking Rust commands |
| tailwindcss | 3.x / 4.x | Utility CSS | Standard for Tauri React apps; pairs well with shadcn/ui |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio | 1.x | Async runtime | Already included by Tauri; use for async commands |
| uuid | 1.x | FHIR resource IDs | Generate UUIDs for resource identifiers |
| chrono | 0.4.x | Timestamps | FHIR date/dateTime handling; audit timestamps |
| security-framework | 3.7.x | Low-level macOS Security API | Only if `keyring` proves insufficient for Secure Enclave access control flags |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rusqlite_migration | refinery | refinery is more feature-rich (SQL file loading, versioned/unversioned) but heavier; rusqlite_migration is lighter with user_version approach |
| rusqlite | sqlx with sqlcipher | sqlx is async-native but SQLCipher support requires custom libsqlite3-sys override; rusqlite's bundled-sqlcipher is simpler |
| keyring | security-framework directly | security-framework gives lower-level control but keyring provides a cleaner API for simple password storage |
| Custom Tauri commands | tauri-plugin-rusqlite2 | Plugin provides ready-made SQLite commands but limits control over encryption key management and FHIR schema design |

**Installation (Rust - Cargo.toml):**
```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
keyring = { version = "3", features = ["apple-native"] }
rusqlite = { version = "0.38", features = ["bundled-sqlcipher", "serde_json"] }
rusqlite_migration = "1"
```

**Installation (Frontend - package.json):**
```bash
npm create tauri-app@latest -- --template react-ts
npm install @tauri-apps/api
npm install -D tailwindcss postcss autoprefixer
```

## Architecture Patterns

### Recommended Project Structure
```
medarc/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.js
├── index.html
├── src/                          # React frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/               # Shared UI components
│   ├── hooks/                    # Custom React hooks
│   ├── lib/                      # Utilities
│   │   └── tauri.ts              # Typed wrappers around invoke()
│   └── types/                    # TypeScript types (mirroring Rust structs)
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json          # IPC permissions
│   └── src/
│       ├── main.rs               # Desktop entry point
│       ├── lib.rs                 # App builder, command registration, state setup
│       ├── commands/              # Tauri command handlers (one file per domain)
│       │   ├── mod.rs
│       │   └── health.rs          # Health check / DB status commands
│       ├── db/                    # Database layer
│       │   ├── mod.rs
│       │   ├── connection.rs      # SQLCipher connection + key management
│       │   ├── migrations.rs      # Schema migrations
│       │   └── models/            # FHIR resource types + SQL mapping
│       │       ├── mod.rs
│       │       └── fhir.rs        # Base FHIR resource types
│       ├── keychain.rs            # macOS Keychain wrapper
│       └── error.rs               # Unified error types
└── tests/                         # Integration tests
```

### Pattern 1: Database Connection with SQLCipher via Managed State

**What:** Initialize SQLCipher connection at app startup, unlock with Keychain-stored key, wrap in Mutex, manage via Tauri state.
**When to use:** Always -- this is the core data access pattern for the entire application.

```rust
// src-tauri/src/db/connection.rs
use rusqlite::Connection;
use std::sync::Mutex;

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn open(db_path: &str, key: &str) -> Result<Self, crate::error::AppError> {
        let conn = Connection::open(db_path)?;

        // PRAGMA key must be the FIRST statement after opening
        conn.pragma_update(None, "key", key)?;

        // Verify encryption is working by reading from the database
        conn.pragma_query_value(None, "cipher_version", |row| row.get::<_, String>(0))?;

        // Enable WAL mode for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Enable foreign keys
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(Database {
            conn: Mutex::new(conn),
        })
    }
}

// src-tauri/src/lib.rs
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Get or generate encryption key from Keychain
            let key = crate::keychain::get_or_create_db_key()?;

            // Resolve database path
            let db_path = app.path().app_data_dir()?.join("medarc.db");

            // Open encrypted database
            let db = Database::open(db_path.to_str().unwrap(), &key)?;

            // Run migrations
            crate::db::migrations::run(&db)?;

            // Register as managed state
            app.manage(db);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health::check_db,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Pattern 2: Keychain Key Management

**What:** Store/retrieve the SQLCipher encryption passphrase in macOS Keychain.
**When to use:** At app startup before opening the database.

```rust
// src-tauri/src/keychain.rs
use keyring::Entry;
use uuid::Uuid;

const SERVICE_NAME: &str = "com.medarc.emr";
const ACCOUNT_NAME: &str = "database-encryption-key";

pub fn get_or_create_db_key() -> Result<String, crate::error::AppError> {
    let entry = Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|e| crate::error::AppError::Keychain(e.to_string()))?;

    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            // First launch: generate a cryptographically random key
            let key = generate_random_key();
            entry.set_password(&key)
                .map_err(|e| crate::error::AppError::Keychain(e.to_string()))?;
            Ok(key)
        }
        Err(e) => Err(crate::error::AppError::Keychain(e.to_string())),
    }
}

fn generate_random_key() -> String {
    // Generate 32 bytes of random data, hex-encode for SQLCipher raw key
    // SQLCipher accepts raw hex keys prefixed with "x'" and suffixed with "'"
    use std::io::Read;
    let mut key_bytes = [0u8; 32];
    // Use OS random source
    getrandom::fill(&mut key_bytes).expect("Failed to generate random key");
    format!("x'{}'", hex::encode(key_bytes))
}
```

### Pattern 3: FHIR Resource Storage with JSON + Indexed Lookups

**What:** Store FHIR R4 resources as JSON in a TEXT column, with virtual generated columns and indexes for searchable fields.
**When to use:** For all FHIR resource types (Patient, Encounter, Observation, etc.).

```sql
-- Migration V1: Base FHIR resource table pattern
CREATE TABLE IF NOT EXISTS fhir_resources (
    id TEXT PRIMARY KEY NOT NULL,          -- FHIR resource logical ID (UUID)
    resource_type TEXT NOT NULL,            -- e.g., "Patient", "Encounter"
    resource JSON NOT NULL,                 -- Full FHIR R4 JSON resource
    version_id INTEGER NOT NULL DEFAULT 1,  -- FHIR versionId for optimistic locking
    last_updated TEXT NOT NULL,             -- ISO 8601 timestamp
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index on resource type for type-specific queries
CREATE INDEX idx_fhir_resources_type ON fhir_resources(resource_type);

-- Virtual generated columns for frequently queried Patient fields
ALTER TABLE fhir_resources ADD COLUMN patient_family_name TEXT
    GENERATED ALWAYS AS (
        CASE WHEN resource_type = 'Patient'
        THEN json_extract(resource, '$.name[0].family')
        ELSE NULL END
    ) VIRTUAL;

ALTER TABLE fhir_resources ADD COLUMN patient_given_name TEXT
    GENERATED ALWAYS AS (
        CASE WHEN resource_type = 'Patient'
        THEN json_extract(resource, '$.name[0].given[0]')
        ELSE NULL END
    ) VIRTUAL;

ALTER TABLE fhir_resources ADD COLUMN patient_birth_date TEXT
    GENERATED ALWAYS AS (
        CASE WHEN resource_type = 'Patient'
        THEN json_extract(resource, '$.birthDate')
        ELSE NULL END
    ) VIRTUAL;

-- Indexes on virtual columns for fast lookups
CREATE INDEX idx_patient_family ON fhir_resources(patient_family_name)
    WHERE resource_type = 'Patient';
CREATE INDEX idx_patient_given ON fhir_resources(patient_given_name)
    WHERE resource_type = 'Patient';
CREATE INDEX idx_patient_dob ON fhir_resources(patient_birth_date)
    WHERE resource_type = 'Patient';
```

### Pattern 4: Tauri Command with Database Access

**What:** Expose database operations as Tauri commands callable from the React frontend.
**When to use:** For every CRUD operation.

```rust
// src-tauri/src/commands/health.rs
use tauri::State;
use crate::db::connection::Database;

#[tauri::command]
pub fn check_db(db: State<'_, Database>) -> Result<DbStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let version: String = conn
        .pragma_query_value(None, "cipher_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let page_count: i64 = conn
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|e| e.to_string())?;

    Ok(DbStatus {
        encrypted: true,
        cipher_version: version,
        page_count,
    })
}

#[derive(serde::Serialize)]
pub struct DbStatus {
    encrypted: bool,
    cipher_version: String,
    page_count: i64,
}
```

```typescript
// src/lib/tauri.ts (frontend)
import { invoke } from "@tauri-apps/api/core";

interface DbStatus {
  encrypted: boolean;
  cipherVersion: string;
  pageCount: number;
}

export async function checkDatabase(): Promise<DbStatus> {
  return invoke<DbStatus>("check_db");
}
```

### Pattern 5: Rust-Native Schema Migrations

**What:** Define and apply database migrations using `rusqlite_migration`.
**When to use:** At app startup, before any data operations.

```rust
// src-tauri/src/db/migrations.rs
use rusqlite_migration::{Migrations, M};

const MIGRATIONS: Migrations<'static> = Migrations::from_slice(&[
    M::up(
        "CREATE TABLE IF NOT EXISTS fhir_resources (
            id TEXT PRIMARY KEY NOT NULL,
            resource_type TEXT NOT NULL,
            resource JSON NOT NULL,
            version_id INTEGER NOT NULL DEFAULT 1,
            last_updated TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_fhir_resources_type
            ON fhir_resources(resource_type);"
    ),
    M::up(
        "CREATE TABLE IF NOT EXISTS fhir_identifiers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            resource_id TEXT NOT NULL REFERENCES fhir_resources(id),
            system TEXT,
            value TEXT NOT NULL,
            UNIQUE(system, value)
        );
        CREATE INDEX IF NOT EXISTS idx_fhir_identifiers_value
            ON fhir_identifiers(value);"
    ),
]);

pub fn run(db: &crate::db::connection::Database) -> Result<(), crate::error::AppError> {
    let mut conn = db.conn.lock().map_err(|e|
        crate::error::AppError::Database(e.to_string())
    )?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        assert!(MIGRATIONS.validate().is_ok());
    }
}
```

### Anti-Patterns to Avoid

- **Setting PRAGMA key after any other operation:** SQLCipher requires `PRAGMA key` to be the FIRST statement after `Connection::open()`. Any other query before key will fail silently or corrupt the database.
- **Storing the encryption key in config files or environment variables:** The key must come from macOS Keychain only. Never use hardcoded keys, .env files, or command-line arguments.
- **Using Tauri's official SQL plugin for SQLCipher:** The official `tauri-plugin-sql` does NOT support SQLCipher. Use rusqlite directly with `bundled-sqlcipher`.
- **Opening multiple concurrent connections to SQLCipher:** Unlike regular SQLite, SQLCipher key derivation is expensive (256K PBKDF2 iterations). Open one connection at startup and reuse it via managed state.
- **Using ALTER TABLE for complex schema changes:** SQLite has limited ALTER TABLE support (cannot DROP COLUMN on older versions, cannot change column types). Write migrations as CREATE new table, copy data, DROP old, RENAME new.
- **Blocking the main thread with database operations:** Use async Tauri commands for any operation that could take >16ms to keep the UI responsive.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SQLite encryption | Custom encryption layer over SQLite | rusqlite + bundled-sqlcipher | SQLCipher handles page-level encryption, HMAC, key derivation -- rolling your own will have security holes |
| Keychain access | Direct FFI to Security.framework | keyring crate with apple-native | Handles all the Core Foundation type bridging and error handling |
| Schema migrations | Custom version tracking table | rusqlite_migration | Handles version tracking, atomic application, rollback support |
| JSON serialization | Manual JSON string building | serde + serde_json | Type-safe serialization with compile-time checking |
| UUID generation | Custom ID generation | uuid crate with v4 feature | Cryptographically random, standards-compliant |
| Random key generation | Custom PRNG | getrandom crate | OS-level CSPRNG (uses Security.framework on macOS) |
| FHIR validation | Custom resource validators | serde struct definitions | Define Rust structs matching FHIR R4 schema, serde handles validation |

**Key insight:** This phase involves security-critical infrastructure (encryption, key management, tamper detection). Every hand-rolled security component is a liability. Use battle-tested libraries.

## Common Pitfalls

### Pitfall 1: PRAGMA Key Ordering
**What goes wrong:** Database appears to open successfully but all queries return "file is encrypted or is not a database" errors.
**Why it happens:** Any SQL statement executed before `PRAGMA key` causes SQLCipher to interpret the file as an unkeyed database.
**How to avoid:** Always call `conn.pragma_update(None, "key", &key)` immediately after `Connection::open()` before any other operation, including PRAGMA calls.
**Warning signs:** "file is encrypted or is not a database" error on first query after opening.

### Pitfall 2: Secure Enclave Does Not Store Symmetric Keys
**What goes wrong:** Developer tries to use `kSecAttrTokenIDSecureEnclave` to store the SQLCipher passphrase directly in the Secure Enclave.
**Why it happens:** Secure Enclave only supports EC P-256 asymmetric keys. Symmetric keys and passphrases cannot be stored directly in the Secure Enclave hardware.
**How to avoid:** Store the passphrase as a `kSecClassGenericPassword` Keychain item with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`. The Keychain itself encrypts the item using Secure Enclave-protected AES-256-GCM keys. This provides hardware-backed protection without requiring direct Secure Enclave storage.
**Warning signs:** `errSecUnimplemented` or `errSecParam` errors when trying to store a symmetric key with Secure Enclave token.

### Pitfall 3: SQLCipher Performance with PBKDF2
**What goes wrong:** App startup takes 1-3 seconds due to SQLCipher key derivation.
**Why it happens:** SQLCipher defaults to 256,000 PBKDF2-HMAC-SHA512 iterations for passphrase-based keys. This is intentional for security but affects startup time.
**How to avoid:** Use a raw hex key (prefixed with `x'...'`) instead of a passphrase. Raw keys skip PBKDF2 entirely since the key is already the correct length. Generate 32 random bytes, hex-encode them, and store in Keychain.
**Warning signs:** Noticeable delay between app launch and first database query completing.

### Pitfall 4: Tauri Command Snake_Case vs camelCase
**What goes wrong:** Frontend `invoke()` calls silently fail or return undefined.
**Why it happens:** Rust command names use snake_case but Tauri converts them to camelCase for the frontend. The frontend must use the snake_case name in `invoke("snake_case_name")` -- Tauri does NOT auto-convert.
**How to avoid:** Always use the exact Rust function name (snake_case) in `invoke()` calls. Rename with `#[tauri::command(rename_all = "snake_case")]` if needed.
**Warning signs:** invoke() promise never resolves; no error in console.

### Pitfall 5: Missing Capability Permissions
**What goes wrong:** Tauri commands exist but the frontend gets a permission denied error.
**Why it happens:** Tauri 2 requires explicit capability declarations for commands the frontend can call. Without a capability file granting access, all custom commands are blocked.
**How to avoid:** Create a capability file in `src-tauri/capabilities/default.json` that grants permissions for all custom commands.
**Warning signs:** Console error: "command not allowed" or "permission denied".

### Pitfall 6: SQLite ALTER TABLE Limitations
**What goes wrong:** Migration fails with "near DROP: syntax error" or similar.
**Why it happens:** SQLite has very limited ALTER TABLE support. It cannot drop columns (before 3.35.0), change column types, or add constraints to existing columns.
**How to avoid:** For complex schema changes, use the 12-step migration pattern: create new table, copy data, drop old table, rename new table. This is what Alembic's `render_as_batch=True` does under the hood.
**Warning signs:** Migration SQL that uses DROP COLUMN, ALTER COLUMN, or ADD CONSTRAINT.

### Pitfall 7: JSON Column Type Confusion
**What goes wrong:** `json_extract()` returns NULL or incorrect types in queries.
**Why it happens:** SQLite's JSON functions require the column to contain valid JSON text. If you use TEXT type and insert non-JSON, or if the JSON path expression doesn't match the actual structure, queries silently return NULL.
**How to avoid:** Use `JSON` as the column type (SQLite treats it as TEXT but signals intent). Validate JSON structure in Rust before insertion using serde. Always test `json_extract()` paths against example FHIR resources.
**Warning signs:** Virtual generated columns always returning NULL despite valid-looking data.

## Code Examples

### Complete Tauri App Setup with SQLCipher

```rust
// src-tauri/src/lib.rs
// Source: Tauri 2 docs + rusqlite docs
mod commands;
mod db;
mod error;
mod keychain;

use db::connection::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("medarc.db");
            let key = keychain::get_or_create_db_key()?;
            let database = Database::open(
                db_path.to_str().expect("Invalid DB path"),
                &key,
            )?;

            db::migrations::run(&database)?;
            app.manage(database);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health::check_db,
            commands::health::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Error Type for Tauri Commands

```rust
// src-tauri/src/error.rs
// Source: Tauri 2 error handling docs + thiserror pattern
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

// Tauri requires Serialize for command error types
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(e: rusqlite_migration::Error) -> Self {
        AppError::Migration(e.to_string())
    }
}
```

### Frontend invoke() Wrapper

```typescript
// src/lib/tauri.ts
// Source: @tauri-apps/api docs
import { invoke } from "@tauri-apps/api/core";

// Type-safe command wrappers
export const commands = {
  checkDb: () => invoke<{
    encrypted: boolean;
    cipher_version: string;
    page_count: number;
  }>("check_db"),

  getAppInfo: () => invoke<{
    version: string;
    db_path: string;
  }>("get_app_info"),
};
```

### Capability Configuration

```json
// src-tauri/capabilities/default.json
// Source: Tauri 2 security/capabilities docs
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-set-title",
    {
      "identifier": "core:path:default",
      "allow": [{ "path": "$APPDATA/**" }]
    }
  ]
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tauri 1.x with Electron-like API | Tauri 2.x with capability-based security, mobile support | Oct 2024 | Commands must have explicit permissions; plugin system redesigned |
| SQLCipher PBKDF2-HMAC-SHA1 | PBKDF2-HMAC-SHA512 (default since SQLCipher 4.0) | 2018 | Stronger key derivation; must use compatible settings when opening existing databases |
| SQLite without JSON support | SQLite JSON1 as built-in (since 3.38.0, 2022) | 2022 | json_extract, json_each, JSONB support built-in; no extension loading needed |
| Alembic (Python) for SQLite migrations | Rust-native migration tools (rusqlite_migration, refinery) | N/A | No Python runtime dependency; simpler deployment |
| FHIR DSTU2/STU3 | FHIR R4 (current normative standard) | Jan 2019 | R4 is the first normative release; use R4 resource definitions |

**Deprecated/outdated:**
- **Tauri 1.x APIs**: Tauri 2 moved most core features into plugins; v1 import paths no longer work
- **SQLCipher 3.x defaults**: If creating new databases, always use SQLCipher 4+ defaults (AES-256-CBC, HMAC-SHA512, 256K iterations)
- **rusqlite `bundled` feature for encryption**: Use `bundled-sqlcipher` specifically; plain `bundled` provides unencrypted SQLite

## Open Questions

1. **Alembic Requirement vs Rust-Native Approach**
   - What we know: FOUN-05 explicitly names Alembic with `render_as_batch=True`. FOUN-06 mandates no Python dependency. The roadmap decision says "Rust owns all CRUD."
   - What's unclear: Whether the intent of FOUN-05 is specifically Alembic or more generally "reliable schema migrations that handle SQLite limitations."
   - Recommendation: Treat FOUN-05 as "schema migrations that handle SQLite's limited ALTER TABLE" and implement with `rusqlite_migration`. The `render_as_batch` concern is a SQLAlchemy-specific workaround; in raw SQL migrations, you simply write the correct 12-step migration pattern directly. Flag this interpretation for stakeholder confirmation.

2. **Single Table vs Per-Resource-Type Tables for FHIR**
   - What we know: A single `fhir_resources` table with `resource_type` discriminator is simpler. Per-type tables (e.g., `patients`, `encounters`) allow more specific indexes and generated columns.
   - What's unclear: At what data volume per-type tables become necessary for performance.
   - Recommendation: Start with a single `fhir_resources` table plus a separate `fhir_identifiers` lookup table. Add per-type lookup tables (e.g., `patient_search`) in later phases if search performance requires it. The virtual generated column approach provides a middle ground.

3. **Raw Key vs Passphrase for SQLCipher**
   - What we know: Passphrase-based keys incur 256K PBKDF2 iterations on every database open. Raw hex keys skip PBKDF2 entirely.
   - What's unclear: Whether the PBKDF2 overhead is acceptable given M1+ Apple Silicon performance.
   - Recommendation: Use raw hex keys (`x'<64 hex chars>'`). Since the key is machine-generated (not user-memorized) and stored in Keychain, PBKDF2 provides no additional security benefit -- the key already has full entropy. This eliminates 1-2 seconds of startup latency.

## Sources

### Primary (HIGH confidence)
- [Tauri 2.x Official Docs](https://v2.tauri.app/) - Project structure, command system, state management, capabilities
- [rusqlite GitHub](https://github.com/rusqlite/rusqlite) - v0.38.0, features: bundled-sqlcipher, serde_json
- [SQLCipher API Reference](https://www.zetetic.net/sqlcipher/sqlcipher-api/) - PRAGMA key, cipher_page_size, kdf_iter, hmac_use
- [SQLCipher Design](https://www.zetetic.net/sqlcipher/design/) - AES-256-CBC, per-page HMAC, PBKDF2 defaults
- [Apple Keychain Data Protection](https://support.apple.com/guide/security/keychain-data-protection-secb0694df1a/web) - Secure Enclave backing for Keychain items
- [Apple Protecting Keys with Secure Enclave](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave) - Limitation: EC P-256 only
- [SQLite JSON1 Extension](https://sqlite.org/json1.html) - json_extract, virtual generated columns, indexing
- [rusqlite_migration docs](https://docs.rs/rusqlite_migration) - API, user_version approach

### Secondary (MEDIUM confidence)
- [keyring crate docs](https://docs.rs/keyring) - v3.6.3, apple-native feature
- [Tauri SQL Plugin](https://v2.tauri.app/plugin/sql/) - Confirmed no SQLCipher support
- [SQLite JSON Virtual Columns + Indexing](https://www.dbpro.app/blog/sqlite-json-virtual-columns-indexing) - Performance patterns
- [FHIR R4 Patient Resource](https://hl7.org/fhir/R4/patient.html) - Resource structure reference
- [FHIR Storage Patterns](https://build.fhir.org/storage.html) - JSON + indexed projections approach

### Tertiary (LOW confidence)
- [tauri-plugin-rusqlite2](https://github.com/razein97/tauri-plugin-rusqlite2) - Community plugin with SQLCipher; not recommended due to limited control
- [Apple Developer Forums](https://developer.apple.com/forums/thread/760303) - Secure Enclave symmetric key limitation discussion

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries are stable releases with clear documentation
- Architecture: HIGH - Patterns verified from official Tauri 2 docs and rusqlite examples
- Pitfalls: HIGH - Derived from official SQLCipher docs, Apple Security docs, and Tauri 2 migration guides
- FHIR storage: MEDIUM - Pattern is sound but specific virtual column approach needs validation at scale
- Keychain/Secure Enclave: MEDIUM - Keyring crate API is simple but Secure Enclave interaction is indirect (Keychain encryption layer)

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (30 days -- all technologies are stable releases)