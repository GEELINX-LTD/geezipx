//! LZH/LHA (`.lzh`, `.lha`) archive reader.
//!
//! GeeZipX exposes LZH as a read-only archive format backed by the `delharc`
//! decoder. The current MVP supports listing, extraction, and integrity
//! verification for the compression methods handled by `delharc`'s default
//! feature set (notably `-lh0-`, `-lh1-`, and `-lh4-` through `-lh7-`).
//!
//! Path handling notes:
//! - `delharc` already normalises path separators to `/` and strips `.` / `..`
//!   path components while percent-encoding non-ASCII/control bytes.
//! - GeeZipX still relies on the shared `extract_all` Zip-Slip guards for the
//!   final destination safety checks, including Windows drive-prefixed paths.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};

use delharc::stub_io::Read as DelharcRead;
use delharc::{LhaDecodeError, LhaDecodeReader, LhaError};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only LZH/LHA archive reader.
pub struct LzhReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for LzhReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LzhReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> LzhReader<R> {
    /// Create an LZH reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        Self {
            inner: reader,
            format: ArchiveFormat::Lzh,
        }
    }
}

impl LzhReader<std::io::Cursor<Vec<u8>>> {
    /// Create an LZH reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        Self::new(std::io::Cursor::new(buf))
    }
}

fn lzh_modified_timestamp(header: &delharc::header::LhaHeader) -> Option<u64> {
    header
        .parse_last_modified()
        .to_utc()
        .and_then(|dt| u64::try_from(dt.timestamp()).ok())
}

fn lzh_entry_path(header: &delharc::header::LhaHeader) -> GeeZipResult<String> {
    let path = header.parse_pathname_to_str().replace('\\', "/");
    if path.is_empty() {
        return Err(GeeZipError::format(
            "LZH entry is missing a pathname",
            ArchiveFormat::Lzh,
        ));
    }
    Ok(path)
}

fn compression_label(header: &delharc::header::LhaHeader) -> String {
    match header.compression_method() {
        Ok(method) => method.to_string(),
        Err(_) => String::from_utf8_lossy(&header.compression).into_owned(),
    }
}

fn unsupported_method_error(header: &delharc::header::LhaHeader, path: &str) -> GeeZipError {
    GeeZipError::format(
        format!(
            "unsupported LZH compression method '{}' for entry '{}'",
            compression_label(header),
            path
        ),
        ArchiveFormat::Lzh,
    )
}

fn convert_lha_error(err: LhaError<std::io::Error>, context: impl Into<String>) -> GeeZipError {
    match err {
        LhaError::Io(io_err) => match io_err.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                GeeZipError::format(format!("invalid LZH archive: {io_err}"), ArchiveFormat::Lzh)
            }
            _ => GeeZipError::io(io_err, context),
        },
        LhaError::HeaderParse(message) => {
            GeeZipError::format(format!("invalid LZH archive: {message}"), ArchiveFormat::Lzh)
        }
        LhaError::Decompress(message) => GeeZipError::format(
            format!("failed to decompress LZH entry: {message}"),
            ArchiveFormat::Lzh,
        ),
        LhaError::Checksum(message) => GeeZipError::format(
            format!("LZH CRC-16 verification failed: {message}"),
            ArchiveFormat::Lzh,
        ),
    }
}

fn convert_lha_decode_error<R>(
    err: LhaDecodeError<R>,
    context: impl Into<String>,
) -> GeeZipError
where
    R: DelharcRead<Error = std::io::Error>,
{
    convert_lha_error(err.into(), context)
}

fn scan_lzh_entries<R: Read + Seek + Send>(inner: &mut R) -> GeeZipResult<Vec<Entry>> {
    inner.seek(SeekFrom::Start(0))?;
    let mut reader = LhaDecodeReader::new(&mut *inner)
        .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?;
    let mut entries = Vec::new();

    loop {
        let header = reader.header();
        let path = lzh_entry_path(header)?;
        entries.push(Entry {
            path,
            size: header.original_size,
            compressed_size: header.compressed_size,
            crc32: None,
            modified: lzh_modified_timestamp(header),
            is_dir: header.is_directory(),
        });

        if !reader
            .next_file()
            .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?
        {
            break;
        }
    }

    Ok(entries)
}

impl<R: Read + Seek + Send> ArchiveReader for LzhReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        scan_lzh_entries(&mut self.inner)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut reader = LhaDecodeReader::new(&mut self.inner)
            .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?;

        loop {
            let header = reader.header();
            let path = lzh_entry_path(header)?;
            if path == entry.path {
                if header.is_directory() {
                    return Ok(0);
                }
                if !reader.is_decoder_supported() {
                    return Err(unsupported_method_error(header, &path));
                }

                let bytes = std::io::copy(&mut reader, writer).map_err(|err| {
                    GeeZipError::io(err, format!("extracting '{}' from LZH archive", entry.path))
                })?;
                reader.crc_check().map_err(|err| {
                    convert_lha_error(
                        err,
                        format!("verifying CRC-16 for '{}' in LZH archive", entry.path),
                    )
                })?;
                return Ok(bytes);
            }

            if !reader
                .next_file()
                .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?
            {
                break;
            }
        }

        Err(GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use delharc::crc::Crc16;

    struct FixtureEntry<'a> {
        path: &'a str,
        data: &'a [u8],
        method: [u8; 5],
    }

    impl<'a> FixtureEntry<'a> {
        fn file(path: &'a str, data: &'a [u8]) -> Self {
            Self {
                path,
                data,
                method: *b"-lh0-",
            }
        }

        fn directory(path: &'a str) -> Self {
            Self {
                path,
                data: &[],
                method: *b"-lhd-",
            }
        }

        fn unsupported(path: &'a str, data: &'a [u8]) -> Self {
            Self {
                path,
                data,
                method: *b"-pm1-",
            }
        }
    }

    fn file_crc16(data: &[u8]) -> u16 {
        let mut crc = Crc16::default();
        crc.digest(data);
        crc.sum16()
    }

    fn append_lzh_entry(out: &mut Vec<u8>, entry: &FixtureEntry<'_>) {
        let name = entry.path.as_bytes();
        assert!(name.len() <= u8::MAX as usize, "LZH pathname too long");
        let compressed_size = if entry.method == *b"-lhd-" {
            0u32
        } else {
            entry.data.len() as u32
        };
        let original_size = if entry.method == *b"-lhd-" {
            0u32
        } else {
            entry.data.len() as u32
        };
        let file_crc = if entry.method == *b"-lhd-" {
            0u16
        } else {
            file_crc16(entry.data)
        };
        let header_len = 20usize + name.len();
        assert!(header_len <= u8::MAX as usize, "LZH header too large");

        let mut header = Vec::with_capacity(header_len);
        header.extend_from_slice(&entry.method);
        header.extend_from_slice(&compressed_size.to_le_bytes());
        header.extend_from_slice(&original_size.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.push(0x20);
        header.push(0);
        header.push(name.len() as u8);
        header.extend_from_slice(name);
        header.extend_from_slice(&file_crc.to_le_bytes());
        assert_eq!(header.len(), header_len);

        let checksum = header.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        out.push(header_len as u8);
        out.push(checksum);
        out.extend_from_slice(&header);
        if entry.method != *b"-lhd-" {
            out.extend_from_slice(entry.data);
        }
    }

    fn build_lzh(entries: &[FixtureEntry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in entries {
            append_lzh_entry(&mut out, entry);
        }
        out.push(0);
        out
    }

    #[test]
    fn lzh_entries_normalize_paths_and_extract_files() {
        let archive = build_lzh(&[
            FixtureEntry::directory("docs\\"),
            FixtureEntry::file("docs\\hello.txt", b"hello"),
            FixtureEntry::file("readme.txt", b"readme"),
        ]);
        let mut reader = LzhReader::from_buf(archive);
        let entries = reader.entries().expect("entries should load");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "docs");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].path, "docs/hello.txt");
        assert_eq!(entries[1].size, 5);
        assert!(!entries[1].is_dir);
        assert_eq!(entries[2].path, "readme.txt");

        let mut out = Vec::new();
        let bytes = reader
            .extract(&entries[1], &mut out)
            .expect("file extraction should succeed");
        assert_eq!(bytes, 5);
        assert_eq!(out, b"hello");
    }

    #[test]
    fn lzh_crc_mismatch_fails_cleanly() {
        let mut archive = build_lzh(&[FixtureEntry::file("hello.txt", b"hello")]);
        let payload_index = archive.len() - 1 - b"hello".len();
        archive[payload_index] ^= 0x01;

        let mut reader = LzhReader::from_buf(archive);
        let entry = reader.entries().unwrap().remove(0);
        let err = reader
            .extract(&entry, &mut std::io::sink())
            .expect_err("CRC mismatch should fail extraction");
        assert!(err.to_string().contains("CRC-16"), "err: {err}");
    }

    #[test]
    fn lzh_extract_all_rejects_windows_absolute_paths() {
        let archive = build_lzh(&[FixtureEntry::file("C:\\escape.txt", b"bad")]);
        let mut reader = LzhReader::from_buf(archive);
        let temp = tempfile::TempDir::new().unwrap();
        let report = reader
            .extract_all(temp.path(), true)
            .expect("extract_all should complete with per-file errors");

        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, "C:/escape.txt");
        assert!(matches!(report.errors[0].1, GeeZipError::PathTraversal { .. }));
        assert!(!temp.path().join("escape.txt").exists());
    }

    #[test]
    fn lzh_unsupported_decoder_is_reported_structurally() {
        let archive = build_lzh(&[FixtureEntry::unsupported("legacy.bin", b"payload")]);
        let mut reader = LzhReader::from_buf(archive);
        let entry = reader.entries().unwrap().remove(0);
        let err = reader
            .extract(&entry, &mut std::io::sink())
            .expect_err("unsupported decoder should fail extraction");
        assert!(err.to_string().contains("unsupported LZH compression method"));
        assert!(err.to_string().contains("-pm1-"));
    }

    #[test]
    fn lzh_trait_object_is_supported() {
        let archive = build_lzh(&[FixtureEntry::file("hello.txt", b"hello")]);
        let mut reader: Box<dyn ArchiveReader> = Box::new(LzhReader::from_buf(archive));
        let entries = reader.entries().expect("trait object should list entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }
}
