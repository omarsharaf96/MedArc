use rusqlite::Connection;
use std::sync::Mutex;

use crate::error::AppError;

pub struct Database {
    pub conn: Mutex<Connection>,
    pub path: String,
}

impl Database {
    pub fn open(db_path: &str, key: &str) -> Result<Self, AppError> {
        let conn = Connection::open(db_path)?;

        // PRAGMA key MUST be the FIRST statement after opening.
        // Any other query before this will fail silently or corrupt the database.
        conn.pragma_update(None, "key", key)?;

        // Verify encryption is working by reading cipher_version.
        // If the key is wrong, this will fail.
        conn.pragma_query_value(None, "cipher_version", |row| row.get::<_, String>(0))
            .map_err(|_| AppError::Database("Failed to verify encryption -- wrong key or corrupted database".to_string()))?;

        // Enable WAL mode for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Enable foreign keys
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(Database {
            conn: Mutex::new(conn),
            path: db_path.to_string(),
        })
    }
}
