//! `geezipx decompress` — extract archives or decode streams.
//!
//! Stream destinations are published only after decoding succeeds. Archive
//! extraction uses the core's safe extraction layer and reports partial failures.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use geezipx_core::archive::{aes, bin, brotli, bzip2, gzip, img, isz, lz, lz4, uu, xxe, xz, zstd};
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::volume::{self, MultiVolumeReader};
use geezipx_core::{ProgressReader, ProgressWriter};

use super::common;
use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use crate::signal::CancellationToken;

fn is_stream(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::Gzip
            | ArchiveFormat::Z
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Brotli
            | ArchiveFormat::Lz4
            | ArchiveFormat::Zstd
            | ArchiveFormat::Xz
            | ArchiveFormat::Lzma
            | ArchiveFormat::Lz
            | ArchiveFormat::Img
            | ArchiveFormat::Isz
            | ArchiveFormat::Aes
            | ArchiveFormat::Uu
            | ArchiveFormat::Xxe
    )
}

fn is_wrapped(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::TarGz
            | ArchiveFormat::TarBz2
            | ArchiveFormat::TarBr
            | ArchiveFormat::TarLz4
            | ArchiveFormat::TarZst
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarLz
    )
}

fn validate_password(format: ArchiveFormat, password: Option<&str>) -> Result<()> {
    if format != ArchiveFormat::Aes {
        common::validate_read_password_support(format, password)?;
    }
    Ok(())
}

/// Execute the `decompress` subcommand.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    archive: Option<&Path>,
    output_dir: &Path,
    stdout: bool,
    overwrite: bool,
    no_progress: bool,
    verbose: bool,
    password: Option<String>,
    use_stdin: bool,
    format: Option<&str>,
) -> Result<()> {
    CancellationToken::reset();
    let cancel = CancellationToken::new();
    if use_stdin {
        let format =
            common::parse_format(format.context("--format is required when using --stdin")?)?;
        if !(is_stream(format) || is_wrapped(format) || format == ArchiveFormat::Bin)
            || format == ArchiveFormat::Isz
        {
            anyhow::bail!("--stdin is only supported for single-stream formats and tar-wrapped streams; got '{format}'");
        }
        validate_password(format, password.as_deref())?;
        let reader = std::io::stdin().lock();
        if stdout {
            decode_to_stdout(reader, format, password.as_deref(), &cancel)?;
        } else {
            fs::create_dir_all(output_dir)
                .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;
            let output = output_dir.join("output");
            decode_to_file(
                reader,
                format,
                password.as_deref(),
                &output,
                overwrite,
                &cancel,
            )?;
        }
        return Ok(());
    }

    let archive = archive.context("archive file is required (or use --stdin)")?;
    if !archive.exists() {
        anyhow::bail!("archive '{}' does not exist", archive.display());
    }
    if volume::is_volume_filename(archive) {
        return decompress_volume(
            archive,
            output_dir,
            stdout,
            overwrite,
            no_progress,
            verbose,
            password.as_deref(),
            format,
            &cancel,
        );
    }
    let format = match format {
        Some(format) => common::parse_format(format)?,
        None => common::detect_archive_format(archive)?,
    };
    validate_password(format, password.as_deref())?;
    if stdout && !(is_stream(format) || is_wrapped(format)) {
        anyhow::bail!("--stdout is only supported for single-stream formats and tar-wrapped raw-stream output; '{format}' is a multi-file archive");
    }
    if !stdout {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;
    }
    if is_stream(format) || stdout {
        let input =
            fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
        if format == ArchiveFormat::Isz {
            // ISZ needs seeking on its input, so cancellation wraps the writer.
            let decode = |writer: &mut dyn Write| -> Result<()> {
                let mut input = &input;
                let mut writer =
                    ProgressWriter::new(writer).with_callback(Box::new(callback(&cancel)));
                isz::isz_decompress(&mut input, &mut writer)
                    .with_context(|| format!("decompressing '{}'", archive.display()))?;
                writer.flush()?;
                check_cancelled(&cancel)
            };
            if stdout {
                decode(&mut std::io::stdout().lock())?;
            } else {
                let output = output_dir.join(output_filename(archive, format));
                common::ensure_distinct_output(archive, &output)?;
                if !skip_existing(&output, overwrite) {
                    let mut pending = common::PendingOutput::new(&output, overwrite)?;
                    decode(&mut pending)?;
                    pending.commit()?;
                }
            }
        } else if stdout {
            decode_to_stdout(input, format, password.as_deref(), &cancel)?;
        } else {
            let output = output_dir.join(output_filename(archive, format));
            common::ensure_distinct_output(archive, &output)?;
            decode_to_file(
                input,
                format,
                password.as_deref(),
                &output,
                overwrite,
                &cancel,
            )?;
        }
        return Ok(());
    }
    decompress_archive(
        archive,
        output_dir,
        format,
        overwrite,
        verbose,
        no_progress,
        password.as_deref(),
        &cancel,
    )
}

fn callback(cancel: &CancellationToken) -> SharedCallback {
    SharedCallback::new(ProgressBarWrapper::hidden(), cancel.clone().into_inner())
}

fn check_cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        anyhow::bail!("Cancelled");
    }
    Ok(())
}

fn skip_existing(output: &Path, overwrite: bool) -> bool {
    if !overwrite && fs::symlink_metadata(output).is_ok() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output.display()
        );
        true
    } else {
        false
    }
}

fn decode_to_stdout<R: Read>(
    reader: R,
    format: ArchiveFormat,
    password: Option<&str>,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut writer = std::io::stdout().lock();
    let bytes = decode(reader, format, &mut writer, password, cancel)?;
    writer
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {bytes} bytes to stdout");
    Ok(())
}

fn decode_to_file<R: Read>(
    reader: R,
    format: ArchiveFormat,
    password: Option<&str>,
    output: &Path,
    overwrite: bool,
    cancel: &CancellationToken,
) -> Result<()> {
    if skip_existing(output, overwrite) {
        return Ok(());
    }
    let mut pending = common::PendingOutput::new(output, overwrite)?;
    let bytes = decode(reader, format, &mut pending, password, cancel)
        .with_context(|| format!("{format} decompression error"))?;
    pending.commit()?;
    eprintln!("Decompressed -> {} ({bytes} bytes)", output.display());
    Ok(())
}

fn decode<R: Read, W: Write>(
    reader: R,
    format: ArchiveFormat,
    writer: &mut W,
    password: Option<&str>,
    cancel: &CancellationToken,
) -> Result<u64> {
    let mut reader = ProgressReader::new(reader).with_callback(Box::new(callback(cancel)));
    // Check cancellation on output too, including buffered or memory-backed decoders.
    let mut writer = ProgressWriter::new(writer).with_callback(Box::new(callback(cancel)));
    let bytes = match format {
        ArchiveFormat::Z => {
            geezipx_core::archive::unix_compress::z_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Gzip | ArchiveFormat::TarGz => {
            gzip::gzip_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Bzip2 | ArchiveFormat::TarBz2 => {
            bzip2::bzip2_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Brotli | ArchiveFormat::TarBr => {
            brotli::brotli_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Lz4 | ArchiveFormat::TarLz4 => {
            lz4::lz4_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Zstd | ArchiveFormat::TarZst => {
            zstd::zstd_decompress(&mut reader, &mut writer)?
        }
        ArchiveFormat::Xz | ArchiveFormat::TarXz => xz::xz_decompress(&mut reader, &mut writer)?,
        ArchiveFormat::TarLz => std::io::copy(
            &mut geezipx_core::archive::tarlz::decoder(&mut reader)?,
            &mut writer,
        )?,
        ArchiveFormat::Lzma => xz::lzma_decompress(&mut reader, &mut writer)?,
        ArchiveFormat::Lz => lz::lz_decompress(&mut reader, &mut writer)?,
        ArchiveFormat::Img => img::img_decompress(&mut reader, &mut writer)?,
        ArchiveFormat::Bin => bin::bin_decompress(&mut reader, &mut writer)?,
        ArchiveFormat::Aes => {
            aes::aes_decrypt(&mut reader, &mut writer, password.unwrap_or_default())?
        }
        ArchiveFormat::Uu => uu::uu_decode_stream(&mut reader, &mut writer)?,
        ArchiveFormat::Xxe => xxe::xxe_decode_stream(&mut reader, &mut writer)?,
        _ => anyhow::bail!("unsupported format for single-stream decompression: {format}"),
    };
    writer.flush()?;
    check_cancelled(cancel)?;
    Ok(bytes)
}

fn output_filename(archive: &Path, format: ArchiveFormat) -> PathBuf {
    match format {
        ArchiveFormat::Z => archive
            .with_extension("")
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("output")),
        ArchiveFormat::Gzip => common::gzip_output_filename(archive),
        ArchiveFormat::Bzip2 => common::bzip2_output_filename(archive),
        ArchiveFormat::Brotli => common::brotli_output_filename(archive),
        ArchiveFormat::Lz4 => common::lz4_output_filename(archive),
        ArchiveFormat::Zstd => common::zstd_output_filename(archive),
        ArchiveFormat::Xz => common::xz_output_filename(archive),
        ArchiveFormat::Lzma => common::lzma_output_filename(archive),
        ArchiveFormat::Lz => common::lz_output_filename(archive),
        ArchiveFormat::Img => common::img_output_filename(archive),
        ArchiveFormat::Bin => common::bin_output_filename(archive),
        ArchiveFormat::Aes => common::aes_output_filename(archive),
        ArchiveFormat::Uu => common::uu_output_filename(archive),
        ArchiveFormat::Xxe => common::xxe_output_filename(archive),
        ArchiveFormat::Isz => archive
            .with_extension("iso")
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("output.iso")),
        _ => PathBuf::from("output"),
    }
}

#[allow(clippy::too_many_arguments)]
fn decompress_archive(
    archive: &Path,
    output_dir: &Path,
    format: ArchiveFormat,
    overwrite: bool,
    verbose: bool,
    no_progress: bool,
    password: Option<&str>,
    cancel: &CancellationToken,
) -> Result<()> {
    let spinner = if !no_progress && !verbose && crate::render::progress::progress_bar_enabled() {
        Some(ProgressBarWrapper::spinner("Extracting..."))
    } else {
        None
    };
    let mut reader = common::open_reader(archive, format, password)?;
    for entry in reader.entries().context("reading archive entries")? {
        if !entry.is_dir {
            common::ensure_distinct_output(archive, &output_dir.join(&entry.path))?;
        }
    }
    let report = reader
        .extract_all_with_cancel(output_dir, overwrite, &|| cancel.is_cancelled())
        .with_context(|| format!("extracting '{}'", archive.display()))?;
    let mut failures = 0;
    for (name, error) in &report.errors {
        if matches!(error, geezipx_core::GeeZipError::ClobberDenied { .. }) {
            eprintln!("Skipped '{name}': file already exists");
        } else {
            failures += 1;
            eprintln!("Warning: failed to extract '{name}': {error}");
        }
    }
    if let Some(spinner) = spinner {
        spinner.finish(if failures == 0 {
            "Extraction complete"
        } else {
            "Extraction failed"
        });
    }
    eprintln!(
        "Extracted {} ({} files, {} bytes, {} skipped)",
        archive.display(),
        report.files_extracted,
        report.bytes_extracted,
        report.files_skipped
    );
    if failures > 0 {
        anyhow::bail!(
            "extraction failed for {} entries ({} extracted, {} skipped)",
            failures,
            report.files_extracted,
            report.files_skipped
        );
    }
    check_cancelled(cancel)
}

#[allow(clippy::too_many_arguments)]
fn decompress_volume(
    archive: &Path,
    output_dir: &Path,
    stdout: bool,
    overwrite: bool,
    no_progress: bool,
    verbose: bool,
    password: Option<&str>,
    format: Option<&str>,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut reader = MultiVolumeReader::open(archive)
        .with_context(|| format!("opening split volume '{}'", archive.display()))?;
    let base = PathBuf::from(reader.base_name());
    let format = match format {
        Some(format) => common::parse_format(format)?,
        None => common::detect_archive_format_from_reader(&mut reader, Some(&base))?,
    };
    validate_password(format, password)?;
    if !stdout {
        fs::create_dir_all(output_dir)?;
    }
    let protected_volumes: Vec<_> = reader
        .volumes
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    if (is_stream(format) || (stdout && is_wrapped(format))) && format != ArchiveFormat::Isz {
        if stdout {
            decode_to_stdout(reader, format, password, cancel)
        } else {
            let output = output_dir.join(output_filename(&base, format));
            for (part, _) in &reader.volumes {
                common::ensure_distinct_output(part, &output)?;
            }
            decode_to_file(reader, format, password, &output, overwrite, cancel)
        }
    } else {
        if stdout {
            anyhow::bail!("--stdout is not supported for multi-file archives in volume mode");
        }
        // Keep ownership of the spill file so every error/cancellation cleans it up.
        let mut temp =
            tempfile::NamedTempFile::new().context("creating temp file for volume spill")?;
        let mut reader = ProgressReader::new(reader).with_callback(Box::new(callback(cancel)));
        std::io::copy(&mut reader, &mut temp).context("reassembling split volumes")?;
        temp.flush()?;
        check_cancelled(cancel)?;
        // Path-backed readers reopen this file; close its handle on all platforms.
        let path = temp.into_temp_path();
        if format == ArchiveFormat::Isz {
            let output = output_dir.join(output_filename(&base, format));
            for part in &protected_volumes {
                common::ensure_distinct_output(part, &output)?;
            }
            if skip_existing(&output, overwrite) {
                return Ok(());
            }
            let mut pending = common::PendingOutput::new(&output, overwrite)?;
            let mut input = fs::File::open(&path)?;
            {
                let mut writer =
                    ProgressWriter::new(&mut pending).with_callback(Box::new(callback(cancel)));
                isz::isz_decompress(&mut input, &mut writer)?;
                writer.flush()?;
            }
            check_cancelled(cancel)?;
            pending.commit()
        } else {
            let mut archive_reader = common::open_reader(&path, format, password)?;
            for entry in archive_reader.entries()? {
                if !entry.is_dir {
                    for part in &protected_volumes {
                        common::ensure_distinct_output(part, &output_dir.join(&entry.path))?;
                    }
                }
            }
            drop(archive_reader);
            decompress_archive(
                &path,
                output_dir,
                format,
                overwrite,
                verbose,
                no_progress,
                password,
                cancel,
            )
        }
    }
}
