//! `geezipx test` — verify archive / compression-stream integrity.
//!
//! Reads each entry to completion without extracting to disk and reports
//! whether the archive is structurally sound.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::detect::ArchiveFormat;

use super::common;
use geezipx_core::test::{verify_archive_reader, verify_single_stream, TestReport};

/// Execute the `test` subcommand.
pub fn execute(archive: &Path, json: bool, password: Option<String>) -> Result<()> {
    let result = run_verify(archive, json, password);

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            if json {
                // Print JSON error to stdout and exit directly to avoid
                // main() printing the error to stderr and breaking JSON.
                let err_output = serde_json::json!({
                    "archive": archive.to_string_lossy(),
                    "ok": false,
                    "error": format!("{e:#}"),
                });
                println!("{err_output}");
                std::process::exit(1);
            }
            Err(e)
        }
    }
}

/// The actual verification logic, separated so `execute` can handle JSON
/// failure gracefully before propagating errors to main.
fn run_verify(archive: &Path, json: bool, password: Option<String>) -> Result<()> {
    if !archive.exists() {
        anyhow::bail!("archive '{}' does not exist", archive.display());
    }

    let format = common::detect_archive_format(archive)?;

    // Validate password: single-stream formats do not support encryption.
    if password.is_some()
        && matches!(
            format,
            ArchiveFormat::Gzip
                | ArchiveFormat::Bzip2
                | ArchiveFormat::Brotli
                | ArchiveFormat::Lz4
                | ArchiveFormat::Zstd
                | ArchiveFormat::Xz
                | ArchiveFormat::Lzma
        )
    {
        anyhow::bail!(
            "--password is only supported for ZIP, 7z, and RAR formats; '{}' does not support encryption",
            format
        );
    }

    let fs_metadata_len = fs::metadata(archive).map(|m| m.len()).unwrap_or(0);

    let report = match format {
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma => verify_single_stream(archive, format)
            .with_context(|| format!("verifying '{}'", archive.display()))?,
        ArchiveFormat::Zip
        | ArchiveFormat::SevenZip
        | ArchiveFormat::Rar
        | ArchiveFormat::Asar
        | ArchiveFormat::Deb
        | ArchiveFormat::Lzh
        | ArchiveFormat::Tar
        | ArchiveFormat::TarGz
        | ArchiveFormat::TarBz2
        | ArchiveFormat::TarBr
        | ArchiveFormat::TarLz4
        | ArchiveFormat::TarZst
        | ArchiveFormat::TarXz => {
            let mut reader = common::open_reader(archive, format, password.as_deref())?;
            verify_archive_reader(&mut *reader)
                .with_context(|| format!("verifying '{}'", archive.display()))?
        }
        _ => anyhow::bail!("unsupported format for verification: {format}"),
    };

    if json {
        print_json(&report, archive, fs_metadata_len)?;
    } else {
        print_text(&report, archive, fs_metadata_len)?;
    }

    Ok(())
}

/// Print test results as human-readable text.
fn print_text(report: &TestReport, archive: &Path, compressed_size: u64) -> Result<()> {
    let status = "ok";
    println!("Archive: {}", archive.display());
    println!("Format:  {}", report.format);
    println!("Status:  {status}");
    println!("Entries: {}", report.entry_count);
    println!(
        "Size:    {} bytes (compressed: {} bytes)",
        report.bytes_read, compressed_size
    );
    let (integrity_label, integrity_status) = match report.format {
        ArchiveFormat::Zip if report.crc32_verified => ("CRC-32", "verified"),
        ArchiveFormat::Lzh => ("Integrity", "verified (CRC-16)"),
        _ => ("Integrity", "verified"),
    };
    println!("{integrity_label}:  {integrity_status}");

    // On failure the function would have returned Err above;
    // reaching here means the archive passed.
    println!("result: OK");
    Ok(())
}

/// Print test results as JSON.
fn print_json(report: &TestReport, archive: &Path, compressed_size: u64) -> Result<()> {
    let output = serde_json::json!({
        "archive": archive.to_string_lossy(),
        "format": report.format.to_string(),
        "ok": true,
        "entry_count": report.entry_count,
        "bytes_read": report.bytes_read,
        "compressed_size": compressed_size,
        "crc32_verified": report.crc32_verified,
    });
    println!("{output}");
    Ok(())
}
