mod auth;
mod commands;
mod db;
mod error;
mod keychain;
mod rbac;

use auth::session::SessionManager;
use db::connection::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("medarc.db");
            let key = keychain::get_or_create_db_key()?;

            let database = Database::open(
                db_path.to_str().expect("Invalid DB path"),
                &key,
            )?;

            db::migrations::run(&database)?;

            let timeout: u32 = {
                let conn = database.conn.lock().map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                })?;
                conn.query_row(
                    "SELECT value FROM app_settings WHERE key = 'session_timeout_minutes'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_else(|_| "15".to_string())
                .parse()
                .unwrap_or(15)
            };

            let session_manager = SessionManager::new(timeout);
            app.manage(session_manager);
            app.manage(database);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health::check_db,
            commands::health::get_app_info,
            commands::fhir::create_resource,
            commands::fhir::get_resource,
            commands::fhir::list_resources,
            commands::fhir::update_resource,
            commands::fhir::delete_resource,
            commands::auth::register_user,
            commands::auth::login,
            commands::auth::logout,
            commands::session::lock_session,
            commands::session::unlock_session,
            commands::session::refresh_session,
            commands::session::get_session_state,
            commands::session::get_session_timeout,
            commands::break_glass::activate_break_glass,
            commands::break_glass::deactivate_break_glass,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
