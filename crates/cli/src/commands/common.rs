//! Shared utilities for CLI commands.
//!
//! Provides format parsing, file collection, and reader/writer factory
//! functions used across multiple subcommands.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use geezipx_core::archive::tar::TarReader;
use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::targz::TarGzReader;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarzst::TarZstReader;
use geezipx_core::archive::tarzst::TarZstWriter;
use geezipx_core::archive::zip::ZipReader;
use geezipx_core::archive::zip::ZipWriter;
use geezipx_core::archive::{ArchiveReader, ArchiveWriter};
use geezipx_core::detect::{self, ArchiveFormat};

// ---------------------------------------------------------------------------
// Format resolution
// ---------------------------------------------------------------------------

/// Parse a user-supplied format string into an [`ArchiveFormat`].
///
/// Accepts: `zip`, `tar`, `tar.gz`, `tgz`, `gz`, `gzip`, `zst`, `zstd`, `tar.zst`, `tzst`.
pub fn parse_format(s: &str) -> Result<ArchiveFormat> {
    match s.to_ascii_lowercase().as_str() {
        "zip" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "tar.gz" | "tgz" => Ok(ArchiveFormat::TarGz),
        "gz" | "gzip" => Ok(ArchiveFormat::Gzip),
        "zst" | "zstd" => Ok(ArchiveFormat::Zstd),
        "tar.zst" | "tzst" => Ok(ArchiveFormat::TarZst),
        "xz" => Ok(ArchiveFormat::Xz),
        "lzma" => Ok(ArchiveFormat::Lzma),
        other => Err(anyhow::anyhow!(
            "unsupported format '{other}'; expected: zip, tar, tar.gz, tgz, gz, gzip, zst, zstd, tar.zst, tzst, xz, lzma"
        )),
    }
}

/// Resolve the compress output format from an optional `--format` flag and the
/// output file extension.  Defaults to `Zip`.
pub fn resolve_format(cli_format: Option<&str>, output: &Path) -> Result<ArchiveFormat> {
    if let Some(s) = cli_format {
        return parse_format(s);
    }
    // Try to infer from the output file extension.
    if let Some(fmt) = detect::detect_from_extension(output) {
        return Ok(fmt);
    }
    Ok(ArchiveFormat::Zip)
}

/// Detect the format of an archive file, combining magic bytes with extension
/// fallback.
///
/// 1. Read `MAGIC_DETECT_SIZE` bytes.
/// 2. If gzip magic, check extension for `.tar.gz` / `.tgz` → `TarGz`.
/// 3. Otherwise return the magic-based result or fall back to extension.
pub fn detect_archive_format(path: &Path) -> Result<ArchiveFormat> {
    let mut file = fs::File::open(path).with_context(|| format!("opening '{}'", path.display()))?;
    let magic =
        detect::read_magic_bytes(&mut file).context("reading magic bytes for format detection")?;
    drop(file);

    match detect::detect_format(&magic) {
        Some(ArchiveFormat::Gzip) => {
            // Gzip magic but the file might be .tar.gz — check extension.
            if let Some(ArchiveFormat::TarGz) = detect::detect_from_extension(path) {
                Ok(ArchiveFormat::TarGz)
            } else {
                Ok(ArchiveFormat::Gzip)
            }
        }
        Some(ArchiveFormat::Zstd) => {
            // Zstd magic but the file might be .tar.zst / .tzst — check extension.
            if let Some(ArchiveFormat::TarZst) = detect::detect_from_extension(path) {
                Ok(ArchiveFormat::TarZst)
            } else {
                Ok(ArchiveFormat::Zstd)
            }
        }
        Some(ArchiveFormat::Xz) => {
            // XZ magic is also present in `.tar.xz` / `.txz`; treat them as
            // single-stream XZ for now, so decompression produces the underlying tar file.
            Ok(ArchiveFormat::Xz)
        }
        Some(ArchiveFormat::Lzma) => {
            // Unreachable in practice: LZMA has no reliable magic, so detection
            // relies on explicit format or extension fallback.
            Ok(ArchiveFormat::Lzma)
        }
        Some(fmt) => Ok(fmt),
        None => {
            // No magic matched; fall back to extension.
            detect::detect_from_extension(path).ok_or_else(|| {
                anyhow::anyhow!("unable to detect archive format for '{}'", path.display())
            })
        }
    }
}

// ---------------------------------------------------------------------------
// File collection
// ---------------------------------------------------------------------------

/// Result of collecting input files: `(source_path, archive_entry_path)`.
pub type FileEntry = (PathBuf, PathBuf);

/// Collect input files, handling directories with `--recursive`.
///
/// * A plain file yields one entry with the file's basename as the archive
///   path.
/// * A directory with `--recursive` yields all files inside it, each with a
///   path relative to the directory's **parent** — i.e. `dir/foo.txt` becomes
///   `dir/foo.txt` inside the archive.
/// * A directory without `--recursive` is an error.
pub fn collect_inputs(inputs: &[PathBuf], recursive: bool) -> Result<Vec<FileEntry>> {
    if inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    let mut result = Vec::new();

    for input in inputs {
        let input = fs::canonicalize(input)
            .with_context(|| format!("resolving path '{}'", input.display()))?;

        if input.is_dir() {
            if !recursive {
                anyhow::bail!(
                    "'{}' is a directory; use --recursive to include it",
                    input.display()
                );
            }
            // The dir name becomes the prefix for all entries inside it.
            let dir_name = input
                .file_name()
                .unwrap_or(input.as_os_str())
                .to_os_string();
            let prefix = PathBuf::from(&dir_name);
            collect_dir_contents(&input, &prefix, &mut result)
                .with_context(|| format!("reading directory '{}'", input.display()))?;
        } else if input.is_file() {
            // Use the file's basename as the archive entry path.
            let name = input
                .file_name()
                .unwrap_or(input.as_os_str())
                .to_os_string();
            result.push((input.clone(), PathBuf::from(name)));
        } else {
            anyhow::bail!("'{}' does not exist", input.display());
        }
    }

    Ok(result)
}

/// Recursively walk `dir` and add files to `entries`, prepending `prefix`
/// to each entry path.
fn collect_dir_contents(dir: &Path, prefix: &Path, entries: &mut Vec<FileEntry>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            collect_dir_contents(&path, &relative, entries)?;
        } else if path.is_file() {
            entries.push((path, relative));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reader / writer factories
// ---------------------------------------------------------------------------

/// Create an archive reader from a file path and detected format.
pub fn open_reader(path: &Path, format: ArchiveFormat) -> Result<Box<dyn ArchiveReader>> {
    let file = fs::File::open(path).with_context(|| format!("opening '{}'", path.display()))?;
    Ok(match format {
        ArchiveFormat::Zip => Box::new(ZipReader::new(file)?),
        ArchiveFormat::Tar => Box::new(TarReader::new(file)),
        ArchiveFormat::TarGz => Box::new(TarGzReader::new(file)),
        ArchiveFormat::TarZst => Box::new(TarZstReader::new(file)),
        ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
            anyhow::bail!(
                "'{format}' is a single-stream compression format; use 'decompress' directly, not an archive reader"
            )
        }
        _ => anyhow::bail!("unsupported format for reading: {format}"),
    })
}

/// Create an archive writer for the given output file, format, and optional
/// compression level.
pub fn create_writer(
    file: fs::File,
    format: ArchiveFormat,
    level: Option<u32>,
) -> Result<Box<dyn ArchiveWriter>> {
    match format {
        ArchiveFormat::Zip => Ok(Box::new(ZipWriter::new(file))),
        ArchiveFormat::Tar => Ok(Box::new(TarWriter::new(file))),
        ArchiveFormat::TarGz => Ok(Box::new(TarGzWriter::new_with_level(file, level))),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstWriter::new_with_level(file, level))),
        ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
            anyhow::bail!(
                "'{format}' is a single-stream compression format; use 'compress' directly, not an archive writer"
            )
        }
        _ => anyhow::bail!("unsupported format for writing: {format}"),
    }
}

/// Infer the decompressed filename for a gzip file by stripping `.gz` or
/// `.gzip` from the filename.
pub fn gzip_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name
        .strip_suffix(".gz")
        .or_else(|| name.strip_suffix(".gzip"))
        .unwrap_or(&name);
    PathBuf::from(stripped)
}

/// Infer the decompressed filename for a zstd file by stripping `.zst` or
/// `.zstd` from the filename.
pub fn zstd_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name
        .strip_suffix(".zst")
        .or_else(|| name.strip_suffix(".zstd"))
        .unwrap_or(&name);
    PathBuf::from(stripped)
}

/// Infer the decompressed filename for an xz file by stripping `.xz`.
pub fn xz_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name.strip_suffix(".xz").unwrap_or(&name);
    PathBuf::from(stripped)
}

/// Infer the decompressed filename for an lzma file by stripping `.lzma`.
pub fn lzma_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name.strip_suffix(".lzma").unwrap_or(&name);
    PathBuf::from(stripped)
}
