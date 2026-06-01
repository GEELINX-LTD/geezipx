//! `geezipx decompress` — extract an archive or decompress a stream.

use std::fs;
use std::io::Write;
use std::path::Path;

use geezipx_core::detect::ArchiveFormat;

use anyhow::{Context, Result};

use super::common;

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

    // Ensure the output directory exists.
    if !stdout {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;
    }

    let show_progress = !no_progress && !verbose && crate::render::progress::progress_bar_enabled();

    match format {
        ArchiveFormat::Gzip => {
            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Decompressing...",
                ))
            } else {
                None
            };

            if let Some(s) = &spinner {
                let result = if stdout {
                    decompress_gzip_stdout(archive)
                } else {
                    decompress_gzip_to_file(archive, output_dir, overwrite)
                };
                match result {
                    Ok(()) => s.finish("Decompressed"),
                    Err(e) => {
                        s.finish("Decompression failed");
                        return Err(anyhow::anyhow!("gzip decompression error: {}", e));
                    }
                }
            } else {
                if stdout {
                    decompress_gzip_stdout(archive)?;
                } else {
                    decompress_gzip_to_file(archive, output_dir, overwrite)?;
                }
            }
        }
        _ => {
            if stdout {
                anyhow::bail!(
                    "--stdout is only supported for single-stream formats (gzip); \
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

            if let Some(s) = &spinner {
                let result = decompress_archive(
                    archive,
                    output_dir,
                    format,
                    overwrite,
                    verbose,
                    show_progress,
                );
                match result {
                    Ok(()) => s.finish("Extraction complete"),
                    Err(e) => {
                        s.finish("Extraction failed");
                        return Err(anyhow::anyhow!("extraction error: {}", e));
                    }
                }
            } else {
                decompress_archive(
                    archive,
                    output_dir,
                    format,
                    overwrite,
                    verbose,
                    show_progress,
                )?;
            }
        }
    }

    Ok(())
}

/// Decompress a gzip stream to stdout.
fn decompress_gzip_stdout(archive: &Path) -> Result<()> {
    let mut file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();
    let bytes = geezipx_core::archive::gzip::gzip_decompress(&mut file, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    // Flush stdout to ensure all bytes are written before exiting.
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a gzip file to a new file in the output directory.
fn decompress_gzip_to_file(archive: &Path, output_dir: &Path, overwrite: bool) -> Result<()> {
    let output_name = common::gzip_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let mut input_file =
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

    let bytes = geezipx_core::archive::gzip::gzip_decompress(&mut input_file, &mut output_file)
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
fn decompress_archive(
    archive: &Path,
    output_dir: &Path,
    format: ArchiveFormat,
    overwrite: bool,
    verbose: bool,
    show_progress: bool,
) -> Result<()> {
    let mut reader = common::open_reader(archive, format)?;
    let report = reader
        .extract_all(output_dir, overwrite)
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
