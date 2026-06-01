//! `geezipx compress` — create an archive or compressed file.

use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::detect::ArchiveFormat;

use super::common;

/// Execute the `compress` subcommand.
pub fn execute(
    inputs: &[std::path::PathBuf],
    output: &Path,
    format: Option<&str>,
    recursive: bool,
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
            let mut reader = open_input(input)?;
            let bytes_read = geezipx_core::archive::gzip::gzip_compress(&mut reader, output_file)
                .with_context(|| format!("gzip compressing '{}'", input.display()))?;
            eprintln!(
                "Compressed {} -> {} ({:.1}% of original)",
                input.display(),
                output.display(),
                if bytes_read > 0 {
                    let compressed_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                    (compressed_size as f64 / bytes_read as f64) * 100.0
                } else {
                    0.0
                },
            );
        }
        _ => {
            // Archive formats: collect files, write entries, finalise.
            let files = common::collect_inputs(inputs, recursive)?;

            let mut writer = common::create_writer(output_file, format)?;

            for (src_path, archive_path) in &files {
                let mut reader = open_input(src_path)?;
                writer
                    .add_entry_from_reader(archive_path, &mut reader)
                    .with_context(|| format!("adding '{}' to archive", archive_path.display()))?;
            }

            let total_bytes = writer.finish()?;
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
