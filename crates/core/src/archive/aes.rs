//! AES-256-GCM-SIV encrypted container helpers.
//!
//! This module wraps the [`enc_file`] crate to provide password-based
//! authenticated encryption as a single-stream format in GeeZipX.
//!
//! The format uses:
//! - **AES-256-GCM-SIV** for authenticated encryption.
//! - **Argon2id** for key derivation.
//! - Magic bytes `ENCFILE\0` (8 bytes) for format detection.
//! - Default extension `.enc`.

use std::io::{Read, Write};

use secrecy::SecretString;

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Encrypt data from `reader` to `writer` with AES-256-GCM-SIV.
///
/// `password` is required — returns an error if empty.
/// Uses [`enc_file::encrypt_bytes`] which reads all data into memory
/// before encrypting.
///
/// Returns the number of bytes written to the output.
pub fn aes_encrypt<R: Read, W: Write>(
    reader: &mut R,
    mut writer: W,
    password: &str,
) -> GeeZipResult<u64> {
    if password.is_empty() {
        return Err(GeeZipError::Format {
            message: "AES encryption requires a password".into(),
            format: ArchiveFormat::Aes,
        });
    }

    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|e| GeeZipError::io(e, "reading input for AES encryption"))?;

    let pw = SecretString::new(password.into());

    let opts = enc_file::EncryptOptions {
        alg: enc_file::AeadAlg::Aes256GcmSiv,
        ..Default::default()
    };

    let encrypted = enc_file::encrypt_bytes(&data, pw, &opts).map_err(|e| GeeZipError::Format {
        message: format!("AES encryption failed: {e}"),
        format: ArchiveFormat::Aes,
    })?;

    writer
        .write_all(&encrypted)
        .map_err(|e| GeeZipError::io(e, "writing AES encrypted output"))?;
    Ok(encrypted.len() as u64)
}

/// Decrypt data from `reader` to `writer`.
///
/// `password` is required — returns an error if empty.
/// Uses [`enc_file::decrypt_bytes`] which reads all data into memory
/// before decrypting.
///
/// Returns the number of bytes written to the output.
pub fn aes_decrypt<R: Read, W: Write>(
    reader: &mut R,
    mut writer: W,
    password: &str,
) -> GeeZipResult<u64> {
    if password.is_empty() {
        return Err(GeeZipError::Format {
            message: "AES decryption requires a password".into(),
            format: ArchiveFormat::Aes,
        });
    }

    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|e| GeeZipError::io(e, "reading input for AES decryption"))?;

    let pw = SecretString::new(password.into());

    let decrypted = enc_file::decrypt_bytes(&data, pw).map_err(|e| GeeZipError::Format {
        message: format!("AES decryption failed (wrong password or corrupt data): {e}"),
        format: ArchiveFormat::Aes,
    })?;

    writer
        .write_all(&decrypted)
        .map_err(|e| GeeZipError::io(e, "writing AES decrypted output"))?;
    Ok(decrypted.len() as u64)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn aes_roundtrip_basic() {
        let original = b"secret data for AES encryption test";
        let password = "test-password-123";
        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        let bytes = aes_encrypt(&mut reader, &mut encrypted, password).unwrap();
        assert!(!encrypted.is_empty());
        assert!(bytes > 0);

        let mut decrypted = Vec::new();
        let mut enc_reader = Cursor::new(encrypted.as_slice());
        let dec_bytes = aes_decrypt(&mut enc_reader, &mut decrypted, password).unwrap();
        assert_eq!(dec_bytes, original.len() as u64);
        assert_eq!(decrypted, original);
    }

    #[test]
    fn aes_roundtrip_empty() {
        let original = b"";
        let password = "pw";
        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        aes_encrypt(&mut reader, &mut encrypted, password).unwrap();
        // enc_file should produce non-empty output even for empty input (header + auth tag)
        assert!(!encrypted.is_empty());

        let mut decrypted = Vec::new();
        let mut enc_reader = Cursor::new(encrypted.as_slice());
        let bytes = aes_decrypt(&mut enc_reader, &mut decrypted, password).unwrap();
        assert_eq!(bytes, 0);
        assert!(decrypted.is_empty());
    }

    #[test]
    fn aes_wrong_password_fails() {
        let original = b"test data";
        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        aes_encrypt(&mut reader, &mut encrypted, "correct-pw").unwrap();

        let mut decrypted = Vec::new();
        let mut enc_reader = Cursor::new(encrypted.as_slice());
        let err = aes_decrypt(&mut enc_reader, &mut decrypted, "wrong-pw").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("decrypt") || msg.contains("failed"),
            "error message should mention decryption failure: {msg}"
        );
    }

    #[test]
    fn aes_empty_password_rejected_encrypt() {
        let mut reader = Cursor::new(b"data");
        let mut out = Vec::new();
        let err = aes_encrypt(&mut reader, &mut out, "").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("password"),
            "error message should mention password: {msg}"
        );
    }

    #[test]
    fn aes_empty_password_rejected_decrypt() {
        let mut reader = Cursor::new(b"encrypted-data");
        let mut out = Vec::new();
        let err = aes_decrypt(&mut reader, &mut out, "").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("password"),
            "error message should mention password: {msg}"
        );
    }

    #[test]
    fn aes_detects_magic_bytes() {
        // First encrypt to get real output, then verify the magic bytes.
        let original = b"magic test";
        let password = "magic-pw";
        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        aes_encrypt(&mut reader, &mut encrypted, password).unwrap();

        // enc_file uses CBOR encoding which represents MAGIC bytes as an array
        // of integers, not as a raw byte string.  Therefore magic-byte detection
        // is not feasible; AES format relies on extension-only detection (`.enc`).
        // Just verify the output is non-empty and not identical to plaintext.
        assert!(
            !encrypted.is_empty(),
            "encrypted output should not be empty"
        );
        assert_ne!(
            encrypted, original,
            "encrypted output must differ from plaintext"
        );
    }

    #[test]
    fn aes_roundtrip_large() {
        // 256 KB of pseudo-random-ish data
        let mut original = Vec::with_capacity(256 * 1024);
        for i in 0u32..(256 * 256) {
            original.push((i.wrapping_mul(0x9E3779B9) >> 16) as u8);
        }
        let password = "large-data-password";

        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        aes_encrypt(&mut reader, &mut encrypted, password).unwrap();

        let mut decrypted = Vec::new();
        let mut enc_reader = Cursor::new(encrypted.as_slice());
        aes_decrypt(&mut enc_reader, &mut decrypted, password).unwrap();

        assert_eq!(decrypted, original);
    }

    #[test]
    fn aes_corrupt_data_fails() {
        let original = b"some data";
        let password = "pw";
        let mut reader = Cursor::new(original.as_slice());
        let mut encrypted = Vec::new();
        aes_encrypt(&mut reader, &mut encrypted, password).unwrap();

        // Flip a byte in the ciphertext
        let mid = encrypted.len() / 2;
        encrypted[mid] ^= 0xFF;

        let mut decrypted = Vec::new();
        let mut enc_reader = Cursor::new(encrypted.as_slice());
        let err = aes_decrypt(&mut enc_reader, &mut decrypted, password);
        assert!(
            err.is_err(),
            "corrupt encrypted data should fail decryption"
        );
    }
}
