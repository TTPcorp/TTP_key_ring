//! Entry model: the decrypted content of a vault.
//!
//! This is the JSON payload described in `docs/vault-format.md` §5.
//! Secret fields (password, notes) are held in [`SecretString`], which is
//! wiped on drop and redacted in `Debug` output.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::crypto::{self, CryptoError};

/// Current payload schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Errors of the entry model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// No entry has the given id.
    #[error("entry not found")]
    NotFound,
    /// The entry name is empty or only whitespace.
    #[error("the entry name cannot be empty")]
    EmptyName,
    /// Generating a random identifier failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

fn new_id() -> Result<Uuid, CryptoError> {
    let mut bytes = [0u8; 16];
    crypto::fill_random(&mut bytes)?;
    Ok(uuid::Builder::from_random_bytes(bytes).into_uuid())
}

/// A window an entry is associated with, for auto-type (Phase 2).
/// A field set to `None` matches any value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowMatch {
    /// Window title (matching rules are defined in Phase 2).
    pub title: Option<String>,
    /// Executable name, e.g. `program.exe`.
    pub process: Option<String>,
}

/// User-editable fields of an entry, used to create or update it.
#[derive(Debug, Clone)]
pub struct EntryInput {
    /// Display name. Must not be empty.
    pub name: String,
    /// User name or e-mail.
    pub username: String,
    /// Password.
    pub password: SecretString,
    /// Associated web addresses.
    pub urls: Vec<String>,
    /// Associated program windows.
    pub windows: Vec<WindowMatch>,
    /// Custom auto-type sequence; `None` means the default sequence.
    pub autotype: Option<String>,
    /// Free-form notes. Treated as secret.
    pub notes: SecretString,
}

impl EntryInput {
    /// Entry with only a name, user name and password.
    pub fn new(
        name: impl Into<String>,
        username: impl Into<String>,
        password: SecretString,
    ) -> Self {
        Self {
            name: name.into(),
            username: username.into(),
            password,
            urls: Vec::new(),
            windows: Vec::new(),
            autotype: None,
            notes: SecretString::from(""),
        }
    }
}

/// A stored credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    id: Uuid,
    name: String,
    username: String,
    #[serde(serialize_with = "serialize_secret")]
    password: SecretString,
    urls: Vec<String>,
    windows: Vec<WindowMatch>,
    autotype: Option<String>,
    #[serde(serialize_with = "serialize_secret")]
    notes: SecretString,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    modified_at: OffsetDateTime,
}

impl Entry {
    /// Unique identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// User name.
    pub fn username(&self) -> &str {
        &self.username
    }
    /// Password (expose only when actually needed).
    pub fn password(&self) -> &SecretString {
        &self.password
    }
    /// Associated web addresses.
    pub fn urls(&self) -> &[String] {
        &self.urls
    }
    /// Associated program windows.
    pub fn windows(&self) -> &[WindowMatch] {
        &self.windows
    }
    /// Custom auto-type sequence, if any.
    pub fn autotype(&self) -> Option<&str> {
        self.autotype.as_deref()
    }
    /// Notes (treated as secret).
    pub fn notes(&self) -> &SecretString {
        &self.notes
    }
    /// Creation time (UTC).
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
    /// Last modification time (UTC).
    pub fn modified_at(&self) -> OffsetDateTime {
        self.modified_at
    }

    fn apply(&mut self, input: EntryInput) {
        self.name = input.name.trim().to_owned();
        self.username = input.username;
        self.password = input.password;
        self.urls = input.urls;
        self.windows = input.windows;
        self.autotype = input.autotype;
        self.notes = input.notes;
    }

    fn matches(&self, needle: &str) -> bool {
        let contains = |haystack: &str| haystack.to_lowercase().contains(needle);
        contains(&self.name)
            || contains(&self.username)
            || self.urls.iter().any(|u| contains(u))
            || self.windows.iter().any(|w| {
                w.title.as_deref().is_some_and(contains)
                    || w.process.as_deref().is_some_and(contains)
            })
    }
}

/// Decrypted vault content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultData {
    schema: u32,
    vault_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    modified_at: OffsetDateTime,
    entries: Vec<Entry>,
}

impl VaultData {
    /// Empty vault content with a fresh identifier.
    pub fn new() -> Result<Self, ModelError> {
        let now = OffsetDateTime::now_utc();
        Ok(Self {
            schema: SCHEMA_VERSION,
            vault_id: new_id()?,
            created_at: now,
            modified_at: now,
            entries: Vec::new(),
        })
    }

    /// Payload schema version.
    pub fn schema(&self) -> u32 {
        self.schema
    }
    /// Vault identifier.
    pub fn vault_id(&self) -> Uuid {
        self.vault_id
    }
    /// Creation time (UTC).
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
    /// Last modification time (UTC).
    pub fn modified_at(&self) -> OffsetDateTime {
        self.modified_at
    }
    /// All entries, in insertion order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Looks an entry up by id.
    pub fn entry(&self, id: Uuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Adds an entry and returns its id.
    pub fn add_entry(&mut self, input: EntryInput) -> Result<Uuid, ModelError> {
        if input.name.trim().is_empty() {
            return Err(ModelError::EmptyName);
        }
        let now = OffsetDateTime::now_utc();
        let mut entry = Entry {
            id: new_id()?,
            name: String::new(),
            username: String::new(),
            password: SecretString::from(""),
            urls: Vec::new(),
            windows: Vec::new(),
            autotype: None,
            notes: SecretString::from(""),
            created_at: now,
            modified_at: now,
        };
        entry.apply(input);
        let id = entry.id;
        self.entries.push(entry);
        self.modified_at = now;
        Ok(id)
    }

    /// Replaces the editable fields of an entry.
    pub fn update_entry(&mut self, id: Uuid, input: EntryInput) -> Result<(), ModelError> {
        if input.name.trim().is_empty() {
            return Err(ModelError::EmptyName);
        }
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(ModelError::NotFound)?;
        let now = OffsetDateTime::now_utc();
        entry.apply(input);
        entry.modified_at = now;
        self.modified_at = now;
        Ok(())
    }

    /// Removes an entry and returns it.
    pub fn remove_entry(&mut self, id: Uuid) -> Result<Entry, ModelError> {
        let index = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(ModelError::NotFound)?;
        self.modified_at = OffsetDateTime::now_utc();
        Ok(self.entries.remove(index))
    }

    /// Case-insensitive substring search over name, user name, URLs and
    /// window title/process. Secrets (password, notes) are never searched.
    /// An empty or blank query returns every entry.
    pub fn search<'a>(&'a self, query: &str) -> impl Iterator<Item = &'a Entry> + 'a {
        let needle = query.trim().to_lowercase();
        self.entries
            .iter()
            .filter(move |e| needle.is_empty() || e.matches(&needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> EntryInput {
        EntryInput::new(name, "user", SecretString::from("pw"))
    }

    #[test]
    fn new_vault_is_empty_with_current_schema() {
        let v = VaultData::new().unwrap();
        assert_eq!(v.schema(), SCHEMA_VERSION);
        assert!(v.entries().is_empty());
        assert_ne!(v.vault_id(), VaultData::new().unwrap().vault_id());
        assert_eq!(v.vault_id().get_version_num(), 4);
    }

    #[test]
    fn add_get_update_remove() {
        let mut v = VaultData::new().unwrap();
        let id = v.add_entry(input("  Mail  ")).unwrap();
        let e = v.entry(id).unwrap();
        assert_eq!(e.name(), "Mail");
        assert_eq!(e.password().expose_secret(), "pw");
        let created = e.created_at();

        let mut changed = input("Mail 2");
        changed.urls = vec!["https://mail.example".into()];
        changed.password = SecretString::from("new");
        v.update_entry(id, changed).unwrap();
        let e = v.entry(id).unwrap();
        assert_eq!(e.name(), "Mail 2");
        assert_eq!(e.password().expose_secret(), "new");
        assert_eq!(e.urls(), ["https://mail.example"]);
        assert_eq!(e.created_at(), created);
        assert!(e.modified_at() >= created);

        let removed = v.remove_entry(id).unwrap();
        assert_eq!(removed.id(), id);
        assert!(v.entry(id).is_none());
    }

    #[test]
    fn ids_are_unique() {
        let mut v = VaultData::new().unwrap();
        let a = v.add_entry(input("a")).unwrap();
        let b = v.add_entry(input("a")).unwrap();
        assert_ne!(a, b);
        assert_eq!(v.entries().len(), 2);
    }

    #[test]
    fn empty_names_are_rejected() {
        let mut v = VaultData::new().unwrap();
        assert_eq!(v.add_entry(input("")), Err(ModelError::EmptyName));
        assert_eq!(v.add_entry(input("   ")), Err(ModelError::EmptyName));
        let id = v.add_entry(input("ok")).unwrap();
        assert_eq!(v.update_entry(id, input(" ")), Err(ModelError::EmptyName));
        assert_eq!(v.entry(id).unwrap().name(), "ok");
    }

    #[test]
    fn unknown_ids_are_reported() {
        let mut v = VaultData::new().unwrap();
        let ghost = Uuid::nil();
        assert_eq!(v.update_entry(ghost, input("x")), Err(ModelError::NotFound));
        assert!(matches!(v.remove_entry(ghost), Err(ModelError::NotFound)));
    }

    #[test]
    fn search_is_case_insensitive_and_skips_secrets() {
        let mut v = VaultData::new().unwrap();
        let mut bank = input("Banco");
        bank.urls = vec!["https://banca.example.com".into()];
        bank.password = SecretString::from("needle-in-password");
        bank.notes = SecretString::from("needle-in-notes");
        let bank = v.add_entry(bank).unwrap();
        let mut erp = input("ERP");
        erp.windows = vec![WindowMatch {
            title: Some("Acceso ERP".into()),
            process: Some("Erp.EXE".into()),
        }];
        let erp = v.add_entry(erp).unwrap();

        let ids = |q: &str| v.search(q).map(Entry::id).collect::<Vec<_>>();
        assert_eq!(ids("banco"), [bank]);
        assert_eq!(ids("BANCA.EXAMPLE"), [bank]);
        assert_eq!(ids("erp.exe"), [erp]);
        assert_eq!(ids("acceso"), [erp]);
        assert_eq!(ids("USER"), [bank, erp]);
        assert_eq!(ids(""), [bank, erp]);
        assert_eq!(ids("   "), [bank, erp]);
        assert!(ids("needle").is_empty());
        assert!(ids("nothing").is_empty());
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let mut v = VaultData::new().unwrap();
        let mut i = input("x");
        i.password = SecretString::from("hunter2-secret");
        i.notes = SecretString::from("private-note");
        v.add_entry(i).unwrap();
        let dbg = format!("{v:?}");
        assert!(!dbg.contains("hunter2-secret"));
        assert!(!dbg.contains("private-note"));
    }

    #[test]
    fn json_round_trip_keeps_everything() {
        let mut v = VaultData::new().unwrap();
        let mut i = input("Full");
        i.urls = vec!["https://a.example".into()];
        i.windows = vec![WindowMatch {
            title: None,
            process: Some("app.exe".into()),
        }];
        i.autotype = Some("{USERNAME}{ENTER}".into());
        i.notes = SecretString::from("notes");
        let id = v.add_entry(i).unwrap();

        let json = serde_json::to_string(&v).unwrap();
        let back: VaultData = serde_json::from_str(&json).unwrap();
        let e = back.entry(id).unwrap();
        assert_eq!(back.vault_id(), v.vault_id());
        assert_eq!(back.created_at(), v.created_at());
        assert_eq!(e.password().expose_secret(), "pw");
        assert_eq!(e.notes().expose_secret(), "notes");
        assert_eq!(e.autotype(), Some("{USERNAME}{ENTER}"));
        assert_eq!(e.windows()[0].process.as_deref(), Some("app.exe"));
        assert_eq!(e.created_at(), v.entry(id).unwrap().created_at());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let v = VaultData::new().unwrap();
        let mut json: serde_json::Value = serde_json::to_value(&v).unwrap();
        json["future_field"] = serde_json::json!(1);
        assert!(serde_json::from_value::<VaultData>(json).is_err());
    }
}
