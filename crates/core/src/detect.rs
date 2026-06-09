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
    /// Bzip2 compressed stream (`BZh`).
    Bzip2,
    /// Brotli-compressed stream (extension-based — `.br`; no stable magic header).
    Brotli,
    /// LZ4 frame (`04 22 4D 18`).
    Lz4,
    /// Tar archive compressed with gzip (extension-based — `.tar.gz`, `.tgz`).
    TarGz,
    /// Tar archive compressed with bzip2 (extension-based — `.tar.bz2`, `.tbz`, `.tbz2`).
    TarBz2,
    /// Tar archive compressed with Brotli (extension-based — `.tar.br`).
    TarBr,
    /// Tar archive compressed with LZ4 (extension-based — `.tar.lz4`).
    TarLz4,
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
    /// Electron ASAR archive (extension-based — `.asar`; no stable magic header).
    Asar,
    /// Debian package archive (extension-based — `.deb`; deliberately no `ar` magic sniff).
    Deb,
    /// LZH/LHA archive (extension-based — `.lzh`, `.lha`; deliberately no magic sniff).
    Lzh,
    /// Unknown or unrecognised format.
    Unknown,
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveFormat::Zip => write!(f, "zip"),
            ArchiveFormat::Tar => write!(f, "tar"),
            ArchiveFormat::Gzip => write!(f, "gzip"),
            ArchiveFormat::Bzip2 => write!(f, "bzip2"),
            ArchiveFormat::Brotli => write!(f, "brotli"),
            ArchiveFormat::Lz4 => write!(f, "lz4"),
            ArchiveFormat::TarGz => write!(f, "tar.gz"),
            ArchiveFormat::TarBz2 => write!(f, "tar.bz2"),
            ArchiveFormat::TarBr => write!(f, "tar.br"),
            ArchiveFormat::TarLz4 => write!(f, "tar.lz4"),
            ArchiveFormat::Xz => write!(f, "xz"),
            ArchiveFormat::Zstd => write!(f, "zstd"),
            ArchiveFormat::TarZst => write!(f, "tar.zst"),
            ArchiveFormat::TarXz => write!(f, "tar.xz"),
            ArchiveFormat::Lzma => write!(f, "lzma"),
            ArchiveFormat::SevenZip => write!(f, "7z"),
            ArchiveFormat::Rar => write!(f, "rar"),
            ArchiveFormat::Asar => write!(f, "asar"),
            ArchiveFormat::Deb => write!(f, "deb"),
            ArchiveFormat::Lzh => write!(f, "lzh"),
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
/// Bzip2 magic: `BZh`.
const MAGIC_BZIP2: &[u8] = b"BZh";
/// LZ4 frame magic: `04 22 4D 18`.
const MAGIC_LZ4_FRAME: &[u8] = &[0x04, 0x22, 0x4D, 0x18];
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
    (".jar", ArchiveFormat::Zip),
    (".war", ArchiveFormat::Zip),
    (".apk", ArchiveFormat::Zip),
    (".ipa", ArchiveFormat::Zip),
    (".xpi", ArchiveFormat::Zip),
    (".tar", ArchiveFormat::Tar),
    (".gz", ArchiveFormat::Gzip),
    (".bz2", ArchiveFormat::Bzip2),
    (".br", ArchiveFormat::Brotli),
    (".lz4", ArchiveFormat::Lz4),
    // Note: .tgz/.tbz/.tbz2 are handled by detect_from_extension's compound checks.
    (".xz", ArchiveFormat::Xz),
    (".zst", ArchiveFormat::Zstd),
    (".zstd", ArchiveFormat::Zstd),
    (".lzma", ArchiveFormat::Lzma),
    (".gzip", ArchiveFormat::Gzip),
    (".7z", ArchiveFormat::SevenZip),
    (".rar", ArchiveFormat::Rar),
    (".asar", ArchiveFormat::Asar),
    (".deb", ArchiveFormat::Deb),
    (".lzh", ArchiveFormat::Lzh),
    (".lha", ArchiveFormat::Lzh),
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
/// **Note**: For tar-wrapped compressed formats such as `.tar.gz`/`.tgz`,
/// `.tar.bz2`/`.tbz`/`.tbz2`, and `.tar.lz4`, this function returns the
/// outer stream format (`Gzip` / `Bzip2` / `Lz4`) because the compression
/// magic header does not reveal whether the inner stream is a tar archive.
/// Brotli streams have no stable magic header here, so `.br` / `.tar.br`
/// rely on extension-based detection only. DEB deliberately does not sniff the
/// outer `ar` magic because many non-DEB archives share it; `.deb` relies on
/// explicit format / extension. ASAR similarly has no reliable fixed magic
/// header and is handled only by explicit format / extension.
pub fn detect_format(data: &[u8]) -> Option<ArchiveFormat> {
    if data.starts_with(MAGIC_ZIP) || data.starts_with(MAGIC_ZIP_EMPTY) {
        return Some(ArchiveFormat::Zip);
    }
    if data.starts_with(MAGIC_GZIP) {
        return Some(ArchiveFormat::Gzip);
    }
    if data.starts_with(MAGIC_BZIP2) {
        return Some(ArchiveFormat::Bzip2);
    }
    if data.starts_with(MAGIC_LZ4_FRAME) {
        return Some(ArchiveFormat::Lz4);
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
    if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz") || lower.ends_with(".tbz2") {
        return Some(ArchiveFormat::TarBz2);
    }
    if lower.ends_with(".tar.br") {
        return Some(ArchiveFormat::TarBr);
    }
    if lower.ends_with(".tar.lz4") {
        return Some(ArchiveFormat::TarLz4);
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
    fn detect_bzip2() {
        assert_eq!(detect_format(b"BZh91AY"), Some(ArchiveFormat::Bzip2));
    }

    #[test]
    fn detect_lz4_magic() {
        assert_eq!(
            detect_format(&[0x04, 0x22, 0x4D, 0x18, 0x64, 0x40, 0xA7, 0x0D]),
            Some(ArchiveFormat::Lz4)
        );
    }

    #[test]
    fn detect_brotli_is_extension_only() {
        let mut compressed = Vec::new();
        let params = brotli::enc::backward_references::BrotliEncoderParams::default();
        brotli::BrotliCompress(
            &mut std::io::Cursor::new(b"hello brotli payload"),
            &mut compressed,
            &params,
        )
        .expect("brotli compression should succeed");

        assert_eq!(detect_format(&compressed), None);
        assert_eq!(
            detect_from_extension(Path::new("archive.br")),
            Some(ArchiveFormat::Brotli)
        );
    }

    #[test]
    fn detect_asar_is_extension_only() {
        let raw = br#"{"files":{"hello.txt":{"size":5,"offset":"0"}}}hello"#;
        assert_eq!(detect_format(raw), None);
        assert_eq!(
            detect_from_extension(Path::new("archive.asar")),
            Some(ArchiveFormat::Asar)
        );
    }

    #[test]
    fn detect_deb_is_extension_only() {
        let raw = b"!<arch>\n";
        assert_eq!(detect_format(raw), None);
        assert_eq!(
            detect_from_extension(Path::new("package.deb")),
            Some(ArchiveFormat::Deb)
        );
    }

    #[test]
    fn detect_lzh_is_extension_only() {
        let raw = b"-lh0-lzh";
        assert_eq!(detect_format(raw), None);
        assert_eq!(
            detect_from_extension(Path::new("archive.lzh")),
            Some(ArchiveFormat::Lzh)
        );
        assert_eq!(
            detect_from_extension(Path::new("archive.lha")),
            Some(ArchiveFormat::Lzh)
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
    fn ext_zip_aliases() {
        for path in [
            "plugin.jar",
            "webapp.war",
            "mobile.apk",
            "bundle.ipa",
            "addon.xpi",
        ] {
            assert_eq!(
                detect_from_extension(Path::new(path)),
                Some(ArchiveFormat::Zip),
                "{path}"
            );
        }
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
    fn ext_bz2() {
        assert_eq!(
            detect_from_extension(Path::new("archive.bz2")),
            Some(ArchiveFormat::Bzip2)
        );
    }

    #[test]
    fn ext_tar_bz2() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.bz2")),
            Some(ArchiveFormat::TarBz2)
        );
    }

    #[test]
    fn ext_tbz() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tbz")),
            Some(ArchiveFormat::TarBz2)
        );
    }

    #[test]
    fn ext_tbz2() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tbz2")),
            Some(ArchiveFormat::TarBz2)
        );
    }

    #[test]
    fn ext_tar_br_takes_priority_over_br() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.br")),
            Some(ArchiveFormat::TarBr)
        );
    }

    #[test]
    fn ext_lz4() {
        assert_eq!(
            detect_from_extension(Path::new("archive.lz4")),
            Some(ArchiveFormat::Lz4)
        );
    }

    #[test]
    fn ext_tar_lz4_takes_priority_over_lz4() {
        assert_eq!(
            detect_from_extension(Path::new("archive.tar.lz4")),
            Some(ArchiveFormat::TarLz4)
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
    fn ext_asar() {
        assert_eq!(
            detect_from_extension(Path::new("archive.asar")),
            Some(ArchiveFormat::Asar)
        );
    }

    #[test]
    fn ext_deb() {
        assert_eq!(
            detect_from_extension(Path::new("package.deb")),
            Some(ArchiveFormat::Deb)
        );
    }

    #[test]
    fn ext_lzh() {
        assert_eq!(
            detect_from_extension(Path::new("archive.lzh")),
            Some(ArchiveFormat::Lzh)
        );
    }

    #[test]
    fn ext_lha() {
        assert_eq!(
            detect_from_extension(Path::new("archive.lha")),
            Some(ArchiveFormat::Lzh)
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
    fn display_bzip2() {
        assert_eq!(ArchiveFormat::Bzip2.to_string(), "bzip2");
    }

    #[test]
    fn display_tarbz2() {
        assert_eq!(ArchiveFormat::TarBz2.to_string(), "tar.bz2");
    }

    #[test]
    fn display_targz() {
        assert_eq!(ArchiveFormat::TarGz.to_string(), "tar.gz");
    }

    #[test]
    fn display_brotli() {
        assert_eq!(ArchiveFormat::Brotli.to_string(), "brotli");
    }

    #[test]
    fn display_lz4() {
        assert_eq!(ArchiveFormat::Lz4.to_string(), "lz4");
    }

    #[test]
    fn display_tarbr() {
        assert_eq!(ArchiveFormat::TarBr.to_string(), "tar.br");
    }

    #[test]
    fn display_tarlz4() {
        assert_eq!(ArchiveFormat::TarLz4.to_string(), "tar.lz4");
    }

    #[test]
    fn display_asar() {
        assert_eq!(ArchiveFormat::Asar.to_string(), "asar");
    }

    #[test]
    fn display_deb() {
        assert_eq!(ArchiveFormat::Deb.to_string(), "deb");
    }

    #[test]
    fn display_lzh() {
        assert_eq!(ArchiveFormat::Lzh.to_string(), "lzh");
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
