//! LLavero Digital desktop application (Tauri 2).

// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;

fn main() {
    let Some(vault_path) = state::default_vault_path() else {
        eprintln!("LLavero Digital: could not determine the user data directory");
        std::process::exit(1);
    };

    let result = tauri::Builder::default()
        .manage(AppState::new(vault_path))
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::list_entries,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("LLavero Digital failed to start: {error}");
        std::process::exit(1);
    }
}
