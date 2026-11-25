//! Tauri commands: thin wrappers around [`AppState`].
//!
//! Commands that run Argon2 are `async` so they execute off the main
//! thread and never freeze the window.

use llavero_core::strength::StrengthEstimate;
use secrecy::SecretString;
use tauri::State;

use crate::state::{
    self, AppError, AppState, CopyField, EntryDetails, EntryForm, EntrySummary, GeneratedPassword,
    GeneratorForm, Revealed, SecretField, StrengthContext, VaultStatus, WebAddressInfo,
};

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

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: String) -> Result<EntryDetails, AppError> {
    state.get(&id)
}

/// Sends one secret to the frontend. The UI only calls it when the user
/// presses "Mostrar" on that field.
#[tauri::command]
pub fn reveal_secret(
    state: State<'_, AppState>,
    id: String,
    field: SecretField,
) -> Result<Revealed, AppError> {
    state.reveal(&id, field)
}

#[tauri::command]
pub fn add_entry(state: State<'_, AppState>, entry: EntryForm) -> Result<String, AppError> {
    state.add(entry)
}

#[tauri::command]
pub fn update_entry(
    state: State<'_, AppState>,
    id: String,
    entry: EntryForm,
) -> Result<(), AppError> {
    state.update(&id, entry)
}

/// Asks for the master password: runs Argon2, so it is `async`.
#[tauri::command]
pub async fn delete_entry(
    state: State<'_, AppState>,
    id: String,
    password: SecretString,
) -> Result<(), AppError> {
    state.delete(&id, &password)
}

#[tauri::command]
pub fn generate_password(options: GeneratorForm) -> Result<GeneratedPassword, AppError> {
    state::generate_password(options)
}

#[tauri::command]
pub fn check_url(input: String) -> Result<WebAddressInfo, AppError> {
    state::check_url(&input)
}

/// Strength of the password being typed. `async` so that the analysis
/// never blocks the window while typing.
#[tauri::command]
pub async fn estimate_strength(
    password: SecretString,
    context: StrengthContext,
) -> Result<StrengthEstimate, AppError> {
    Ok(state::estimate_strength(&password, &context))
}

/// Copies a user name or password to the clipboard; the value never
/// reaches the frontend. Returns the seconds until it is cleared.
#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, AppState>,
    id: String,
    field: CopyField,
) -> Result<u64, AppError> {
    state.copy(&id, field)
}
