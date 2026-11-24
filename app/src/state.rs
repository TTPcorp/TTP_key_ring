//! Application state and the logic behind each command, independent of
//! Tauri so it can be unit-tested.

use std::path::PathBuf;
use std::sync::Mutex;

use llavero_core::crypto::KdfParams;
use llavero_core::vault::{Vault, VaultError};
use secrecy::SecretString;
use serde::Serialize;

/// Error sent to the frontend: only a stable code, never secrets or paths.
/// The UI translates codes into user-facing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "code")]
pub enum AppError {
    NotAVault,
    UnsupportedVersion,
    Damaged,
    WrongPasswordOrCorrupted,
    UnsupportedSchema,
    EmptyPassword,
    AlreadyExists,
    Locked,
    Io,
    Internal,
}

impl From<VaultError> for AppError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::NotAVault => Self::NotAVault,
            VaultError::UnsupportedVersion => Self::UnsupportedVersion,
            VaultError::InvalidHeader
            | VaultError::InvalidKdfParams
            | VaultError::InvalidPayload => Self::Damaged,
            VaultError::WrongPasswordOrCorrupted => Self::WrongPasswordOrCorrupted,
            VaultError::UnsupportedSchema => Self::UnsupportedSchema,
            VaultError::EmptyPassword => Self::EmptyPassword,
            VaultError::AlreadyExists => Self::AlreadyExists,
            VaultError::Io(_) => Self::Io,
            VaultError::TooLarge
            | VaultError::InvalidPath
            | VaultError::Crypto(_)
            | VaultError::Model(_) => Self::Internal,
        }
    }
}

/// Whether a vault file exists and whether it is unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
}

/// Non-secret view of an entry for the list. Passwords and notes are
/// never sent to the frontend by this command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntrySummary {
    pub id: String,
    pub name: String,
    pub username: String,
    pub urls: Vec<String>,
}

/// Folder name under the user data directory. Debug builds use a separate
/// folder so development never touches the real vault.
const DATA_FOLDER: &str = if cfg!(debug_assertions) {
    "LLavero Digital (desarrollo)"
} else {
    "LLavero Digital"
};

/// Default vault location: `<data dir>/LLavero Digital/llavero.vault`
/// (`%APPDATA%\LLavero Digital\llavero.vault` on Windows).
pub fn default_vault_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(DATA_FOLDER).join("llavero.vault"))
}

/// Shared state: the vault path and the unlocked vault, if any.
/// Locking drops the [`Vault`], which wipes the key and secrets.
pub struct AppState {
    path: PathBuf,
    kdf: KdfParams,
    vault: Mutex<Option<Vault>>,
}

impl AppState {
    /// State for the vault at `path`, using the default KDF parameters
    /// for new vaults.
    pub fn new(path: PathBuf) -> Self {
        Self::with_kdf(path, KdfParams::DEFAULT)
    }

    fn with_kdf(path: PathBuf, kdf: KdfParams) -> Self {
        Self {
            path,
            kdf,
            vault: Mutex::new(None),
        }
    }

    fn slot(&self) -> Result<std::sync::MutexGuard<'_, Option<Vault>>, AppError> {
        self.vault.lock().map_err(|_| AppError::Internal)
    }

    pub fn status(&self) -> Result<VaultStatus, AppError> {
        Ok(VaultStatus {
            exists: self.path.try_exists().map_err(|_| AppError::Io)?,
            unlocked: self.slot()?.is_some(),
        })
    }

    /// Creates a new vault at the default location and leaves it unlocked.
    pub fn create(&self, password: &SecretString) -> Result<(), AppError> {
        let mut slot = self.slot()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AppError::Io)?;
        }
        *slot = Some(Vault::create(&self.path, password, self.kdf)?);
        Ok(())
    }

    pub fn unlock(&self, password: &SecretString) -> Result<(), AppError> {
        let mut slot = self.slot()?;
        *slot = Some(Vault::open(&self.path, password)?);
        Ok(())
    }

    pub fn lock(&self) -> Result<(), AppError> {
        *self.slot()? = None;
        Ok(())
    }

    pub fn list(&self, query: &str) -> Result<Vec<EntrySummary>, AppError> {
        let slot = self.slot()?;
        let vault = slot.as_ref().ok_or(AppError::Locked)?;
        Ok(vault
            .data()
            .search(query)
            .map(|e| EntrySummary {
                id: e.id().to_string(),
                name: e.name().to_owned(),
                username: e.username().to_owned(),
                urls: e.urls().to_vec(),
            })
            .collect())
    }

    /// Test helper: run `f` on the unlocked vault.
    #[cfg(test)]
    fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> T) -> Option<T> {
        self.slot().ok()?.as_mut().map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llavero_core::model::EntryInput;
    use std::path::Path;

    fn pw(s: &str) -> SecretString {
        SecretString::from(s)
    }

    fn state(dir: &Path) -> AppState {
        let fast = KdfParams::new(KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST, 1).unwrap();
        AppState::with_kdf(dir.join("sub").join("llavero.vault"), fast)
    }

    #[test]
    fn full_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path());
        assert_eq!(
            s.status().unwrap(),
            VaultStatus {
                exists: false,
                unlocked: false
            }
        );
        assert_eq!(s.list("").unwrap_err(), AppError::Locked);

        s.create(&pw("a long master password")).unwrap();
        assert_eq!(
            s.status().unwrap(),
            VaultStatus {
                exists: true,
                unlocked: true
            }
        );

        s.with_vault(|v| {
            let mut e = EntryInput::new("Correo", "yo@example.com", pw("hidden-secret"));
            e.urls = vec!["https://mail.example.com".into()];
            v.data_mut().add_entry(e).unwrap();
            v.save().unwrap();
        })
        .unwrap();

        s.lock().unwrap();
        assert_eq!(
            s.status().unwrap(),
            VaultStatus {
                exists: true,
                unlocked: false
            }
        );
        assert_eq!(
            s.unlock(&pw("wrong")).unwrap_err(),
            AppError::WrongPasswordOrCorrupted
        );
        s.unlock(&pw("a long master password")).unwrap();

        let list = s.list("correo").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Correo");
        assert_eq!(list[0].urls, ["https://mail.example.com"]);
        assert!(s.list("nothing").unwrap().is_empty());
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path());
        s.create(&pw("first password")).unwrap();
        s.lock().unwrap();
        assert_eq!(
            s.create(&pw("second password")).unwrap_err(),
            AppError::AlreadyExists
        );
        assert!(s.unlock(&pw("first password")).is_ok());
    }

    #[test]
    fn empty_password_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            state(dir.path()).create(&pw("")).unwrap_err(),
            AppError::EmptyPassword
        );
    }

    #[test]
    fn list_never_contains_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path());
        s.create(&pw("master password!")).unwrap();
        s.with_vault(|v| {
            let mut e = EntryInput::new("n", "u", pw("password-value-xyz"));
            e.notes = pw("notes-value-xyz");
            v.data_mut().add_entry(e).unwrap();
        })
        .unwrap();
        let json = serde_json::to_string(&s.list("").unwrap()).unwrap();
        assert!(!json.contains("password-value-xyz"));
        assert!(!json.contains("notes-value-xyz"));
    }

    #[test]
    fn errors_serialize_as_codes_only() {
        let json = serde_json::to_string(&AppError::WrongPasswordOrCorrupted).unwrap();
        assert_eq!(json, r#"{"code":"WrongPasswordOrCorrupted"}"#);
    }

    #[test]
    fn default_path_is_under_the_data_dir() {
        if let (Some(path), Some(data)) = (default_vault_path(), dirs::data_dir()) {
            assert!(path.starts_with(data));
            assert!(path.ends_with(Path::new(DATA_FOLDER).join("llavero.vault")));
            // Tests run as debug builds: never the production folder.
            assert_eq!(DATA_FOLDER, "LLavero Digital (desarrollo)");
        }
    }
}
