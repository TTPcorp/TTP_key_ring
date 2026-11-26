//! Application state and the logic behind each command, independent of
//! Tauri so it can be unit-tested.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use llavero_core::autotype::{Sequence, SequenceError};
use llavero_core::crypto::KdfParams;
use llavero_core::generator::{self, GeneratorError, PasswordOptions};
use llavero_core::model::{Entry, EntryInput, ModelError, VaultData, WindowMatch};
use llavero_core::strength::{self, StrengthEstimate};
use llavero_core::vault::{Vault, VaultError};
use llavero_core::web::{self, WebAddressError, WebWarning};
use llavero_platform::{Clipboard, SystemClipboard, clear_if_unchanged};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use zeroize::Zeroizing;

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
    EmptyName,
    EntryNotFound,
    InvalidGeneratorOptions,
    NotAWebAddress,
    UnsupportedScheme,
    EmbeddedCredentials,
    ClipboardUnavailable,
    NothingToCopy,
    AutoTypeSequenceInvalid,
    AutoTypeExpired,
    AutoTypeWindowChanged,
    AutoTypeModifierPressed,
    AutoTypeBlocked,
    AutoTypeUnsupported,
    Io,
    Internal,
}

impl From<ModelError> for AppError {
    fn from(e: ModelError) -> Self {
        match e {
            ModelError::EmptyName => Self::EmptyName,
            ModelError::NotFound => Self::EntryNotFound,
            ModelError::Crypto(_) => Self::Internal,
        }
    }
}

impl From<WebAddressError> for AppError {
    fn from(e: WebAddressError) -> Self {
        match e {
            WebAddressError::NotAWebAddress => Self::NotAWebAddress,
            WebAddressError::UnsupportedScheme => Self::UnsupportedScheme,
            WebAddressError::EmbeddedCredentials => Self::EmbeddedCredentials,
        }
    }
}

impl From<GeneratorError> for AppError {
    fn from(e: GeneratorError) -> Self {
        match e {
            GeneratorError::InvalidLength
            | GeneratorError::NoCharacterClass
            | GeneratorError::TooShortForClasses => Self::InvalidGeneratorOptions,
            GeneratorError::RandomUnavailable => Self::Internal,
        }
    }
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
            VaultError::Model(m) => m.into(),
            VaultError::TooLarge | VaultError::InvalidPath | VaultError::Crypto(_) => {
                Self::Internal
            }
        }
    }
}

/// Why the vault was locked automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LockReason {
    /// No activity for [`IDLE_TIMEOUT`].
    Inactivity,
    /// The Windows session was locked (Win+L).
    SessionLocked,
}

/// Whether a vault file exists, whether it is unlocked and, if it was
/// locked automatically, why (so the UI can say so).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
    pub lock_reason: Option<LockReason>,
}

/// Time without activity after which the vault locks itself.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Non-secret view of an entry for the list. Passwords and notes are
/// never sent to the frontend by this command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntrySummary {
    pub id: String,
    pub name: String,
    pub username: String,
    pub urls: Vec<String>,
}

/// Non-secret view of a single entry, used to fill the edit form.
/// Says whether a password and notes exist, but never their content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetails {
    pub id: String,
    pub name: String,
    pub username: String,
    pub urls: Vec<String>,
    pub has_password: bool,
    pub has_notes: bool,
    pub windows: Vec<WindowForm>,
    /// Custom auto-type sequence; empty means the default one.
    pub autotype: String,
}

/// A program window associated with an entry, as edited in the form.
/// Blank fields are empty strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowForm {
    /// Window title; `*` is a wildcard.
    pub title: String,
    /// Executable name, e.g. `programa.exe`.
    pub process: String,
}

impl From<&WindowMatch> for WindowForm {
    fn from(w: &WindowMatch) -> Self {
        Self {
            title: w.title.clone().unwrap_or_default(),
            process: w.process.clone().unwrap_or_default(),
        }
    }
}

/// Entry form sent by the frontend to create or update an entry.
///
/// `password` and `notes` are `None` when the user did not touch them:
/// on update the stored value is kept; on create they become empty.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryForm {
    pub name: String,
    pub username: String,
    pub password: Option<SecretString>,
    pub urls: Vec<String>,
    pub notes: Option<SecretString>,
    /// Associated program windows (auto-type).
    pub windows: Vec<WindowForm>,
    /// Custom auto-type sequence; blank means the default one.
    pub autotype: String,
}

/// Field the user asked to copy to the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyField {
    Username,
    Password,
}

/// Seconds a copied user name or password stays on the clipboard
/// (within the 15–30 s range of the requirements).
pub const CLIPBOARD_CLEAR_SECONDS: u64 = 20;

/// The last value copied, until it is cleared. `generation` grows with
/// every copy, so a timer only clears the copy that started it.
#[derive(Default)]
struct PendingCopy {
    generation: u64,
    value: Option<Zeroizing<String>>,
}

/// Secret field the user explicitly asked to reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretField {
    Password,
    Notes,
}

/// A secret sent to the frontend only on an explicit user action
/// (reveal, or generate). Stays in a zeroizing type until serialized.
#[derive(Debug)]
pub struct Revealed(SecretString);

impl Serialize for Revealed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.expose_secret())
    }
}

/// Password generator options chosen in the UI.
///
/// With `target_bits` set (64, 128, 256...), the length is the shortest
/// that reaches that entropy and `length` is ignored.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratorForm {
    pub target_bits: Option<u32>,
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl From<GeneratorForm> for PasswordOptions {
    fn from(f: GeneratorForm) -> Self {
        Self {
            length: f.length,
            lowercase: f.lowercase,
            uppercase: f.uppercase,
            digits: f.digits,
            symbols: f.symbols,
            exclude_ambiguous: f.exclude_ambiguous,
            require_each_class: true,
        }
    }
}

/// A freshly generated password and its approximate strength.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedPassword {
    pub password: Revealed,
    pub entropy_bits: f64,
    pub length: usize,
}

/// Generates a password with the given options.
pub fn generate_password(form: GeneratorForm) -> Result<GeneratedPassword, AppError> {
    let mut options = PasswordOptions::from(form);
    if let Some(bits) = form.target_bits {
        options.length = generator::length_for_entropy(&options, bits)?;
    }
    Ok(GeneratedPassword {
        password: Revealed(generator::generate(&options)?),
        entropy_bits: generator::entropy_bits(&options),
        length: options.length,
    })
}

/// Fields of the entry being edited, used to detect passwords that
/// contain the entry's own data (name, user name, site).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrengthContext {
    pub name: String,
    pub username: String,
    pub urls: Vec<String>,
}

/// Parts of the URL or of an e-mail that say nothing about the person.
const CONTEXT_STOPWORDS: &[&str] = &["http", "https", "www", "mail", "login"];

/// Words an attacker targeting this entry would try: the whole fields
/// and their alphanumeric pieces of 4+ characters.
fn context_words(ctx: &StrengthContext) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let fields = [ctx.name.as_str(), ctx.username.as_str()]
        .into_iter()
        .chain(ctx.urls.iter().map(String::as_str));
    for field in fields {
        let field = field.trim().to_lowercase();
        let pieces = field
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_owned)
            .chain(std::iter::once(field.clone()));
        for piece in pieces {
            if piece.chars().count() >= 4
                && !CONTEXT_STOPWORDS.contains(&piece.as_str())
                && !words.contains(&piece)
            {
                words.push(piece);
            }
        }
    }
    words
}

/// Estimates the strength of a password the user is typing.
pub fn estimate_strength(password: &SecretString, ctx: &StrengthContext) -> StrengthEstimate {
    let words = context_words(ctx);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    strength::estimate(password, &words)
}

/// Result of checking one web address, for live feedback in the form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebAddressInfo {
    pub url: String,
    pub host: String,
    pub warnings: Vec<WebWarning>,
}

/// Checks a web address without any network access.
pub fn check_url(input: &str) -> Result<WebAddressInfo, AppError> {
    let checked = web::check_web_address(input)?;
    Ok(WebAddressInfo {
        url: checked.url,
        host: checked.host,
        warnings: checked.warnings,
    })
}

/// Drops blank lines, then checks and normalizes every web address.
/// Any invalid address rejects the whole form; duplicates are removed.
fn clean_urls(urls: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut clean: Vec<String> = Vec::new();
    for raw in urls.iter().filter(|u| !u.trim().is_empty()) {
        let url = web::check_web_address(raw)?.url;
        if !clean.contains(&url) {
            clean.push(url);
        }
    }
    Ok(clean)
}

/// Drops blank associations, trims the fields, keeps only the file name
/// of the executable (folders are ignored when matching) and removes
/// duplicates.
fn clean_windows(windows: Vec<WindowForm>) -> Vec<WindowMatch> {
    let mut clean: Vec<WindowMatch> = Vec::new();
    for w in windows {
        let title = w.title.trim();
        let process = w.process.trim();
        let process = process.rsplit(['\\', '/']).next().unwrap_or(process).trim();
        if title.is_empty() && process.is_empty() {
            continue;
        }
        let m = WindowMatch {
            title: (!title.is_empty()).then(|| title.to_owned()),
            process: (!process.is_empty()).then(|| process.to_owned()),
        };
        if !clean.contains(&m) {
            clean.push(m);
        }
    }
    clean
}

/// A blank sequence means the default one; anything else must be valid.
fn clean_autotype(sequence: String) -> Result<Option<String>, AppError> {
    if sequence.trim().is_empty() {
        return Ok(None);
    }
    Sequence::parse(&sequence).map_err(|_| AppError::AutoTypeSequenceInvalid)?;
    Ok(Some(sequence))
}

/// Why an auto-type sequence is not valid, for live feedback in the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceProblem {
    pub reason: SequenceProblemReason,
    /// Character position of the offending brace, when there is one.
    pub position: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SequenceProblemReason {
    TooLong,
    UnclosedBrace,
    UnexpectedClosingBrace,
    UnknownPlaceholder,
    InvalidDelay,
}

/// Checks a sequence typed in the form. A blank one is valid (it means
/// the default sequence).
pub fn check_autotype_sequence(sequence: &str) -> Option<SequenceProblem> {
    use SequenceProblemReason as R;
    if sequence.trim().is_empty() {
        return None;
    }
    let (reason, position) = match Sequence::parse(sequence).err()? {
        SequenceError::Empty => return None,
        SequenceError::TooLong => (R::TooLong, None),
        SequenceError::UnclosedBrace(p) => (R::UnclosedBrace, Some(p)),
        SequenceError::UnexpectedClosingBrace(p) => (R::UnexpectedClosingBrace, Some(p)),
        SequenceError::UnknownPlaceholder(p) => (R::UnknownPlaceholder, Some(p)),
        SequenceError::InvalidDelay(p) => (R::InvalidDelay, Some(p)),
    };
    Some(SequenceProblem { reason, position })
}

/// Finds an entry by the string id the frontend got from the list.
fn find<'a>(data: &'a VaultData, id: &str) -> Result<&'a Entry, AppError> {
    data.entries()
        .iter()
        .find(|e| e.id().to_string() == id)
        .ok_or(AppError::EntryNotFound)
}

fn copy_secret(s: &SecretString) -> SecretString {
    SecretString::from(s.expose_secret())
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
    clipboard: Arc<dyn Clipboard>,
    clipboard_delay: Duration,
    pending_copy: Arc<Mutex<PendingCopy>>,
    last_activity: Mutex<Instant>,
    lock_reason: Mutex<Option<LockReason>>,
}

impl AppState {
    /// State for the vault at `path`, using the default KDF parameters
    /// for new vaults and the system clipboard.
    pub fn new(path: PathBuf) -> Self {
        Self::with_parts(
            path,
            KdfParams::DEFAULT,
            Arc::new(SystemClipboard::new()),
            Duration::from_secs(CLIPBOARD_CLEAR_SECONDS),
        )
    }

    fn with_parts(
        path: PathBuf,
        kdf: KdfParams,
        clipboard: Arc<dyn Clipboard>,
        clipboard_delay: Duration,
    ) -> Self {
        Self {
            path,
            kdf,
            vault: Mutex::new(None),
            clipboard,
            clipboard_delay,
            pending_copy: Arc::new(Mutex::new(PendingCopy::default())),
            last_activity: Mutex::new(Instant::now()),
            lock_reason: Mutex::new(None),
        }
    }

    fn slot(&self) -> Result<std::sync::MutexGuard<'_, Option<Vault>>, AppError> {
        self.vault.lock().map_err(|_| AppError::Internal)
    }

    pub fn status(&self) -> Result<VaultStatus, AppError> {
        Ok(VaultStatus {
            exists: self.path.try_exists().map_err(|_| AppError::Io)?,
            unlocked: self.slot()?.is_some(),
            lock_reason: *self.lock_reason.lock().map_err(|_| AppError::Internal)?,
        })
    }

    /// Records user activity (keys, clicks, mouse in the window), which
    /// postpones the inactivity lock.
    pub fn touch(&self) {
        if let Ok(mut last) = self.last_activity.lock() {
            *last = Instant::now();
        }
    }

    /// Called periodically: locks the vault if the session is locked or
    /// there has been no activity for [`IDLE_TIMEOUT`]. Returns the reason
    /// if it locked it now.
    pub fn auto_lock(&self, now: Instant, session_locked: bool) -> Option<LockReason> {
        let unlocked = self.slot().map(|s| s.is_some()).unwrap_or(false);
        if !unlocked {
            return None;
        }
        let idle = self
            .last_activity
            .lock()
            .map(|last| now.saturating_duration_since(*last))
            .unwrap_or(Duration::MAX);
        let reason = if session_locked {
            LockReason::SessionLocked
        } else if idle >= IDLE_TIMEOUT {
            LockReason::Inactivity
        } else {
            return None;
        };
        self.lock().ok()?;
        if let Ok(mut r) = self.lock_reason.lock() {
            *r = Some(reason);
        }
        Some(reason)
    }

    /// Fresh start after creating or opening the vault.
    fn opened(&self) {
        self.touch();
        if let Ok(mut r) = self.lock_reason.lock() {
            *r = None;
        }
    }

    /// Creates a new vault at the default location and leaves it unlocked.
    pub fn create(&self, password: &SecretString) -> Result<(), AppError> {
        let mut slot = self.slot()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AppError::Io)?;
        }
        *slot = Some(Vault::create(&self.path, password, self.kdf)?);
        drop(slot);
        self.opened();
        Ok(())
    }

    pub fn unlock(&self, password: &SecretString) -> Result<(), AppError> {
        let mut slot = self.slot()?;
        *slot = Some(Vault::open(&self.path, password)?);
        drop(slot);
        self.opened();
        Ok(())
    }

    /// Locks the vault and removes anything it put on the clipboard.
    pub fn lock(&self) -> Result<(), AppError> {
        *self.slot()? = None;
        self.clear_clipboard_now();
        if let Ok(mut r) = self.lock_reason.lock() {
            *r = None;
        }
        Ok(())
    }

    /// Copies the user name or password of an entry to the clipboard
    /// without sending it to the frontend, and schedules its removal.
    /// Returns the seconds until it is cleared.
    pub fn copy(&self, id: &str, field: CopyField) -> Result<u64, AppError> {
        self.touch();
        let value = {
            let slot = self.slot()?;
            let vault = slot.as_ref().ok_or(AppError::Locked)?;
            let e = find(vault.data(), id)?;
            Zeroizing::new(match field {
                CopyField::Username => e.username().to_owned(),
                CopyField::Password => e.password().expose_secret().to_owned(),
            })
        };
        if value.is_empty() {
            return Err(AppError::NothingToCopy);
        }
        self.clipboard
            .set_secret(&value)
            .map_err(|_| AppError::ClipboardUnavailable)?;

        let generation = {
            let mut pending = self.pending_copy.lock().map_err(|_| AppError::Internal)?;
            pending.generation += 1;
            pending.value = Some(value);
            pending.generation
        };
        let pending = Arc::clone(&self.pending_copy);
        let clipboard = Arc::clone(&self.clipboard);
        let delay = self.clipboard_delay;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if let Ok(mut pending) = pending.lock()
                && pending.generation == generation
                && let Some(value) = pending.value.take()
            {
                // Best effort: if the clipboard is busy there is nothing
                // more to do, and the value is wiped from memory anyway.
                let _ = clear_if_unchanged(clipboard.as_ref(), &value);
            }
        });
        Ok(delay.as_secs())
    }

    /// Clears the last copied value from the clipboard right away (if the
    /// user has not copied anything else since). Used on lock and exit.
    pub fn clear_clipboard_now(&self) {
        if let Ok(mut pending) = self.pending_copy.lock()
            && let Some(value) = pending.value.take()
        {
            let _ = clear_if_unchanged(self.clipboard.as_ref(), &value);
        }
    }

    pub fn list(&self, query: &str) -> Result<Vec<EntrySummary>, AppError> {
        self.touch();
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

    pub fn get(&self, id: &str) -> Result<EntryDetails, AppError> {
        self.touch();
        let slot = self.slot()?;
        let vault = slot.as_ref().ok_or(AppError::Locked)?;
        let e = find(vault.data(), id)?;
        Ok(EntryDetails {
            id: e.id().to_string(),
            name: e.name().to_owned(),
            username: e.username().to_owned(),
            urls: e.urls().to_vec(),
            has_password: !e.password().expose_secret().is_empty(),
            has_notes: !e.notes().expose_secret().is_empty(),
            windows: e.windows().iter().map(WindowForm::from).collect(),
            autotype: e.autotype().unwrap_or_default().to_owned(),
        })
    }

    /// Returns one secret field. Only called on an explicit user action.
    pub fn reveal(&self, id: &str, field: SecretField) -> Result<Revealed, AppError> {
        self.touch();
        let slot = self.slot()?;
        let vault = slot.as_ref().ok_or(AppError::Locked)?;
        let e = find(vault.data(), id)?;
        Ok(Revealed(copy_secret(match field {
            SecretField::Password => e.password(),
            SecretField::Notes => e.notes(),
        })))
    }

    pub fn add(&self, form: EntryForm) -> Result<String, AppError> {
        self.touch();
        self.modify(|data| {
            let mut input = EntryInput::new(
                form.name.trim(),
                form.username.trim(),
                form.password.unwrap_or_else(|| SecretString::from("")),
            );
            input.urls = clean_urls(form.urls)?;
            input.windows = clean_windows(form.windows);
            input.autotype = clean_autotype(form.autotype)?;
            if let Some(notes) = form.notes {
                input.notes = notes;
            }
            Ok(data.add_entry(input)?.to_string())
        })
    }

    /// Updates the editable fields of an entry. Untouched secrets are kept
    /// as they are.
    pub fn update(&self, id: &str, form: EntryForm) -> Result<(), AppError> {
        self.touch();
        self.modify(|data| {
            let current = find(data, id)?;
            let input = EntryInput {
                name: form.name.trim().to_owned(),
                username: form.username.trim().to_owned(),
                password: form
                    .password
                    .unwrap_or_else(|| copy_secret(current.password())),
                urls: clean_urls(form.urls)?,
                windows: clean_windows(form.windows),
                autotype: clean_autotype(form.autotype)?,
                notes: form.notes.unwrap_or_else(|| copy_secret(current.notes())),
            };
            let entry_id = current.id();
            Ok(data.update_entry(entry_id, input)?)
        })
    }

    /// Deletes an entry after checking the master password, so that an
    /// accidental click can never remove anything.
    pub fn delete(&self, id: &str, password: &SecretString) -> Result<(), AppError> {
        self.touch();
        {
            let slot = self.slot()?;
            let vault = slot.as_ref().ok_or(AppError::Locked)?;
            find(vault.data(), id)?;
            vault.verify_password(password)?;
        }
        self.modify(|data| {
            let entry_id = find(data, id)?.id();
            data.remove_entry(entry_id)?;
            Ok(())
        })
    }

    /// Applies a change to the unlocked vault and saves it. If the change
    /// or the save fails, the in-memory content is rolled back so that
    /// what the UI shows always matches the file on disk.
    fn modify<T>(
        &self,
        change: impl FnOnce(&mut VaultData) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut slot = self.slot()?;
        let vault = slot.as_mut().ok_or(AppError::Locked)?;
        let snapshot = vault.data().clone();
        let result = change(vault.data_mut()).and_then(|value| {
            vault.save()?;
            Ok(value)
        });
        if result.is_err() {
            *vault.data_mut() = snapshot;
        }
        result
    }

    /// Runs `f` on the content of the unlocked vault.
    pub fn with_data<T>(&self, f: impl FnOnce(&VaultData) -> T) -> Result<T, AppError> {
        let slot = self.slot()?;
        let vault = slot.as_ref().ok_or(AppError::Locked)?;
        Ok(f(vault.data()))
    }

    /// Test helper: run `f` on the unlocked vault.
    #[cfg(test)]
    pub(crate) fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> T) -> Option<T> {
        self.slot().ok()?.as_mut().map(f)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use llavero_core::model::EntryInput;
    use std::path::Path;

    fn pw(s: &str) -> SecretString {
        SecretString::from(s)
    }

    pub(crate) fn state(dir: &Path) -> AppState {
        let fast = KdfParams::new(KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST, 1).unwrap();
        AppState::with_parts(
            dir.join("sub").join("llavero.vault"),
            fast,
            Arc::new(FakeClipboard::default()),
            Duration::from_secs(3600),
        )
    }

    /// In-memory clipboard.
    #[derive(Default)]
    struct FakeClipboard {
        content: Mutex<Option<String>>,
    }

    impl Clipboard for FakeClipboard {
        fn set_secret(&self, text: &str) -> Result<(), llavero_platform::ClipboardError> {
            *self.content.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
        fn get_text(&self) -> Result<Option<Zeroizing<String>>, llavero_platform::ClipboardError> {
            Ok(self.content.lock().unwrap().clone().map(Zeroizing::new))
        }
        fn clear(&self) -> Result<(), llavero_platform::ClipboardError> {
            *self.content.lock().unwrap() = None;
            Ok(())
        }
    }

    fn with_clipboard(dir: &Path, delay: Duration) -> (AppState, Arc<FakeClipboard>) {
        let fast = KdfParams::new(KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST, 1).unwrap();
        let clipboard = Arc::new(FakeClipboard::default());
        let s = AppState::with_parts(
            dir.join("llavero.vault"),
            fast,
            Arc::clone(&clipboard) as Arc<dyn Clipboard>,
            delay,
        );
        s.create(&pw("master password!")).unwrap();
        (s, clipboard)
    }

    fn clip(c: &FakeClipboard) -> Option<String> {
        c.content.lock().unwrap().clone()
    }

    #[test]
    fn copy_puts_the_value_and_clears_it_later() {
        let dir = tempfile::tempdir().unwrap();
        let (s, c) = with_clipboard(dir.path(), Duration::from_millis(100));
        let id = s.add(form("n", Some("secret-pw"), None)).unwrap();

        assert_eq!(s.copy(&id, CopyField::Password).unwrap(), 0);
        assert_eq!(clip(&c).as_deref(), Some("secret-pw"));
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(clip(&c), None);

        s.copy(&id, CopyField::Username).unwrap();
        assert_eq!(clip(&c).as_deref(), Some("yo@example.com"));
    }

    #[test]
    fn a_newer_copy_is_not_cleared_by_an_older_timer() {
        let dir = tempfile::tempdir().unwrap();
        let (s, c) = with_clipboard(dir.path(), Duration::from_millis(300));
        let id = s.add(form("n", Some("secret-pw"), None)).unwrap();
        s.copy(&id, CopyField::Password).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        s.copy(&id, CopyField::Username).unwrap();
        // The first timer fires here: the newer copy must survive it.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(clip(&c).as_deref(), Some("yo@example.com"));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(clip(&c), None);
    }

    #[test]
    fn what_the_user_copies_afterwards_is_never_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let (s, c) = with_clipboard(dir.path(), Duration::from_millis(100));
        let id = s.add(form("n", Some("secret-pw"), None)).unwrap();
        s.copy(&id, CopyField::Password).unwrap();
        c.set_secret("something the user copied").unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(clip(&c).as_deref(), Some("something the user copied"));
    }

    #[test]
    fn locking_clears_the_clipboard_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (s, c) = with_clipboard(dir.path(), Duration::from_secs(3600));
        let id = s.add(form("n", Some("secret-pw"), None)).unwrap();
        s.copy(&id, CopyField::Password).unwrap();
        s.lock().unwrap();
        assert_eq!(clip(&c), None);
    }

    #[test]
    fn copy_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (s, _c) = with_clipboard(dir.path(), Duration::from_secs(3600));
        let id = s.add(form("n", None, None)).unwrap();
        assert_eq!(
            s.copy(&id, CopyField::Password).unwrap_err(),
            AppError::NothingToCopy
        );
        assert_eq!(
            s.copy("missing", CopyField::Username).unwrap_err(),
            AppError::EntryNotFound
        );
        s.lock().unwrap();
        assert_eq!(
            s.copy(&id, CopyField::Username).unwrap_err(),
            AppError::Locked
        );
    }

    #[test]
    fn locks_after_the_idle_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let start = Instant::now();
        assert_eq!(s.auto_lock(start + IDLE_TIMEOUT / 2, false), None);
        assert!(s.status().unwrap().unlocked);
        assert_eq!(
            s.auto_lock(start + IDLE_TIMEOUT + Duration::from_secs(1), false),
            Some(LockReason::Inactivity)
        );
        let status = s.status().unwrap();
        assert!(!status.unlocked);
        assert_eq!(status.lock_reason, Some(LockReason::Inactivity));
        // Already locked: nothing more to do.
        assert_eq!(s.auto_lock(start + IDLE_TIMEOUT * 3, true), None);
        // Opening again clears the reason.
        s.unlock(&pw("master password!")).unwrap();
        assert_eq!(s.status().unwrap().lock_reason, None);
    }

    #[test]
    fn activity_postpones_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        std::thread::sleep(Duration::from_millis(20));
        let before_touch = Instant::now();
        s.touch();
        // Measured from the last activity, not from the unlock.
        assert_eq!(
            s.auto_lock(
                before_touch + IDLE_TIMEOUT - Duration::from_millis(1),
                false
            ),
            None
        );
        // Using the vault also counts as activity.
        let before_list = Instant::now();
        s.list("").unwrap();
        assert_eq!(
            s.auto_lock(before_list + IDLE_TIMEOUT - Duration::from_millis(1), false),
            None
        );
        assert!(s.status().unwrap().unlocked);
    }

    #[test]
    fn locks_at_once_when_the_session_is_locked() {
        let dir = tempfile::tempdir().unwrap();
        let (s, c) = with_clipboard(dir.path(), Duration::from_secs(3600));
        let id = s.add(form("n", Some("secret-pw"), None)).unwrap();
        s.copy(&id, CopyField::Password).unwrap();
        assert_eq!(
            s.auto_lock(Instant::now(), true),
            Some(LockReason::SessionLocked)
        );
        assert!(!s.status().unwrap().unlocked);
        // Locking also cleared the copied password.
        assert_eq!(clip(&c), None);
    }

    #[test]
    fn manual_lock_has_no_reason() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        s.auto_lock(Instant::now(), true);
        s.unlock(&pw("master password!")).unwrap();
        s.lock().unwrap();
        assert_eq!(s.status().unwrap().lock_reason, None);
        let json = serde_json::to_string(&LockReason::SessionLocked).unwrap();
        assert_eq!(json, r#""sessionLocked""#);
    }

    #[test]
    fn full_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path());
        assert_eq!(
            s.status().unwrap(),
            VaultStatus {
                exists: false,
                unlocked: false,
                lock_reason: None
            }
        );
        assert_eq!(s.list("").unwrap_err(), AppError::Locked);

        s.create(&pw("a long master password")).unwrap();
        assert_eq!(
            s.status().unwrap(),
            VaultStatus {
                exists: true,
                unlocked: true,
                lock_reason: None
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
                unlocked: false,
                lock_reason: None
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

    fn form(name: &str, password: Option<&str>, notes: Option<&str>) -> EntryForm {
        EntryForm {
            name: name.into(),
            username: " yo@example.com ".into(),
            password: password.map(pw),
            urls: vec![
                " https://A.example.com ".into(),
                "   ".into(),
                "a.example.com".into(),
            ],
            notes: notes.map(pw),
            windows: vec![],
            autotype: String::new(),
        }
    }

    fn unlocked(dir: &Path) -> AppState {
        let s = state(dir);
        s.create(&pw("master password!")).unwrap();
        s
    }

    fn revealed(s: &AppState, id: &str, field: SecretField) -> String {
        s.reveal(id, field).unwrap().0.expose_secret().to_owned()
    }

    #[test]
    fn add_get_update_delete_persist_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());

        let id = s
            .add(form(" Correo ", Some("secret-1"), Some("note-1")))
            .unwrap();
        let details = s.get(&id).unwrap();
        assert_eq!(
            details,
            EntryDetails {
                id: id.clone(),
                name: "Correo".into(),
                username: "yo@example.com".into(),
                urls: vec!["https://a.example.com/".into()],
                has_password: true,
                has_notes: true,
                windows: vec![],
                autotype: String::new(),
            }
        );

        // Untouched secrets are kept on update.
        s.update(&id, form("Correo 2", None, None)).unwrap();
        assert_eq!(s.get(&id).unwrap().name, "Correo 2");
        assert_eq!(revealed(&s, &id, SecretField::Password), "secret-1");
        assert_eq!(revealed(&s, &id, SecretField::Notes), "note-1");

        // Touched secrets are replaced, including clearing the notes.
        s.update(&id, form("Correo 2", Some("secret-2"), Some("")))
            .unwrap();
        assert_eq!(revealed(&s, &id, SecretField::Password), "secret-2");
        assert!(!s.get(&id).unwrap().has_notes);

        // Every change is already on disk.
        s.lock().unwrap();
        s.unlock(&pw("master password!")).unwrap();
        assert_eq!(s.get(&id).unwrap().name, "Correo 2");
        assert_eq!(revealed(&s, &id, SecretField::Password), "secret-2");

        assert_eq!(
            s.delete(&id, &pw("wrong")).unwrap_err(),
            AppError::WrongPasswordOrCorrupted
        );
        assert!(s.get(&id).is_ok());
        s.delete(&id, &pw("master password!")).unwrap();
        assert_eq!(s.get(&id).unwrap_err(), AppError::EntryNotFound);
        s.lock().unwrap();
        s.unlock(&pw("master password!")).unwrap();
        assert!(s.list("").unwrap().is_empty());
    }

    #[test]
    fn add_without_secrets_stores_empty_ones() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let id = s.add(form("n", None, None)).unwrap();
        let d = s.get(&id).unwrap();
        assert!(!d.has_password);
        assert!(!d.has_notes);
    }

    #[test]
    fn update_round_trips_autotype_fields() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let id = s
            .with_vault(|v| {
                let mut e = EntryInput::new("n", "u", pw("p"));
                e.autotype = Some("{USERNAME}{TAB}{PASSWORD}{ENTER}".into());
                e.windows = vec![llavero_core::model::WindowMatch {
                    title: Some("Login".into()),
                    process: Some("app.exe".into()),
                }];
                v.data_mut().add_entry(e).unwrap().to_string()
            })
            .unwrap();
        // The form sends back what `get` returned.
        let details = s.get(&id).unwrap();
        let mut f = form("renamed", None, None);
        f.windows = details.windows;
        f.autotype = details.autotype;
        s.update(&id, f).unwrap();
        s.with_vault(|v| {
            let e = &v.data().entries()[0];
            assert_eq!(e.autotype(), Some("{USERNAME}{TAB}{PASSWORD}{ENTER}"));
            assert_eq!(e.windows().len(), 1);
        })
        .unwrap();
    }

    #[test]
    fn invalid_changes_are_rejected_and_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        assert_eq!(
            s.add(form("   ", None, None)).unwrap_err(),
            AppError::EmptyName
        );
        let id = s.add(form("keep", None, None)).unwrap();
        assert_eq!(
            s.update(&id, form("", None, None)).unwrap_err(),
            AppError::EmptyName
        );
        assert_eq!(s.get(&id).unwrap().name, "keep");
        for missing in ["not-an-id", "00000000-0000-0000-0000-000000000000"] {
            assert_eq!(s.get(missing).unwrap_err(), AppError::EntryNotFound);
            assert_eq!(
                s.delete(missing, &pw("master password!")).unwrap_err(),
                AppError::EntryNotFound
            );
            assert_eq!(
                s.update(missing, form("x", None, None)).unwrap_err(),
                AppError::EntryNotFound
            );
        }
        assert_eq!(s.list("").unwrap().len(), 1);
    }

    #[test]
    fn failed_save_rolls_back_memory() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let id = s.add(form("before", None, None)).unwrap();
        // Make saving impossible by removing the vault's folder.
        std::fs::remove_dir_all(dir.path().join("sub")).unwrap();
        assert_eq!(s.add(form("new", None, None)).unwrap_err(), AppError::Io);
        assert_eq!(
            s.update(&id, form("after", None, None)).unwrap_err(),
            AppError::Io
        );
        // The password check reads the file, which is gone too.
        assert_eq!(
            s.delete(&id, &pw("master password!")).unwrap_err(),
            AppError::Io
        );
        let list = s.list("").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "before");
    }

    #[test]
    fn entry_commands_require_an_unlocked_vault() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let id = s.add(form("n", Some("p"), None)).unwrap();
        s.lock().unwrap();
        assert_eq!(s.get(&id).unwrap_err(), AppError::Locked);
        assert_eq!(
            s.reveal(&id, SecretField::Password).unwrap_err(),
            AppError::Locked
        );
        assert_eq!(s.add(form("n", None, None)).unwrap_err(), AppError::Locked);
        assert_eq!(
            s.update(&id, form("n", None, None)).unwrap_err(),
            AppError::Locked
        );
        assert_eq!(
            s.delete(&id, &pw("master password!")).unwrap_err(),
            AppError::Locked
        );
    }

    #[test]
    fn details_never_contain_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let id = s
            .add(form(
                "n",
                Some("password-value-xyz"),
                Some("notes-value-xyz"),
            ))
            .unwrap();
        let json = serde_json::to_string(&s.get(&id).unwrap()).unwrap();
        assert!(!json.contains("password-value-xyz"));
        assert!(!json.contains("notes-value-xyz"));
        assert!(json.contains(r#""hasPassword":true"#));
    }

    #[test]
    fn form_deserializes_absent_secrets_as_untouched() {
        let f: EntryForm = serde_json::from_str(
            r#"{"name":"n","username":"u","password":null,"urls":[],"notes":"x","windows":[],"autotype":""}"#,
        )
        .unwrap();
        assert!(f.password.is_none());
        assert_eq!(f.notes.unwrap().expose_secret(), "x");
        assert!(
            serde_json::from_str::<EntryForm>(
                r#"{"name":"n","username":"u","password":null,"urls":[],"notes":null,"windows":[],"autotype":"","extra":1}"#
            )
            .is_err()
        );
    }

    #[test]
    fn generator_command() {
        let options = GeneratorForm {
            target_bits: None,
            length: 24,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: false,
            exclude_ambiguous: true,
        };
        let generated = generate_password(options).unwrap();
        assert_eq!(generated.password.0.expose_secret().chars().count(), 24);
        assert!(generated.entropy_bits > 100.0);
        let json = serde_json::to_value(&generated).unwrap();
        assert!(json["password"].is_string());
        assert!(json["entropyBits"].is_number());

        let none = GeneratorForm {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..options
        };
        assert_eq!(
            generate_password(none).unwrap_err(),
            AppError::InvalidGeneratorOptions
        );
        let short = GeneratorForm {
            length: 4,
            ..options
        };
        assert_eq!(
            generate_password(short).unwrap_err(),
            AppError::InvalidGeneratorOptions
        );
    }

    #[test]
    fn generator_by_entropy_target() {
        let base = GeneratorForm {
            target_bits: Some(128),
            length: 0,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        };
        for bits in [64, 128, 256] {
            let g = generate_password(GeneratorForm {
                target_bits: Some(bits),
                ..base
            })
            .unwrap();
            assert!(g.entropy_bits >= f64::from(bits));
            assert_eq!(g.password.0.expose_secret().len(), g.length);
        }
        assert_eq!(generate_password(base).unwrap().length, 20);
        assert_eq!(
            generate_password(GeneratorForm {
                target_bits: Some(100_000),
                ..base
            })
            .unwrap_err(),
            AppError::InvalidGeneratorOptions
        );
    }

    #[test]
    fn invalid_web_addresses_block_saving() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        for (url, code) in [
            ("my bank", AppError::NotAWebAddress),
            ("javascript:alert(1)", AppError::UnsupportedScheme),
            (
                "https://bank.example@evil.example",
                AppError::EmbeddedCredentials,
            ),
        ] {
            let mut f = form("n", None, None);
            f.urls = vec!["https://ok.example.com".into(), url.into()];
            assert_eq!(s.add(f).unwrap_err(), code, "{url}");
        }
        assert!(s.list("").unwrap().is_empty());
    }

    #[test]
    fn check_url_reports_normalized_form_and_warnings() {
        let info = check_url("http://Example.com").unwrap();
        assert_eq!(info.url, "http://example.com/");
        assert_eq!(info.host, "example.com");
        assert_eq!(info.warnings, [WebWarning::NotEncrypted]);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""warnings":["notEncrypted"]"#));
        assert_eq!(
            check_url("not a site").unwrap_err(),
            AppError::NotAWebAddress
        );
    }

    #[test]
    fn strength_uses_the_entry_data() {
        let ctx = StrengthContext {
            name: "Banco Santander".into(),
            username: "maria.lopez@example.com".into(),
            urls: vec!["https://www.bancosantander.es/".into()],
        };
        let words = context_words(&ctx);
        for w in [
            "banco",
            "santander",
            "maria",
            "lopez",
            "maria.lopez@example.com",
        ] {
            assert!(words.contains(&w.to_owned()), "{w} in {words:?}");
        }
        for w in ["www", "https", "es"] {
            assert!(!words.contains(&w.to_owned()), "{w} in {words:?}");
        }
        let weak = estimate_strength(&pw("Santander2024!"), &ctx);
        assert_eq!(
            weak.warning,
            Some(llavero_core::strength::StrengthWarning::ContainsEntryData)
        );
        let strong = estimate_strength(&pw("kQ7#mZ2!pL9@xW4$tR8&"), &ctx);
        assert!(strong.bits > 100.0);
        let json = serde_json::to_string(&weak).unwrap();
        assert!(json.contains(r#""warning":"containsEntryData""#));
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

    fn window(title: &str, process: &str) -> WindowForm {
        WindowForm {
            title: title.into(),
            process: process.into(),
        }
    }

    #[test]
    fn auto_type_fields_are_saved_cleaned_and_returned() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let mut f = form("ERP", Some("pw"), None);
        f.windows = vec![
            window(" ERP* ", r" C:\Program Files\ERP\erp.exe "),
            window("  ", " "),
            window("ERP*", "erp.exe"),
            window("Solo título", ""),
        ];
        f.autotype = "{USERNAME}{ENTER}{DELAY 300}{PASSWORD}{ENTER}".into();
        let id = s.add(f).unwrap();

        let details = s.get(&id).unwrap();
        assert_eq!(
            details.windows,
            vec![window("ERP*", "erp.exe"), window("Solo título", "")]
        );
        assert_eq!(
            details.autotype,
            "{USERNAME}{ENTER}{DELAY 300}{PASSWORD}{ENTER}"
        );
        s.with_vault(|v| {
            let entry = &v.data().entries()[0];
            assert_eq!(entry.windows()[1].process, None);
        })
        .unwrap();

        // A blank sequence goes back to the default one; associations can
        // be removed.
        let mut f = form("ERP", None, None);
        f.autotype = "   ".into();
        s.update(&id, f).unwrap();
        let details = s.get(&id).unwrap();
        assert!(details.windows.is_empty());
        assert_eq!(details.autotype, "");
        s.with_vault(|v| assert_eq!(v.data().entries()[0].autotype(), None))
            .unwrap();
    }

    #[test]
    fn invalid_sequence_is_rejected_and_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let s = unlocked(dir.path());
        let mut f = form("ERP", Some("pw"), None);
        f.autotype = "{USERNAME}{TABB}".into();
        assert_eq!(s.add(f), Err(AppError::AutoTypeSequenceInvalid));
        assert!(s.list("").unwrap().is_empty());

        let id = s.add(form("ERP", Some("pw"), None)).unwrap();
        let mut f = form("Renamed", None, None);
        f.autotype = "{".into();
        assert_eq!(s.update(&id, f), Err(AppError::AutoTypeSequenceInvalid));
        assert_eq!(s.get(&id).unwrap().name, "ERP");
    }

    #[test]
    fn sequence_check_for_the_form() {
        assert_eq!(check_autotype_sequence(""), None);
        assert_eq!(check_autotype_sequence("  "), None);
        assert_eq!(check_autotype_sequence("{USERNAME}{TAB}{PASSWORD}"), None);
        assert_eq!(
            check_autotype_sequence("ab{TABB}"),
            Some(SequenceProblem {
                reason: SequenceProblemReason::UnknownPlaceholder,
                position: Some(2),
            })
        );
        assert_eq!(
            check_autotype_sequence("{DELAY 0}"),
            Some(SequenceProblem {
                reason: SequenceProblemReason::InvalidDelay,
                position: Some(0),
            })
        );
        assert_eq!(
            check_autotype_sequence(&"x".repeat(1001)),
            Some(SequenceProblem {
                reason: SequenceProblemReason::TooLong,
                position: None,
            })
        );
        let json = serde_json::to_string(&check_autotype_sequence("a}")).unwrap();
        assert_eq!(json, r#"{"reason":"unexpectedClosingBrace","position":1}"#);
    }
}
