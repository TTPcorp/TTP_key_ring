//! Tauri commands: thin wrappers around [`AppState`].
//!
//! Commands that run Argon2 are `async` so they execute off the main
//! thread and never freeze the window.

use secrecy::SecretString;
use tauri::State;

use crate::state::{AppError, AppState, EntrySummary, VaultStatus};

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, AppError> {
    state.status()
}

#[tauri::command]
pub async fn create_vault(
    state: State<'_, AppState>,
    password: SecretString,
) -> Result<(), AppError> {
    state.create(&password)
}

#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    password: SecretString,
) -> Result<(), AppError> {
    state.unlock(&password)
}

#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> Result<(), AppError> {
    state.lock()
}

#[tauri::command]
pub fn list_entries(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<EntrySummary>, AppError> {
    state.list(&query)
}
