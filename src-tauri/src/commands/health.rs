use serde::Serialize;
use tauri::State;

use crate::db::connection::Database;

#[derive(Serialize)]
pub struct DbStatus {
    pub encrypted: bool,
    pub cipher_version: String,
    pub page_count: i64,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub db_path: String,
}

#[tauri::command]
pub fn check_db(db: State<'_, Database>) -> Result<DbStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let cipher_version: String = conn
        .pragma_query_value(None, "cipher_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let page_count: i64 = conn
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|e| e.to_string())?;

    Ok(DbStatus {
        encrypted: true,
        cipher_version,
        page_count,
    })
}

#[tauri::command]
pub fn get_app_info(db: State<'_, Database>) -> Result<AppInfo, String> {
    let db_path = db.path.clone();

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_path,
    })
}
