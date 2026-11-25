//! The `.vault` file: creating, opening, saving and backing up a vault.
//! See `docs/vault-format.md`.

mod header;
mod storage;

use std::io;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::crypto::{self, CryptoError, KdfParams, Nonce, Salt, TAG_LEN, VaultKey};
use crate::model::{ModelError, SCHEMA_VERSION, VaultData};

use header::Header;
pub use header::{FORMAT_VERSION, HEADER_LEN, MAX_CIPHERTEXT_LEN};

/// Errors when creating, opening or saving a vault.
///
/// No variant carries passwords, keys, decrypted data or file paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// Not a vault file (wrong signature or too short).
    #[error("the file is not a vault")]
    NotAVault,
    /// The file was written by a newer, unknown format version.
    #[error("unsupported vault format version")]
    UnsupportedVersion,
    /// Malformed header or inconsistent length.
    #[error("the vault file is damaged (invalid header)")]
    InvalidHeader,
    /// KDF parameters in the header are out of bounds.
    #[error("the vault file has invalid key derivation parameters")]
    InvalidKdfParams,
    /// Authentication failed: wrong master password or modified file.
    #[error("wrong master password or corrupted vault")]
    WrongPasswordOrCorrupted,
    /// Authenticated content that does not follow the schema.
    #[error("the vault content is invalid")]
    InvalidPayload,
    /// Content written by a newer version of the application.
    #[error("the vault was created by a newer version of the application")]
    UnsupportedSchema,
    /// The vault content exceeds the maximum size.
    #[error("the vault content is too large")]
    TooLarge,
    /// The master password is empty.
    #[error("the master password cannot be empty")]
    EmptyPassword,
    /// Refusing to overwrite an existing file.
    #[error("a file already exists at the destination")]
    AlreadyExists,
    /// The path has no file name.
    #[error("invalid file path")]
    InvalidPath,
    /// Error in the crypto layer (randomness, key derivation, encryption).
    #[error(transparent)]
    Crypto(CryptoError),
    /// Error in the entry model.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// File-system error (only the kind is kept, never the path).
    #[error("file system error: {0}")]
    Io(io::ErrorKind),
}

impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}

impl From<CryptoError> for VaultError {
    fn from(e: CryptoError) -> Self {
        match e {
            CryptoError::Decryption => Self::WrongPasswordOrCorrupted,
            CryptoError::InvalidKdfParams => Self::InvalidKdfParams,
            other => Self::Crypto(other),
        }
    }
}

const MAX_FILE_LEN: u64 = HEADER_LEN as u64 + MAX_CIPHERTEXT_LEN;

/// Serializes `value` as JSON into a wiped-on-drop buffer of exact size,
/// so that growth never leaves unwiped copies of the plaintext behind.
fn to_json_zeroizing(data: &VaultData) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    struct Counter(usize);
    impl io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, data).map_err(|_| VaultError::InvalidPayload)?;
    let mut buf = Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *buf, data).map_err(|_| VaultError::InvalidPayload)?;
    Ok(buf)
}

/// Parses the decrypted payload, checking the schema version first so a
/// newer vault is reported as such and not as "invalid".
fn parse_payload(plaintext: &[u8]) -> Result<VaultData, VaultError> {
    #[derive(Deserialize)]
    struct SchemaProbe {
        schema: u32,
    }
    let probe: SchemaProbe =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::InvalidPayload)?;
    match probe.schema {
        SCHEMA_VERSION => {}
        s if s > SCHEMA_VERSION => return Err(VaultError::UnsupportedSchema),
        _ => return Err(VaultError::InvalidPayload),
    }
    serde_json::from_slice(plaintext).map_err(|_| VaultError::InvalidPayload)
}

/// Encrypts `data` into a complete vault file image.
fn encode(
    data: &VaultData,
    key: &VaultKey,
    salt: &Salt,
    kdf: &KdfParams,
) -> Result<Vec<u8>, VaultError> {
    let plaintext = to_json_zeroizing(data)?;
    let ciphertext_len = (plaintext.len() as u64).saturating_add(TAG_LEN as u64);
    if ciphertext_len > MAX_CIPHERTEXT_LEN {
        return Err(VaultError::TooLarge);
    }
    let header = Header {
        kdf: *kdf,
        salt: salt.clone(),
        nonce: Nonce::random()?,
        ciphertext_len,
    };
    let header_bytes = header.to_bytes();
    let ciphertext = crypto::encrypt(key, &header.nonce, &header_bytes, &plaintext)?;
    let mut file = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    file.extend_from_slice(&header_bytes);
    file.extend_from_slice(&ciphertext);
    Ok(file)
}

/// Opened vault parts.
#[derive(Debug)]
struct Decoded {
    data: VaultData,
    key: VaultKey,
    salt: Salt,
    kdf: KdfParams,
}

/// Validates, decrypts and parses a complete vault file image.
fn decode(bytes: &[u8], password: &SecretString) -> Result<Decoded, VaultError> {
    let header = Header::parse(bytes, bytes.len() as u64)?;
    let (aad, ciphertext) = bytes.split_at(HEADER_LEN);
    let key = crypto::derive_key(password, &header.salt, &header.kdf)?;
    let plaintext = crypto::decrypt(&key, &header.nonce, aad, ciphertext)?;
    let data = parse_payload(&plaintext)?;
    Ok(Decoded {
        data,
        key,
        salt: header.salt,
        kdf: header.kdf,
    })
}

fn check_password(password: &SecretString) -> Result<(), VaultError> {
    if password.expose_secret().is_empty() {
        Err(VaultError::EmptyPassword)
    } else {
        Ok(())
    }
}

fn read_vault_file(path: &Path) -> Result<Vec<u8>, VaultError> {
    storage::read_bounded(path, MAX_FILE_LEN)?.ok_or(VaultError::InvalidHeader)
}

/// An unlocked vault bound to a file on disk.
///
/// Holds the derived key so that saving does not re-run Argon2.
/// Dropping it wipes the key and all secrets in memory.
#[derive(Debug)]
pub struct Vault {
    path: PathBuf,
    data: VaultData,
    key: VaultKey,
    salt: Salt,
    kdf: KdfParams,
    dirty: bool,
}

impl Vault {
    /// Creates a new, empty vault and writes it to `path`.
    /// Fails with [`VaultError::AlreadyExists`] if the file exists.
    pub fn create(
        path: impl Into<PathBuf>,
        password: &SecretString,
        kdf: KdfParams,
    ) -> Result<Self, VaultError> {
        let path = path.into();
        check_password(password)?;
        if path.file_name().is_none() {
            return Err(VaultError::InvalidPath);
        }
        if path.try_exists()? {
            return Err(VaultError::AlreadyExists);
        }
        let salt = Salt::random()?;
        let key = crypto::derive_key(password, &salt, &kdf)?;
        let mut vault = Self {
            path,
            data: VaultData::new()?,
            key,
            salt,
            kdf,
            dirty: true,
        };
        vault.save()?;
        Ok(vault)
    }

    /// Opens and decrypts the vault at `path`.
    pub fn open(path: impl Into<PathBuf>, password: &SecretString) -> Result<Self, VaultError> {
        let path = path.into();
        let bytes = read_vault_file(&path)?;
        let d = decode(&bytes, password)?;
        Ok(Self {
            path,
            data: d.data,
            key: d.key,
            salt: d.salt,
            kdf: d.kdf,
            dirty: false,
        })
    }

    /// Path of the vault file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Key derivation parameters currently in use.
    pub fn kdf_params(&self) -> KdfParams {
        self.kdf
    }

    /// Decrypted content (read-only).
    pub fn data(&self) -> &VaultData {
        &self.data
    }

    /// Decrypted content for editing. Marks the vault as having unsaved
    /// changes.
    pub fn data_mut(&mut self) -> &mut VaultData {
        self.dirty = true;
        &mut self.data
    }

    /// Whether there are changes not yet written to disk.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Encrypts with a fresh nonce and writes the file atomically,
    /// keeping the previous version as `<file>.bak`.
    pub fn save(&mut self) -> Result<(), VaultError> {
        let bytes = encode(&self.data, &self.key, &self.salt, &self.kdf)?;
        storage::write_atomic(&self.path, &bytes, true)?;
        self.dirty = false;
        Ok(())
    }

    /// Changes the master password (and optionally the KDF parameters),
    /// with a new salt, and saves immediately.
    ///
    /// `current` is verified against the file on disk first.
    pub fn change_master_password(
        &mut self,
        current: &SecretString,
        new: &SecretString,
        kdf: KdfParams,
    ) -> Result<(), VaultError> {
        check_password(new)?;
        decode(&read_vault_file(&self.path)?, current)?;
        let salt = Salt::random()?;
        let key = crypto::derive_key(new, &salt, &kdf)?;
        let previous = (
            std::mem::replace(&mut self.key, key),
            std::mem::replace(&mut self.salt, salt),
            std::mem::replace(&mut self.kdf, kdf),
        );
        if let Err(e) = self.save() {
            // Keep memory consistent with what is on disk.
            (self.key, self.salt, self.kdf) = previous;
            return Err(e);
        }
        Ok(())
    }

    /// Checks `password` against the master password of the file on disk,
    /// for confirming sensitive actions. Runs the full key derivation, so
    /// it is as slow (and as hard to brute-force) as opening the vault.
    pub fn verify_password(&self, password: &SecretString) -> Result<(), VaultError> {
        check_password(password)?;
        decode(&read_vault_file(&self.path)?, password)?;
        Ok(())
    }

    /// Copies the encrypted file, as last saved, to `destination`.
    /// Refuses to overwrite an existing file. Unsaved changes are not
    /// included: call [`Vault::save`] first if needed.
    pub fn export_backup(&self, destination: &Path) -> Result<(), VaultError> {
        if destination.file_name().is_none() {
            return Err(VaultError::InvalidPath);
        }
        if destination.try_exists()? {
            return Err(VaultError::AlreadyExists);
        }
        let bytes = read_vault_file(&self.path)?;
        storage::write_atomic(destination, &bytes, false)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryInput;
    use std::fs;

    fn pw(s: &str) -> SecretString {
        SecretString::from(s)
    }

    /// Cheapest parameters the format accepts, to keep tests fast.
    fn fast() -> KdfParams {
        KdfParams::new(KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST, 1).unwrap()
    }

    fn new_vault(dir: &Path) -> (PathBuf, Vault) {
        let path = dir.join("llavero.vault");
        let vault = Vault::create(&path, &pw("master"), fast()).unwrap();
        (path, vault)
    }

    #[test]
    fn create_save_open_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut vault) = new_vault(dir.path());
        assert!(!vault.is_dirty());
        let mut input = EntryInput::new("Correo", "yo@example.com", pw("s3cr3t-ñ"));
        input.urls = vec!["https://mail.example.com".into()];
        let id = vault.data_mut().add_entry(input).unwrap();
        assert!(vault.is_dirty());
        vault.save().unwrap();
        assert!(!vault.is_dirty());

        let reopened = Vault::open(&path, &pw("master")).unwrap();
        let entry = reopened.data().entry(id).unwrap();
        assert_eq!(entry.name(), "Correo");
        assert_eq!(entry.password().expose_secret(), "s3cr3t-ñ");
        assert_eq!(reopened.data().vault_id(), vault.data().vault_id());
        assert_eq!(reopened.kdf_params(), fast());
    }

    #[test]
    fn file_on_disk_does_not_contain_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut vault) = new_vault(dir.path());
        vault
            .data_mut()
            .add_entry(EntryInput::new(
                "VisibleName",
                "visible-user",
                pw("visible-pass"),
            ))
            .unwrap();
        vault.save().unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"LLAVERO\0");
        for needle in [
            &b"VisibleName"[..],
            b"visible-user",
            b"visible-pass",
            b"entries",
        ] {
            assert!(!bytes.windows(needle.len()).any(|w| w == needle));
        }
    }

    #[test]
    fn wrong_password_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = new_vault(dir.path());
        assert_eq!(
            Vault::open(&path, &pw("Master")).unwrap_err(),
            VaultError::WrongPasswordOrCorrupted
        );
    }

    #[test]
    fn empty_password_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.vault");
        assert_eq!(
            Vault::create(&path, &pw(""), fast()).unwrap_err(),
            VaultError::EmptyPassword
        );
        assert!(!path.exists());
    }

    #[test]
    fn create_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = new_vault(dir.path());
        let before = fs::read(&path).unwrap();
        assert_eq!(
            Vault::create(&path, &pw("other"), fast()).unwrap_err(),
            VaultError::AlreadyExists
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn every_save_uses_a_fresh_nonce() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut vault) = new_vault(dir.path());
        let first = fs::read(&path).unwrap();
        vault.save().unwrap();
        let second = fs::read(&path).unwrap();
        assert_eq!(first[..60], second[..60], "same magic/params/salt");
        assert_ne!(first[60..84], second[60..84], "different nonce");
        assert_ne!(first[HEADER_LEN..], second[HEADER_LEN..]);
    }

    #[test]
    fn save_keeps_previous_version_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut vault) = new_vault(dir.path());
        vault
            .data_mut()
            .add_entry(EntryInput::new("one", "", pw("1")))
            .unwrap();
        vault.save().unwrap();
        let bak = Vault::open(dir.path().join("llavero.vault.bak"), &pw("master")).unwrap();
        assert!(bak.data().entries().is_empty());
        let current = Vault::open(&path, &pw("master")).unwrap();
        assert_eq!(current.data().entries().len(), 1);
    }

    #[test]
    fn any_byte_flip_in_the_file_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = new_vault(dir.path());
        let good = fs::read(&path).unwrap();
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0x01;
            assert!(decode(&bad, &pw("master")).is_err(), "byte {i}");
        }
    }

    #[test]
    fn truncated_or_extended_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = new_vault(dir.path());
        let good = fs::read(&path).unwrap();
        for len in [0, 7, HEADER_LEN - 1, HEADER_LEN, good.len() - 1] {
            assert!(decode(&good[..len], &pw("master")).is_err(), "len {len}");
        }
        let mut longer = good.clone();
        longer.push(0);
        assert_eq!(
            decode(&longer, &pw("master")).unwrap_err(),
            VaultError::InvalidHeader
        );
    }

    #[test]
    fn non_vault_files_are_identified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, "just some text, long enough to fill a header..........................................").unwrap();
        assert_eq!(
            Vault::open(&path, &pw("x")).unwrap_err(),
            VaultError::NotAVault
        );
        assert_eq!(
            Vault::open(dir.path().join("missing.vault"), &pw("x")).unwrap_err(),
            VaultError::Io(io::ErrorKind::NotFound)
        );
    }

    /// Encrypts an arbitrary payload with valid crypto, to test schema checks.
    fn file_with_payload(payload: &[u8]) -> Vec<u8> {
        let salt = Salt::random().unwrap();
        let key = crypto::derive_key(&pw("master"), &salt, &fast()).unwrap();
        let header = Header {
            kdf: fast(),
            salt,
            nonce: Nonce::random().unwrap(),
            ciphertext_len: (payload.len() + TAG_LEN) as u64,
        };
        let hb = header.to_bytes();
        let mut out = hb.to_vec();
        out.extend(crypto::encrypt(&key, &header.nonce, &hb, payload).unwrap());
        out
    }

    #[test]
    fn newer_schema_is_reported_and_garbage_is_invalid() {
        let newer = file_with_payload(br#"{"schema": 2, "something_new": true}"#);
        assert_eq!(
            decode(&newer, &pw("master")).unwrap_err(),
            VaultError::UnsupportedSchema
        );
        for payload in [&b"not json"[..], br#"{"schema": 0}"#, br#"{"schema": 1}"#] {
            assert_eq!(
                decode(&file_with_payload(payload), &pw("master")).unwrap_err(),
                VaultError::InvalidPayload
            );
        }
    }

    #[test]
    fn change_master_password() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut vault) = new_vault(dir.path());
        let old_salt = fs::read(&path).unwrap()[28..60].to_vec();

        assert_eq!(
            vault
                .change_master_password(&pw("wrong"), &pw("new"), fast())
                .unwrap_err(),
            VaultError::WrongPasswordOrCorrupted
        );
        assert_eq!(
            vault
                .change_master_password(&pw("master"), &pw(""), fast())
                .unwrap_err(),
            VaultError::EmptyPassword
        );
        assert!(Vault::open(&path, &pw("master")).is_ok());

        let stronger = KdfParams::new(KdfParams::MIN_M_COST_KIB * 2, 3, 2).unwrap();
        vault
            .change_master_password(&pw("master"), &pw("new"), stronger)
            .unwrap();
        assert_ne!(fs::read(&path).unwrap()[28..60], old_salt[..]);
        assert_eq!(
            Vault::open(&path, &pw("master")).unwrap_err(),
            VaultError::WrongPasswordOrCorrupted
        );
        let reopened = Vault::open(&path, &pw("new")).unwrap();
        assert_eq!(reopened.kdf_params(), stronger);
    }

    #[test]
    fn verify_password() {
        let dir = tempfile::tempdir().unwrap();
        let (_, vault) = new_vault(dir.path());
        assert!(vault.verify_password(&pw("master")).is_ok());
        assert!(matches!(
            vault.verify_password(&pw("wrong password")),
            Err(VaultError::WrongPasswordOrCorrupted)
        ));
        assert!(matches!(
            vault.verify_password(&pw("")),
            Err(VaultError::EmptyPassword)
        ));
    }

    #[test]
    fn export_backup_copies_saved_file_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let (path, vault) = new_vault(dir.path());
        let dest = dir.path().join("copia.vault");
        vault.export_backup(&dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), fs::read(&path).unwrap());
        assert!(Vault::open(&dest, &pw("master")).is_ok());
        assert_eq!(
            vault.export_backup(&dest).unwrap_err(),
            VaultError::AlreadyExists
        );
    }

    #[test]
    fn debug_output_has_no_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let (_, mut vault) = new_vault(dir.path());
        vault
            .data_mut()
            .add_entry(EntryInput::new("n", "u", pw("top-secret-value")))
            .unwrap();
        assert!(!format!("{vault:?}").contains("top-secret-value"));
    }
}
