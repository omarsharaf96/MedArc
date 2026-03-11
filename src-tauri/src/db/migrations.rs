use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

use crate::db::connection::Database;
use crate::error::AppError;

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE IF NOT EXISTS app_metadata (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('schema_version', '1');
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('created_at', datetime('now'));"
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
