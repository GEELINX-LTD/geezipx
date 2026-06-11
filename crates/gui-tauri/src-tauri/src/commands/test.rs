//! `test_archive` command — verify archive integrity.

use std::path::PathBuf;

use serde::Serialize;
use tokio::task::spawn_blocking;

use geezipx_core::archive::cpio::verify_cpio_archive;
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::test::{verify_archive_reader, verify_single_stream};

use crate::commands::list::{detect_archive_format, open_reader, validate_read_password_support};

/// Result of an archive integrity test.
#[derive(Debug, Serialize)]
pub struct TestArchiveResult {
    /// Detected format name.
    pub format: String,
    /// Number of entries processed.
    pub entry_count: u64,
    /// Uncompressed bytes read.
    pub bytes_read: u64,
    /// Whether per-entry CRC-32 checksums were validated.
    pub crc32_verified: bool,
}

/// Test/verify the integrity of an archive.
///
/// - For archive-based formats (zip, tar, tar.gz, tar.bz2, tar.br, tar.lz4, tar.zst, tar.xz, 7z, rar):
///   iterates every entry and streams its content to sink, triggering format-level
///   integrity checks.
/// - For single-stream formats (gzip, bzip2, brotli, lz4, zstd, xz, lzma): decodes the full stream
///   to sink and validates the checksum footer.
///
/// Returns an error if the archive is corrupt or the format is not supported.
#[tauri::command]
pub async fn test_archive(
    archive_path: String,
    password: Option<String>,
) -> Result<TestArchiveResult, String> {
    let path_buf = PathBuf::from(&archive_path);
    let pwd = password;

    spawn_blocking(move || {
        let format = detect_archive_format(&path_buf)?;
        validate_read_password_support(format, pwd.as_deref())?;

        match format {
            // Single-stream formats: use verify_single_stream
            ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Brotli
            | ArchiveFormat::Lz4
            | ArchiveFormat::Zstd
            | ArchiveFormat::Xz
            | ArchiveFormat::Lzma => {
                let report = verify_single_stream(&path_buf, format)
                    .map_err(|e| format!("Verification failed: {}", e))?;
                Ok(TestArchiveResult {
                    format: format.to_string(),
                    entry_count: report.entry_count,
                    bytes_read: report.bytes_read,
                    crc32_verified: report.crc32_verified,
                })
            }
            ArchiveFormat::Cpio => {
                let report = verify_cpio_archive(&path_buf)
                    .map_err(|e| format!("Verification failed: {}", e))?;

                Ok(TestArchiveResult {
                    format: format.to_string(),
                    entry_count: report.entry_count,
                    bytes_read: report.bytes_read,
                    crc32_verified: report.crc32_verified,
                })
            }
            // Archive-based formats: create reader + verify_archive_reader
            _ => {
                let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;

                let report = verify_archive_reader(&mut *reader)
                    .map_err(|e| format!("Verification failed: {}", e))?;

                Ok(TestArchiveResult {
                    format: format.to_string(),
                    entry_count: report.entry_count,
                    bytes_read: report.bytes_read,
                    crc32_verified: report.crc32_verified,
                })
            }
        }
    })
    .await
    .map_err(|e| format!("Internal error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::test_archive;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn push_newc_hex(out: &mut Vec<u8>, value: usize) {
        out.extend_from_slice(format!("{value:08x}").as_bytes());
    }

    fn pad_newc(out: &mut Vec<u8>) {
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }

    fn build_test_cpio(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();

        for (path, data) in entries {
            let mode = 0o100644usize;
            out.extend_from_slice(b"070701");
            push_newc_hex(&mut out, 1);
            push_newc_hex(&mut out, mode);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, 1);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, data.len());
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, 0);
            push_newc_hex(&mut out, path.len() + 1);
            push_newc_hex(&mut out, 0);
            out.extend_from_slice(path.as_bytes());
            out.push(0);
            pad_newc(&mut out);
            out.extend_from_slice(data);
            pad_newc(&mut out);
        }

        out.extend_from_slice(b"070701");
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 1);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, 0);
        push_newc_hex(&mut out, "TRAILER!!!".len() + 1);
        push_newc_hex(&mut out, 0);
        out.extend_from_slice(b"TRAILER!!!\0");
        pad_newc(&mut out);

        out
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        dir.push(format!("geezipx-gui-test-{prefix}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_archive_rejects_password_for_cpio() {
        let root = unique_test_dir("cpio-password");
        let archive = root.join("archive.cpio");
        fs::write(&archive, build_test_cpio(&[("hello.txt", b"hello")])).unwrap();

        let err = test_archive(
            archive.to_string_lossy().to_string(),
            Some("secret".to_string()),
        )
        .await
        .unwrap_err();

        assert!(err.contains("Password is only supported for ZIP, 7z, and RAR"));
        assert!(err.contains("cpio"));

        let _ = fs::remove_dir_all(root);
    }
}
