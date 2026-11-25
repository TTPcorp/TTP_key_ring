//! LLavero Digital desktop application (Tauri 2).

// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;
use tauri::Manager;

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
            commands::get_entry,
            commands::reveal_secret,
            commands::add_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::generate_password,
            commands::check_url,
            commands::estimate_strength,
            commands::copy_to_clipboard,
        ])
        .build(tauri::generate_context!());

    let app = match result {
        Ok(app) => app,
        Err(error) => {
            eprintln!("LLavero Digital failed to start: {error}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        // Never leave a copied password behind when the app closes.
        if let tauri::RunEvent::Exit = event {
            handle.state::<AppState>().clear_clipboard_now();
        }
    });
}
