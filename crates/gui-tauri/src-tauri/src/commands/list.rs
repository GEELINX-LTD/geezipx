//! `list_archive` command — list entries inside an archive.
//!
//! Also provides shared helpers (`detect_archive_format`, `open_reader`) used
//! by other command modules (e.g. `test`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::task::spawn_blocking;

use geezipx_core::archive::asar::AsarReader;
use geezipx_core::archive::deb::DebReader;
use geezipx_core::archive::lzh::LzhReader;
#[cfg(feature = "rar")]
use geezipx_core::archive::rar::RarReader;
use geezipx_core::archive::seven_zip::SevenZipReader;
use geezipx_core::archive::tar::TarReader;
use geezipx_core::archive::tarbr::TarBrReader;
use geezipx_core::archive::tarbz2::TarBz2Reader;
use geezipx_core::archive::targz::TarGzReader;
use geezipx_core::archive::tarlz4::TarLz4Reader;
use geezipx_core::archive::tarxz::TarXzReader;
use geezipx_core::archive::tarzst::TarZstReader;
use geezipx_core::archive::zip::ZipReader;
use geezipx_core::archive::ArchiveReader;
use geezipx_core::detect::{self, ArchiveFormat};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Serializable entry information returned to the frontend.
#[derive(Debug, Serialize)]
pub struct EntryInfo {
    /// Relative path inside the archive.
    pub path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Compressed size in bytes (0 if unknown).
    pub compressed_size: u64,
    /// CRC-32 checksum, if the format provides it.
    pub crc32: Option<u32>,
    /// Last modification time as Unix timestamp.
    pub modified: Option<u64>,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

// ---------------------------------------------------------------------------
// Shared helpers (pub(crate))
// ---------------------------------------------------------------------------

/// Detect archive format by combining magic-byte and extension heuristics.
///
/// - Gzip magic + `.tar.gz`/`.tgz` extension → `TarGz`
/// - Bzip2 magic + `.tar.bz2`/`.tbz`/`.tbz2` extension → `TarBz2`
/// - LZ4 magic + `.tar.lz4` extension → `TarLz4`
/// - Zstd magic + `.tar.zst`/`.tzst` extension → `TarZst`
/// - XZ magic + `.tar.xz`/`.txz` extension → `TarXz`
/// - Pure magic match → format from magic
/// - Fallback → extension-based detection (`.asar` / `.deb` / `.lzh` / `.lha` are extension-only)
pub(crate) fn detect_archive_format(path: &Path) -> Result<ArchiveFormat, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
    let magic = detect::read_magic_bytes(&mut file)
        .map_err(|e| format!("Cannot read magic bytes from '{}': {}", path.display(), e))?;
    drop(file);

    match detect::detect_format(&magic) {
        Some(ArchiveFormat::Gzip) => {
            // Gzip magic but the file might be .tar.gz — check extension.
            match detect::detect_from_extension(path) {
                Some(ArchiveFormat::TarGz) => Ok(ArchiveFormat::TarGz),
                _ => Ok(ArchiveFormat::Gzip),
            }
        }
        Some(ArchiveFormat::Bzip2) => match detect::detect_from_extension(path) {
            Some(ArchiveFormat::TarBz2) => Ok(ArchiveFormat::TarBz2),
            _ => Ok(ArchiveFormat::Bzip2),
        },
        Some(ArchiveFormat::Lz4) => match detect::detect_from_extension(path) {
            Some(ArchiveFormat::TarLz4) => Ok(ArchiveFormat::TarLz4),
            _ => Ok(ArchiveFormat::Lz4),
        },
        Some(ArchiveFormat::Zstd) => match detect::detect_from_extension(path) {
            Some(ArchiveFormat::TarZst) => Ok(ArchiveFormat::TarZst),
            _ => Ok(ArchiveFormat::Zstd),
        },
        Some(ArchiveFormat::Xz) => match detect::detect_from_extension(path) {
            Some(ArchiveFormat::TarXz) => Ok(ArchiveFormat::TarXz),
            _ => Ok(ArchiveFormat::Xz),
        },
        Some(fmt) => Ok(fmt),
        None => {
            // No magic matched; fall back to extension.
            detect::detect_from_extension(path)
                .ok_or_else(|| format!("Unable to detect archive format for '{}'", path.display()))
        }
    }
}

/// Open an archive reader for the given path and format.
///
/// Returns an error for single-stream compression formats (gzip, bzip2, brotli, lz4, zstd, xz, lzma)
/// that do not support listing entries.
pub(crate) fn open_reader(
    path: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<Box<dyn ArchiveReader>, String> {
    // Password validation: only ZIP, 7z, and RAR support encryption.
    if password.is_some()
        && format != ArchiveFormat::Zip
        && format != ArchiveFormat::SevenZip
        && format != ArchiveFormat::Rar
    {
        return Err(format!(
            "Password is only supported for ZIP, 7z, and RAR formats; '{}' does not support encryption",
            format
        ));
    }

    // Single-stream formats cannot be read as archives.
    match format {
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma => {
            return Err(format!(
                "'{}' is a single-stream compression format; it does not contain a directory listing",
                format
            ));
        }
        _ => {}
    }

    match format {
        ArchiveFormat::Zip => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            let mut reader =
                Box::new(ZipReader::new(file).map_err(|e| format!("Cannot open ZIP: {}", e))?);
            if let Some(pwd) = password {
                reader.set_password(pwd);
            }
            Ok(reader)
        }
        ArchiveFormat::SevenZip => {
            let mut reader = Box::new(SevenZipReader::new(path));
            if let Some(pwd) = password {
                reader.set_password(pwd);
            }
            Ok(reader)
        }
        ArchiveFormat::Asar => Ok(Box::new(AsarReader::new(path))),
        ArchiveFormat::Deb => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(DebReader::new(file)))
        }
        ArchiveFormat::Lzh => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(LzhReader::new(file)))
        }
        ArchiveFormat::Tar => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarReader::new(file)))
        }
        ArchiveFormat::TarGz => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarGzReader::new(file)))
        }
        ArchiveFormat::TarBz2 => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarBz2Reader::new(file)))
        }
        ArchiveFormat::TarBr => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarBrReader::new(file)))
        }
        ArchiveFormat::TarLz4 => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarLz4Reader::new(file)))
        }
        ArchiveFormat::TarZst => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarZstReader::new(file)))
        }
        ArchiveFormat::TarXz => {
            let file = fs::File::open(path)
                .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
            Ok(Box::new(TarXzReader::new(file)))
        }
        #[cfg(feature = "rar")]
        ArchiveFormat::Rar => {
            let mut reader = Box::new(RarReader::new(path));
            if let Some(pwd) = password {
                // set_password returns GeeZipResult — discard on purpose
                // (password support is best-effort for RAR).
                let _ = reader.set_password(pwd);
            }
            Ok(reader)
        }
        _ => Err(format!("Unsupported format for reading: {format}")),
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// List entries inside an archive.
///
/// For single-stream formats (gzip, bzip2, brotli, lz4, zstd, xz, lzma) this returns an error
/// because they do not contain a directory structure — use `test_archive` instead.
#[tauri::command]
pub async fn list_archive(
    archive_path: String,
    password: Option<String>,
) -> Result<Vec<EntryInfo>, String> {
    let path_buf = PathBuf::from(&archive_path);
    let pwd = password;

    spawn_blocking(move || {
        let format = detect_archive_format(&path_buf)?;
        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;

        let entries = reader
            .entries()
            .map_err(|e| format!("Failed to read entries: {}", e))?;

        let info: Vec<EntryInfo> = entries
            .iter()
            .map(|entry| EntryInfo {
                path: entry.path.clone(),
                size: entry.size,
                compressed_size: entry.compressed_size,
                crc32: entry.crc32,
                modified: entry.modified,
                is_dir: entry.is_dir,
            })
            .collect();

        Ok(info)
    })
    .await
    .map_err(|e| format!("Internal error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn build_test_lzh() -> Vec<u8> {
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
        archive.push((22 + name.len()) as u8);
        archive.push(checksum);
        archive.extend_from_slice(&header);
        archive.extend_from_slice(data);
        archive.push(0);
        archive
    }

    fn unique_test_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "geezipx-gui-list-test-{prefix}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_archive_format_lzh_extension() {
        let temp = unique_test_dir("detect-lzh");
        let archive = temp.join("archive.lzh");
        std::fs::write(&archive, build_test_lzh()).unwrap();

        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::Lzh);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn open_reader_lzh_lists_entries() {
        let temp = unique_test_dir("open-lzh");
        let archive = temp.join("archive.lha");
        std::fs::write(&archive, build_test_lzh()).unwrap();

        let mut reader = open_reader(&archive, ArchiveFormat::Lzh, None).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");

        let _ = std::fs::remove_dir_all(temp);
    }
}
