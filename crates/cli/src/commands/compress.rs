//! `geezipx compress` — create an archive or compressed file.

use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use geezipx_core::archive::aes;
use geezipx_core::archive::bin;
use geezipx_core::archive::brotli;
use geezipx_core::archive::bzip2;
use geezipx_core::archive::gzip;
use geezipx_core::archive::img;
use geezipx_core::archive::isz;
use geezipx_core::archive::lz;
use geezipx_core::archive::lz4;
use geezipx_core::archive::uu;
use geezipx_core::archive::xxe;
use geezipx_core::archive::xz;
use geezipx_core::archive::zstd;
use geezipx_core::config::{CompressOptions, SevenZipOptions};
use geezipx_core::detect::ArchiveFormat;

use crate::render::progress::{ProgressBarWrapper, SharedCallback};
use geezipx_core::ProgressReader;

use super::common;
use geezipx_core::volume::{MultiVolumeWriter, VolumePattern};

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
            | ArchiveFormat::Img
            | ArchiveFormat::Aes
            | ArchiveFormat::Bin
            | ArchiveFormat::Isz
            | ArchiveFormat::Uu
            | ArchiveFormat::Xxe
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
    seven_z_method: Option<&str>,
    dict_size: Option<&str>,
    encrypt_filenames: bool,
    no_encrypt_filenames: bool,
    solid: bool,
    split_size: Option<&str>,
) -> Result<()> {
    let compress_options = CompressOptions {
        level,
        jobs: if jobs == 1 { None } else { Some(jobs) },
        password,
        seven_zip: None, // built after format resolution
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

    // --split-size is only supported for archive (multi-file) formats.
    if split_size.is_some()
        && (is_single_stream_format(format) || (is_tar_wrapped_format(format) && use_stdin))
    {
        anyhow::bail!(
            "--split-size is only supported for archive formats (zip, tar, 7z, etc.); '{}' is a stream format",
            format
        );
    }

    crate::signal::CancellationToken::reset();
    let cancel_token = crate::signal::CancellationToken::new();

    // Build 7z-specific options and validate
    let seven_z_opts = if format == ArchiveFormat::SevenZip {
        let dict_size_parsed = match dict_size {
            Some(s) => Some(parse_dict_size(s)?),
            None => None,
        };
        let encrypt_filenames_val = if no_encrypt_filenames {
            Some(false)
        } else if encrypt_filenames {
            Some(true)
        } else {
            None
        };
        Some(SevenZipOptions {
            method: seven_z_method.map(|s| s.to_string()),
            dictionary_size: dict_size_parsed,
            encrypt_filenames: encrypt_filenames_val,
            solid: if solid { Some(true) } else { None },
        })
    } else if seven_z_method.is_some() || dict_size.is_some() {
        anyhow::bail!("--7z-method and --dict-size are only valid for 7z format; got '{format}'");
    } else {
        None
    };

    let mut compress_options = compress_options;
    compress_options.seven_zip = seven_z_opts;

    validate_compress_inputs(inputs, format, &compress_options, use_stdin, use_stdout)?;
    if let Some(output) = output {
        for input in inputs {
            common::ensure_distinct_output(input, output)?;
        }
    }
    if let Some(size) = split_size {
        parse_split_size(size)?;
    }
    if sfx && format != ArchiveFormat::Zip {
        anyhow::bail!("--sfx requires ZIP format");
    }
    #[cfg(not(feature = "sfx"))]
    if sfx {
        anyhow::bail!("SFX support is disabled in this build");
    }
    #[cfg(feature = "sfx")]
    {
        let target = sfx_target
            .map(common::parse_sfx_target)
            .transpose()?
            .unwrap_or_else(geezipx_core::sfx::SfxTarget::host);
        if sfx {
            geezipx_core::sfx::write_zip_sfx(std::io::empty(), std::io::sink(), target)
                .context("checking SFX stub availability")?;
        }
    }

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
        split_size,
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
        let pending = output_path
            .map(|path| common::PendingOutput::new(path, true))
            .transpose()?;
        let writer: Box<dyn Write> = if let Some(ref pending) = pending {
            Box::new(pending.reopen()?)
        } else {
            Box::new(std::io::stdout().lock())
        };

        if verbose {
            eprintln!("Compressing: {input_display}");
        }
        let wrapper = ProgressBarWrapper::hidden();
        let shared = SharedCallback::new(wrapper, cancel_token.clone().into_inner());
        let mut pr = ProgressReader::new(reader).with_callback(Box::new(shared));
        let bytes = compress_single_stream(&mut pr, writer, compress_options, format)?;
        if let Some(pending) = pending {
            pending.commit()?;
        } else {
            std::io::stdout()
                .flush()
                .context("flushing compressed stdout")?;
        }
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
    let pending = common::PendingOutput::new(output_path, true)?;
    let output_file = pending.reopen()?;
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
                    return Err(e);
                }
            };
        pending.commit()?;
        let output_path = output.unwrap();
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
        let bytes_read = compress_single_stream(&mut pr, output_file, compress_options, format)?;
        pending.commit()?;
        let output_path = output.unwrap();
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
    split_size: Option<&str>,
) -> Result<()> {
    let output_path = output.unwrap();
    let files = common::collect_inputs(inputs, recursive)?;
    for entry in &files {
        common::ensure_distinct_output(&entry.real_path, output_path)?;
    }
    let pending = common::PendingOutput::new(output_path, true)?;
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

    let output_file = pending.reopen()?;
    let mut writer = common::create_writer(output_file, format, compress_options)?;

    for entry in &files {
        if entry.is_dir {
            if let Err(err) = writer.add_directory(&entry.archive_path) {
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
            return Err(anyhow::anyhow!("failed to finalize archive: {}", e));
        }
    };

    if let Some(ref cb) = shared_cb {
        cb.clone_inner().lock().unwrap().finish(&format!(
            "Created {} with {} entries",
            output_path.display(),
            processed_files
        ));
    }

    // SFX post-processing: wrap ZIP in self-extracting stub.
    #[cfg(feature = "sfx")]
    let pending = if sfx {
        let target = if let Some(t) = sfx_target {
            crate::commands::common::parse_sfx_target(t)?
        } else {
            geezipx_core::sfx::SfxTarget::host()
        };

        let zip_file = std::fs::File::open(pending.path())
            .with_context(|| format!("re-opening '{}' for SFX wrap", output_path.display()))?;
        let mut zip_data = ProgressReader::new(BufReader::new(zip_file)).with_callback(Box::new(
            SharedCallback::new(
                ProgressBarWrapper::hidden(),
                cancel_token.clone().into_inner(),
            ),
        ));
        let mut sfx_output = common::PendingOutput::new(output_path, true)?;
        geezipx_core::sfx::write_zip_sfx(&mut zip_data, &mut sfx_output, target)
            .context("creating SFX")?;

        // Set executable permission on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(sfx_output.path())
                .with_context(|| format!("reading permissions of '{}'", output_path.display()))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(sfx_output.path(), perms).with_context(|| {
                format!(
                    "setting executable permission on '{}'",
                    output_path.display()
                )
            })?;
        }

        sfx_output
    } else {
        pending
    };

    #[cfg(not(feature = "sfx"))]
    let _ = sfx_target;

    // Split from the staged archive before replacing any existing destination.
    if let Some(size_str) = split_size {
        if split_output_into_volumes(pending.path(), output_path, size_str)? {
            return Ok(());
        }
    }
    pending.commit()?;
    if sfx {
        eprintln!("SFX written to {}", output_path.display());
    } else {
        eprintln!(
            "Created {} with {} entries ({} bytes)",
            output_path.display(),
            files.len(),
            total_bytes
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

    // Installer and RAR creation are deliberately unsupported.
    if matches!(format, ArchiveFormat::Rar | ArchiveFormat::Nsis) {
        anyhow::bail!(
            "{format} writing is not supported; use list, test, or decompress for read-only {format} support"
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
        || format == ArchiveFormat::SevenZip
        || format == ArchiveFormat::Zip
        || format == ArchiveFormat::Zipx
    {
        if let Some(l) = options.level {
            if l > 9 {
                anyhow::bail!("{} compression level must be 0..=9, got {}", format, l);
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

    // Password-protected archive creation is currently supported for ZIP, 7z, and AES.
    if let Some(password) = options.password.as_deref() {
        if !matches!(format, ArchiveFormat::Zip | ArchiveFormat::Zipx)
            && format != ArchiveFormat::SevenZip
            && format != ArchiveFormat::Aes
        {
            anyhow::bail!(
                "--password is only supported for ZIP, 7z, and AES formats; '{}' does not support encryption",
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
    mut writer: W,
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
        ArchiveFormat::Img => img::img_compress(reader, writer)
            .map_err(|e| anyhow::anyhow!("img pass-through error: {}", e)),
        ArchiveFormat::Aes => {
            let password = options.password.as_deref().unwrap_or("");
            aes::aes_encrypt(reader, writer, password)
                .map_err(|e| anyhow::anyhow!("aes encryption error: {}", e))
        }
        ArchiveFormat::Bin => bin::bin_compress(reader, writer)
            .map_err(|e| anyhow::anyhow!("bin pass-through error: {}", e)),
        ArchiveFormat::Isz => isz::isz_compress(reader, writer, &options)
            .map_err(|e| anyhow::anyhow!("isz compression error: {}", e)),
        ArchiveFormat::Uu => {
            let name = "data.bin";
            uu::uu_encode_to_writer(reader, name, &mut writer)
                .map_err(|e| anyhow::anyhow!("uu encoding error: {}", e))
        }
        ArchiveFormat::Xxe => {
            let name = "data.bin";
            xxe::xxe_encode_to_writer(reader, name, &mut writer)
                .map_err(|e| anyhow::anyhow!("xxe encoding error: {}", e))
        }
        _ => anyhow::bail!("cannot compress '{}' as a single stream", format),
    }
}

/// Parse a human-readable dictionary size string (e.g., "16M", "64M", "1G") into bytes.
fn parse_dict_size(s: &str) -> anyhow::Result<u32> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("--dict-size cannot be empty");
    }

    // Check for suffix
    let (num_part, multiplier) =
        if let Some(rest) = s.strip_suffix(|c: char| c.is_ascii_alphabetic()) {
            let suffix = &s[rest.len()..];
            let m = match suffix.to_ascii_uppercase().as_str() {
                "K" => 1024u64,
                "M" => 1024 * 1024,
                "G" => 1024 * 1024 * 1024,
                _ => anyhow::bail!(
                    "unsupported --dict-size suffix '{}'; use K, M, or G",
                    suffix
                ),
            };
            (rest, m)
        } else {
            (s, 1u64)
        };

    let num: u64 = num_part
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid --dict-size value '{}'", s))?;

    let bytes = num
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("--dict-size value '{}' is too large", s))?;

    if bytes > u32::MAX as u64 {
        anyhow::bail!("--dict-size value '{}' exceeds maximum (4 GiB)", s);
    }

    Ok(bytes as u32)
}

/// Parse a human-readable size string (e.g., "10M", "1G", "500K") into bytes.
fn parse_split_size(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("--split-size cannot be empty");
    }

    let (num_part, multiplier) =
        if let Some(rest) = s.strip_suffix(|c: char| c.is_ascii_alphabetic()) {
            let suffix = &s[rest.len()..];
            let m = match suffix.to_ascii_uppercase().as_str() {
                "K" => 1024u64,
                "M" => 1024 * 1024,
                "G" => 1024 * 1024 * 1024,
                _ => anyhow::bail!(
                    "unsupported --split-size suffix '{}'; use K, M, or G",
                    suffix
                ),
            };
            (rest, m)
        } else {
            (s, 1u64)
        };

    let num: u64 = num_part
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid --split-size value '{}'", s))?;

    let bytes = num
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("--split-size value '{}' is too large", s))?;

    if bytes == 0 {
        anyhow::bail!("--split-size must be positive");
    }

    Ok(bytes)
}

/// Split an output file into multiple volumes using the .NNN naming convention.
fn split_output_into_volumes(source: &Path, output: &Path, size_str: &str) -> Result<bool> {
    let max_size = parse_split_size(size_str)?;

    let file_meta =
        std::fs::metadata(source).with_context(|| format!("stat '{}'", source.display()))?;
    let file_size = file_meta.len();

    if file_size <= max_size {
        eprintln!(
            "Output size ({} bytes) <= split size ({} bytes); splitting skipped",
            file_size, max_size
        );
        return Ok(false);
    }

    let num_volumes = file_size.div_ceil(max_size);
    eprintln!(
        "Splitting {} ({} bytes) into {} volume(s) of {} bytes each",
        output.display(),
        file_size,
        num_volumes,
        max_size
    );

    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Refuse old sets, including stale trailing volumes from a previous split.
    let volume_prefix = format!(
        "{}.",
        output
            .file_name()
            .context("output needs a filename")?
            .to_string_lossy()
    );
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(suffix) = name.to_string_lossy().strip_prefix(&volume_prefix) {
            if suffix.len() >= 3 && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                anyhow::bail!("split volume '{}' already exists", entry.path().display());
            }
        }
    }
    let staging = tempfile::TempDir::new_in(parent).context("creating volume staging directory")?;
    let staged_output = staging.path().join(output.file_name().unwrap());
    let mut reader = ProgressReader::new(BufReader::new(
        fs::File::open(source)
            .with_context(|| format!("re-opening '{}' for splitting", source.display()))?,
    ))
    .with_callback(Box::new(SharedCallback::new(
        ProgressBarWrapper::hidden(),
        crate::signal::CancellationToken::new().into_inner(),
    )));
    let mut writer = MultiVolumeWriter::new(&staged_output, VolumePattern::Nnn, max_size, 1)
        .with_context(|| format!("creating volume set from '{}'", output.display()))?;

    std::io::copy(&mut reader, &mut writer).with_context(|| "writing split volumes")?;

    let vol_paths = writer
        .finish()
        .with_context(|| "finalizing split volumes")?;

    // Publish exclusively on the same filesystem.
    // Roll back only files created by this operation if publication fails.
    let mut published = Vec::new();
    for path in &vol_paths {
        let target = parent.join(path.file_name().context("volume needs a filename")?);
        if crate::signal::is_cancelled() {
            for created in &published {
                let _ = fs::remove_file(created);
            }
            anyhow::bail!("Cancelled");
        }
        let publication = tempfile::TempPath::try_from_path(path).and_then(|staged| {
            staged
                .persist_noclobber(&target)
                .map_err(|error| error.error)
        });
        if let Err(error) = publication {
            for created in &published {
                let _ = fs::remove_file(created);
            }
            return Err(error)
                .with_context(|| format!("publishing split volume '{}'", target.display()));
        }
        published.push(target);
    }

    eprintln!("Split into {} volume(s):", published.len());
    for path in &published {
        eprintln!("  {}", path.display());
    }

    Ok(true)
}
