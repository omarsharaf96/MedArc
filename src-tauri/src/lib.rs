mod commands;
mod db;
mod error;
mod keychain;

// Placeholder -- will be completed in Task 2
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
