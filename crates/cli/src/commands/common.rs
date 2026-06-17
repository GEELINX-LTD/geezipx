//! Shared utilities for CLI commands.
//!
//! Provides format parsing, file collection, and reader/writer factory
//! functions used across multiple subcommands.

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use geezipx_core::archive::asar::AsarReader;
use geezipx_core::archive::cab::CabReader;
use geezipx_core::archive::cpio::{CpioReader, CpioWriter};
use geezipx_core::archive::deb::DebReader;
use geezipx_core::archive::iso::IsoReader;
use geezipx_core::archive::iso::IsoWriter;
use geezipx_core::archive::lzh::{LzhReader, LzhWriter};
#[cfg(feature = "rar")]
use geezipx_core::archive::rar::RarReader;
use geezipx_core::archive::seven_zip::{SevenZipReader, SevenZipWriter};
use geezipx_core::archive::tar::TarReader;
use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::tarbr::TarBrReader;
use geezipx_core::archive::tarbr::TarBrWriter;
use geezipx_core::archive::tarbz2::TarBz2Reader;
use geezipx_core::archive::tarbz2::TarBz2Writer;
use geezipx_core::archive::targz::TarGzReader;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarlz4::TarLz4Reader;
use geezipx_core::archive::tarlz4::TarLz4Writer;
use geezipx_core::archive::tarxz::TarXzReader;
use geezipx_core::archive::tarxz::TarXzWriter;
use geezipx_core::archive::tarzst::TarZstReader;
use geezipx_core::archive::tarzst::TarZstWriter;
#[cfg(feature = "wim")]
use geezipx_core::archive::wim::WimReader;
use geezipx_core::archive::zip::ZipReader;
use geezipx_core::archive::zip::ZipWriter;
#[cfg(feature = "zpaq")]
use geezipx_core::archive::zpaq::ZpaqReader;
use geezipx_core::archive::{ArchiveReader, ArchiveWriter};
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::{self, ArchiveFormat};

// ---------------------------------------------------------------------------
// Format resolution
// ---------------------------------------------------------------------------

/// Parse a user-supplied format string into an [`ArchiveFormat`].
///
/// Accepts: `zip`, ZIP-derived aliases (`zipx`, `jar`, `war`, `apk`, `ipa`, `xpi`),
/// `tar`, `tar.gz`, `tgz`, `tar.bz2`, `tbz`, `tbz2`, `tar.br`, `gz`, `gzip`,
/// `bz2`, `bzip2`, `br`, `brotli`, `lz4`, `tar.lz4`, `zst`, `zstd`, `tar.zst`,
/// `tzst`, `tar.xz`, `txz`, `xz`, `lzma`, `7z`, `rar`, `cab`, `asar`, `deb`,
/// `lzh`, `lha`, `iso`, `cpio`, `zpaq`, `zpq`.
pub fn parse_format(s: &str) -> Result<ArchiveFormat> {
    match s.to_ascii_lowercase().as_str() {
        "zip" | "zipx" | "jar" | "war" | "apk" | "ipa" | "xpi" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "tar.gz" | "tgz" => Ok(ArchiveFormat::TarGz),
        "tar.bz2" | "tbz" | "tbz2" => Ok(ArchiveFormat::TarBz2),
        "tar.br" => Ok(ArchiveFormat::TarBr),
        "gz" | "gzip" => Ok(ArchiveFormat::Gzip),
        "bz2" | "bzip2" => Ok(ArchiveFormat::Bzip2),
        "br" | "brotli" => Ok(ArchiveFormat::Brotli),
        "lz4" => Ok(ArchiveFormat::Lz4),
        "tar.lz4" => Ok(ArchiveFormat::TarLz4),
        "zst" | "zstd" => Ok(ArchiveFormat::Zstd),
        "tar.zst" | "tzst" => Ok(ArchiveFormat::TarZst),
        "tar.xz" | "txz" => Ok(ArchiveFormat::TarXz),
        "xz" => Ok(ArchiveFormat::Xz),
        "lzma" => Ok(ArchiveFormat::Lzma),
        "7z" => Ok(ArchiveFormat::SevenZip),
        "rar" => Ok(ArchiveFormat::Rar),
        "cab" => Ok(ArchiveFormat::Cab),
        "asar" => Ok(ArchiveFormat::Asar),
        "deb" => Ok(ArchiveFormat::Deb),
        "lzh" | "lha" => Ok(ArchiveFormat::Lzh),
        "iso" => Ok(ArchiveFormat::Iso),
        "cpio" => Ok(ArchiveFormat::Cpio),
        "wim" | "swm" => Ok(ArchiveFormat::Wim),
        "zpaq" | "zpq" => Ok(ArchiveFormat::Zpaq),
        other => Err(anyhow::anyhow!(
            "unsupported format '{other}'; expected: zip, zipx, jar, war, apk, ipa, xpi, tar, tar.gz, tgz, tar.bz2, tbz, tbz2, tar.br, gz, gzip, bz2, bzip2, br, brotli, lz4, tar.lz4, zst, zstd, tar.zst, tzst, tar.xz, txz, xz, lzma, 7z, rar, cab, asar, deb, lzh, lha, iso, cpio, zpaq, zpq, wim, swm"
        )),
    }
}

/// Parse a user-supplied SFX target string.
///
/// Accepts: `linux`, `windows`, `macos` (or `mac`).
#[cfg(feature = "sfx")]
pub fn parse_sfx_target(s: &str) -> anyhow::Result<geezipx_core::sfx::SfxTarget> {
    match s.to_ascii_lowercase().as_str() {
        "linux" => Ok(geezipx_core::sfx::SfxTarget::Linux),
        "windows" => Ok(geezipx_core::sfx::SfxTarget::Windows),
        "macos" | "mac" => Ok(geezipx_core::sfx::SfxTarget::MacOS),
        other => Err(anyhow::anyhow!(
            "unsupported SFX target '{}'; expected: linux, windows, macos",
            other
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
/// 2. If gzip/bzip2/lz4 magic, check tar-wrapped extensions first.
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
        Some(ArchiveFormat::Lz4) => {
            // LZ4 frame magic is also present in `.tar.lz4`.
            if let Some(ArchiveFormat::TarLz4) = detect::detect_from_extension(path) {
                Ok(ArchiveFormat::TarLz4)
            } else {
                Ok(ArchiveFormat::Lz4)
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
    let mut deferred_empty_dirs = Vec::new();

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
            let has_children =
                collect_dir_contents(&input, &prefix, &mut result, &mut deferred_empty_dirs)
                    .with_context(|| format!("reading directory '{}'", input.display()))?;
            if !has_children {
                deferred_empty_dirs.push(FileEntry {
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

    result.extend(deferred_empty_dirs);
    Ok(result)
}

/// Recursively walk `dir` and add files to `entries`, prepending `prefix`
/// to each entry path.
fn collect_dir_contents(
    dir: &Path,
    prefix: &Path,
    entries: &mut Vec<FileEntry>,
    deferred_empty_dirs: &mut Vec<FileEntry>,
) -> io::Result<bool> {
    let mut has_children = false;
    let mut dir_entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    dir_entries.sort_by_key(|a| a.file_name());

    for entry in dir_entries {
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            let child_has_children =
                collect_dir_contents(&path, &relative, entries, deferred_empty_dirs)?;
            if child_has_children {
                has_children = true;
            } else {
                // Empty directories must still be archived, but defer all of them
                // until after data-bearing entries to keep 7z extraction stable.
                deferred_empty_dirs.push(FileEntry {
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

pub fn validate_read_password_support(format: ArchiveFormat, password: Option<&str>) -> Result<()> {
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

    Ok(())
}

pub fn open_reader(
    path: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<Box<dyn ArchiveReader>> {
    let file = fs::File::open(path).with_context(|| format!("opening '{}'", path.display()))?;

    validate_read_password_support(format, password)?;

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
        ArchiveFormat::Asar => Box::new(AsarReader::new(path)),
        ArchiveFormat::Cab => Box::new(CabReader::new(path)),
        ArchiveFormat::Deb => Box::new(DebReader::new(file)),
        ArchiveFormat::Cpio => Box::new(CpioReader::new(path)),
        ArchiveFormat::Lzh => Box::new(LzhReader::new(file)),
        ArchiveFormat::Iso => Box::new(IsoReader::new(file)),
        #[cfg(feature = "wim")]
        ArchiveFormat::Wim => Box::new(WimReader::open(path)?),
        #[cfg(not(feature = "wim"))]
        ArchiveFormat::Wim => anyhow::bail!(
            "'wim' support is disabled in this build; rebuild with --features wim"
        ),
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => Box::new(ZpaqReader::new(path)),
        #[cfg(not(feature = "zpaq"))]
        ArchiveFormat::Zpaq => anyhow::bail!(
            "'zpaq' support is disabled in this build; rebuild with --features zpaq"
        ),
        ArchiveFormat::Tar => Box::new(TarReader::new(file)),
        ArchiveFormat::TarGz => Box::new(TarGzReader::new(file)),
        ArchiveFormat::TarBz2 => Box::new(TarBz2Reader::new(file)),
        ArchiveFormat::TarBr => Box::new(TarBrReader::new(file)),
        ArchiveFormat::TarLz4 => Box::new(TarLz4Reader::new(file)),
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
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
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
    // Validate password: only ZIP and 7z formats support password for writing.
    if options.password.is_some()
        && format != ArchiveFormat::Zip
        && format != ArchiveFormat::SevenZip
    {
        anyhow::bail!(
            "--password is only supported for ZIP and 7z formats; '{}' does not support encryption",
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
        ArchiveFormat::SevenZip => {
            let mut writer = SevenZipWriter::new(file)?;
            if let Some(pwd) = &options.password {
                writer.set_password(pwd)?;
            }
            Ok(Box::new(writer))
        }
        ArchiveFormat::TarGz => Ok(Box::new(TarGzWriter::new_with_options(file, options))),
        ArchiveFormat::TarBz2 => Ok(Box::new(TarBz2Writer::new_with_options(file, options))),
        ArchiveFormat::TarBr => Ok(Box::new(TarBrWriter::new_with_options(file, options)?)),
        ArchiveFormat::TarLz4 => Ok(Box::new(TarLz4Writer::new_with_options(file, options)?)),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstWriter::new_with_options(file, options))),
        ArchiveFormat::Lzh => Ok(Box::new(LzhWriter::new(file))),
        ArchiveFormat::Iso => Ok(Box::new(IsoWriter::new(file))),
        ArchiveFormat::Asar | ArchiveFormat::Cab | ArchiveFormat::Deb | ArchiveFormat::Wim => {
            anyhow::bail!("'{format}' is a read-only archive format; writing is not supported")
        }
        ArchiveFormat::Cpio => Ok(Box::new(CpioWriter::new(file))),
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => {
            let writer = geezipx_core::archive::zpaq::ZpaqWriter::new(file, options.level);
            Ok(Box::new(writer))
        }

        #[cfg(not(feature = "zpaq"))]
        ArchiveFormat::Zpaq => {
            anyhow::bail!("'zpaq' support is disabled in this build; rebuild with --features zpaq")
        }
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
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

/// Infer the decompressed filename for a Brotli file by stripping `.br`.
pub fn brotli_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name.strip_suffix(".br").unwrap_or(&name);
    PathBuf::from(stripped)
}

/// Infer the decompressed filename for an lz4 file by stripping `.lz4`.
pub fn lz4_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = name.strip_suffix(".lz4").unwrap_or(&name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn build_raw_asar(header_json: &str, payload: &[u8]) -> Vec<u8> {
        let mut json = header_json.as_bytes().to_vec();
        let json_size = json.len() as u32;
        let aligned_json_size = json_size + (4 - (json_size % 4)) % 4;
        json.resize(aligned_json_size as usize, 0);

        let mut out = Vec::with_capacity(16 + json.len() + payload.len());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&(aligned_json_size + 8).to_le_bytes());
        out.extend_from_slice(&(aligned_json_size + 4).to_le_bytes());
        out.extend_from_slice(&json_size.to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(payload);
        out
    }

    fn build_raw_tar(path: &[u8], data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name_len = path.len().min(99);
        header[..name_len].copy_from_slice(&path[..name_len]);
        header[100..108].copy_from_slice(b"0000644\0");
        let size_oct = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size_oct.as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        for b in header.iter_mut().take(156).skip(148) {
            *b = b' ';
        }
        let cksum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{:06o}\0 ", cksum);
        header[148..156].copy_from_slice(cksum_str.as_bytes());

        let mut archive = header.to_vec();
        archive.extend_from_slice(data);
        let padding = (512 - data.len() % 512) % 512;
        archive.extend(std::iter::repeat_n(0, padding));
        archive.extend_from_slice(&[0u8; 1024]);
        archive
    }

    fn append_ar_member(out: &mut Vec<u8>, name: &str, data: &[u8]) {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8o}{:<10}`\n",
            name,
            0,
            0,
            0,
            0o100644,
            data.len()
        );
        assert_eq!(header.len(), 60);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            out.push(b'\n');
        }
    }

    fn build_test_deb() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"!<arch>\n");
        append_ar_member(&mut out, "debian-binary", b"2.0\n");
        append_ar_member(&mut out, "control.tar.gz", b"ignored");
        append_ar_member(&mut out, "data.tar", &build_raw_tar(b"hello.txt", b"hello"));
        out
    }

    fn build_test_cab(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = cab::CabinetBuilder::new();
        {
            let folder = builder.add_folder(cab::CompressionType::MsZip);
            for (path, _) in entries {
                folder.add_file(*path);
            }
        }

        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = builder.build(cursor).unwrap();
        let mut index = 0usize;
        while let Some(mut file_writer) = writer.next_file().unwrap() {
            file_writer.write_all(entries[index].1).unwrap();
            index += 1;
        }
        writer.finish().unwrap().into_inner()
    }

    fn push_newc_hex(out: &mut Vec<u8>, value: u64, width: usize) {
        out.extend_from_slice(format!("{value:0width$X}", width = width).as_bytes());
    }

    fn build_test_cpio(entries: &[(&str, &[u8])]) -> Vec<u8> {
        fn append_newc_entry(out: &mut Vec<u8>, inode: u32, path: &str, data: &[u8]) {
            out.extend_from_slice(b"070701");
            push_newc_hex(out, u64::from(inode), 8);
            push_newc_hex(out, 0o100644, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, 1, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, data.len() as u64, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, 0, 8);
            push_newc_hex(out, (path.len() + 1) as u64, 8);
            push_newc_hex(out, 0, 8);
            out.extend_from_slice(path.as_bytes());
            out.push(0);
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
            out.extend_from_slice(data);
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
        }

        let mut out = Vec::new();
        for (index, (path, data)) in entries.iter().enumerate() {
            append_newc_entry(&mut out, (index + 1) as u32, path, data);
        }
        append_newc_entry(&mut out, 0, "TRAILER!!!", b"");
        out
    }

    #[cfg(feature = "zpaq")]
    fn build_test_zpaq() -> Vec<u8> {
        zpaq_rs::archive_from_entries(
            &[zpaq_rs::ArchiveEntry {
                path: "hello.txt",
                data: b"hello zpaq\n",
                comment: None,
            }],
            "1",
        )
        .unwrap()
    }

    #[test]
    fn collect_inputs_defers_empty_directories_until_after_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(src.join("a_empty")).unwrap();
        std::fs::create_dir_all(src.join("b_nested")).unwrap();
        std::fs::write(src.join("b_nested/file.txt"), "nested file").unwrap();

        let entries = collect_inputs(std::slice::from_ref(&src), true).unwrap();
        let archive_paths: Vec<_> = entries
            .iter()
            .map(|entry| entry.archive_path.clone())
            .collect();

        assert_eq!(
            archive_paths,
            vec![
                PathBuf::from("src").join("b_nested").join("file.txt"),
                PathBuf::from("src").join("a_empty"),
            ]
        );
    }

    #[test]
    fn collect_inputs_defers_nested_empty_directories_globally() {
        let temp = tempfile::TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(src.join("a_parent/empty")).unwrap();
        std::fs::write(src.join("z.txt"), "later file").unwrap();

        let entries = collect_inputs(std::slice::from_ref(&src), true).unwrap();
        let archive_paths: Vec<_> = entries
            .iter()
            .map(|entry| entry.archive_path.clone())
            .collect();

        assert_eq!(
            archive_paths,
            vec![
                PathBuf::from("src").join("z.txt"),
                PathBuf::from("src").join("a_parent").join("empty"),
            ]
        );
    }

    #[test]
    fn collect_inputs_defers_top_level_empty_directory_inputs() {
        let temp = tempfile::TempDir::new().unwrap();
        let empty_dir = temp.path().join("a_empty");
        std::fs::create_dir_all(&empty_dir).unwrap();
        let file = temp.path().join("b.txt");
        std::fs::write(&file, "top-level").unwrap();

        let entries = collect_inputs(&[empty_dir, file], true).unwrap();
        let archive_paths: Vec<_> = entries
            .iter()
            .map(|entry| entry.archive_path.clone())
            .collect();

        assert_eq!(
            archive_paths,
            vec![PathBuf::from("b.txt"), PathBuf::from("a_empty")]
        );
    }

    #[test]
    fn parse_format_asar() {
        assert_eq!(parse_format("asar").unwrap(), ArchiveFormat::Asar);
    }

    #[test]
    fn parse_format_zipx_alias() {
        assert_eq!(parse_format("zipx").unwrap(), ArchiveFormat::Zip);
    }

    #[test]
    fn parse_format_deb() {
        assert_eq!(parse_format("deb").unwrap(), ArchiveFormat::Deb);
    }

    #[test]
    fn parse_format_cab() {
        assert_eq!(parse_format("cab").unwrap(), ArchiveFormat::Cab);
    }

    #[test]
    fn parse_format_lzh_aliases() {
        assert_eq!(parse_format("lzh").unwrap(), ArchiveFormat::Lzh);
        assert_eq!(parse_format("lha").unwrap(), ArchiveFormat::Lzh);
    }

    #[test]
    fn parse_format_iso() {
        assert_eq!(parse_format("iso").unwrap(), ArchiveFormat::Iso);
    }

    #[test]
    fn parse_format_cpio() {
        assert_eq!(parse_format("cpio").unwrap(), ArchiveFormat::Cpio);
    }

    #[test]
    fn parse_format_zpaq_aliases() {
        assert_eq!(parse_format("zpaq").unwrap(), ArchiveFormat::Zpaq);
        assert_eq!(parse_format("zpq").unwrap(), ArchiveFormat::Zpaq);
    }

    #[test]
    fn detect_archive_format_asar_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("app.asar");
        let header = r#"{"files":{"hello.txt":{"size":5,"offset":"0"}}}"#;
        std::fs::write(&archive, build_raw_asar(header, b"hello")).unwrap();

        assert_eq!(
            detect_archive_format(&archive).unwrap(),
            ArchiveFormat::Asar
        );
    }

    #[test]
    fn detect_archive_format_deb_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("package.deb");
        std::fs::write(&archive, build_test_deb()).unwrap();

        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::Deb);
    }

    #[test]
    fn detect_archive_format_cab_magic() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.cab");
        std::fs::write(&archive, build_test_cab(&[("hello.txt", b"hello")])).unwrap();

        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::Cab);
    }

    fn build_test_lzh() -> Vec<u8> {
        fn crc16(data: &[u8]) -> u16 {
            let mut sum = 0u16;
            for &byte in data {
                sum ^= u16::from(byte);
                for _ in 0..8 {
                    if sum & 1 == 1 {
                        sum = (sum >> 1) ^ 0xA001;
                    } else {
                        sum >>= 1;
                    }
                }
            }
            sum
        }

        let name = b"hello.txt";
        let data = b"hello";
        let mut header = Vec::new();
        header.extend_from_slice(b"-lh0-");
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.push(0x20);
        header.push(0);
        header.push(name.len() as u8);
        header.extend_from_slice(name);
        header.extend_from_slice(&crc16(data).to_le_bytes());

        let checksum = header.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        let mut archive = Vec::new();
        archive.push(header.len() as u8);
        archive.push(checksum);
        archive.extend_from_slice(&header);
        archive.extend_from_slice(data);
        archive.push(0);
        archive
    }

    #[test]
    fn detect_archive_format_lzh_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.lzh");
        std::fs::write(&archive, build_test_lzh()).unwrap();

        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::Lzh);
    }

    #[test]
    fn detect_archive_format_iso_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("disc.iso");
        std::fs::write(&archive, b"not an iso").unwrap();

        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::Iso);
    }

    #[test]
    fn detect_archive_format_cpio_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.cpio");
        std::fs::write(&archive, build_test_cpio(&[("hello.txt", b"hello")])).unwrap();

        assert_eq!(
            detect_archive_format(&archive).unwrap(),
            ArchiveFormat::Cpio
        );
    }

    #[test]
    fn detect_archive_format_zpaq_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("backup.zpaq");
        std::fs::write(&archive, b"not a zpaq yet").unwrap();

        assert_eq!(
            detect_archive_format(&archive).unwrap(),
            ArchiveFormat::Zpaq
        );
    }

    #[test]
    fn open_reader_asar_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("app.asar");
        let header = r#"{"files":{"hello.txt":{"size":5,"offset":"0"}}}"#;
        std::fs::write(&archive, build_raw_asar(header, b"hello")).unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Asar, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn open_reader_deb_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("package.deb");
        std::fs::write(&archive, build_test_deb()).unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Deb, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn open_reader_cab_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.cab");
        std::fs::write(
            &archive,
            build_test_cab(&[("docs\\hello.txt", b"hello"), ("readme.txt", b"readme")]),
        )
        .unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Cab, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.path == "docs/hello.txt"));
        assert!(entries.iter().any(|entry| entry.path == "readme.txt"));
    }

    #[test]
    fn open_reader_lzh_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.lzh");
        std::fs::write(&archive, build_test_lzh()).unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Lzh, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn open_reader_cpio_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.cpio");
        std::fs::write(
            &archive,
            build_test_cpio(&[("docs/hello.txt", b"hello"), ("readme.txt", b"readme")]),
        )
        .unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Cpio, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.path == "docs/hello.txt"));
        assert!(entries.iter().any(|entry| entry.path == "readme.txt"));
    }

    #[cfg(feature = "zpaq")]
    #[test]
    fn open_reader_zpaq_lists_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = temp.path().join("archive.zpaq");
        std::fs::write(&archive, build_test_zpaq()).unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Zpaq, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn create_writer_deb_is_read_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.deb");
        let file = fs::File::create(&output).unwrap();
        match create_writer(file, ArchiveFormat::Deb, CompressOptions::default()) {
            Ok(_) => panic!("deb writer should be rejected"),
            Err(err) => assert!(err.to_string().contains("read-only archive format")),
        }
    }

    #[test]
    fn create_writer_cab_is_read_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.cab");
        let file = fs::File::create(&output).unwrap();
        match create_writer(file, ArchiveFormat::Cab, CompressOptions::default()) {
            Ok(_) => panic!("cab writer should be rejected"),
            Err(err) => assert!(err.to_string().contains("read-only archive format")),
        }
    }

    #[test]
    fn create_writer_lzh_is_writable() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.lzh");
        let file = fs::File::create(&output).unwrap();
        let mut writer =
            create_writer(file, ArchiveFormat::Lzh, CompressOptions::default()).unwrap();
        writer
            .add_entry_from_reader(
                std::path::Path::new("hello.txt"),
                &mut std::io::Cursor::new(b"hello from cli".to_vec()),
            )
            .unwrap();
        let bytes_written = writer.finish().unwrap();
        assert_eq!(bytes_written, fs::metadata(&output).unwrap().len());

        let mut reader = open_reader(&output, ArchiveFormat::Lzh, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn create_writer_iso_is_writable() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.iso");
        let file = fs::File::create(&output).unwrap();
        let mut writer =
            create_writer(file, ArchiveFormat::Iso, CompressOptions::default()).unwrap();
        writer
            .add_entry_from_reader(
                std::path::Path::new("CLI.TXT"),
                &mut std::io::Cursor::new(b"cli iso test"),
            )
            .unwrap();
        let bytes_written = writer.finish().unwrap();
        assert!(bytes_written > 0);
        assert_eq!(bytes_written, fs::metadata(&output).unwrap().len());

        let mut reader = open_reader(&output, ArchiveFormat::Iso, None).unwrap();
        let entries = reader.entries().unwrap();
        let file_entry = entries.iter().find(|e| e.path.contains("CLI.TXT")).unwrap();
        assert!(!file_entry.is_dir);
        let mut out = Vec::new();
        reader.extract(file_entry, &mut out).unwrap();
        assert_eq!(out, b"cli iso test");
    }
    #[test]
    fn create_writer_cpio_is_writable() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.cpio");
        let file = fs::File::create(&output).unwrap();
        let result = create_writer(file, ArchiveFormat::Cpio, CompressOptions::default());
        assert!(
            result.is_ok(),
            "cpio writer should be supported, got error: {}",
            result
                .as_ref()
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
        );
    }

    #[test]
    fn create_writer_zpaq_is_writable() {
        let temp = tempfile::TempDir::new().unwrap();
        let output = temp.path().join("out.zpaq");
        let file = fs::File::create(&output).unwrap();
        let mut writer = create_writer(file, ArchiveFormat::Zpaq, CompressOptions::default())
            .expect("zpaq writer should be creatable");
        writer
            .add_entry_from_reader(std::path::Path::new("hello.txt"), &mut "hello".as_bytes())
            .expect("file should be added");
        let written = writer.finish().expect("writer should finish");
        assert!(written > 0);
    }
}
