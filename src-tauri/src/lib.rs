mod audit;
mod auth;
mod commands;
mod db;
mod device_id;
mod error;
mod keychain;
mod rbac;

use auth::session::SessionManager;
use db::connection::Database;
use device_id::DeviceId;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            use tauri::Manager;

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("medarc.db");
            let key = keychain::get_or_create_db_key()?;

            let database = Database::open(db_path.to_str().expect("Invalid DB path"), &key)?;

            db::migrations::run(&database)?;

            let timeout: u32 = {
                let conn = database
                    .conn
                    .lock()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
            // Hardware-derived machine UID — stable across reboots, logged at startup.
            app.manage(DeviceId::from_machine_uid());
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
            commands::auth::complete_login,
            commands::auth::check_first_run,
            commands::session::lock_session,
            commands::session::unlock_session,
            commands::session::refresh_session,
            commands::session::get_session_state,
            commands::session::get_session_timeout,
            commands::break_glass::activate_break_glass,
            commands::break_glass::deactivate_break_glass,
            commands::mfa::setup_totp,
            commands::mfa::verify_totp_setup,
            commands::mfa::disable_totp,
            commands::mfa::check_totp,
            commands::mfa::check_biometric,
            commands::mfa::enable_touch_id,
            commands::mfa::disable_touch_id,
            commands::audit::get_audit_log,
            commands::audit::verify_audit_chain_cmd,
            commands::patient::create_patient,
            commands::patient::get_patient,
            commands::patient::update_patient,
            commands::patient::search_patients,
            commands::patient::delete_patient,
            commands::patient::upsert_care_team,
            commands::patient::get_care_team,
            commands::patient::add_related_person,
            commands::patient::list_related_persons,
            // S05 — Clinical Patient Data
            commands::clinical::add_allergy,
            commands::clinical::list_allergies,
            commands::clinical::update_allergy,
            commands::clinical::delete_allergy,
            commands::clinical::add_problem,
            commands::clinical::list_problems,
            commands::clinical::update_problem,
            commands::clinical::add_medication,
            commands::clinical::list_medications,
            commands::clinical::update_medication,
            commands::clinical::add_immunization,
            commands::clinical::list_immunizations,
            // S06 — Scheduling
            commands::scheduling::create_appointment,
            commands::scheduling::list_appointments,
            commands::scheduling::update_appointment,
            commands::scheduling::cancel_appointment,
            commands::scheduling::search_open_slots,
            commands::scheduling::update_flow_status,
            commands::scheduling::get_flow_board,
            commands::scheduling::add_to_waitlist,
            commands::scheduling::list_waitlist,
            commands::scheduling::discharge_waitlist,
            commands::scheduling::create_recall,
            commands::scheduling::list_recalls,
            commands::scheduling::complete_recall,
            // S07 — Clinical Documentation
            commands::documentation::create_encounter,
            commands::documentation::get_encounter,
            commands::documentation::list_encounters,
            commands::documentation::update_encounter,
            commands::documentation::record_vitals,
            commands::documentation::list_vitals,
            commands::documentation::save_ros,
            commands::documentation::get_ros,
            commands::documentation::save_physical_exam,
            commands::documentation::get_physical_exam,
            commands::documentation::list_templates,
            commands::documentation::get_template,
            commands::documentation::request_cosign,
            commands::documentation::approve_cosign,
            commands::documentation::list_pending_cosigns,
            commands::documentation::check_drug_allergy_alerts,
            // S08 — Lab Results & Document Management
            commands::labs::add_lab_catalogue_entry,
            commands::labs::list_lab_catalogue,
            commands::labs::create_lab_order,
            commands::labs::list_lab_orders,
            commands::labs::enter_lab_result,
            commands::labs::list_lab_results,
            commands::labs::sign_lab_result,
            commands::labs::upload_document,
            commands::labs::list_documents,
            commands::labs::verify_document_integrity,
            // S09 — Backup, Distribution & Release
            commands::backup::create_backup,
            commands::backup::restore_backup,
            commands::backup::list_backups,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
