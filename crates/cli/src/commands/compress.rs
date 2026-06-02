//! `geezipx compress` — create an archive or compressed file.

use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::archive::gzip;
use geezipx_core::archive::xz;
use geezipx_core::archive::zstd;
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::ArchiveFormat;

use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use geezipx_core::ProgressReader;

use super::common;

/// Execute the `compress` subcommand.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    inputs: &[std::path::PathBuf],
    output: &Path,
    format: Option<&str>,
    recursive: bool,
    level: Option<u32>,
    jobs: u32,
    no_progress: bool,
    verbose: bool,
) -> Result<()> {
    let compress_options = CompressOptions {
        level,
        jobs: if jobs == 1 { None } else { Some(jobs) },
    };
    let format = common::resolve_format(format, output)?;
    // Create a cancellation token for Ctrl+C (SIGINT) handling.
    let cancel_token = crate::signal::CancellationToken::new();
    validate_compress_inputs(inputs, format, &compress_options)?;

    // Create the output file early so we fail-fast if the path is invalid.
    let output_file = fs::File::create(output)
        .with_context(|| format!("creating output '{}'", output.display()))?;

    match format {
        ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
            // Gzip/Zstd are single-stream compression formats — no ArchiveWriter trait.
            let input = &inputs[0];
            let file_size = std::fs::metadata(input)
                .with_context(|| format!("reading metadata for '{}'", input.display()))?
                .len();
            let reader = open_input(input)?;

            let show_progress = !no_progress
                && !verbose
                && file_size > 0
                && crate::render::progress::progress_bar_enabled();

            let bytes_read = if show_progress {
                let wrapper = ProgressBarWrapper::determinate(file_size);
                wrapper.set_message(&format!("Compressing: {}", input.display()));
                let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
                let inner = shared.clone_inner();
                let mut pr = ProgressReader::new(reader)
                    .with_total(file_size)
                    .with_callback(Box::new(shared));
                let r = match compress_single_stream(&mut pr, output_file, compress_options, format)
                {
                    Ok(bytes) => {
                        inner
                            .lock()
                            .unwrap()
                            .finish(&format!("Compressed {}", input.display()));
                        bytes
                    }
                    Err(e) => {
                        inner.lock().unwrap().finish("Compression failed");
                        if cancel_token.is_cancelled() {
                            let _ = std::fs::remove_file(output);
                            eprintln!("Cancelled");
                            std::process::exit(130);
                        }
                        return Err(e);
                    }
                };
                r
            } else {
                if verbose {
                    eprintln!("Compressing: {} ({} bytes)", input.display(), file_size);
                }
                let wrapper = ProgressBarWrapper::hidden();
                let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
                let mut pr = ProgressReader::new(reader)
                    .with_total(file_size)
                    .with_callback(Box::new(shared));
                let r = match compress_single_stream(&mut pr, output_file, compress_options, format)
                {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        if cancel_token.is_cancelled() {
                            let _ = std::fs::remove_file(output);
                            eprintln!("Cancelled");
                            std::process::exit(130);
                        }
                        return Err(e);
                    }
                };
                if verbose {
                    eprintln!("  Done: {} bytes", r);
                }
                r
            };

            eprintln!(
                "Compressed {} -> {} ({:.1}% of original)",
                input.display(),
                output.display(),
                if bytes_read > 0 {
                    let compressed_size = std::fs::metadata(output)
                        .map(|m| m.len())
                        .unwrap_or_else(|e| {
                            eprintln!("Warning: could not stat output file: {}", e);
                            0
                        });
                    (compressed_size as f64 / bytes_read as f64) * 100.0
                } else {
                    0.0
                },
            );
        }
        _ => {
            // Archive formats: collect files, write entries, finalise.
            let files = common::collect_inputs(inputs, recursive)?;

            let total_bytes_all: u64 = files
                .iter()
                .filter(|e| !e.is_dir)
                .filter_map(|e| std::fs::metadata(&e.real_path).ok().map(|m| m.len()))
                .sum();

            let show_progress = !no_progress
                && !verbose
                && total_bytes_all > 0
                && crate::render::progress::progress_bar_enabled();

            let mut processed_files: usize = 0;
            let shared_cb = {
                let cancelled = cancel_token.clone().into_inner();
                if show_progress {
                    let wrapper = ProgressBarWrapper::determinate(total_bytes_all);
                    Some(SharedCallback::new(wrapper, cancelled))
                } else {
                    let wrapper = ProgressBarWrapper::hidden();
                    Some(SharedCallback::new(wrapper, cancelled))
                }
            };

            let mut writer = common::create_writer(output_file, format, compress_options)?;

            for entry in &files {
                if entry.is_dir {
                    writer.add_directory(&entry.archive_path).with_context(|| {
                        format!("failed to add directory '{}'", entry.archive_path.display())
                    })?;
                    processed_files += 1;
                    continue;
                }

                let file_size = std::fs::metadata(&entry.real_path)
                    .with_context(|| {
                        format!("reading metadata for '{}'", entry.real_path.display())
                    })?
                    .len();
                let reader = open_input(&entry.real_path)?;

                if let Some(ref cb) = shared_cb {
                    let inner = cb.clone_inner();
                    if verbose {
                        eprintln!(
                            "Adding: {} ({} bytes)",
                            entry.real_path.display(),
                            file_size
                        );
                    }
                    if show_progress {
                        inner
                            .lock()
                            .unwrap()
                            .set_message(&format!("Compressing: {}", entry.archive_path.display()));
                    }
                    let mut pr = ProgressReader::new(reader)
                        .with_total(file_size)
                        .with_callback(Box::new(SharedCallback {
                            inner: inner.clone(),
                            cancelled: cb.cancelled.clone(),
                        }));
                    if let Err(e) = writer.add_entry_from_reader(&entry.archive_path, &mut pr) {
                        inner.lock().unwrap().finish("Compression failed");
                        if cancel_token.is_cancelled() {
                            let _ = std::fs::remove_file(output);
                            eprintln!("Cancelled after {}/{} files", processed_files, files.len());
                            std::process::exit(130);
                        }
                        return Err(anyhow::anyhow!(
                            "failed to add {}: {}",
                            entry.archive_path.display(),
                            e
                        ));
                    }
                    processed_files += 1;
                }
            }

            let total_bytes = match writer.finish() {
                Ok(t) => t,
                Err(e) => {
                    if let Some(ref cb) = shared_cb {
                        cb.clone_inner()
                            .lock()
                            .unwrap()
                            .finish("Compression failed");
                    }
                    if cancel_token.is_cancelled() {
                        let _ = std::fs::remove_file(output);
                        eprintln!("Cancelled after {}/{} files", processed_files, files.len());
                        std::process::exit(130);
                    }
                    return Err(anyhow::anyhow!("failed to finalize archive: {}", e));
                }
            };

            if let Some(ref cb) = shared_cb {
                cb.clone_inner().lock().unwrap().finish(&format!(
                    "Created {} with {} entries",
                    output.display(),
                    files.len()
                ));
            }

            eprintln!(
                "Created {} with {} entries ({} bytes)",
                output.display(),
                files.len(),
                total_bytes,
            );
        }
    }

    Ok(())
}

/// Validate input constraints for the given format.
fn validate_compress_inputs(
    inputs: &[std::path::PathBuf],
    format: ArchiveFormat,
    options: &CompressOptions,
) -> Result<()> {
    if inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    // Single-stream formats (gzip, zstd, xz, lzma) only accept one input.
    if (format == ArchiveFormat::Gzip
        || format == ArchiveFormat::Zstd
        || format == ArchiveFormat::Xz
        || format == ArchiveFormat::Lzma)
        && inputs.len() > 1
    {
        anyhow::bail!(
            "{} compression only supports a single input file (got {})",
            format,
            inputs.len()
        );
    }

    // Gzip/xz/lzma/tar.gz/tar.xz level is limited to 0..=9; zstd/tar.zst supports 0..=22.
    if format == ArchiveFormat::Gzip
        || format == ArchiveFormat::Xz
        || format == ArchiveFormat::Lzma
        || format == ArchiveFormat::TarGz
        || format == ArchiveFormat::TarXz
    {
        if let Some(l) = options.level {
            if l > 9 {
                anyhow::bail!("{} compression level must be 0..=9, got {}", format, l);
            }
        }
    }

    // Resolve all paths and check they exist.
    for input in inputs {
        if !input.exists() {
            anyhow::bail!("input '{}' does not exist", input.display());
        }
        if (format == ArchiveFormat::Gzip
            || format == ArchiveFormat::Zstd
            || format == ArchiveFormat::Xz
            || format == ArchiveFormat::Lzma)
            && input.is_dir()
        {
            anyhow::bail!(
                "{} compression does not support directories ('{}')",
                format,
                input.display()
            );
        }
    }

    Ok(())
}

/// Open a file for reading with a buffered reader.
fn open_input(path: &Path) -> Result<impl Read> {
    let file =
        fs::File::open(path).with_context(|| format!("opening input '{}'", path.display()))?;
    Ok(BufReader::new(file))
}

/// Compress a single stream using format-appropriate encoder with options.
fn compress_single_stream<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
    format: ArchiveFormat,
) -> anyhow::Result<u64> {
    match format {
        ArchiveFormat::Gzip => gzip::gzip_compress_with_options(reader, writer, options)
            .map_err(|e| anyhow::anyhow!("gzip compression error: {}", e)),
        ArchiveFormat::Zstd => zstd::zstd_compress_with_options(reader, writer, options)
            .map_err(|e| anyhow::anyhow!("zstd compression error: {}", e)),
        ArchiveFormat::Xz => xz::xz_compress_with_options(reader, writer, options)
            .map_err(|e| anyhow::anyhow!("xz compression error: {}", e)),
        ArchiveFormat::Lzma => xz::lzma_compress_with_options(reader, writer, options)
            .map_err(|e| anyhow::anyhow!("lzma compression error: {}", e)),
        _ => anyhow::bail!("cannot compress '{}' as a single stream", format),
    }
}
