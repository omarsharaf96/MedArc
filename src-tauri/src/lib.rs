mod audit;
mod auth;
mod commands;
mod db;
mod device_id;
mod error;
mod keychain;
mod rbac;

use auth::session::SessionManager;
use commands::audio_capture::AudioRecordingState;
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
            app.manage(AudioRecordingState::new());

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
            commands::mfa::biometric_authenticate,
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
            // M003/S01 — PT Note Templates
            commands::pt_notes::create_pt_note,
            commands::pt_notes::get_pt_note,
            commands::pt_notes::list_pt_notes,
            commands::pt_notes::update_pt_note,
            commands::pt_notes::cosign_pt_note,
            commands::pt_notes::lock_pt_note,
            // M003/S02 — Objective Measures & Outcome Scores
            commands::objective_measures::record_objective_measures,
            commands::objective_measures::get_objective_measures,
            commands::objective_measures::record_outcome_score,
            commands::objective_measures::list_outcome_scores,
            commands::objective_measures::get_outcome_score,
            commands::objective_measures::get_outcome_comparison,
            // M003/S04 — Document Center, Intake Surveys, Referral Tracking
            commands::document_center::upload_categorized_document,
            commands::document_center::list_patient_documents,
            commands::document_center::get_document,
            commands::document_center::update_document_category,
            commands::document_center::delete_document,
            commands::document_center::create_survey_template,
            commands::document_center::list_survey_templates,
            commands::document_center::get_survey_template,
            commands::document_center::submit_survey_response,
            commands::document_center::list_survey_responses,
            commands::document_center::get_survey_response,
            commands::document_center::create_referral,
            commands::document_center::get_referral,
            commands::document_center::list_referrals,
            commands::document_center::update_referral,
            // M003/S03 — Audio Capture & Transcription
            commands::audio_capture::start_audio_recording,
            commands::audio_capture::stop_audio_recording,
            commands::audio_capture::get_audio_level,
            commands::audio_capture::check_microphone_available,
            commands::transcription::transcribe_audio,
            commands::transcription::check_whisper_model,
            commands::transcription::download_whisper_model,
            // M003/S03 — LLM Integration (Ollama + Bedrock)
            commands::llm_integration::check_ollama_status,
            commands::llm_integration::generate_note_draft,
            commands::llm_integration::suggest_cpt_codes,
            commands::llm_integration::extract_objective_data,
            commands::llm_integration::configure_llm_settings,
            // M003/S06 — Fax Integration (Phaxio)
            commands::fax_integration::configure_phaxio,
            commands::fax_integration::test_phaxio_connection,
            commands::fax_integration::send_fax,
            commands::fax_integration::poll_received_faxes,
            commands::fax_integration::create_fax_contact,
            commands::fax_integration::list_fax_contacts,
            commands::fax_integration::update_fax_contact,
            commands::fax_integration::delete_fax_contact,
            commands::fax_integration::list_fax_log,
            commands::fax_integration::get_fax_status,
            commands::fax_integration::retry_fax,
            // M003/S05 — PDF Export & Report Generation
            commands::pdf_export::generate_note_pdf,
            commands::pdf_export::generate_progress_report,
            commands::pdf_export::generate_insurance_narrative,
            commands::pdf_export::generate_legal_report,
            commands::pdf_export::generate_chart_export,
            // M003/S07 — Authorization & Visit Tracking
            commands::auth_tracking::create_auth_record,
            commands::auth_tracking::get_auth_record,
            commands::auth_tracking::list_auth_records,
            commands::auth_tracking::update_auth_record,
            commands::auth_tracking::increment_visit_count,
            commands::auth_tracking::get_auth_alerts,
            commands::auth_tracking::generate_reauth_letter,
            // M003/S02 — HEP Builder
            commands::hep::list_exercises,
            commands::hep::search_exercises,
            commands::hep::create_hep_program,
            commands::hep::get_hep_program,
            commands::hep::list_hep_programs,
            commands::hep::update_hep_program,
            commands::hep::create_hep_template,
            commands::hep::list_hep_templates,
            commands::hep::get_hep_template,
            // M004/S01 — CPT Billing Engine
            commands::billing::list_cpt_codes,
            commands::billing::calculate_billing_units,
            commands::billing::create_fee_schedule_entry,
            commands::billing::list_fee_schedule,
            commands::billing::get_encounter_billing_summary,
            commands::billing::save_encounter_billing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
