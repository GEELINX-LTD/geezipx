//! `geezipx compress` — create an archive or compressed file.

use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::archive::brotli;
use geezipx_core::archive::bzip2;
use geezipx_core::archive::gzip;
use geezipx_core::archive::lz;
use geezipx_core::archive::lz4;
use geezipx_core::archive::xz;
use geezipx_core::archive::zstd;
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::ArchiveFormat;

use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use geezipx_core::ProgressReader;

use super::common;

fn is_single_stream_format(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Brotli
            | ArchiveFormat::Lz4
            | ArchiveFormat::Zstd
            | ArchiveFormat::Xz
            | ArchiveFormat::Lzma
            | ArchiveFormat::Lz
    )
}

fn is_tar_wrapped_format(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::TarGz
            | ArchiveFormat::TarBz2
            | ArchiveFormat::TarBr
            | ArchiveFormat::TarLz4
            | ArchiveFormat::TarZst
            | ArchiveFormat::TarXz
    )
}

/// Execute the `compress` subcommand.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    inputs: &[std::path::PathBuf],
    output: Option<&Path>,
    format: Option<&str>,
    recursive: bool,
    level: Option<u32>,
    jobs: u32,
    no_progress: bool,
    verbose: bool,
    password: Option<String>,
    use_stdin: bool,
    use_stdout: bool,
    sfx: bool,
    sfx_target: Option<&str>,
) -> Result<()> {
    let compress_options = CompressOptions {
        level,
        jobs: if jobs == 1 { None } else { Some(jobs) },
        password,
    };

    // Resolve format: when using --stdin or --stdout without --output, --format is required.
    let format = if use_stdin || use_stdout {
        common::parse_format(
            format.context("--format is required when using --stdin or --stdout")?,
        )?
    } else {
        common::resolve_format(format, output.context("--output is required")?)?
    };

    // Stdio compression is only supported for raw single-stream formats and
    // tar-wrapped codecs operating on a raw tar stream from stdin.
    if (use_stdin || use_stdout)
        && !(is_single_stream_format(format) || is_tar_wrapped_format(format))
    {
        anyhow::bail!(
            "--stdin/--stdout is only supported for single-stream formats \
             (gzip, bzip2, brotli, lz4, zstd, xz, lzma, tar.gz, tar.bz2, tar.br, tar.lz4, tar.zst, tar.xz); got '{format}'"
        );
    }

    let cancel_token = crate::signal::CancellationToken::new();
    validate_compress_inputs(inputs, format, &compress_options, use_stdin, use_stdout)?;

    if is_single_stream_format(format) || (is_tar_wrapped_format(format) && use_stdin) {
        return compress_single_stream_mode(
            inputs,
            output,
            format,
            compress_options,
            no_progress,
            verbose,
            cancel_token,
            use_stdin,
            use_stdout,
        );
    }

    // Archive formats: collect files, write entries, finalise.
    compress_archive_mode(
        inputs,
        output,
        format,
        recursive,
        compress_options,
        no_progress,
        verbose,
        cancel_token,
        sfx,
        sfx_target,
    )
}

/// Compress a single-stream format, handling stdin/stdout modes.
#[allow(clippy::too_many_arguments)]
fn compress_single_stream_mode(
    inputs: &[std::path::PathBuf],
    output: Option<&Path>,
    format: ArchiveFormat,
    compress_options: CompressOptions,
    no_progress: bool,
    verbose: bool,
    cancel_token: crate::signal::CancellationToken,
    use_stdin: bool,
    use_stdout: bool,
) -> Result<()> {
    // ---- reader ----
    let file_size;
    let input_display;
    let reader: Box<dyn Read> = if use_stdin {
        file_size = 0;
        input_display = "stdin".to_string();
        Box::new(std::io::stdin().lock())
    } else {
        let input = &inputs[0];
        let size = std::fs::metadata(input)
            .with_context(|| format!("reading metadata for '{}'", input.display()))?
            .len();
        file_size = size;
        input_display = input.display().to_string();
        Box::new(open_input(input)?)
    };

    // Use simple (no progress bar) compression for stdin or stdout modes.
    if use_stdin || use_stdout {
        let output_path = if use_stdout {
            None
        } else {
            Some(output.unwrap())
        };
        let writer: Box<dyn Write> =
            if use_stdout {
                Box::new(std::io::stdout().lock())
            } else {
                Box::new(fs::File::create(output_path.unwrap()).with_context(|| {
                    format!("creating output '{}'", output_path.unwrap().display())
                })?)
            };

        if verbose {
            eprintln!("Compressing: {input_display}");
        }
        let wrapper = ProgressBarWrapper::hidden();
        let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
        let mut pr = ProgressReader::new(reader).with_callback(Box::new(shared));
        let bytes = compress_single_stream(&mut pr, writer, compress_options, format).inspect_err(
            |_| {
                if cancel_token.is_cancelled() {
                    if !use_stdout {
                        let _ = std::fs::remove_file(output_path.unwrap());
                    }
                    eprintln!("Cancelled");
                    std::process::exit(130);
                }
            },
        )?;
        if verbose {
            eprintln!("  Done: {bytes} bytes");
        }
        if use_stdout && !use_stdin {
            eprintln!("Compressed {input_display} to stdout ({bytes} bytes)");
        } else if !use_stdout {
            eprintln!(
                "Compressed stdin -> {} ({bytes} bytes)",
                output_path.unwrap().display()
            );
        }
        // stdin->stdout: no message to avoid breaking pipe
        return Ok(());
    }

    // ---- file-to-file with progress bar (existing behavior) ----
    let input = &inputs[0];
    let output_path = output.unwrap();
    let output_file = fs::File::create(output_path)
        .with_context(|| format!("creating output '{}'", output_path.display()))?;
    let show_progress = !no_progress
        && !verbose
        && file_size > 0
        && crate::render::progress::progress_bar_enabled();

    if show_progress {
        let wrapper = ProgressBarWrapper::determinate(file_size);
        wrapper.set_message(&format!("Compressing: {}", input.display()));
        let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
        let inner = shared.clone_inner();
        let mut pr = ProgressReader::new(reader)
            .with_total(file_size)
            .with_callback(Box::new(shared));
        let bytes_read =
            match compress_single_stream(&mut pr, output_file, compress_options, format) {
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
                        let _ = std::fs::remove_file(output_path);
                        eprintln!("Cancelled");
                        std::process::exit(130);
                    }
                    return Err(e);
                }
            };
        eprintln!(
            "Compressed {} -> {} ({:.1}% of original)",
            input.display(),
            output_path.display(),
            if bytes_read > 0 {
                let compressed_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
                (compressed_size as f64 / bytes_read as f64) * 100.0
            } else {
                0.0
            },
        );
    } else {
        if verbose {
            eprintln!("Compressing: {} ({} bytes)", input.display(), file_size);
        }
        let wrapper = ProgressBarWrapper::hidden();
        let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
        let mut pr = ProgressReader::new(reader)
            .with_total(file_size)
            .with_callback(Box::new(shared));
        let bytes_read =
            match compress_single_stream(&mut pr, output_file, compress_options, format) {
                Ok(bytes) => bytes,
                Err(e) => {
                    if cancel_token.is_cancelled() {
                        let _ = std::fs::remove_file(output_path);
                        eprintln!("Cancelled");
                        std::process::exit(130);
                    }
                    return Err(e);
                }
            };
        if verbose {
            eprintln!("  Done: {bytes_read} bytes");
        }
        eprintln!(
            "Compressed {} -> {} ({:.1}% of original)",
            input.display(),
            output_path.display(),
            if bytes_read > 0 {
                let compressed_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
                (compressed_size as f64 / bytes_read as f64) * 100.0
            } else {
                0.0
            },
        );
    }
    Ok(())
}

/// Compress a multi-file archive format.
#[allow(clippy::too_many_arguments)]
fn compress_archive_mode(
    inputs: &[std::path::PathBuf],
    output: Option<&Path>,
    format: ArchiveFormat,
    recursive: bool,
    compress_options: CompressOptions,
    no_progress: bool,
    verbose: bool,
    cancel_token: crate::signal::CancellationToken,
    sfx: bool,
    sfx_target: Option<&str>,
) -> Result<()> {
    let output_path = output.unwrap();
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

    let output_file = fs::File::create(output_path)
        .with_context(|| format!("creating output '{}'", output_path.display()))?;
    let mut writer = common::create_writer(output_file, format, compress_options)?;

    for entry in &files {
        if entry.is_dir {
            if let Err(err) = writer.add_directory(&entry.archive_path) {
                let _ = std::fs::remove_file(output_path);
                return Err(anyhow::anyhow!(
                    "failed to add directory {}: {}",
                    entry.archive_path.display(),
                    err
                ));
            }
            processed_files += 1;
            continue;
        }

        let file_size = std::fs::metadata(&entry.real_path)
            .with_context(|| format!("reading metadata for '{}'", entry.real_path.display()))?
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
                    let _ = std::fs::remove_file(output_path);
                    eprintln!("Cancelled after {}/{} files", processed_files, files.len());
                    std::process::exit(130);
                }
                let _ = std::fs::remove_file(output_path);
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
                let _ = std::fs::remove_file(output_path);
                eprintln!("Cancelled after {}/{} files", processed_files, files.len());
                std::process::exit(130);
            }
            let _ = std::fs::remove_file(output_path);
            return Err(anyhow::anyhow!("failed to finalize archive: {}", e));
        }
    };

    if let Some(ref cb) = shared_cb {
        cb.clone_inner().lock().unwrap().finish(&format!(
            "Created {} with {} entries",
            output_path.display(),
            files.len()
        ));
    }

    // SFX post-processing: wrap ZIP in self-extracting stub.
    #[cfg(feature = "sfx")]
    if sfx {
        use std::io::Read;
        let target = if let Some(t) = sfx_target {
            crate::commands::common::parse_sfx_target(t)?
        } else {
            geezipx_core::sfx::SfxTarget::host()
        };

        // Read the ZIP file back into memory.
        let mut zip_file = std::fs::File::open(output_path)
            .with_context(|| format!("re-opening '{}' for SFX wrap", output_path.display()))?;
        let mut zip_data = Vec::new();
        zip_file
            .read_to_end(&mut zip_data)
            .with_context(|| format!("reading '{}' for SFX wrap", output_path.display()))?;
        drop(zip_file);

        let sfx_data =
            geezipx_core::sfx::create_zip_sfx(&zip_data, target).context("creating SFX")?;

        // Write back the SFX data.
        std::fs::write(output_path, &sfx_data)
            .with_context(|| format!("writing SFX to '{}'", output_path.display()))?;

        // Set executable permission on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(output_path)
                .with_context(|| format!("reading permissions of '{}'", output_path.display()))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(output_path, perms).with_context(|| {
                format!(
                    "setting executable permission on '{}'",
                    output_path.display()
                )
            })?;
        }

        eprintln!(
            "SFX written to {}{}",
            output_path.display(),
            target.extension()
        );
    } else {
        eprintln!(
            "Created {} with {} entries ({} bytes)",
            output_path.display(),
            files.len(),
            total_bytes,
        );
    }

    #[cfg(not(feature = "sfx"))]
    {
        let _ = sfx;
        let _ = sfx_target;
        eprintln!(
            "Created {} with {} entries ({} bytes)",
            output_path.display(),
            files.len(),
            total_bytes,
        );
    }

    Ok(())
}

/// Validate input constraints for the given format.
fn validate_compress_inputs(
    inputs: &[std::path::PathBuf],
    format: ArchiveFormat,
    options: &CompressOptions,
    use_stdin: bool,
    use_stdout: bool,
) -> Result<()> {
    if inputs.is_empty() && !use_stdin {
        anyhow::bail!("at least one input file is required");
    }

    // RAR writing is not supported; only list, test, and decompress for read-only.
    if format == ArchiveFormat::Rar {
        anyhow::bail!(
            "rar writing is not supported; use list, test, or decompress for read-only rar support"
        );
    }

    if is_tar_wrapped_format(format) && use_stdout && !use_stdin {
        anyhow::bail!(
            "--stdout for '{}' only supports raw tar input via --stdin; archiving file/directory inputs to stdout is not supported yet. Use -o/--output instead",
            format
        );
    }

    // Single-stream formats (gzip, bzip2, brotli, lz4, zstd, xz, lzma) only accept one input.
    if is_single_stream_format(format) && inputs.len() > 1 {
        anyhow::bail!(
            "{} compression only supports a single input file (got {})",
            format,
            inputs.len()
        );
    }

    // Gzip/bzip2/xz/lzma/tar.gz/tar.bz2/tar.xz levels are limited to 0..=9;
    // brotli/tar.br supports 0..=11; zstd/tar.zst supports 0..=22; lz4/tar.lz4
    // accepts only 0 or omitted. For bzip2/tar.bz2, level 0 maps to the default
    // encoder level because libbz2 has no store-only mode.
    if format == ArchiveFormat::Gzip
        || format == ArchiveFormat::Bzip2
        || format == ArchiveFormat::Xz
        || format == ArchiveFormat::Lzma
        || format == ArchiveFormat::TarGz
        || format == ArchiveFormat::TarBz2
        || format == ArchiveFormat::TarXz
    {
        if let Some(l) = options.level {
            if l > 9 {
                anyhow::bail!("{} compression level must be 0..=9, got {}", format, l);
            }
        }

        // 7z levels are limited to 0..=9
        if format == ArchiveFormat::SevenZip {
            if let Some(l) = options.level {
                if l > 9 {
                    anyhow::bail!("7z compression level must be 0..=9, got {}", l);
                }
            }
        }
    }

    if format == ArchiveFormat::Brotli || format == ArchiveFormat::TarBr {
        if let Some(l) = options.level {
            if l > 11 {
                anyhow::bail!("{} compression level must be 0..=11, got {}", format, l);
            }
        }
    }

    if (format == ArchiveFormat::Lz4 || format == ArchiveFormat::TarLz4)
        && options.level.is_some_and(|l| l != 0)
    {
        anyhow::bail!(
            "{} compression level is not configurable in the current encoder; use 0 or omit the level",
            format
        );
    }

    // Resolve all paths and check they exist.
    for input in inputs {
        if !input.exists() {
            anyhow::bail!("input '{}' does not exist", input.display());
        }
        if is_single_stream_format(format) && input.is_dir() {
            anyhow::bail!(
                "{} compression does not support directories ('{}')",
                format,
                input.display()
            );
        }
    }

    // Password-protected archive creation is currently supported for ZIP and 7z.
    if let Some(password) = options.password.as_deref() {
        if format != ArchiveFormat::Zip && format != ArchiveFormat::SevenZip {
            anyhow::bail!(
                "--password is only supported for ZIP and 7z formats; '{}' does not support encryption",
                format
            );
        }
        if password.is_empty() {
            anyhow::bail!("--password cannot be empty");
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
        // Note: single-stream compression (used for --stdin/--stdout) does
        // NOT benefit from --jobs for tar.gz. The gzp parallel gzip is
        // only active in archive mode via TarGzWriter::new_with_options.
        // Brotli and lz4 currently accept --jobs for forward compatibility,
        // but the selected encoder paths are single-threaded.
        ArchiveFormat::Gzip | ArchiveFormat::TarGz => {
            gzip::gzip_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("gzip compression error: {}", e))
        }
        ArchiveFormat::Bzip2 | ArchiveFormat::TarBz2 => {
            bzip2::bzip2_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("bzip2 compression error: {}", e))
        }
        ArchiveFormat::Brotli | ArchiveFormat::TarBr => {
            brotli::brotli_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("brotli compression error: {}", e))
        }
        ArchiveFormat::Lz4 | ArchiveFormat::TarLz4 => {
            lz4::lz4_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("lz4 compression error: {}", e))
        }
        ArchiveFormat::Zstd | ArchiveFormat::TarZst => {
            zstd::zstd_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("zstd compression error: {}", e))
        }
        ArchiveFormat::Xz | ArchiveFormat::TarXz => {
            xz::xz_compress_with_options(reader, writer, options)
                .map_err(|e| anyhow::anyhow!("xz compression error: {}", e))
        }
        ArchiveFormat::Lzma => xz::lzma_compress_with_options(reader, writer, options)
            .map_err(|e| anyhow::anyhow!("lzma compression error: {}", e)),
        ArchiveFormat::Lz => lz::lz_compress_with_options(reader, writer, &options)
            .map_err(|e| anyhow::anyhow!("lz compression error: {}", e)),
        _ => anyhow::bail!("cannot compress '{}' as a single stream", format),
    }
}
