//! `geezipx decompress` — extract an archive or decompress a stream.
use std::sync::atomic::Ordering;

use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

use geezipx_core::detect::ArchiveFormat;

use anyhow::{Context, Result};

use super::common;
use crate::render::progress::{ProgressBarWrapper, SharedCallback};
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
use geezipx_core::volume::{self, MultiVolumeReader};
use geezipx_core::ProgressReader;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::NamedTempFile;

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
    // ---- stdin mode: read from stdin, no archive file ----
    if use_stdin {
        return decompress_stdin_mode(format, output_dir, stdout, overwrite, verbose);
    }

    // ---- file-based mode ----
    let archive = archive.unwrap();
    if !archive.exists() {
        anyhow::bail!("archive '{}' does not exist", archive.display());
    }

    // ---- split volume detection ----
    if volume::is_volume_filename(archive) {
        return execute_decompress_volume(
            archive,
            output_dir,
            stdout,
            overwrite,
            no_progress,
            verbose,
            password.as_deref(),
        );
    }

    let format = common::detect_archive_format(archive)?;

    // Validate password: single-stream formats (gzip, bzip2, zstd, xz, lzma) do not support encryption (except AES).
    if password.is_some()
        && matches!(
            format,
            ArchiveFormat::Gzip
                | ArchiveFormat::Bzip2
                | ArchiveFormat::Brotli
                | ArchiveFormat::Lz4
                | ArchiveFormat::Zstd
                | ArchiveFormat::Xz
                | ArchiveFormat::Lz
                | ArchiveFormat::Lzma
                | ArchiveFormat::Img
                | ArchiveFormat::Bin
                | ArchiveFormat::Isz
                | ArchiveFormat::Uu
                | ArchiveFormat::Xxe
        )
    {
        anyhow::bail!(
            "--password is only supported for ZIP, 7z, RAR, and AES formats; '{}' does not support encryption",
            format
        );
    }

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
        ArchiveFormat::Bzip2 => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_bzip2_stdout(archive, cancel_flag)
            } else {
                decompress_bzip2_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("bzip2 decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Brotli => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_brotli_stdout(archive, cancel_flag)
            } else {
                decompress_brotli_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("brotli decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Lz4 => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_lz4_stdout(archive, cancel_flag)
            } else {
                decompress_lz4_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("lz4 decompression error: {}", e));
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
        ArchiveFormat::Xz => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_xz_stdout(archive, cancel_flag)
            } else {
                decompress_xz_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("xz decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Lzma => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_lzma_stdout(archive, cancel_flag)
            } else {
                decompress_lzma_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("lzma decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Lz => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_lz_stdout(archive, cancel_flag)
            } else {
                decompress_lz_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("lz decompression error: {}", e));
                }
            }
        }

        ArchiveFormat::Img => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_img_stdout(archive, cancel_flag)
            } else {
                decompress_img_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("img pass-through error: {}", e));
                }
            }
        }

        ArchiveFormat::Bin => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_bin_stdout(archive, cancel_flag)
            } else {
                decompress_bin_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("bin pass-through error: {}", e));
                }
            }
        }

        ArchiveFormat::Isz => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_isz_stdout(archive, cancel_flag)
            } else {
                decompress_isz_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("isz decompression error: {}", e));
                }
            }
        }

        ArchiveFormat::Aes => {
            let cancel_flag = cancel_token.clone().into_inner();
            let aes_password = password.clone().unwrap_or_default();

            let result = if stdout {
                decompress_aes_stdout(archive, &aes_password, cancel_flag)
            } else {
                decompress_aes_to_file(archive, output_dir, overwrite, &aes_password, cancel_flag)
            };

            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Decrypting...",
                ))
            } else {
                None
            };

            match result {
                Ok(()) => {
                    if let Some(s) = &spinner {
                        s.finish("Decrypted");
                    }
                }
                Err(e) => {
                    if let Some(s) = &spinner {
                        s.finish("Decryption failed");
                    }
                    if cancel_token.is_cancelled() {
                        eprintln!("Cancelled");
                        std::process::exit(130);
                    }
                    return Err(anyhow::anyhow!("aes decryption error: {}", e));
                }
            }
        }

        ArchiveFormat::Uu => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_uu_stdout(archive, cancel_flag)
            } else {
                decompress_uu_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("uu decompression error: {}", e));
                }
            }
        }
        ArchiveFormat::Xxe => {
            let cancel_flag = cancel_token.clone().into_inner();

            let result = if stdout {
                decompress_xxe_stdout(archive, cancel_flag)
            } else {
                decompress_xxe_to_file(archive, output_dir, overwrite, cancel_flag)
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
                    return Err(anyhow::anyhow!("xxe decompression error: {}", e));
                }
            }
        }
        _ => {
            if stdout {
                let cancel_flag = cancel_token.clone().into_inner();
                match format {
                    ArchiveFormat::TarGz => decompress_gzip_stdout(archive, cancel_flag),
                    ArchiveFormat::TarBz2 => decompress_bzip2_stdout(archive, cancel_flag),
                    ArchiveFormat::TarBr => decompress_brotli_stdout(archive, cancel_flag),
                    ArchiveFormat::TarLz4 => decompress_lz4_stdout(archive, cancel_flag),
                    ArchiveFormat::TarZst => decompress_zstd_stdout(archive, cancel_flag),
                    ArchiveFormat::TarXz => decompress_xz_stdout(archive, cancel_flag),
                    _ => anyhow::bail!(
                        "--stdout is only supported for single-stream formats \
                         (gzip, bzip2, brotli, lz4, zstd, xz, lzma) and tar-wrapped raw-stream output; '{}' is a multi-file archive",
                        format
                    ),
                }?;
                return Ok(());
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
                password,
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

/// Decompress single-stream format from stdin, writing to stdout or a file.
fn decompress_stdin_mode(
    format: Option<&str>,
    output_dir: &Path,
    to_stdout: bool,
    overwrite: bool,
    verbose: bool,
) -> Result<()> {
    let fmt = common::parse_format(format.context("--format is required when using --stdin")?)?;

    // Validate: only single-stream formats work with stdin
    match fmt {
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma
        | ArchiveFormat::Img
        | ArchiveFormat::Uu
        | ArchiveFormat::Xxe
        | ArchiveFormat::TarGz
        | ArchiveFormat::TarBz2
        | ArchiveFormat::TarBr
        | ArchiveFormat::TarLz4
        | ArchiveFormat::TarZst
        | ArchiveFormat::TarXz => {}
        _ => anyhow::bail!(
            "--stdin is only supported for single-stream formats \
             (gzip, bzip2, brotli, lz4, zstd, xz, lzma, uu, uue, xxe, tar.gz, tar.bz2, tar.br, tar.lz4, tar.zst, tar.xz); got '{fmt}'"
        ),
    }

    let mut reader = std::io::stdin().lock();

    if to_stdout {
        let mut writer = std::io::stdout().lock();
        let bytes = match fmt {
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
            ArchiveFormat::Xz | ArchiveFormat::TarXz => {
                xz::xz_decompress(&mut reader, &mut writer)?
            }
            ArchiveFormat::Uu => {
                let mut content = String::new();
                reader.read_to_string(&mut content)?;
                let (_, data) = uu::uu_decode(&content)
                    .ok_or_else(|| anyhow::anyhow!("failed to decode uu from stdin"))?;
                let n = data.len() as u64;
                writer.write_all(&data)?;
                n
            }
            ArchiveFormat::Xxe => {
                let mut content = String::new();
                reader.read_to_string(&mut content)?;
                let (_, data) = xxe::xxe_decode(&content)
                    .map_err(|e| anyhow::anyhow!("failed to decode xxe from stdin: {}", e))?;
                let n = data.len() as u64;
                writer.write_all(&data)?;
                n
            }
            ArchiveFormat::Lzma => xz::lzma_decompress(&mut reader, &mut writer)?,
            _ => unreachable!(),
        };
        if verbose {
            eprintln!("Decompressed stdin to stdout ({bytes} bytes)");
        }
        writer
            .flush()
            .context("flushing stdout after decompression")?;
    } else {
        let output_path = output_dir.join("output");
        if !overwrite && output_path.exists() {
            eprintln!(
                "Warning: '{}' already exists, skipping (use --force to overwrite)",
                output_path.display()
            );
            return Ok(());
        }
        let mut writer = fs::File::create(&output_path)
            .with_context(|| format!("creating output '{}'", output_path.display()))?;
        let bytes = match fmt {
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
            ArchiveFormat::Xz | ArchiveFormat::TarXz => {
                xz::xz_decompress(&mut reader, &mut writer)?
            }
            ArchiveFormat::Img => img::img_decompress(&mut reader, &mut writer)?,
            ArchiveFormat::Bin => bin::bin_decompress(&mut reader, &mut writer)?,
            ArchiveFormat::Uu => {
                let mut content = String::new();
                reader.read_to_string(&mut content)?;
                let (_, data) = uu::uu_decode(&content)
                    .ok_or_else(|| anyhow::anyhow!("failed to decode uu from stdin"))?;
                let n = data.len() as u64;
                writer.write_all(&data)?;
                n
            }
            ArchiveFormat::Xxe => {
                let mut content = String::new();
                reader.read_to_string(&mut content)?;
                let (_, data) = xxe::xxe_decode(&content)
                    .map_err(|e| anyhow::anyhow!("failed to decode xxe from stdin: {}", e))?;
                let n = data.len() as u64;
                writer.write_all(&data)?;
                n
            }
            ArchiveFormat::Lzma => xz::lzma_decompress(&mut reader, &mut writer)?,
            _ => unreachable!(),
        };
        eprintln!(
            "Decompressed stdin -> {} ({} bytes)",
            output_path.display(),
            bytes
        );
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

/// Decompress an lz stream to stdout.
fn decompress_lz_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = lz::lz_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an lz file to a new file in the output directory.
fn decompress_lz_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::lz_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = lz::lz_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress an IMG stream to stdout (pass-through).
fn decompress_img_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::img::img_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an IMG file to a new file in the output directory (pass-through).
fn decompress_img_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::img_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::img::img_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress a BIN stream to stdout (pass-through).
fn decompress_bin_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = bin::bin_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a BIN file to a new file in the output directory (pass-through).
fn decompress_bin_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::bin_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = bin::bin_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decrypt an AES-encrypted stream to stdout.
fn decompress_aes_stdout(
    archive: &Path,
    password: &str,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = aes::aes_decrypt(&mut reader, &mut stdout, password)
        .with_context(|| format!("decrypting '{}'", archive.display()))?;
    stdout.flush().context("flushing stdout after decryption")?;
    eprintln!("Decrypted {} bytes to stdout", bytes);
    Ok(())
}

/// Decrypt an AES-encrypted file to a new file in the output directory.
fn decompress_aes_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    password: &str,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::aes_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = aes::aes_decrypt(&mut reader, &mut output_file, password)
        .with_context(|| format!("decrypting '{}'", archive.display()))?;

    eprintln!(
        "Decrypted {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress a bzip2 stream to stdout.
fn decompress_bzip2_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::bzip2::bzip2_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a bzip2 file to a new file in the output directory.
fn decompress_bzip2_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::bzip2_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::bzip2::bzip2_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress a Brotli stream to stdout.
fn decompress_brotli_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::brotli::brotli_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress a Brotli file to a new file in the output directory.
fn decompress_brotli_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::brotli_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::brotli::brotli_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress an lz4 stream to stdout.
fn decompress_lz4_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file_size = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let wrapper = crate::render::progress::ProgressBarWrapper::hidden();
    let shared = crate::render::progress::SharedCallback::new(wrapper, cancel_flag);
    let mut reader = geezipx_core::ProgressReader::new(file)
        .with_total(file_size)
        .with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::lz4::lz4_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an lz4 file to a new file in the output directory.
fn decompress_lz4_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::lz4_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let mut output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let wrapper = ProgressBarWrapper::hidden();
    let shared = SharedCallback::new(wrapper, cancel_flag);
    let mut reader = ProgressReader::new(input_file).with_callback(Box::new(shared));

    let bytes = geezipx_core::archive::lz4::lz4_decompress(&mut reader, &mut output_file)
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

/// Decode a UU/UUE file to stdout.
fn decompress_uu_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    if cancel_flag.load(Ordering::Relaxed) {
        anyhow::bail!("Cancelled");
    }
    let bytes = uu::uu_decode_to_writer(archive, &mut std::io::stdout().lock())
        .context("decoding uu file")?;
    eprintln!("Decoded {} bytes to stdout", bytes);
    Ok(())
}

/// Decode a UU/UUE file to a file on disk.
fn decompress_uu_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    if cancel_flag.load(Ordering::Relaxed) {
        anyhow::bail!("Cancelled");
    }
    let output_name = common::uu_output_filename(archive);
    let output_path = output_dir.join(&output_name);
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }
    let mut file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;
    let bytes = uu::uu_decode_to_writer(archive, &mut file)
        .with_context(|| format!("decoding '{}'", archive.display()))?;
    eprintln!(
        "Decoded {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes
    );
    Ok(())
}

/// Decode an XXE file to stdout.
fn decompress_xxe_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    if cancel_flag.load(Ordering::Relaxed) {
        anyhow::bail!("Cancelled");
    }
    let bytes = xxe::xxe_decode_to_writer(archive, &mut std::io::stdout().lock())
        .context("decoding xxe file")?;
    eprintln!("Decoded {} bytes to stdout", bytes);
    Ok(())
}

/// Decode an XXE file to a file on disk.
fn decompress_xxe_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    if cancel_flag.load(Ordering::Relaxed) {
        anyhow::bail!("Cancelled");
    }
    let output_name = common::xxe_output_filename(archive);
    let output_path = output_dir.join(&output_name);
    if !overwrite && output_path.exists() {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }
    let mut file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;
    let bytes = xxe::xxe_decode_to_writer(archive, &mut file)
        .with_context(|| format!("decoding '{}'", archive.display()))?;
    eprintln!(
        "Decoded {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes
    );
    Ok(())
}

/// Decompress a multi-file archive (zip, tar, tar.gz) using `extract_all`.
///
/// If `cancel_flag` is set, extraction stops as early as possible and
/// returns [`geezipx_core::GeeZipError::Cancelled`].
#[allow(clippy::too_many_arguments)]
fn decompress_archive(
    archive: &Path,
    output_dir: &Path,
    format: ArchiveFormat,
    overwrite: bool,
    verbose: bool,
    show_progress: bool,
    cancel_flag: Arc<AtomicBool>,
    password: Option<String>,
) -> Result<()> {
    let report = common::open_reader(archive, format, password.as_deref())?
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

/// Decompress an xz stream to stdout.
fn decompress_xz_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
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

    let bytes = xz::xz_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    // Flush stdout to ensure all bytes are written before exiting.
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an xz file to a new file in the output directory.
fn decompress_xz_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::xz_output_filename(archive);
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

    let bytes = xz::xz_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Decompress an lzma stream to stdout.
fn decompress_lzma_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
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

    let bytes = xz::lzma_decompress(&mut reader, &mut stdout)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;
    // Flush stdout to ensure all bytes are written before exiting.
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an lzma file to a new file in the output directory.
fn decompress_lzma_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = common::lzma_output_filename(archive);
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

    let bytes = xz::lzma_decompress(&mut reader, &mut output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

/// Determine the output filename for an ISZ — strip `.isz` and append `.iso`.
fn isz_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    PathBuf::from(format!("{}.iso", name))
}

/// Decompress an ISZ stream to stdout.
fn decompress_isz_stdout(archive: &Path, cancel_flag: Arc<AtomicBool>) -> Result<()> {
    let file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let mut stdout = std::io::stdout().lock();

    let _cancel = cancel_flag; // ISZ requires seeking, progress wrapper not used
    let bytes = isz::isz_decompress(&mut &file, &mut stdout).map_err(|e| anyhow::anyhow!("{e}"))?;
    stdout
        .flush()
        .context("flushing stdout after decompression")?;
    eprintln!("Decompressed {} bytes to stdout", bytes);
    Ok(())
}

/// Decompress an ISZ file to `.iso` in the output directory.
fn decompress_isz_to_file(
    archive: &Path,
    output_dir: &Path,
    overwrite: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    let output_name = isz_output_filename(archive);
    let output_path = output_dir.join(&output_name);

    if output_path.exists() && !overwrite {
        eprintln!(
            "Warning: '{}' already exists, skipping (use --force to overwrite)",
            output_path.display()
        );
        return Ok(());
    }

    let _cancel = cancel_flag; // ISZ requires seeking, progress wrapper not used
    let input_file =
        fs::File::open(archive).with_context(|| format!("opening '{}'", archive.display()))?;
    let output_file = fs::File::create(&output_path)
        .with_context(|| format!("creating '{}'", output_path.display()))?;

    let bytes = isz::isz_decompress(&mut &input_file, output_file)
        .with_context(|| format!("decompressing '{}'", archive.display()))?;

    eprintln!(
        "Decompressed {} -> {} ({} bytes)",
        archive.display(),
        output_path.display(),
        bytes,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Split volume decompression
// ---------------------------------------------------------------------------

/// Execute decompression for a split volume set (e.g. archive.001, archive.002 ...).
#[allow(clippy::too_many_arguments)]
fn execute_decompress_volume(
    archive: &Path,
    output_dir: &Path,
    stdout: bool,
    overwrite: bool,
    no_progress: bool,
    verbose: bool,
    password: Option<&str>,
) -> Result<()> {
    let mut reader = MultiVolumeReader::open(archive)
        .with_context(|| format!("opening split volume '{}'", archive.display()))?;
    let base_name = reader.base_name().to_string();
    let format = common::detect_archive_format_from_reader(&mut reader, None)?;

    // Validate password
    if password.is_some()
        && !matches!(
            format,
            ArchiveFormat::Zip | ArchiveFormat::SevenZip | ArchiveFormat::Rar
        )
    {
        anyhow::bail!(
            "--password is only supported for ZIP, 7z, and RAR formats; '{}' does not support encryption",
            format
        );
    }

    let cancel_token = crate::signal::CancellationToken::new();

    if !stdout {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;
    }

    match format {
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma
        | ArchiveFormat::Lz
        | ArchiveFormat::Img
        | ArchiveFormat::Bin => {
            let show_progress =
                !no_progress && !verbose && crate::render::progress::progress_bar_enabled();
            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Decompressing...",
                ))
            } else {
                None
            };

            let result = if stdout {
                let mut writer = std::io::stdout().lock();
                let bytes = decompress_single_stream(&mut reader, format, &mut writer)
                    .with_context(|| format!("decompressing volume set '{}'", archive.display()))?;
                writer
                    .flush()
                    .context("flushing stdout after decompression")?;
                if verbose {
                    eprintln!(
                        "Decompressed {} bytes from {} parts to stdout",
                        bytes,
                        reader.volumes.len()
                    );
                }
                Ok(())
            } else {
                let output_path = output_dir.join(&base_name);
                if !overwrite && output_path.exists() {
                    eprintln!(
                        "Warning: '{}' already exists, skipping (use --force to overwrite)",
                        output_path.display()
                    );
                    return Ok(());
                }
                let mut writer = fs::File::create(&output_path)
                    .with_context(|| format!("creating '{}'", output_path.display()))?;
                let bytes = decompress_single_stream(&mut reader, format, &mut writer)
                    .with_context(|| format!("decompressing volume set '{}'", archive.display()))?;
                eprintln!(
                    "Decompressed {} -> {} ({} bytes)",
                    archive.display(),
                    output_path.display(),
                    bytes,
                );
                Ok(())
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
                    return Err(e);
                }
            }
        }
        _ => {
            // Archive format: spill volume data to a temp file.
            if stdout {
                anyhow::bail!("--stdout is not supported for multi-file archives in volume mode");
            }
            let show_progress =
                !no_progress && !verbose && crate::render::progress::progress_bar_enabled();
            let spinner = if show_progress {
                Some(crate::render::progress::ProgressBarWrapper::spinner(
                    "Reassembling...",
                ))
            } else {
                None
            };

            // Spill the combined volume stream to a temporary file.
            let mut tmp = NamedTempFile::new().context("creating temp file for volume spill")?;
            // Reader is already at position 0 (rewound by detect_archive_format_from_reader).
            std::io::copy(&mut reader, &mut tmp)
                .with_context(|| format!("spilling volume data for '{}'", archive.display()))?;
            tmp.flush()
                .context("flushing temp file after volume spill")?;
            let (_, tmp_path) = tmp
                .keep()
                .map_err(|e| anyhow::anyhow!("failed to persist temp file: {}", e.error))?;

            if let Some(s) = &spinner {
                s.finish("Reassembled");
            }

            let cancel_flag = cancel_token.clone().into_inner();
            let result = decompress_archive(
                &tmp_path,
                output_dir,
                format,
                overwrite,
                verbose,
                show_progress,
                cancel_flag,
                password.map(|s| s.to_string()),
            );

            // Clean up temp file.
            let _ = fs::remove_file(&tmp_path);

            result?;
        }
    }

    Ok(())
}

/// Decompress a single-stream format from a reader into a writer.
fn decompress_single_stream<R: Read, W: Write>(
    reader: &mut R,
    format: ArchiveFormat,
    writer: &mut W,
) -> Result<u64> {
    let bytes = match format {
        ArchiveFormat::Gzip => gzip::gzip_decompress(reader, writer)?,
        ArchiveFormat::Bzip2 => bzip2::bzip2_decompress(reader, writer)?,
        ArchiveFormat::Brotli => brotli::brotli_decompress(reader, writer)?,
        ArchiveFormat::Lz4 => lz4::lz4_decompress(reader, writer)?,
        ArchiveFormat::Zstd => zstd::zstd_decompress(reader, writer)?,
        ArchiveFormat::Xz => xz::xz_decompress(reader, writer)?,
        ArchiveFormat::Lzma => xz::lzma_decompress(reader, writer)?,
        ArchiveFormat::Lz => lz::lz_decompress(reader, writer)?,
        ArchiveFormat::Img => img::img_decompress(reader, writer)?,
        ArchiveFormat::Bin => bin::bin_decompress(reader, writer)?,
        _ => anyhow::bail!(
            "unsupported format for single-stream decompression: {}",
            format
        ),
    };
    Ok(bytes)
}
