//! Fixed-size binary header of the `.vault` file (format v1).
//! See `docs/vault-format.md` §3.

use crate::crypto::{KdfParams, NONCE_LEN, Nonce, SALT_LEN, Salt, TAG_LEN};

use super::VaultError;

/// File signature: `"LLAVERO\0"`.
pub const MAGIC: [u8; 8] = *b"LLAVERO\0";
/// Format version written by this implementation.
pub const FORMAT_VERSION: u16 = 1;
/// Header size in bytes.
pub const HEADER_LEN: usize = 92;
/// Identifier of Argon2id v0x13.
pub const KDF_ARGON2ID: u8 = 0x01;
/// Identifier of XChaCha20-Poly1305.
pub const CIPHER_XCHACHA20_POLY1305: u8 = 0x01;
/// Largest accepted ciphertext (256 MiB).
pub const MAX_CIPHERTEXT_LEN: u64 = 256 * 1024 * 1024;

/// Parsed, validated header.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub kdf: KdfParams,
    pub salt: Salt,
    pub nonce: Nonce,
    pub ciphertext_len: u64,
}

impl Header {
    /// Serializes the header. These exact bytes are the AEAD associated data.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..8].copy_from_slice(&MAGIC);
        out[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        // 10..12 flags = 0
        out[12] = KDF_ARGON2ID;
        out[13] = CIPHER_XCHACHA20_POLY1305;
        // 14..16 reserved = 0
        out[16..20].copy_from_slice(&self.kdf.m_cost_kib().to_le_bytes());
        out[20..24].copy_from_slice(&self.kdf.t_cost().to_le_bytes());
        out[24..28].copy_from_slice(&self.kdf.p_cost().to_le_bytes());
        out[28..60].copy_from_slice(self.salt.as_bytes());
        out[60..84].copy_from_slice(self.nonce.as_bytes());
        out[84..92].copy_from_slice(&self.ciphertext_len.to_le_bytes());
        out
    }

    /// Parses and validates a header. `file_len` is the total file size,
    /// which must be exactly `HEADER_LEN + ciphertext_len`.
    ///
    /// Validation order matters: identification errors first, then
    /// structural errors, then KDF bounds (checked before any Argon2 run).
    pub fn parse(bytes: &[u8], file_len: u64) -> Result<Self, VaultError> {
        let h: &[u8; HEADER_LEN] = bytes
            .get(..HEADER_LEN)
            .and_then(|s| s.try_into().ok())
            .ok_or(VaultError::NotAVault)?;
        if h[0..8] != MAGIC {
            return Err(VaultError::NotAVault);
        }
        if u16::from_le_bytes([h[8], h[9]]) != FORMAT_VERSION {
            return Err(VaultError::UnsupportedVersion);
        }
        let flags = u16::from_le_bytes([h[10], h[11]]);
        let reserved = u16::from_le_bytes([h[14], h[15]]);
        if flags != 0
            || reserved != 0
            || h[12] != KDF_ARGON2ID
            || h[13] != CIPHER_XCHACHA20_POLY1305
        {
            return Err(VaultError::InvalidHeader);
        }

        let ciphertext_len = u64::from_le_bytes(array(&h[84..92]));
        let expected_len = (HEADER_LEN as u64).checked_add(ciphertext_len);
        if ciphertext_len < TAG_LEN as u64
            || ciphertext_len > MAX_CIPHERTEXT_LEN
            || expected_len != Some(file_len)
        {
            return Err(VaultError::InvalidHeader);
        }

        let kdf = KdfParams::new(
            u32::from_le_bytes(array(&h[16..20])),
            u32::from_le_bytes(array(&h[20..24])),
            u32::from_le_bytes(array(&h[24..28])),
        )
        .map_err(|_| VaultError::InvalidKdfParams)?;

        Ok(Self {
            kdf,
            salt: Salt::from_bytes(array::<SALT_LEN>(&h[28..60])),
            nonce: Nonce::from_bytes(array::<NONCE_LEN>(&h[60..84])),
            ciphertext_len,
        })
    }
}

/// Copies a slice of known length into an array.
/// Only called with in-bounds constant ranges of the header.
fn array<const N: usize>(slice: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Header {
        Header {
            kdf: KdfParams::DEFAULT,
            salt: Salt::from_bytes([0xAA; SALT_LEN]),
            nonce: Nonce::from_bytes([0xBB; NONCE_LEN]),
            ciphertext_len: 100,
        }
    }

    const FILE_LEN: u64 = HEADER_LEN as u64 + 100;

    #[test]
    fn layout_matches_specification() {
        let b = sample().to_bytes();
        assert_eq!(&b[0..8], b"LLAVERO\0");
        assert_eq!(&b[8..10], &[1, 0]);
        assert_eq!(&b[10..12], &[0, 0]);
        assert_eq!((b[12], b[13]), (1, 1));
        assert_eq!(&b[14..16], &[0, 0]);
        assert_eq!(&b[16..20], &65_536u32.to_le_bytes());
        assert_eq!(&b[20..24], &3u32.to_le_bytes());
        assert_eq!(&b[24..28], &4u32.to_le_bytes());
        assert_eq!(&b[28..60], &[0xAA; 32]);
        assert_eq!(&b[60..84], &[0xBB; 24]);
        assert_eq!(&b[84..92], &100u64.to_le_bytes());
    }

    #[test]
    fn round_trip() {
        let h = sample();
        assert_eq!(Header::parse(&h.to_bytes(), FILE_LEN).unwrap(), h);
    }

    #[test]
    fn short_input_or_bad_magic_is_not_a_vault() {
        let b = sample().to_bytes();
        assert_eq!(
            Header::parse(&b[..91], FILE_LEN),
            Err(VaultError::NotAVault)
        );
        assert_eq!(Header::parse(&[], 0), Err(VaultError::NotAVault));
        let mut bad = b;
        bad[0] = b'X';
        assert_eq!(Header::parse(&bad, FILE_LEN), Err(VaultError::NotAVault));
    }

    #[test]
    fn unknown_version_is_unsupported() {
        let mut b = sample().to_bytes();
        b[8] = 2;
        assert_eq!(
            Header::parse(&b, FILE_LEN),
            Err(VaultError::UnsupportedVersion)
        );
    }

    #[test]
    fn invalid_structural_fields_are_rejected() {
        for (offset, value) in [(10, 1), (11, 0x80), (12, 2), (13, 0), (14, 1), (15, 1)] {
            let mut b = sample().to_bytes();
            b[offset] = value;
            assert_eq!(
                Header::parse(&b, FILE_LEN),
                Err(VaultError::InvalidHeader),
                "offset {offset}"
            );
        }
    }

    #[test]
    fn length_must_match_file_size() {
        let b = sample().to_bytes();
        for len in [FILE_LEN - 1, FILE_LEN + 1, 0, u64::MAX] {
            assert_eq!(
                Header::parse(&b, len),
                Err(VaultError::InvalidHeader),
                "len {len}"
            );
        }
        for ct_len in [0u64, 15, MAX_CIPHERTEXT_LEN + 1, u64::MAX] {
            let mut h = sample();
            h.ciphertext_len = ct_len;
            let file_len = (HEADER_LEN as u64).wrapping_add(ct_len);
            assert_eq!(
                Header::parse(&h.to_bytes(), file_len),
                Err(VaultError::InvalidHeader),
                "ct_len {ct_len}"
            );
        }
    }

    #[test]
    fn weak_or_huge_kdf_params_are_rejected() {
        for (offset, value) in [
            (16usize, 1024u32), // m_cost: 1 MiB
            (16, u32::MAX),     // m_cost: huge
            (20, 1),            // t_cost too low
            (20, 1_000_000),    // t_cost huge
            (24, 0),            // p_cost zero
            (24, 64),           // p_cost too high
        ] {
            let mut b = sample().to_bytes();
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(
                Header::parse(&b, FILE_LEN),
                Err(VaultError::InvalidKdfParams),
                "offset {offset} value {value}"
            );
        }
    }
}
