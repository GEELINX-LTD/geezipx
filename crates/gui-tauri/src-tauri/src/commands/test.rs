//! `test_archive` command — verify archive integrity.

use std::path::PathBuf;

use serde::Serialize;
use tokio::task::spawn_blocking;

use geezipx_core::detect::ArchiveFormat;
use geezipx_core::test::{verify_archive_reader, verify_single_stream};

use crate::commands::list::{detect_archive_format, open_reader};

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
/// - For archive-based formats (zip, tar, tar.gz, tar.zst, tar.xz, 7z, rar):
///   iterates every entry and streams its content to sink, triggering format-level
///   integrity checks.
/// - For single-stream formats (gzip, bzip2, zstd, xz, lzma): decodes the full stream
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

        match format {
            // Single-stream formats: use verify_single_stream
            ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
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
