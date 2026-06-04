//! Format detection from magic bytes and file extensions.
//!
//! This module provides lightweight magic-byte matching to identify
//! archive and compression formats without pulling in heavy format
//! libraries.  Tar is recognised via file extension only (no magic).

use std::fmt;
use std::path::Path;

// ---------------------------------------------------------------------------
// ArchiveFormat
// ---------------------------------------------------------------------------

/// Supported archive and compression format identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArchiveFormat {
    /// ZIP archive.
    Zip,
    /// GNU tar archive (no magic — extension-based).
    Tar,
    /// Gzip compressed stream (`\x1F\x8B`).
    Gzip,
    /// Tar archive compressed with gzip (extension-based — `.tar.gz`, `.tgz`).
    TarGz,
    /// LZMA-compressed XZ stream (`\xFD7zXZ\x00`).
    Xz,
    /// Zstandard frame (`\x28\xB5\x2F\xFD`).
    Zstd,
    /// Tar archive compressed with Zstandard (extension-based — `.tar.zst`, `.tzst`).
    TarZst,
    /// LZMA Alone compressed stream (no magic — extension-based).
    Lzma,
    /// Tar archive compressed with XZ (extension-based — `.tar.xz`, `.txz`).
    TarXz,
    /// 7z archive (`37 7A BC AF 27 1C`).
    SevenZip,
    /// RAR archive (`52 61 72 21 1A 07`).
    Rar,
    /// Unknown or unrecognised format.
    Unknown,
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveFormat::Zip => write!(f, "zip"),
            ArchiveFormat::Tar => write!(f, "tar"),
            ArchiveFormat::Gzip => write!(f, "gzip"),
            ArchiveFormat::TarGz => write!(f, "tar.gz"),
            ArchiveFormat::Xz => write!(f, "xz"),
            ArchiveFormat::Zstd => write!(f, "zstd"),
            ArchiveFormat::TarZst => write!(f, "tar.zst"),
            ArchiveFormat::TarXz => write!(f, "tar.xz"),
            ArchiveFormat::Lzma => write!(f, "lzma"),
            ArchiveFormat::SevenZip => write!(f, "7z"),
            ArchiveFormat::Rar => write!(f, "rar"),
            ArchiveFormat::Unknown => write!(f, "unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Magic constants
// ---------------------------------------------------------------------------

/// ZIP local file header signature: `PK\x03\x04`.
const MAGIC_ZIP: &[u8] = b"PK\x03\x04";
/// ZIP empty archive (EOCD-only) signature: `PK\x05\x06`.
const MAGIC_ZIP_EMPTY: &[u8] = b"PK\x05\x06";
/// Gzip magic: `\x1F\x8B`.
const MAGIC_GZIP: &[u8] = &[0x1F, 0x8B];
/// Zstandard magic: `\x28\xB5\x2F\xFD`.
const MAGIC_ZSTD: &[u8] = &[0x28, 0xB5, 0x2F, 0xFD];
/// XZ magic: `\xFD7zXZ\x00`.
const MAGIC_XZ: &[u8] = &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];
/// 7z magic: `37 7A BC AF 27 1C`.
pub const MAGIC_SEVENZIP: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
/// RAR magic: `52 61 72 21 1A 07` (RAR4 and RAR5 share this prefix).
pub const MAGIC_RAR: &[u8] = b"Rar!\x1A\x07";

/// Map of well-known single extensions to their archive format.
const EXTENSION_MAP: &[(&str, ArchiveFormat)] = &[
    (".zip", ArchiveFormat::Zip),
    (".tar", ArchiveFormat::Tar),
    (".gz", ArchiveFormat::Gzip),
    // Note: .tgz is handled by detect_from_extension's compound check → TarGz
    (".xz", ArchiveFormat::Xz),
    (".zst", ArchiveFormat::Zstd),
    (".zstd", ArchiveFormat::Zstd),
    (".lzma", ArchiveFormat::Lzma),
    (".gzip", ArchiveFormat::Gzip),
    (".7z", ArchiveFormat::SevenZip),
    (".rar", ArchiveFormat::Rar),
];

// ---------------------------------------------------------------------------
// Magic-number-based detection
// ---------------------------------------------------------------------------

/// Detect the archive format from magic bytes.
///
/// Reads the first few bytes of `data` and compares against known magic
/// sequences.  Returns `None` when no magic matches; the caller can
/// fall back to extension-based detection.
///
/// **Note**: For `.tar.gz` and `.tgz` files, this function returns `Gzip`
/// because the gzip magic header (`\x1F\x8B`) does not reveal whether the
/// inner stream is a tar archive.
pub fn detect_format(data: &[u8]) -> Option<ArchiveFormat> {
    if data.starts_with(MAGIC_ZIP) || data.starts_with(MAGIC_ZIP_EMPTY) {
        return Some(ArchiveFormat::Zip);
    }
    if data.starts_with(MAGIC_GZIP) {
        return Some(ArchiveFormat::Gzip);
    }
    if data.starts_with(MAGIC_ZSTD) {
        return Some(ArchiveFormat::Zstd);
    }
    if data.starts_with(MAGIC_SEVENZIP) {
        return Some(ArchiveFormat::SevenZip);
    }
    if data.starts_with(MAGIC_RAR) {
        return Some(ArchiveFormat::Rar);
    }
    if data.starts_with(MAGIC_XZ) {
        return Some(ArchiveFormat::Xz);
    }
    None
}

// ---------------------------------------------------------------------------
// Extension-based detection
// ---------------------------------------------------------------------------

/// Detect the archive format from a file path's extension.
///
/// This is a best-effort fallback for formats without a magic signature
/// (e.g. tar) and for stream sources where magic detection isn't possible.
///
/// **Compound extensions** (`.tar.gz`, `.tgz`) are checked first so they
/// take priority over the single-extension map.
pub fn detect_from_extension(path: &Path) -> Option<ArchiveFormat> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();

    // Check compound extensions first (e.g. .tar.gz, .tar.xz).
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some(ArchiveFormat::TarGz);
    }
    if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        return Some(ArchiveFormat::TarXz);
    }
    if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        return Some(ArchiveFormat::TarZst);
    }

    // Check single extensions.
    for (ext, format) in EXTENSION_MAP {
        if lower.ends_with(ext) {
            return Some(*format);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Number of bytes needed for magic-byte detection of all supported formats.
pub const MAGIC_DETECT_SIZE: usize = 8;

/// Read up to `MAGIC_DETECT_SIZE` bytes from a reader for detection.
///
/// **Note:** this function **consumes** the bytes from the reader; it
/// does NOT peek or un-read them.  If the data needs to be re-read after
/// detection, the caller is responsible for buffering appropriately.
/// If the reader returns fewer than `size` bytes, the buffer is returned
/// as-is.
pub fn read_magic_bytes<R: std::io::Read>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; MAGIC_DETECT_SIZE];
    let n = reader.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Magic-number detection
    // ---------------------------------------------------------------

    #[test]
    fn detect_zip() {
        assert_eq!(detect_format(b"PK\x03\x04..."), Some(ArchiveFormat::Zip));
    }

    #[test]
    fn detect_empty_zip() {
        assert_eq!(detect_format(b"PK\x05\x06..."), Some(ArchiveFormat::Zip));
    }

    #[test]
    fn detect_gzip() {
        assert_eq!(
            detect_format(&[0x1F, 0x8B, 0x08, 0x00]),
            Some(ArchiveFormat::Gzip)
        );
    }

    #[test]
    fn detect_zstd() {
        assert_eq!(
            detect_format(&[0x28, 0xB5, 0x2F, 0xFD]),
            Some(ArchiveFormat::Zstd)
        );
    }

    #[test]
    fn detect_xz() {
        assert_eq!(
            detect_format(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]),
            Some(ArchiveFormat::Xz)
        );
    }

    #[test]
    fn detect_unknown_magic() {
        assert_eq!(detect_format(b"\x00\x01\x02\x03"), None);
    }

    #[test]
    fn detect_empty_data() {
        assert_eq!(detect_format(b""), None);
    }

    #[test]
    fn detect_short_data() {
        assert_eq!(detect_format(b"P"), None);
    }

    // ---------------------------------------------------------------
    // Extension detection
    // ---------------------------------------------------------------

    #[test]
    fn ext_zip() {
        assert_eq!(
            detect_from_extension(Path::new("archive.zip")),
            Some(ArchiveFormat::Zip)
        );
    }

    #[test]
    fn ext_tar() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar")),
            Some(ArchiveFormat::Tar)
        );
    }

    #[test]
    fn ext_gz() {
        assert_eq!(
            detect_from_extension(Path::new("file.gz")),
            Some(ArchiveFormat::Gzip)
        );
    }

    #[test]
    fn ext_gzip() {
        assert_eq!(
            detect_from_extension(Path::new("archive.gzip")),
            Some(ArchiveFormat::Gzip)
        );
    }

    #[test]
    fn ext_tar_gz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn ext_tgz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tgz")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn ext_xz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.xz")),
            Some(ArchiveFormat::Xz)
        );
    }

    #[test]
    fn ext_zst() {
        assert_eq!(
            detect_from_extension(Path::new("archive.zst")),
            Some(ArchiveFormat::Zstd)
        );
    }

    #[test]
    fn ext_zstd() {
        assert_eq!(
            detect_from_extension(Path::new("archive.zstd")),
            Some(ArchiveFormat::Zstd)
        );
    }

    #[test]
    fn ext_tar_xz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
    }

    #[test]
    fn ext_txz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.txz")),
            Some(ArchiveFormat::TarXz)
        );
    }

    #[test]
    fn ext_tar_zst() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.zst")),
            Some(ArchiveFormat::TarZst)
        );
    }

    #[test]
    fn ext_tzst() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tzst")),
            Some(ArchiveFormat::TarZst)
        );
    }

    #[test]
    fn ext_lzma() {
        assert_eq!(
            detect_from_extension(Path::new("archive.lzma")),
            Some(ArchiveFormat::Lzma)
        );
    }

    #[test]
    fn ext_unknown() {
        assert_eq!(detect_from_extension(Path::new("readme.md")), None);
    }

    #[test]
    fn ext_no_extension() {
        assert_eq!(detect_from_extension(Path::new("Makefile")), None);
    }

    #[test]
    fn ext_case_insensitive_zip() {
        assert_eq!(
            detect_from_extension(Path::new("Archive.ZIP")),
            Some(ArchiveFormat::Zip)
        );
    }

    #[test]
    fn ext_case_insensitive_targz() {
        assert_eq!(
            detect_from_extension(Path::new("ARCHIVE.TAR.GZ")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn ext_tar_gz_mixed_case_path() {
        assert_eq!(
            detect_from_extension(Path::new("archive.Tar.Gz")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn ext_dotfile() {
        assert_eq!(
            detect_from_extension(Path::new(".hidden.gz")),
            Some(ArchiveFormat::Gzip)
        );
    }

    // ---------------------------------------------------------------
    // ArchiveFormat Display
    // ---------------------------------------------------------------

    #[test]
    fn display_zip() {
        assert_eq!(ArchiveFormat::Zip.to_string(), "zip");
    }

    #[test]
    fn display_targz() {
        assert_eq!(ArchiveFormat::TarGz.to_string(), "tar.gz");
    }

    #[test]
    fn display_unknown() {
        assert_eq!(ArchiveFormat::Unknown.to_string(), "unknown");
    }

    // ---------------------------------------------------------------
    // RAR detection
    // ---------------------------------------------------------------

    #[test]
    fn detect_rar_magic() {
        assert_eq!(detect_format(b"Rar!\x1A\x07..."), Some(ArchiveFormat::Rar));
    }

    #[test]
    fn detect_rar_extension() {
        assert_eq!(
            detect_from_extension(Path::new("archive.rar")),
            Some(ArchiveFormat::Rar)
        );
    }

    #[test]
    fn display_rar() {
        assert_eq!(ArchiveFormat::Rar.to_string(), "rar");
    }

    #[test]
    fn display_lzma() {
        assert_eq!(ArchiveFormat::Lzma.to_string(), "lzma");
    }

    #[test]
    fn display_tarxz() {
        assert_eq!(ArchiveFormat::TarXz.to_string(), "tar.xz");
    }
    // ---------------------------------------------------------------
    // read_magic_bytes
    // ---------------------------------------------------------------

    #[test]
    fn read_magic_from_slice() {
        let data = b"PK\x03\x04";
        let mut cursor = std::io::Cursor::new(data);
        let magic = read_magic_bytes(&mut cursor).unwrap();
        assert_eq!(magic, b"PK\x03\x04");
    }

    #[test]
    fn read_magic_short_input() {
        let data = b"PK";
        let mut cursor = std::io::Cursor::new(data);
        let magic = read_magic_bytes(&mut cursor).unwrap();
        assert_eq!(magic, b"PK");
    }
}
