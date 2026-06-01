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
    no_progress: bool,
    verbose: bool,
) -> Result<()> {
    let format = common::resolve_format(format, output)?;
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
            let mut reader = open_input(input)?;

            let show_progress = !no_progress
                && !verbose
                && file_size > 0
                && crate::render::progress::progress_bar_enabled();

            let bytes_read = if show_progress {
                let wrapper = ProgressBarWrapper::determinate(file_size);
                wrapper.set_message(&format!("Compressing: {}", input.display()));
                let shared = SharedCallback::new(wrapper);
                let inner = shared.clone_inner();
                let mut pr = ProgressReader::new(reader)
                    .with_total(file_size)
                    .with_callback(Box::new(shared));
                let r = match geezipx_core::archive::gzip::gzip_compress(&mut pr, output_file) {
                    Ok(bytes) => {
                        inner
                            .lock()
                            .unwrap()
                            .finish(&format!("Compressed {}", input.display()));
                        bytes
                    }
                    Err(e) => {
                        inner.lock().unwrap().finish("Compression failed");
                        return Err(anyhow::anyhow!("gzip compression error: {}", e));
                    }
                };
                r
            } else {
                if verbose {
                    eprintln!("Compressing: {} ({} bytes)", input.display(), file_size);
                }
                let r = geezipx_core::archive::gzip::gzip_compress(&mut reader, output_file)
                    .with_context(|| format!("gzip compressing '{}'", input.display()))?;
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

            let shared_cb = if show_progress {
                let wrapper = ProgressBarWrapper::determinate(total_bytes_all);
                Some(SharedCallback::new(wrapper))
            } else {
                None
            };

            let mut writer = common::create_writer(output_file, format)?;

            for (src_path, archive_path) in &files {
                let file_size = std::fs::metadata(src_path)
                    .with_context(|| format!("reading metadata for '{}'", src_path.display()))?
                    .len();
                let mut reader = open_input(src_path)?;

                if let Some(ref cb) = shared_cb {
                    let inner = cb.clone_inner();
                    inner
                        .lock()
                        .unwrap()
                        .set_message(&format!("Compressing: {}", archive_path.display()));
                    let mut pr = ProgressReader::new(reader)
                        .with_total(file_size)
                        .with_callback(Box::new(SharedCallback {
                            inner: inner.clone(),
                        }));
                    if let Err(e) = writer.add_entry_from_reader(archive_path, &mut pr) {
                        inner.lock().unwrap().finish("Compression failed");
                        return Err(anyhow::anyhow!(
                            "failed to add {}: {}",
                            archive_path.display(),
                            e
                        ));
                    }
                } else {
                    if verbose {
                        eprintln!("Adding: {} ({} bytes)", src_path.display(), file_size);
                    }
                    writer
                        .add_entry_from_reader(archive_path, &mut reader)
                        .with_context(|| {
                            format!("adding '{}' to archive", archive_path.display())
                        })?;
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
