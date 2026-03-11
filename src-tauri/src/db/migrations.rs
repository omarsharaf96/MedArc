use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

use crate::db::connection::Database;
use crate::error::AppError;

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        // Migration 1: App metadata table
        M::up(
            "CREATE TABLE IF NOT EXISTS app_metadata (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('schema_version', '1');
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('created_at', datetime('now'));"
        ),
        // Migration 2: FHIR resources table with JSON column and indexes
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
            CREATE INDEX IF NOT EXISTS idx_fhir_resources_type ON fhir_resources(resource_type);
            CREATE INDEX IF NOT EXISTS idx_fhir_resources_updated ON fhir_resources(last_updated);"
        ),
        // Migration 3: FHIR identifier lookup table for fast identifier-based queries
        M::up(
            "CREATE TABLE IF NOT EXISTS fhir_identifiers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                resource_id TEXT NOT NULL REFERENCES fhir_resources(id) ON DELETE CASCADE,
                system TEXT,
                value TEXT NOT NULL,
                UNIQUE(system, value)
            );
            CREATE INDEX IF NOT EXISTS idx_fhir_identifiers_value ON fhir_identifiers(value);
            CREATE INDEX IF NOT EXISTS idx_fhir_identifiers_resource ON fhir_identifiers(resource_id);"
        ),
        // Migration 4: Users table for authentication
        M::up(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY NOT NULL,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL CHECK (role IN ('SystemAdmin', 'Physician', 'Nurse', 'MedicalAssistant', 'FrontDesk')),
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                touch_id_enabled INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                failed_login_attempts INTEGER NOT NULL DEFAULT 0,
                locked_until TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);"
        ),
        // Migration 5: Sessions table for session state tracking
        M::up(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                state TEXT NOT NULL CHECK (state IN ('active', 'locked', 'expired')),
                last_activity TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);"
        ),
        // Migration 6: Break glass log for emergency access audit trail
        M::up(
            "CREATE TABLE IF NOT EXISTS break_glass_log (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                reason TEXT NOT NULL,
                patient_id TEXT,
                activated_at TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT,
                deactivated_at TEXT,
                actions_taken TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_break_glass_user_id ON break_glass_log(user_id);
            CREATE INDEX IF NOT EXISTS idx_break_glass_activated ON break_glass_log(activated_at);"
        ),
        // Migration 7: App settings with default values
        M::up(
            "CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('session_timeout_minutes', '15');
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('max_failed_logins', '5');
            INSERT OR IGNORE INTO app_settings (key, value) VALUES ('lockout_duration_minutes', '30');"
        ),
    ])
});

pub fn run(db: &Database) -> Result<(), AppError> {
    let mut conn = db.conn.lock().map_err(|e| {
        AppError::Database(e.to_string())
    })?;
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
