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
