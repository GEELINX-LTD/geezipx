//! ALZ archive reader (`.alz`).
//!
//! GeeZipX exposes ALZ as a read-only archive format backed by the
//! [`unalz_rs`] crate.  The reader supports listing and extraction.
//!
//! # Design notes
//!
//! - **Read-only** — ALZ creation is out of scope.
//! - **Path-based** — The reader stores the archive path and re-opens the
//!   file on each operation because [`unalz_rs::archive::AlzArchive`]
//!   consumes and owns its file handle.
//! - **No stable magic** — Detection relies on the `.alz` extension.

use std::fmt;
use std::io::{Read, SeekFrom, Write};
use std::path::PathBuf;

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only ALZ archive reader.
pub struct AlzReader {
    path: PathBuf,
    format: ArchiveFormat,
}

impl fmt::Debug for AlzReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AlzReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl AlzReader {
    /// Create a new ALZ reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Alz,
        }
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        use unalz_rs::archive::AlzArchive;

        let archive = AlzArchive::open(&self.path.to_string_lossy()).map_err(|e| {
            GeeZipError::format(
                format!("invalid ALZ archive '{}': {e}", self.path.display()),
                ArchiveFormat::Alz,
            )
        })?;

        let entries: Vec<_> = archive
            .entries
            .iter()
            .map(|file_entry| {
                let is_dir = file_entry.is_directory();
                let original_name = file_entry.file_name.replace('\\', "/");
                let path = if is_dir {
                    original_name.trim_end_matches(&['/', '\\']).to_owned()
                } else {
                    original_name
                };
                Entry {
                    path,
                    size: file_entry.uncompressed_size,
                    compressed_size: file_entry.compressed_size,
                    crc32: Some(file_entry.file_crc),
                    modified: Some(crate::archive::datetime_to_timestamp(
                        ((file_entry.file_time_date >> 16) & 0xFFFF) as u64,
                        ((file_entry.file_time_date >> 8) & 0xFF) as u64,
                        (file_entry.file_time_date & 0xFF) as u64,
                        ((file_entry.file_time_date >> 11) & 0x1F) as u64,
                        ((file_entry.file_time_date >> 5) & 0x3F) as u64,
                        ((file_entry.file_time_date & 0x1F) * 2) as u64,
                    )),
                    is_dir,
                }
            })
            .collect();

        Ok(entries)
    }
}

impl ArchiveReader for AlzReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.collect_entries()
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }

        use std::io::{Read, Seek};
        use unalz_rs::archive::{AlzArchive, CompressionMethod};

        let path_str = self.path.to_string_lossy();
        let mut archive = AlzArchive::open(&path_str).map_err(|e| {
            GeeZipError::format(
                format!("invalid ALZ archive '{}': {e}", self.path.display()),
                ArchiveFormat::Alz,
            )
        })?;

        // Find the matching entry.
        let alz_entry = archive
            .entries
            .iter()
            .find(|e| {
                let name = e.file_name.replace('\\', "/");
                let is_dir = e.is_directory();
                let path = if is_dir {
                    name.trim_end_matches(&['/', '\\']).to_owned()
                } else {
                    name
                };
                path == entry.path
            })
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;

        if alz_entry.is_directory() {
            return Ok(0);
        }

        if alz_entry.is_encrypted() {
            return Err(GeeZipError::Crypto {
                message: "ALZ entry is encrypted; password-protected ALZ not yet supported".into(),
            });
        }

        archive
            .reader
            .seek(SeekFrom::Start(alz_entry.data_pos))
            .map_err(|e| {
                GeeZipError::io(
                    e,
                    format!("seeking in ALZ archive '{}'", self.path.display()),
                )
            })?;

        let mut limited = (&mut archive.reader).take(alz_entry.compressed_size);

        match alz_entry.compression_method {
            CompressionMethod::Store => {
                let mut buf = [0u8; 32768];
                let mut remaining = alz_entry.compressed_size;
                while remaining > 0 {
                    let to_read = (remaining as usize).min(buf.len());
                    limited.read_exact(&mut buf[..to_read]).map_err(|e| {
                        GeeZipError::io(e, format!("reading ALZ entry '{}'", entry.path))
                    })?;
                    writer.write_all(&buf[..to_read]).map_err(|e| {
                        GeeZipError::io(e, format!("writing ALZ entry '{}'", entry.path))
                    })?;
                    remaining -= to_read as u64;
                }
                Ok(alz_entry.uncompressed_size)
            }
            CompressionMethod::Deflate => {
                let mut out_buf = Vec::new();
                unalz_rs::decompress::deflate::extract_deflate(
                    &mut limited,
                    &mut out_buf,
                    alz_entry.compressed_size,
                    None,
                )
                .map_err(|e| {
                    GeeZipError::format(
                        format!(
                            "deflate decompress error for ALZ entry '{}': {e}",
                            entry.path
                        ),
                        ArchiveFormat::Alz,
                    )
                })?;
                let len = out_buf.len() as u64;
                writer.write_all(&out_buf).map_err(|e| {
                    GeeZipError::io(e, format!("writing ALZ entry '{}'", entry.path))
                })?;
                Ok(len)
            }
            CompressionMethod::Bzip2 => {
                let mut out_buf = Vec::new();
                unalz_rs::decompress::bzip2::extract_bzip2(
                    &mut limited,
                    &mut out_buf,
                    alz_entry.compressed_size,
                    None,
                )
                .map_err(|e| {
                    GeeZipError::format(
                        format!("bzip2 decompress error for ALZ entry '{}': {e}", entry.path),
                        ArchiveFormat::Alz,
                    )
                })?;
                let len = out_buf.len() as u64;
                writer.write_all(&out_buf).map_err(|e| {
                    GeeZipError::io(e, format!("writing ALZ entry '{}'", entry.path))
                })?;
                Ok(len)
            }
            CompressionMethod::Unknown(n) => Err(GeeZipError::format(
                format!(
                    "unsupported compression method {n} for ALZ entry '{}'",
                    entry.path
                ),
                ArchiveFormat::Alz,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alz_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("broken.alz");
        std::fs::write(&archive, b"MALFORMED_ALZ_FILE").unwrap();
        let mut reader = AlzReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Alz,
                ..
            }
        ));
        assert!(err.to_string().contains("invalid ALZ archive"));
    }

    #[test]
    fn alz_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("trait.alz");
        std::fs::write(&archive, b"FAKE").unwrap();
        let reader: Box<dyn ArchiveReader> = Box::new(AlzReader::new(&archive));
        assert_eq!(reader.format(), ArchiveFormat::Alz);
    }

    #[test]
    fn alz_missing_entry_is_handled() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("empty.alz");
        std::fs::write(&archive, b"FAKE").unwrap();
        let mut reader = AlzReader::new(&archive);
        let mut sink = Vec::new();

        let result = reader.extract(
            &Entry {
                path: "missing.txt".into(),
                size: 0,
                compressed_size: 0,
                crc32: None,
                modified: None,
                is_dir: false,
            },
            &mut sink,
        );
        assert!(result.is_err());
    }
}
