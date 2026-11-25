//! Tauri build script.
//!
//! The app manifest lists every command the frontend may call. With it,
//! commands are denied unless a capability (see `capabilities/`) grants
//! them explicitly: least privilege for the IPC surface.

fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "vault_status",
            "create_vault",
            "unlock_vault",
            "lock_vault",
            "list_entries",
            "get_entry",
            "reveal_secret",
            "add_entry",
            "update_entry",
            "delete_entry",
            "generate_password",
            "check_url",
        ]));
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("tauri build script failed: {error:#}");
        std::process::exit(1);
    }
}
