//! Shared utilities for CLI commands.
//!
//! Provides format parsing, file collection, and reader/writer factory
//! functions used across multiple subcommands.

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
#[cfg(feature = "rar")]
use geezipx_core::archive::rar::RarReader;
use geezipx_core::archive::seven_zip::SevenZipReader;
use geezipx_core::archive::tar::TarReader;
use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::tarbz2::TarBz2Reader;
use geezipx_core::archive::tarbz2::TarBz2Writer;
use geezipx_core::archive::targz::TarGzReader;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarxz::TarXzReader;
use geezipx_core::archive::tarxz::TarXzWriter;
use geezipx_core::archive::tarzst::TarZstReader;
use geezipx_core::archive::tarzst::TarZstWriter;
use geezipx_core::archive::zip::ZipReader;
use geezipx_core::archive::zip::ZipWriter;
use geezipx_core::archive::{ArchiveReader, ArchiveWriter};
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::{self, ArchiveFormat};

// ---------------------------------------------------------------------------
// Format resolution
// ---------------------------------------------------------------------------

/// Parse a user-supplied format string into an [`ArchiveFormat`].
///
/// Accepts: `zip`, ZIP-derived aliases (`jar`, `war`, `apk`, `ipa`, `xpi`),
/// `tar`, `tar.gz`, `tgz`, `tar.bz2`, `tbz`, `tbz2`, `bz2`, `bzip2`,
/// `zst`, `zstd`, `tar.zst`, `tzst`, `tar.xz`, `txz`, `xz`, `lzma`, `7z`, `rar`.
pub fn parse_format(s: &str) -> Result<ArchiveFormat> {
    match s.to_ascii_lowercase().as_str() {
        "zip" | "jar" | "war" | "apk" | "ipa" | "xpi" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "tar.gz" | "tgz" => Ok(ArchiveFormat::TarGz),
        "tar.bz2" | "tbz" | "tbz2" => Ok(ArchiveFormat::TarBz2),
        "gz" | "gzip" => Ok(ArchiveFormat::Gzip),
        "bz2" | "bzip2" => Ok(ArchiveFormat::Bzip2),
        "zst" | "zstd" => Ok(ArchiveFormat::Zstd),
        "tar.zst" | "tzst" => Ok(ArchiveFormat::TarZst),
        "tar.xz" | "txz" => Ok(ArchiveFormat::TarXz),
        "xz" => Ok(ArchiveFormat::Xz),
        "lzma" => Ok(ArchiveFormat::Lzma),
        "7z" => Ok(ArchiveFormat::SevenZip),
        "rar" => Ok(ArchiveFormat::Rar),
        other => Err(anyhow::anyhow!(
            "unsupported format '{other}'; expected: zip, jar, war, apk, ipa, xpi, tar, tar.gz, tgz, tar.bz2, tbz, tbz2, gz, gzip, bz2, bzip2, zst, zstd, tar.zst, tzst, tar.xz, txz, xz, lzma, 7z, rar"
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
/// 2. If gzip/bzip2 magic, check tar-wrapped extensions first.
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
        Some(ArchiveFormat::Bzip2) => {
            // Bzip2 magic is also present in `.tar.bz2` / `.tbz` / `.tbz2`.
            if let Some(ArchiveFormat::TarBz2) = detect::detect_from_extension(path) {
                Ok(ArchiveFormat::TarBz2)
            } else {
                Ok(ArchiveFormat::Bzip2)
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
            // XZ magic is also present in `.tar.xz` / `.txz` — check extension.
            if let Some(ArchiveFormat::TarXz) = detect::detect_from_extension(path) {
                Ok(ArchiveFormat::TarXz)
            } else {
                Ok(ArchiveFormat::Xz)
            }
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
/// A file or directory entry collected for compression.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Real path on the filesystem.
    pub real_path: PathBuf,
    /// Relative path inside the archive.
    pub archive_path: PathBuf,
    /// Whether this entry represents a directory (not a regular file).
    pub is_dir: bool,
}

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
            let has_children = collect_dir_contents(&input, &prefix, &mut result)
                .with_context(|| format!("reading directory '{}'", input.display()))?;
            if !has_children {
                result.push(FileEntry {
                    real_path: input.clone(),
                    archive_path: prefix.clone(),
                    is_dir: true,
                });
            }
        } else if input.is_file() {
            // Use the file's basename as the archive entry path.
            let name = input
                .file_name()
                .unwrap_or(input.as_os_str())
                .to_os_string();
            result.push(FileEntry {
                real_path: input.clone(),
                archive_path: PathBuf::from(name),
                is_dir: false,
            });
        } else {
            anyhow::bail!("'{}' does not exist", input.display());
        }
    }

    Ok(result)
}

/// Recursively walk `dir` and add files to `entries`, prepending `prefix`
/// to each entry path.
fn collect_dir_contents(
    dir: &Path,
    prefix: &Path,
    entries: &mut Vec<FileEntry>,
) -> io::Result<bool> {
    let mut has_children = false;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            let child_has_children = collect_dir_contents(&path, &relative, entries)?;
            if child_has_children {
                has_children = true;
            } else {
                // Empty directory: add it as a directory entry.
                entries.push(FileEntry {
                    real_path: path,
                    archive_path: relative,
                    is_dir: true,
                });
                has_children = true;
            }
        } else if path.is_file() {
            entries.push(FileEntry {
                real_path: path,
                archive_path: relative,
                is_dir: false,
            });
            has_children = true;
        }
    }
    Ok(has_children)
}

// ---------------------------------------------------------------------------
// Reader / writer factories
// ---------------------------------------------------------------------------

pub fn open_reader(
    path: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<Box<dyn ArchiveReader>> {
    let file = fs::File::open(path).with_context(|| format!("opening '{}'", path.display()))?;

    // Validate password: only ZIP, 7z, and RAR support it.
    if password.is_some()
        && format != ArchiveFormat::Zip
        && format != ArchiveFormat::SevenZip
        && format != ArchiveFormat::Rar
    {
        anyhow::bail!(
            "--password is only supported for ZIP, 7z, and RAR formats; '{}' does not support encryption",
            format
        );
    }

    Ok(match format {
        ArchiveFormat::Zip => {
            let mut reader = Box::new(ZipReader::new(file)?);
            if let Some(pwd) = password {
                reader.set_password(pwd);
            }
            reader
        }
        ArchiveFormat::SevenZip => {
            let mut reader = Box::new(SevenZipReader::new(path));
            if let Some(pwd) = password {
                reader.set_password(pwd);
            }
            reader
        }
        ArchiveFormat::Tar => Box::new(TarReader::new(file)),
        ArchiveFormat::TarGz => Box::new(TarGzReader::new(file)),
        ArchiveFormat::TarBz2 => Box::new(TarBz2Reader::new(file)),
        ArchiveFormat::TarZst => Box::new(TarZstReader::new(file)),
        ArchiveFormat::TarXz => Box::new(TarXzReader::new(file)),
        #[cfg(feature = "rar")]
        ArchiveFormat::Rar => {
            let mut reader = Box::new(RarReader::new(path));
            if let Some(pwd) = password {
                let _ = reader.set_password(pwd);
            }
            reader
        }
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma => anyhow::bail!(
            "'{}' is a single-stream compression format; use 'decompress' directly, not an archive reader",
            format
        ),
        _ => anyhow::bail!("unsupported format for reading: {format}"),
    })
}

/// Create an archive writer with the given compression options.
pub fn create_writer(
    file: fs::File,
    format: ArchiveFormat,
    options: CompressOptions,
) -> Result<Box<dyn ArchiveWriter>> {
    // Validate password: only ZIP format supports password for writing.
    if options.password.is_some() && format != ArchiveFormat::Zip {
        anyhow::bail!(
            "--password is only supported for ZIP format; '{}' does not support encryption",
            format
        );
    }
    // Validate: non-empty password required.
    if let Some(ref pwd) = options.password {
        if pwd.is_empty() {
            anyhow::bail!("--password cannot be empty");
        }
    }
    match format {
        ArchiveFormat::Zip => {
            let mut writer = ZipWriter::new(file);
            if let Some(pwd) = &options.password {
                writer.set_password(pwd);
            }
            Ok(Box::new(writer))
        }
        ArchiveFormat::Tar => Ok(Box::new(TarWriter::new(file))),
        ArchiveFormat::TarGz => Ok(Box::new(TarGzWriter::new_with_options(file, options))),
        ArchiveFormat::TarBz2 => Ok(Box::new(TarBz2Writer::new_with_options(file, options))),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstWriter::new_with_options(file, options))),
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma => {
            anyhow::bail!(
                "'{format}' is a single-stream compression format; use 'compress' directly, not an archive writer"
            )
        }
        ArchiveFormat::TarXz => Ok(Box::new(TarXzWriter::new_with_options(file, options))),
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

/// Infer the decompressed filename for a bzip2 file by stripping `.bz2`.
pub fn bzip2_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name.strip_suffix(".bz2").unwrap_or(&name);
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

// ---------------------------------------------------------------------------
// Password resolution
// ---------------------------------------------------------------------------

/// Resolve a password from one of three mutually exclusive sources:
/// `--password`, `--password-file`, `--password-stdin`.
///
/// Returns `None` if no source was provided.  For `--password-file` and
/// `--password-stdin` the trailing newline (LF or CRLF) is stripped.
/// Returns an error if more than one source is specified, if the file
/// cannot be read, or if the resolved password is empty.
pub fn resolve_password(
    password: Option<String>,
    password_file: Option<PathBuf>,
    password_stdin: bool,
) -> Result<Option<String>> {
    let sources = password.is_some() as u8 + password_file.is_some() as u8 + password_stdin as u8;
    if sources > 1 {
        anyhow::bail!("--password, --password-file, and --password-stdin are mutually exclusive");
    }

    if let Some(path) = password_file {
        let mut buf = String::new();
        fs::File::open(&path)
            .with_context(|| format!("opening password file '{}'", path.display()))?
            .read_to_string(&mut buf)
            .with_context(|| format!("reading password file '{}'", path.display()))?;
        let pwd = buf
            .strip_suffix("\r\n")
            .or_else(|| buf.strip_suffix('\n'))
            .unwrap_or(&buf)
            .to_string();
        if pwd.is_empty() {
            anyhow::bail!("password file '{}' is empty", path.display());
        }
        return Ok(Some(pwd));
    }

    if password_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading password from stdin")?;
        let pwd = buf
            .strip_suffix("\r\n")
            .or_else(|| buf.strip_suffix('\n'))
            .unwrap_or(&buf)
            .to_string();
        if pwd.is_empty() {
            anyhow::bail!("password from stdin is empty");
        }
        return Ok(Some(pwd));
    }

    Ok(password)
}
