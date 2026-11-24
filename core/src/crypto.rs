//! Cryptographic building blocks for the vault (see `docs/vault-format.md`).
//!
//! - Key derivation: Argon2id v0x13, 32-byte output.
//! - Authenticated encryption: XChaCha20-Poly1305 (24-byte nonce, 16-byte tag).
//! - Randomness: the operating system CSPRNG.
//!
//! No primitive is implemented here: this module only wires audited crates
//! together, validates parameters and makes sure secret material lives in
//! types that are wiped on drop.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{AeadInOut, KeyInit, XChaCha20Poly1305, XNonce};
use rand::TryRng;
use rand::rngs::SysRng;
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

/// Length in bytes of the derived vault key.
pub const KEY_LEN: usize = 32;
/// Length in bytes of the per-vault Argon2id salt.
pub const SALT_LEN: usize = 32;
/// Length in bytes of the XChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 24;
/// Length in bytes of the Poly1305 authentication tag.
pub const TAG_LEN: usize = 16;

/// Errors produced by the crypto layer.
///
/// Variants deliberately carry no data: messages must never leak secrets,
/// and "wrong password" is indistinguishable from "tampered data".
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    /// KDF parameters are outside the accepted bounds.
    #[error("key derivation parameters are outside the accepted range")]
    InvalidKdfParams,
    /// The operating system random number generator failed.
    #[error("the system random number generator is unavailable")]
    RandomUnavailable,
    /// Argon2 failed (e.g. not enough memory).
    #[error("key derivation failed")]
    KeyDerivation,
    /// Encryption failed (only possible with absurdly large inputs).
    #[error("encryption failed")]
    Encryption,
    /// Authentication failed: wrong password, or the data was modified.
    #[error("wrong password or corrupted data")]
    Decryption,
}

/// Argon2id cost parameters, validated against the bounds of the vault format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
}

impl KdfParams {
    /// Minimum memory cost in KiB (19 MiB, OWASP recommendation).
    pub const MIN_M_COST_KIB: u32 = 19 * 1024;
    /// Maximum memory cost in KiB accepted when opening a vault (4 GiB).
    pub const MAX_M_COST_KIB: u32 = 4 * 1024 * 1024;
    /// Minimum number of iterations.
    pub const MIN_T_COST: u32 = 2;
    /// Maximum number of iterations accepted when opening a vault.
    pub const MAX_T_COST: u32 = 100;
    /// Minimum parallelism.
    pub const MIN_P_COST: u32 = 1;
    /// Maximum parallelism accepted when opening a vault.
    pub const MAX_P_COST: u32 = 16;

    /// Parameters used when creating a new vault: 64 MiB, t=3, p=4.
    pub const DEFAULT: Self = Self {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 4,
    };

    /// Validates and builds a set of parameters.
    ///
    /// This is cheap and must be called *before* running Argon2 on
    /// parameters read from an untrusted file.
    pub fn new(m_cost_kib: u32, t_cost: u32, p_cost: u32) -> Result<Self, CryptoError> {
        let in_range = (Self::MIN_M_COST_KIB..=Self::MAX_M_COST_KIB).contains(&m_cost_kib)
            && (Self::MIN_T_COST..=Self::MAX_T_COST).contains(&t_cost)
            && (Self::MIN_P_COST..=Self::MAX_P_COST).contains(&p_cost)
            && u64::from(m_cost_kib) >= 8 * u64::from(p_cost);
        if in_range {
            Ok(Self {
                m_cost_kib,
                t_cost,
                p_cost,
            })
        } else {
            Err(CryptoError::InvalidKdfParams)
        }
    }

    /// Memory cost in KiB.
    pub fn m_cost_kib(&self) -> u32 {
        self.m_cost_kib
    }

    /// Number of iterations.
    pub fn t_cost(&self) -> u32 {
        self.t_cost
    }

    /// Degree of parallelism (lanes).
    pub fn p_cost(&self) -> u32 {
        self.p_cost
    }

    /// Cheap parameters that bypass the minimums. Tests only.
    #[cfg(test)]
    pub(crate) fn insecure_for_tests() -> Self {
        Self {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Fills `buf` from the operating system CSPRNG.
fn fill_random(buf: &mut [u8]) -> Result<(), CryptoError> {
    SysRng
        .try_fill_bytes(buf)
        .map_err(|_| CryptoError::RandomUnavailable)
}

/// Per-vault Argon2id salt. Not secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salt([u8; SALT_LEN]);

impl Salt {
    /// Generates a fresh random salt.
    pub fn random() -> Result<Self, CryptoError> {
        let mut bytes = [0u8; SALT_LEN];
        fill_random(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Wraps a salt read from a vault header.
    pub fn from_bytes(bytes: [u8; SALT_LEN]) -> Self {
        Self(bytes)
    }

    /// Raw bytes, for writing the header.
    pub fn as_bytes(&self) -> &[u8; SALT_LEN] {
        &self.0
    }
}

/// XChaCha20-Poly1305 nonce. Not secret, but must never be reused with the
/// same key: create a new one with [`Nonce::random`] for every save.
///
/// Intentionally neither `Clone` nor `Copy`, to make accidental reuse harder.
#[derive(Debug, PartialEq, Eq)]
pub struct Nonce([u8; NONCE_LEN]);

impl Nonce {
    /// Generates a fresh random nonce. 192 bits make random nonces safe.
    pub fn random() -> Result<Self, CryptoError> {
        let mut bytes = [0u8; NONCE_LEN];
        fill_random(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Wraps a nonce read from a vault header (for decryption).
    pub fn from_bytes(bytes: [u8; NONCE_LEN]) -> Self {
        Self(bytes)
    }

    /// Raw bytes, for writing the header.
    pub fn as_bytes(&self) -> &[u8; NONCE_LEN] {
        &self.0
    }
}

/// Symmetric key derived from the master password. Wiped on drop.
///
/// Neither `Clone` nor printable: its `Debug` output is redacted.
pub struct VaultKey(Zeroizing<[u8; KEY_LEN]>);

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey([REDACTED])")
    }
}

impl VaultKey {
    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&(*self.0).into())
    }

    #[cfg(test)]
    pub(crate) fn expose_for_tests(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

/// Derives the vault key from the master password with Argon2id.
///
/// The password is used as its raw UTF-8 bytes, without Unicode
/// normalization (see `docs/vault-format.md`, §8).
pub fn derive_key(
    password: &SecretString,
    salt: &Salt,
    params: &KdfParams,
) -> Result<VaultKey, CryptoError> {
    let argon_params = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        Some(KEY_LEN),
    )
    .map_err(|_| CryptoError::InvalidKdfParams)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);

    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(
            password.expose_secret().as_bytes(),
            salt.as_bytes(),
            key.as_mut_slice(),
        )
        .map_err(|_| CryptoError::KeyDerivation)?;
    Ok(VaultKey(key))
}

/// Encrypts `plaintext`, authenticating `aad` as well.
///
/// Returns `ciphertext || tag` (`plaintext.len() + TAG_LEN` bytes).
/// The working buffer is wiped if encryption fails.
pub fn encrypt(
    key: &VaultKey,
    nonce: &Nonce,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    // Reserve room for the tag up front so the buffer never reallocates
    // (a reallocation would leave an unwiped plaintext copy behind).
    let mut buffer = Zeroizing::new(Vec::with_capacity(plaintext.len() + TAG_LEN));
    buffer.extend_from_slice(plaintext);
    key.cipher()
        .encrypt_in_place(&XNonce::from(nonce.0), aad, &mut *buffer)
        .map_err(|_| CryptoError::Encryption)?;
    // Now ciphertext: hand it out without wiping.
    Ok(std::mem::take(&mut *buffer))
}

/// Verifies and decrypts `ciphertext || tag` produced by [`encrypt`].
///
/// The plaintext is returned in a buffer that is wiped on drop.
pub fn decrypt(
    key: &VaultKey,
    nonce: &Nonce,
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if ciphertext.len() < TAG_LEN {
        return Err(CryptoError::Decryption);
    }
    let mut buffer = Zeroizing::new(ciphertext.to_vec());
    key.cipher()
        .decrypt_in_place(&XNonce::from(nonce.0), aad, &mut *buffer)
        .map_err(|_| CryptoError::Decryption)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password(s: &str) -> SecretString {
        SecretString::from(s)
    }

    fn test_key(pw: &str, salt: &Salt) -> VaultKey {
        derive_key(&password(pw), salt, &KdfParams::insecure_for_tests()).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // ---- KdfParams -------------------------------------------------------

    #[test]
    fn default_params_are_valid() {
        let d = KdfParams::default();
        assert_eq!(
            KdfParams::new(d.m_cost_kib(), d.t_cost(), d.p_cost()),
            Ok(d)
        );
        assert_eq!((d.m_cost_kib(), d.t_cost(), d.p_cost()), (65_536, 3, 4));
    }

    #[test]
    fn params_bounds_are_inclusive() {
        assert!(KdfParams::new(KdfParams::MIN_M_COST_KIB, 2, 1).is_ok());
        assert!(KdfParams::new(KdfParams::MAX_M_COST_KIB, 100, 16).is_ok());
    }

    #[test]
    fn params_out_of_range_are_rejected() {
        let bad = [
            (KdfParams::MIN_M_COST_KIB - 1, 3, 1),
            (KdfParams::MAX_M_COST_KIB + 1, 3, 1),
            (65_536, 1, 1),
            (65_536, 101, 1),
            (65_536, 3, 0),
            (65_536, 3, 17),
            (0, 0, 0),
            (u32::MAX, u32::MAX, u32::MAX),
        ];
        for (m, t, p) in bad {
            assert_eq!(
                KdfParams::new(m, t, p),
                Err(CryptoError::InvalidKdfParams),
                "m={m} t={t} p={p}"
            );
        }
    }

    // ---- Randomness ------------------------------------------------------

    #[test]
    fn random_salts_and_nonces_differ() {
        assert_ne!(Salt::random().unwrap(), Salt::random().unwrap());
        assert_ne!(Nonce::random().unwrap(), Nonce::random().unwrap());
        assert_ne!(Salt::random().unwrap().as_bytes(), &[0u8; SALT_LEN]);
        assert_ne!(Nonce::random().unwrap().as_bytes(), &[0u8; NONCE_LEN]);
    }

    // ---- Key derivation --------------------------------------------------

    #[test]
    fn derivation_is_deterministic() {
        let salt = Salt::from_bytes([7; SALT_LEN]);
        let a = test_key("correct horse", &salt);
        let b = test_key("correct horse", &salt);
        assert_eq!(a.expose_for_tests(), b.expose_for_tests());
    }

    #[test]
    fn derivation_depends_on_password_and_salt() {
        let s1 = Salt::from_bytes([1; SALT_LEN]);
        let s2 = Salt::from_bytes([2; SALT_LEN]);
        let base = test_key("password", &s1);
        assert_ne!(
            base.expose_for_tests(),
            test_key("Password", &s1).expose_for_tests()
        );
        assert_ne!(
            base.expose_for_tests(),
            test_key("password", &s2).expose_for_tests()
        );
    }

    #[test]
    fn derivation_works_with_minimum_real_params() {
        let params = KdfParams::new(KdfParams::MIN_M_COST_KIB, 2, 1).unwrap();
        let salt = Salt::from_bytes([3; SALT_LEN]);
        assert!(derive_key(&password("x"), &salt, &params).is_ok());
    }

    /// Pinned Argon2id output. Guards against accidental changes of
    /// algorithm, version, output length or password encoding.
    /// Cross-checked with an independent implementation (argon2-cffi).
    #[test]
    fn argon2id_regression_vector() {
        let salt = Salt::from_bytes(*b"llavero-test-salt-32-bytes-long!");
        let key = test_key("contraseña de prueba", &salt);
        assert_eq!(
            hex(key.expose_for_tests()),
            "a026d968289080ef5067218246bdb79fc7dad16ec27f7f02ded211687a8a0a6f"
        );
    }

    #[test]
    fn vault_key_debug_is_redacted() {
        let key = test_key("secret", &Salt::from_bytes([0; SALT_LEN]));
        assert_eq!(format!("{key:?}"), "VaultKey([REDACTED])");
    }

    // ---- Encryption ------------------------------------------------------

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = test_key("pw", &Salt::from_bytes([9; SALT_LEN]));
        let nonce = Nonce::random().unwrap();
        let ct = encrypt(&key, &nonce, b"header", b"hello vault").unwrap();
        assert_eq!(ct.len(), b"hello vault".len() + TAG_LEN);
        assert_ne!(&ct[..11], b"hello vault");
        let pt = decrypt(&key, &nonce, b"header", &ct).unwrap();
        assert_eq!(pt.as_slice(), b"hello vault");
    }

    #[test]
    fn empty_plaintext_round_trip() {
        let key = test_key("pw", &Salt::from_bytes([9; SALT_LEN]));
        let nonce = Nonce::random().unwrap();
        let ct = encrypt(&key, &nonce, b"", b"").unwrap();
        assert_eq!(ct.len(), TAG_LEN);
        assert!(decrypt(&key, &nonce, b"", &ct).unwrap().is_empty());
    }

    #[test]
    fn wrong_key_fails() {
        let salt = Salt::from_bytes([9; SALT_LEN]);
        let nonce = Nonce::random().unwrap();
        let ct = encrypt(&test_key("right", &salt), &nonce, b"", b"data").unwrap();
        assert_eq!(
            decrypt(&test_key("wrong", &salt), &nonce, b"", &ct),
            Err(CryptoError::Decryption)
        );
    }

    #[test]
    fn any_modification_is_detected() {
        let key = test_key("pw", &Salt::from_bytes([9; SALT_LEN]));
        let nonce = Nonce::from_bytes([5; NONCE_LEN]);
        let aad = b"authenticated header";
        let ct = encrypt(&key, &nonce, aad, b"some secret payload").unwrap();

        for i in 0..ct.len() {
            let mut bad = ct.clone();
            bad[i] ^= 0x01;
            assert!(decrypt(&key, &nonce, aad, &bad).is_err(), "ct byte {i}");
        }
        for i in 0..aad.len() {
            let mut bad = aad.to_vec();
            bad[i] ^= 0x01;
            assert!(decrypt(&key, &nonce, &bad, &ct).is_err(), "aad byte {i}");
        }
        for i in 0..NONCE_LEN {
            let mut bad = [5; NONCE_LEN];
            bad[i] ^= 0x01;
            assert!(
                decrypt(&key, &Nonce::from_bytes(bad), aad, &ct).is_err(),
                "nonce byte {i}"
            );
        }
    }

    #[test]
    fn truncated_or_extended_ciphertext_fails() {
        let key = test_key("pw", &Salt::from_bytes([9; SALT_LEN]));
        let nonce = Nonce::from_bytes([5; NONCE_LEN]);
        let ct = encrypt(&key, &nonce, b"", b"payload").unwrap();
        for len in 0..ct.len() {
            assert_eq!(
                decrypt(&key, &nonce, b"", &ct[..len]),
                Err(CryptoError::Decryption),
                "len {len}"
            );
        }
        let mut longer = ct.clone();
        longer.push(0);
        assert_eq!(
            decrypt(&key, &nonce, b"", &longer),
            Err(CryptoError::Decryption)
        );
    }

    #[test]
    fn different_nonces_give_different_ciphertexts() {
        let key = test_key("pw", &Salt::from_bytes([9; SALT_LEN]));
        let a = encrypt(&key, &Nonce::random().unwrap(), b"", b"same").unwrap();
        let b = encrypt(&key, &Nonce::random().unwrap(), b"", b"same").unwrap();
        assert_ne!(a, b);
    }

    /// Pinned XChaCha20-Poly1305 output with a fixed key, cross-checked with
    /// an independent implementation (libsodium via PyNaCl).
    #[test]
    fn xchacha20poly1305_regression_vector() {
        let key = VaultKey(Zeroizing::new(*b"0123456789abcdef0123456789abcdef"));
        let nonce = Nonce::from_bytes(*b"llavero-nonce-24-bytes!!");
        let ct = encrypt(&key, &nonce, b"aad", b"plaintext").unwrap();
        assert_eq!(
            hex(&ct),
            "522102b1d9b2195c84c9ed780b83ec95cd9f0411409fb331d1"
        );
    }
}
