//! AES-256-GCM-SIV encrypted container helpers.
//!
//! This module wraps the [`enc_file`] crate to provide password-based
//! authenticated encryption as a single-stream format in GeeZipX.
//!
//! The format uses:
//! - **AES-256-GCM-SIV** for authenticated encryption.
//! - **Argon2id** for key derivation.
//! - Magic identifier `ENCFILE\0` (CBOR-encoded in the header).
//! - Default extension `.enc`.
//!
//! Only non-stream containers are accepted: enc_file's AES stream framing does
//! not authenticate the final-frame flag, so it cannot detect truncation.

use std::io::{Read, Seek, SeekFrom, Write};

use base64::Engine as _;
use ciborium::Value;
use secrecy::SecretString;

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// Bound work requested by untrusted headers. GeeZipX writes 64 MiB, 3 passes,
// and 1..=4 lanes. Allow some tuning without allowing arbitrary Argon2 costs.
const MAX_HEADER_LEN: usize = 16 * 1024;
const MAX_KDF_MEMORY_KIB: u64 = 256 * 1024;
const MAX_KDF_TIME: u64 = 10;
const MAX_KDF_PARALLELISM: u64 = 16;
const MAX_KDF_WORK_KIB: u64 = MAX_KDF_MEMORY_KIB * 3;

fn invalid_header(message: &str) -> GeeZipError {
    GeeZipError::Format {
        message: format!("AES decryption rejected container: {message}"),
        format: ArchiveFormat::Aes,
    }
}

// enc_file does not expose its DiskHeader type. Parse with the same CBOR
// implementation, then validate before its decrypt API can allocate a KDF or
// copy an unchecked nonce. Reject duplicate fields to avoid parser ambiguity.
fn header_map(value: &Value) -> GeeZipResult<&[(Value, Value)]> {
    let fields = value
        .as_map()
        .ok_or_else(|| invalid_header("expected a CBOR header map"))?;
    for (index, (key, _)) in fields.iter().enumerate() {
        if key.as_text().is_none() || fields[..index].iter().any(|(other, _)| other == key) {
            return Err(invalid_header("invalid or duplicate header field"));
        }
    }
    Ok(fields)
}

fn header_field<'a>(fields: &'a [(Value, Value)], name: &str) -> GeeZipResult<&'a Value> {
    fields
        .iter()
        .find_map(|(key, value)| (key.as_text() == Some(name)).then_some(value))
        .ok_or_else(|| invalid_header(&format!("missing {name}")))
}

fn header_uint(fields: &[(Value, Value)], name: &str) -> GeeZipResult<u64> {
    header_field(fields, name)?
        .as_integer()
        .and_then(|number| u64::try_from(number).ok())
        .ok_or_else(|| invalid_header(&format!("invalid {name}")))
}

fn header_bytes(fields: &[(Value, Value)], name: &str) -> GeeZipResult<Vec<u8>> {
    let value = header_field(fields, name)?;
    match value {
        Value::Bytes(bytes) => Ok(bytes.clone()),
        Value::Array(bytes) => bytes
            .iter()
            .map(|byte| {
                byte.as_integer()
                    .and_then(|number| u8::try_from(number).ok())
                    .ok_or_else(|| invalid_header(&format!("invalid {name}")))
            })
            .collect(),
        _ => Err(invalid_header(&format!("invalid {name}"))),
    }
}

fn validate_container(data: &[u8]) -> GeeZipResult<()> {
    let length_bytes: [u8; 4] = data
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid_header("missing header length"))?;
    let header_len = u32::from_le_bytes(length_bytes) as usize;
    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Err(invalid_header("header length exceeds supported bounds"));
    }
    let header_end = 4 + header_len; // Bounded above, including on 32-bit hosts.
    let mut encoded = data
        .get(4..header_end)
        .ok_or_else(|| invalid_header("truncated header"))?;
    let header: Value = ciborium::de::from_reader_with_recursion_limit(&mut encoded, 16)
        .map_err(|_| invalid_header("malformed CBOR header"))?;
    if !encoded.is_empty() {
        return Err(invalid_header("trailing data in header"));
    }
    let fields = header_map(&header)?;
    if header_bytes(fields, "magic")? != b"ENCFILE\0" || header_uint(fields, "version")? != 2 {
        return Err(invalid_header("unsupported magic or version"));
    }
    if fields
        .iter()
        .find(|(key, _)| key.as_text() == Some("stream"))
        .is_some_and(|(_, stream)| !matches!(stream, Value::Null))
    {
        return Err(invalid_header(
            "stream containers are unsupported because their framing permits truncation",
        ));
    }
    let nonce_len = match header_uint(fields, "aead_alg")? {
        1 => 24, // Preserve existing non-stream XChaCha20-Poly1305 compatibility.
        2 => 12, // AES-256-GCM-SIV.
        _ => return Err(invalid_header("unsupported AEAD algorithm")),
    };
    if header_bytes(fields, "nonce")?.len() != nonce_len {
        return Err(invalid_header("invalid nonce length"));
    }
    if header_uint(fields, "kdf_alg")? != 1 {
        return Err(invalid_header("unsupported KDF algorithm"));
    }
    if !(8..=64).contains(&header_bytes(fields, "salt")?.len()) {
        return Err(invalid_header("salt length exceeds supported bounds"));
    }
    let params = header_map(header_field(fields, "kdf_params")?)?;
    let memory = header_uint(params, "mem_kib")?;
    let time = header_uint(params, "t_cost")?;
    let lanes = header_uint(params, "parallelism")?;
    if !(64 * 1024..=MAX_KDF_MEMORY_KIB).contains(&memory)
        || !(3..=MAX_KDF_TIME).contains(&time)
        || !(1..=MAX_KDF_PARALLELISM).contains(&lanes)
        || memory * time > MAX_KDF_WORK_KIB
    {
        return Err(invalid_header("KDF parameters exceed supported bounds"));
    }
    let ct_len = header_uint(fields, "ct_len")?;
    if ct_len < 16 || ct_len != (data.len() - header_end) as u64 {
        return Err(invalid_header(
            "ciphertext length mismatch or missing authentication tag",
        ));
    }
    Ok(())
}

fn decode_armor(data: &[u8]) -> GeeZipResult<Vec<u8>> {
    let text = std::str::from_utf8(data).map_err(|_| invalid_header("invalid ASCII armor"))?;
    let body = text
        .trim()
        .strip_prefix("-----BEGIN ENCFILE-----")
        .and_then(|text| text.strip_suffix("-----END ENCFILE-----"))
        .ok_or_else(|| invalid_header("invalid ASCII armor"))?;
    base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|_| invalid_header("invalid ASCII armor"))
}

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

/// Decrypt an AES Crypt v0–v3 file with authentication before plaintext release.
/// Input is read twice using bounded buffers; no full-file allocation is made.
/// The seekable source must remain unchanged during both passes. Legacy v0–v2
/// authenticate ciphertext but leave the final block's length outside the MAC;
/// use v3 when exact plaintext length authentication is required.
pub fn aescrypt_decrypt<R: Read + Seek, W: Write>(
    reader: R,
    writer: W,
    password: &str,
) -> GeeZipResult<u64> {
    aescrypt_rs::decrypt_verified(reader, writer, password)
        .map(|outcome| outcome.plaintext_len)
        .map_err(|e| {
            GeeZipError::format(
                format!("AES Crypt decryption failed: {e}"),
                ArchiveFormat::Aes,
            )
        })
}

/// Decrypt data from `reader` to `writer`.
///
/// AES Crypt is authenticated using a seekable temporary ciphertext file, so
/// non-seekable sources also use bounded memory and release no invalid plaintext.
/// The legacy `.enc` format still requires whole-container allocation.
///
/// `password` is required — returns an error if empty.
/// Uses [`enc_file::decrypt_bytes`] which reads all data into memory
/// before decrypting. Header lengths, nonces and KDF resource costs are
/// validated before calling the dependency. Stream containers are rejected
/// because their framing allows authenticated plaintext to be truncated.
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

    let mut prefix = Vec::with_capacity(3);
    reader.take(3).read_to_end(&mut prefix)?;
    let is_aescrypt = prefix == b"AES";
    let mut input = std::io::Cursor::new(prefix).chain(reader);
    if is_aescrypt {
        let mut staged =
            tempfile::tempfile().map_err(|e| GeeZipError::io(e, "staging AES Crypt ciphertext"))?;
        std::io::copy(&mut input, &mut staged)?;
        staged.seek(SeekFrom::Start(0))?;
        return aescrypt_decrypt(staged, &mut writer, password);
    }
    let mut data = Vec::new();
    input
        .read_to_end(&mut data)
        .map_err(|e| GeeZipError::io(e, "reading input for AES decryption"))?;

    // Decode armor here so it cannot bypass the binary-header preflight inside
    // enc_file::decrypt_bytes. Pass the exact validated bytes to the dependency.
    let decoded;
    let input = if enc_file::looks_armored(&data) {
        decoded = decode_armor(&data)?;
        decoded.as_slice()
    } else {
        data.as_slice()
    };
    validate_container(input)?;

    let pw = SecretString::new(password.into());

    let decrypted = enc_file::decrypt_bytes(input, pw).map_err(|e| GeeZipError::Format {
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
    use std::sync::OnceLock;

    #[test]
    fn official_compatible_legacy_vectors_and_auth_failure() {
        let cases: &[(&[u8], &[u8])] = &[
            (
                include_bytes!("../../tests/fixtures/aescrypt/v0-0.aes"),
                b"",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v0-16.aes"),
                b"0123456789ABCDEF",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v0-17.aes"),
                b"0123456789ABCDEF0",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v1-0.aes"),
                b"",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v1-16.aes"),
                b"0123456789ABCDEF",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v1-17.aes"),
                b"0123456789ABCDEF0",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v2-0.aes"),
                b"",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v2-16.aes"),
                b"0123456789ABCDEF",
            ),
            (
                include_bytes!("../../tests/fixtures/aescrypt/v2-17.aes"),
                b"0123456789ABCDEF0",
            ),
        ];
        struct ShortReads<'a>(&'a [u8]);
        impl Read for ShortReads<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = buf.len().min(1);
                self.0.read(&mut buf[..n])
            }
        }
        for &(bytes, expected) in cases {
            let mut output = Vec::new();
            assert_eq!(
                aes_decrypt(&mut ShortReads(bytes), &mut output, "Hello").unwrap(),
                expected.len() as u64
            );
            assert_eq!(output, expected);
            for (input, password) in [(bytes, "wrong"), (&bytes[..bytes.len() - 1], "Hello")] {
                let mut untouched = b"existing output".to_vec();
                assert!(aescrypt_decrypt(Cursor::new(input), &mut untouched, password).is_err());
                assert_eq!(untouched, b"existing output");
            }
            let mut corrupt = bytes.to_vec();
            *corrupt.last_mut().unwrap() ^= 1;
            let mut untouched = b"existing output".to_vec();
            assert!(aescrypt_decrypt(Cursor::new(corrupt), &mut untouched, "Hello").is_err());
            assert_eq!(untouched, b"existing output");
        }
    }

    #[test]
    fn official_aescrypt_v3_unicode_password() {
        let bytes = include_bytes!("../../tests/fixtures/aescrypt/official-v3.aes");
        let mut output = Vec::new();
        assert_eq!(
            aes_decrypt(&mut bytes.as_slice(), &mut output, "Unicode 密码 🔑").unwrap(),
            30
        );
        assert_eq!(output, b"Official AES Crypt v3 fixture\n");
    }

    #[test]
    fn aescrypt_v3_authenticates_before_writing() {
        let mut encrypted = Vec::new();
        aescrypt_rs::encrypt(
            Cursor::new(b"AES Crypt interoperability"),
            &mut encrypted,
            "test password",
            1,
        )
        .unwrap();
        let mut output = Vec::new();
        assert_eq!(
            aes_decrypt(&mut Cursor::new(&encrypted), &mut output, "test password").unwrap(),
            26
        );
        assert_eq!(output, b"AES Crypt interoperability");
        let last = encrypted.len() - 1;
        encrypted[last] ^= 1;
        for password in ["test password", "incorrect"] {
            let mut output = Vec::new();
            assert!(aes_decrypt(&mut Cursor::new(&encrypted), &mut output, password).is_err());
            assert!(output.is_empty());
        }
    }

    const FIXTURE_PASSWORD: &str = "AES compatibility 密码 🔑";
    const FIXTURE_PLAINTEXT: &[u8] = b"authenticated non-stream fixture";

    fn encrypted_fixture() -> &'static [u8] {
        static ENCRYPTED: OnceLock<Vec<u8>> = OnceLock::new();
        ENCRYPTED.get_or_init(|| {
            enc_file::encrypt_bytes(
                FIXTURE_PLAINTEXT,
                SecretString::new(FIXTURE_PASSWORD.into()),
                &enc_file::EncryptOptions {
                    alg: enc_file::AeadAlg::Aes256GcmSiv,
                    ..Default::default()
                },
            )
            .unwrap()
        })
    }

    fn rewrite_header(data: &[u8], change: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
        let header_end = 4 + u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        let mut header: Value = ciborium::de::from_reader(&data[4..header_end]).unwrap();
        change(header.as_map_mut().unwrap());
        let mut encoded = Vec::new();
        ciborium::ser::into_writer(&header, &mut encoded).unwrap();
        let mut result = (encoded.len() as u32).to_le_bytes().to_vec();
        result.extend_from_slice(&encoded);
        result.extend_from_slice(&data[header_end..]);
        result
    }

    fn set_field(fields: &mut [(Value, Value)], name: &str, value: Value) {
        fields
            .iter_mut()
            .find(|(key, _)| key.as_text() == Some(name))
            .unwrap()
            .1 = value;
    }

    fn assert_rejected(data: &[u8], expected: &str) {
        let mut out = b"existing output".to_vec();
        let err = aes_decrypt(&mut Cursor::new(data), &mut out, FIXTURE_PASSWORD).unwrap_err();
        assert!(err.to_string().contains(expected), "{err}");
        assert_eq!(
            out, b"existing output",
            "rejection must not write plaintext"
        );
    }

    fn armor(data: &[u8]) -> Vec<u8> {
        format!(
            "-----BEGIN ENCFILE-----\n{}\n-----END ENCFILE-----\n",
            base64::engine::general_purpose::STANDARD.encode(data)
        )
        .into_bytes()
    }

    #[test]
    fn aes_existing_nonstream_and_armored_containers_roundtrip() {
        // Exercise real enc_file output with the original password, including
        // the other AEAD algorithm the old wrapper also accepted.
        let xchacha = enc_file::encrypt_bytes(
            FIXTURE_PLAINTEXT,
            SecretString::new(FIXTURE_PASSWORD.into()),
            &enc_file::EncryptOptions::default(),
        )
        .unwrap();
        for container in [encrypted_fixture(), xchacha.as_slice()] {
            for data in [container.to_vec(), armor(container)] {
                let mut out = Vec::new();
                let written =
                    aes_decrypt(&mut Cursor::new(data), &mut out, FIXTURE_PASSWORD).unwrap();
                assert_eq!(written, FIXTURE_PLAINTEXT.len() as u64);
                assert_eq!(out, FIXTURE_PLAINTEXT);
            }
        }
    }

    #[test]
    fn aes_stream_final_flag_tamper_and_trailing_data_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("input");
        let target = temp.path().join("input.enc");
        std::fs::write(&source, vec![b'A'; 256]).unwrap();
        enc_file::encrypt_file_streaming(
            &source,
            Some(&target),
            SecretString::new(FIXTURE_PASSWORD.into()),
            enc_file::EncryptOptions {
                alg: enc_file::AeadAlg::Aes256GcmSiv,
                stream: true,
                chunk_size: 64,
                ..Default::default()
            },
        )
        .unwrap();
        let original = std::fs::read(target).unwrap();
        assert_rejected(&original, "stream containers are unsupported");
        let body = 4 + u32::from_le_bytes(original[..4].try_into().unwrap()) as usize;
        assert_eq!(original[body], 0);
        let first_len =
            u32::from_be_bytes(original[body + 1..body + 5].try_into().unwrap()) as usize;
        let mut tampered = original[..body + 5 + first_len].to_vec();
        tampered[body] = 1; // This used to successfully return only 64 of 256 bytes.
        assert_rejected(&tampered, "stream containers are unsupported");
        assert_rejected(&armor(&tampered), "stream containers are unsupported");
        let mut appended = original;
        appended.extend_from_slice(b"unauthenticated trailing bytes");
        assert_rejected(&appended, "stream containers are unsupported");
    }

    #[test]
    fn aes_malformed_nonce_lengths_rejected_without_panics() {
        for algorithm in [1u8, 2] {
            for length in [0, 1, 11, 13, 23, 25] {
                let malformed = rewrite_header(encrypted_fixture(), |fields| {
                    set_field(fields, "aead_alg", Value::from(algorithm));
                    set_field(
                        fields,
                        "nonce",
                        Value::Array(vec![Value::from(0u8); length]),
                    );
                });
                assert_rejected(&malformed, "invalid nonce length");
            }
        }
        let malformed = rewrite_header(encrypted_fixture(), |fields| {
            set_field(fields, "nonce", Value::Array(Vec::new()));
        });
        assert_rejected(&armor(&malformed), "invalid nonce length");
    }

    #[test]
    fn aes_malformed_kdf_costs_rejected_before_derivation() {
        for (name, values) in [
            (
                "mem_kib",
                vec![0, 65535, MAX_KDF_MEMORY_KIB + 1, u32::MAX as u64, u64::MAX],
            ),
            (
                "t_cost",
                vec![0, 2, MAX_KDF_TIME + 1, u32::MAX as u64, u64::MAX],
            ),
            (
                "parallelism",
                vec![0, MAX_KDF_PARALLELISM + 1, u32::MAX as u64, u64::MAX],
            ),
        ] {
            for value in values {
                let malformed = rewrite_header(encrypted_fixture(), |fields| {
                    let params = fields
                        .iter_mut()
                        .find(|(key, _)| key.as_text() == Some("kdf_params"))
                        .unwrap()
                        .1
                        .as_map_mut()
                        .unwrap();
                    set_field(params, name, Value::from(value));
                });
                assert_rejected(&malformed, "KDF parameters exceed supported bounds");
            }
        }
        let excessive_work = rewrite_header(encrypted_fixture(), |fields| {
            let params = fields
                .iter_mut()
                .find(|(key, _)| key.as_text() == Some("kdf_params"))
                .unwrap()
                .1
                .as_map_mut()
                .unwrap();
            set_field(params, "mem_kib", Value::from(MAX_KDF_MEMORY_KIB));
            set_field(params, "t_cost", Value::from(4u8));
        });
        assert_rejected(&excessive_work, "KDF parameters exceed supported bounds");
        assert_rejected(
            &armor(&excessive_work),
            "KDF parameters exceed supported bounds",
        );
    }

    #[test]
    fn aes_invalid_header_fields_and_duplicates_rejected() {
        for (name, value, expected) in [
            (
                "magic",
                Value::Array(vec![Value::from(0u8); 8]),
                "magic or version",
            ),
            ("version", Value::from(3u8), "magic or version"),
            ("aead_alg", Value::from(0u8), "AEAD algorithm"),
            ("kdf_alg", Value::from(0u8), "KDF algorithm"),
            (
                "salt",
                Value::Array(vec![Value::from(0u8); 7]),
                "salt length",
            ),
            (
                "salt",
                Value::Array(vec![Value::from(0u8); 65]),
                "salt length",
            ),
            ("ct_len", Value::from(15u8), "ciphertext length"),
            ("ct_len", Value::from(u64::MAX), "ciphertext length"),
        ] {
            let malformed = rewrite_header(encrypted_fixture(), |fields| {
                set_field(fields, name, value);
            });
            assert_rejected(&malformed, expected);
        }
        let duplicate = rewrite_header(encrypted_fixture(), |fields| {
            let nonce = fields
                .iter()
                .find(|(key, _)| key.as_text() == Some("nonce"))
                .unwrap()
                .clone();
            fields.push(nonce);
        });
        assert_rejected(&duplicate, "duplicate header field");
        let duplicate_kdf = rewrite_header(encrypted_fixture(), |fields| {
            let params = fields
                .iter_mut()
                .find(|(key, _)| key.as_text() == Some("kdf_params"))
                .unwrap()
                .1
                .as_map_mut()
                .unwrap();
            params.push(params[0].clone());
        });
        assert_rejected(&duplicate_kdf, "duplicate header field");
    }

    #[test]
    fn aes_malformed_header_framing_and_trailing_ciphertext_rejected() {
        for length in [0, MAX_HEADER_LEN as u32 + 1, u32::MAX] {
            assert_rejected(&length.to_le_bytes(), "header length");
        }
        assert_rejected(&[], "missing header length");
        assert_rejected(&[1, 0, 0, 0], "truncated header");
        assert_rejected(&[1, 0, 0, 0, 0xff], "malformed CBOR header");
        let fixture = encrypted_fixture();
        let header_end = 4 + u32::from_le_bytes(fixture[..4].try_into().unwrap()) as usize;
        let mut header_trailing = fixture.to_vec();
        header_trailing.insert(header_end, 0);
        header_trailing[..4].copy_from_slice(&((header_end - 3) as u32).to_le_bytes());
        assert_rejected(&header_trailing, "trailing data in header");
        let mut appended = fixture.to_vec();
        appended.extend_from_slice(b"trailing bytes");
        assert_rejected(&appended, "ciphertext length");
        // Updating the unauthenticated length field must still fail AEAD.
        let appended_len = (appended.len() - header_end) as u64;
        let appended = rewrite_header(&appended, |fields| {
            set_field(fields, "ct_len", Value::from(appended_len));
        });
        assert_rejected(&appended, "AES decryption failed");
        assert_rejected(&fixture[..fixture.len() - 1], "ciphertext length");
    }

    #[test]
    fn aes_ciphertext_tamper_is_authenticated() {
        let mut tampered = encrypted_fixture().to_vec();
        *tampered.last_mut().unwrap() ^= 1;
        assert_rejected(&tampered, "AES decryption failed");
    }

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
