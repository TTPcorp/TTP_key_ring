//! LLavero Digital desktop application (Tauri 2).

// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autotype;
mod commands;
mod state;

use autotype::AutoTypeFlow;
use llavero_platform::{Hotkey, HotkeyError, SystemAutoType};
use state::AppState;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Manager;

fn main() {
    let Some(vault_path) = state::default_vault_path() else {
        eprintln!("LLavero Digital: could not determine the user data directory");
        std::process::exit(1);
    };

    let result = tauri::Builder::default()
        .manage(AppState::new(vault_path))
        .manage(AutoTypeFlow::new(
            Arc::new(SystemAutoType::new()),
            Hotkey::DEFAULT.to_string(),
        ))
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
            commands::user_activity,
            commands::autotype_notice,
            commands::autotype_choose,
            commands::autotype_cancel,
        ])
        .setup(|app| {
            // Auto-lock watcher: every 2 s, lock the vault if the Windows
            // session is locked or the idle timeout has passed.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    let session_locked = llavero_platform::is_session_locked().unwrap_or(false);
                    handle
                        .state::<AppState>()
                        .auto_lock(Instant::now(), session_locked);
                }
            });
            start_autotype(app);
            Ok(())
        })
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

/// Keeps the global shortcut registered for the lifetime of the app.
struct HotkeyGuard(#[allow(dead_code)] llavero_platform::HotkeyRegistration);

/// Registers the auto-type shortcut. Its listener only queues the press;
/// a worker thread runs the flow and shows the window when there is
/// something to tell the user.
fn start_autotype(app: &mut tauri::App) {
    let (presses, pressed) = std::sync::mpsc::channel::<()>();
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        while pressed.recv().is_ok() {
            let show = handle
                .state::<AutoTypeFlow>()
                .on_hotkey(&handle.state::<AppState>());
            if show && let Some(window) = handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    });
    match llavero_platform::register_hotkey(Hotkey::DEFAULT, move || {
        let _ = presses.send(());
    }) {
        Ok(registration) => {
            app.manage(HotkeyGuard(registration));
        }
        // Not available on this OS yet: auto-type is simply absent.
        Err(HotkeyError::Unsupported) => {}
        Err(HotkeyError::AlreadyInUse | HotkeyError::Invalid | HotkeyError::Failed) => {
            app.state::<AutoTypeFlow>().hotkey_unavailable();
        }
    }
}
