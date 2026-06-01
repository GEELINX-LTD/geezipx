//! `geezipx compress` — create an archive or compressed file.

use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::detect::ArchiveFormat;

use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use geezipx_core::ProgressReader;

use super::common;

/// Execute the `compress` subcommand.
pub fn execute(
    inputs: &[std::path::PathBuf],
    output: &Path,
    format: Option<&str>,
    recursive: bool,
    level: Option<u32>,
    no_progress: bool,
    verbose: bool,
) -> Result<()> {
    let format = common::resolve_format(format, output)?;
    // Create a cancellation token for Ctrl+C (SIGINT) handling.
    let cancel_token = crate::signal::CancellationToken::new();
    validate_compress_inputs(inputs, format)?;

    // Create the output file early so we fail-fast if the path is invalid.
    let output_file = fs::File::create(output)
        .with_context(|| format!("creating output '{}'", output.display()))?;

    match format {
        ArchiveFormat::Gzip => {
            // Gzip is a single-stream compression — no ArchiveWriter trait.
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
                let r = match geezipx_core::archive::gzip::gzip_compress_with_level(
                    &mut pr,
                    output_file,
                    level,
                ) {
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
                        return Err(anyhow::anyhow!("gzip compression error: {}", e));
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
                let r = match geezipx_core::archive::gzip::gzip_compress_with_level(
                    &mut pr,
                    output_file,
                    level,
                ) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        if cancel_token.is_cancelled() {
                            let _ = std::fs::remove_file(output);
                            eprintln!("Cancelled");
                            std::process::exit(130);
                        }
                        return Err(anyhow::anyhow!("gzip compression error: {}", e));
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
                .filter_map(|(p, _)| std::fs::metadata(p).ok().map(|m| m.len()))
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

            let mut writer = common::create_writer(output_file, format, level)?;

            for (src_path, archive_path) in &files {
                let file_size = std::fs::metadata(src_path)
                    .with_context(|| format!("reading metadata for '{}'", src_path.display()))?
                    .len();
                let reader = open_input(src_path)?;

                if let Some(ref cb) = shared_cb {
                    let inner = cb.clone_inner();
                    if verbose {
                        eprintln!("Adding: {} ({} bytes)", src_path.display(), file_size);
                    }
                    if show_progress {
                        inner
                            .lock()
                            .unwrap()
                            .set_message(&format!("Compressing: {}", archive_path.display()));
                    }
                    let mut pr = ProgressReader::new(reader)
                        .with_total(file_size)
                        .with_callback(Box::new(SharedCallback {
                            inner: inner.clone(),
                            cancelled: cb.cancelled.clone(),
                        }));
                    if let Err(e) = writer.add_entry_from_reader(archive_path, &mut pr) {
                        inner.lock().unwrap().finish("Compression failed");
                        if cancel_token.is_cancelled() {
                            let _ = std::fs::remove_file(output);
                            eprintln!("Cancelled after {}/{} files", processed_files, files.len());
                            std::process::exit(130);
                        }
                        return Err(anyhow::anyhow!(
                            "failed to add {}: {}",
                            archive_path.display(),
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
fn validate_compress_inputs(inputs: &[std::path::PathBuf], format: ArchiveFormat) -> Result<()> {
    if inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    if format == ArchiveFormat::Gzip && inputs.len() > 1 {
        anyhow::bail!(
            "gzip compression only supports a single input file (got {})",
            inputs.len()
        );
    }

    // Resolve all paths and check they exist.
    for input in inputs {
        if !input.exists() {
            anyhow::bail!("input '{}' does not exist", input.display());
        }
        if format == ArchiveFormat::Gzip && input.is_dir() {
            anyhow::bail!(
                "gzip compression does not support directories ('{}')",
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
