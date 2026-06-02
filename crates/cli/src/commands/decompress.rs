//! `geezipx decompress` — extract an archive or decompress a stream.

use std::fs;
use std::io::Write;
use std::path::Path;

use geezipx_core::detect::ArchiveFormat;

use anyhow::{Context, Result};

use super::common;
use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use geezipx_core::ProgressReader;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Execute the `decompress` subcommand.
pub fn execute(
    archive: &Path,
    output_dir: &Path,
    stdout: bool,
    overwrite: bool,
    no_progress: bool,
    verbose: bool,
) -> Result<()> {
    if !archive.exists() {
        anyhow::bail!("archive '{}' does not exist", archive.display());
    }

    let format = common::detect_archive_format(archive)?;

    let cancel_token = crate::signal::CancellationToken::new();

    // Ensure the output directory exists.
    if !stdout {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;
    }

    let show_progress = !no_progress && !verbose && crate::render::progress::progress_bar_enabled();

    match format {
        ArchiveFormat::Gzip => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_gzip_stdout(archive, cancel_flag)
            } else {
                decompress_gzip_to_file(archive, output_dir, overwrite, cancel_flag)
            };

            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Decompressing...",
                ))
            } else {
                None
            };

            match result {
                Ok(()) => {
                    if let Some(s) = &spinner {
                        s.finish("Decompressed");
                    }
                }
                Err(e) => {
                    if let Some(s) = &spinner {
                        s.finish("Decompression failed");
                    }
                    if cancel_token.is_cancelled() {
                        eprintln!("Cancelled");
                        std::process::exit(130);
                    }
                    return Err(anyhow::anyhow!("gzip decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Zstd => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_zstd_stdout(archive, cancel_flag)
            } else {
                decompress_zstd_to_file(archive, output_dir, overwrite, cancel_flag)
            };

            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Decompressing...",
                ))
            } else {
                None
            };

            match result {
                Ok(()) => {
                    if let Some(s) = &spinner {
                        s.finish("Decompressed");
                    }
                }
                Err(e) => {
                    if let Some(s) = &spinner {
                        s.finish("Decompression failed");
                    }
                    if cancel_token.is_cancelled() {
                        eprintln!("Cancelled");
                        std::process::exit(130);
                    }
                    return Err(anyhow::anyhow!("zstd decompression error: {}", e));
                }
            }
        }
        _ => {
            if stdout {
                anyhow::bail!(
                    "--stdout is only supported for single-stream formats (gzip, zstd); \
                     '{}' is a multi-file archive",
                    format
                );
            }
            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Extracting...",
                ))
            } else {
                None
            };

            let cancel_flag = cancel_token.clone().into_inner();
            let result = decompress_archive(
                archive,
                output_dir,
                format,
                overwrite,
                verbose,
                show_progress,
                cancel_flag,
            );
            match result {
                Ok(()) => {
                    if let Some(s) = &spinner {
                        s.finish("Extraction complete");
                    }
                }
                Err(e) => {
                    if let Some(s) = &spinner {
                        s.finish("Extraction failed");
                    }
                    if cancel_token.is_cancelled() {
                        eprintln!(
                            "Cancelled \u{2014} extracted files preserved in {}",
                            output_dir.display()
                        );
                        std::process::exit(130);
                    }
                    return Err(anyhow::anyhow!("extraction error: {}", e));
                }
            }
        }
    }

    Ok(())
}

/// Decompress a gzip stream to stdout.
fn decompress_gzip_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    // Wrap reader with cancellation support.
    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::gzip::gzip_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    // Flush stdout to ensure all bytes are written before exiting.
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a gzip file to a new file in the output directory.
fn decompress_gzip_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::gzip_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    // Check for clobber (no-clobber mode).
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    // Wrap reader with cancellation support.
    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::gzip::gzip_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress a zstd stream to stdout.
fn decompress_zstd_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    // Wrap reader with cancellation support.
    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::zstd::zstd_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    // Flush stdout to ensure all bytes are written before exiting.
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a zstd file to a new file in the output directory.
fn decompress_zstd_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::zstd_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    // Check for clobber (no-clobber mode).
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    // Wrap reader with cancellation support.
    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::zstd::zstd_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress a multi-file archive (zip, tar, tar.gz) using `extract_all`.
///
/// If `cancel_flag` is set, extraction stops as early as possible and
/// returns [`geezipx_core::GeeZipError::Cancelled`].
fn decompress_archive(
    archive: &Path,
    output_dir: &Path,
    format: ArchiveFormat,
    overwrite: bool,
    verbose: bool,
    show_progress: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let mut reader = common::open_reader(archive, format)?;
    let report = reader
        .extract_all_with_cancel(output_dir, overwrite, &|| {
            cancel_flag.load(std::sync::atomic::Ordering::SeqCst)
        })
        .with_context(|| format!("extracting '{}'", archive.display()))?;

    // Report any per-file errors.
    for (entry_name, err) in &report.errors {
        eprintln!("Warning: failed to extract '{entry_name}': {err}");
    }

    // Skip summary message when progress bar already shows it.
    if !show_progress || verbose {
        eprintln!(
            "Extracted {} ({} files, {} bytes, {} skipped)",
            archive.display(),
            report.files_extracted,
            report.bytes_extracted,
            report.files_skipped,
        );
    }

    // Return error if nothing was extracted.
    if report.files_extracted == 0 && report.errors.is_empty() {
        anyhow::bail!(
            "archive '{}' contained no extractable entries",
            archive.display()
        );
    }

    Ok(())
}
