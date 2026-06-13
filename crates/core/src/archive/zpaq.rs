//! ZPAQ (`.zpaq`, `.zpq`) archive reader.
//!
//! GeeZipX exposes ZPAQ as a read-only archive format backed by the `zpaq_rs`
//! crate. The current MVP supports `list`, `decompress`, and `test`.
//!
//! Implementation note:
//! - `zpaq_rs` currently exposes whole-member byte extraction helpers for
//!   addressing a stored file by path. GeeZipX therefore reuses those helpers
//!   for `extract()`, which means a selected member may be buffered in memory
//!   before it is written to the caller's `Write` sink.
//! - Batch extraction still flows through GeeZipX's shared
//!   `ArchiveReader::extract_all` implementation so path traversal protection
//!   and no-clobber behavior remain centralized.

use std::fmt;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::{
    datetime_to_timestamp, is_entry_path_dangerous, ArchiveReader, ArchiveWriter, Entry,
};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const DATE_RANGE: std::ops::Range<usize> = 2..12;
const TIME_RANGE: std::ops::Range<usize> = 13..21;
const SIZE_RANGE: std::ops::Range<usize> = 22..34;
const PATH_START: usize = 41;

/// Read-only ZPAQ archive reader.
pub struct ZpaqReader {
    path: PathBuf,
    format: ArchiveFormat,
    entries: Option<Vec<Entry>>,
}

impl fmt::Debug for ZpaqReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZpaqReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ZpaqReader {
    /// Create a ZPAQ reader for the archive at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Zpaq,
            entries: None,
        }
    }

    fn archive_path_str(&self) -> GeeZipResult<&str> {
        self.path.to_str().ok_or_else(|| {
            GeeZipError::format("ZPAQ archive path is not valid UTF-8", ArchiveFormat::Zpaq)
        })
    }

    fn ensure_entries(&mut self) -> GeeZipResult<()> {
        if self.entries.is_none() {
            self.entries = Some(scan_zpaq_entries(&self.path)?);
        }
        Ok(())
    }

    fn find_entry(&mut self, path: &str) -> GeeZipResult<Entry> {
        self.ensure_entries()?;
        self.entries
            .as_ref()
            .and_then(|entries| entries.iter().find(|entry| entry.path == path))
            .cloned()
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: path.to_owned(),
            })
    }
}

fn convert_zpaq_archive_error(err: zpaq_rs::ZpaqError, context: impl Into<String>) -> GeeZipError {
    let context = context.into();
    let message = err.to_string();
    let message = if message.contains("password incorrect") {
        "invalid or unsupported ZPAQ archive".to_owned()
    } else {
        message
    };
    GeeZipError::format(format!("{context}: {message}"), ArchiveFormat::Zpaq)
}

fn convert_zpaq_entry_error(err: zpaq_rs::ZpaqError, entry_name: &str) -> GeeZipError {
    let message = err.to_string();
    if message.contains("file path not found in archive") {
        GeeZipError::EntryNotFound {
            name: entry_name.to_owned(),
        }
    } else {
        GeeZipError::format(
            format!("extracting ZPAQ entry '{entry_name}': {message}"),
            ArchiveFormat::Zpaq,
        )
    }
}

fn looks_like_listing_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| matches!(idx, 4 | 7) || byte.is_ascii_digit())
}

fn looks_like_listing_time(time: &str) -> bool {
    let bytes = time.as_bytes();
    bytes.len() == 8
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| matches!(idx, 2 | 5) || byte.is_ascii_digit())
}

fn parse_listing_timestamp(date: &str, time: &str) -> Option<u64> {
    if !looks_like_listing_date(date) || !looks_like_listing_time(time) {
        return None;
    }

    let year = date[0..4].parse().ok()?;
    let month = date[5..7].parse().ok()?;
    let day = date[8..10].parse().ok()?;
    let hour = time[0..2].parse().ok()?;
    let minute = time[3..5].parse().ok()?;
    let second = time[6..8].parse().ok()?;
    Some(datetime_to_timestamp(
        year, month, day, hour, minute, second,
    ))
}

fn parse_listing_size(size: &str) -> GeeZipResult<i64> {
    size.parse::<i64>().map_err(|_| {
        GeeZipError::format(
            format!("invalid ZPAQ listing size field '{size}'"),
            ArchiveFormat::Zpaq,
        )
    })
}

fn probe_entry_size(archive_path: &Path, entry_path: &str) -> GeeZipResult<u64> {
    let archive_path = archive_path.to_str().ok_or_else(|| {
        GeeZipError::format("ZPAQ archive path is not valid UTF-8", ArchiveFormat::Zpaq)
    })?;
    let bytes = zpaq_rs::archive_read_file_bytes_from_file(archive_path, entry_path)
        .map_err(|err| convert_zpaq_entry_error(err, entry_path))?;
    Ok(bytes.len() as u64)
}

fn parse_listing_line(archive_path: &Path, line: &str) -> GeeZipResult<Option<Entry>> {
    let Some(status) = line.as_bytes().first().copied() else {
        return Ok(None);
    };
    if !matches!(status, b'=' | b'#' | b'+' | b'-') {
        return Ok(None);
    }
    if line.len() <= PATH_START {
        return Ok(None);
    }

    let Some(date) = line.get(DATE_RANGE.clone()) else {
        return Ok(None);
    };
    let Some(time) = line.get(TIME_RANGE.clone()) else {
        return Ok(None);
    };
    if !looks_like_listing_date(date) || !looks_like_listing_time(time) {
        return Ok(None);
    }

    let Some(size_field) = line.get(SIZE_RANGE.clone()) else {
        return Ok(None);
    };
    let size_field = size_field.trim();
    if size_field.is_empty() {
        return Ok(None);
    }

    let path = line[PATH_START..].trim_start();
    if path.is_empty() {
        return Err(GeeZipError::format(
            "ZPAQ listing entry is missing a pathname",
            ArchiveFormat::Zpaq,
        ));
    }

    let is_dir = path.ends_with('/');
    let listed_size = parse_listing_size(size_field)?;
    let size = if is_dir {
        0
    } else if listed_size >= 0 {
        listed_size as u64
    } else {
        probe_entry_size(archive_path, path)?
    };

    Ok(Some(Entry {
        path: path.to_owned(),
        size,
        compressed_size: 0,
        crc32: None,
        modified: parse_listing_timestamp(date, time),
        is_dir,
    }))
}

fn scan_zpaq_entries(path: &Path) -> GeeZipResult<Vec<Entry>> {
    let archive_path = path.to_str().ok_or_else(|| {
        GeeZipError::format("ZPAQ archive path is not valid UTF-8", ArchiveFormat::Zpaq)
    })?;
    let listing = zpaq_rs::zpaq_list(archive_path, &[]).map_err(|err| {
        convert_zpaq_archive_error(err, format!("listing ZPAQ archive '{}'", path.display()))
    })?;

    let mut entries = Vec::new();
    for line in listing.stdout.lines() {
        if let Some(entry) = parse_listing_line(path, line)? {
            entries.push(entry);
        }
    }

    Ok(entries)
}

impl ArchiveReader for ZpaqReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.ensure_entries()?;
        Ok(self.entries.clone().unwrap_or_default())
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let entry = self.find_entry(&entry.path)?;
        if entry.is_dir {
            return Ok(0);
        }

        let bytes =
            zpaq_rs::archive_read_file_bytes_from_file(self.archive_path_str()?, &entry.path)
                .map_err(|err| convert_zpaq_entry_error(err, &entry.path))?;
        writer.write_all(&bytes).map_err(|err| {
            GeeZipError::io(
                err,
                format!("writing extracted ZPAQ entry '{}'", entry.path),
            )
        })?;
        Ok(bytes.len() as u64)
    }
}

// ---------------------------------------------------------------------------
// ZpaqWriter
// ---------------------------------------------------------------------------

/// ZPAQ archive writer.
///
/// Entries are buffered in memory during `add_entry_from_reader` calls and
/// compressed in a single pass when `finish()` is called via
/// `zpaq_rs::archive_from_entries`.
pub struct ZpaqWriter<W: Write + Send> {
    inner: Option<W>,
    entries: Vec<BufferedZpaqEntry>,
    method: String,
    format: ArchiveFormat,
}

struct BufferedZpaqEntry {
    path: String,
    data: Vec<u8>,
}

impl<W: Write + Send> fmt::Debug for ZpaqWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZpaqWriter")
            .field("format", &self.format)
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> ZpaqWriter<W> {
    pub fn new(writer: W, level: Option<u32>) -> Self {
        let method = match level {
            None | Some(0) => "1",
            Some(1) => "1",
            Some(2) => "2",
            Some(3) => "3",
            Some(4) => "4",
            Some(5) => "5",
            Some(_) => "5", // Fallback: treat anything above 5 as "5"
        };
        Self {
            inner: Some(writer),
            entries: Vec::new(),
            method: method.to_string(),
            format: ArchiveFormat::Zpaq,
        }
    }

    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let mut writer = self.inner.take().ok_or_else(|| {
            GeeZipError::format("ZPAQ writer already finalised", ArchiveFormat::Zpaq)
        })?;

        let entry_refs: Vec<zpaq_rs::ArchiveEntry> = self
            .entries
            .iter()
            .map(|e| zpaq_rs::ArchiveEntry {
                path: &e.path,
                data: &e.data,
                comment: None,
            })
            .collect();

        let archive_bytes =
            zpaq_rs::archive_from_entries(&entry_refs, &self.method).map_err(|err| {
                GeeZipError::format(
                    format!("ZPAQ compression failed: {err}"),
                    ArchiveFormat::Zpaq,
                )
            })?;

        writer
            .write_all(&archive_bytes)
            .map_err(|err| GeeZipError::io(err, "writing ZPAQ archive"))?;
        writer
            .flush()
            .map_err(|err| GeeZipError::io(err, "flushing ZPAQ archive"))?;

        Ok((archive_bytes.len() as u64, writer))
    }
}

impl<W: Write + Send> ArchiveWriter for ZpaqWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let raw = path.to_str().ok_or_else(|| {
            GeeZipError::format(
                format!("non-UTF-8 path: {}", path.display()),
                ArchiveFormat::Zpaq,
            )
        })?;

        // ZPAQ uses unix-style paths internally
        let normalized = raw.replace('\\', "/");

        if is_entry_path_dangerous(Path::new(&normalized)) {
            return Err(GeeZipError::format(
                format!("invalid ZPAQ entry path: {raw}"),
                ArchiveFormat::Zpaq,
            ));
        }

        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(|e| {
            GeeZipError::io(e, format!("reading data for ZPAQ entry '{normalized}'"))
        })?;

        self.entries.push(BufferedZpaqEntry {
            path: normalized,
            data,
        });
        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let raw = path.to_str().ok_or_else(|| {
            GeeZipError::format(
                format!("non-UTF-8 path: {}", path.display()),
                ArchiveFormat::Zpaq,
            )
        })?;
        let normalized = raw.replace('\\', "/");

        if is_entry_path_dangerous(Path::new(&normalized)) {
            return Err(GeeZipError::format(
                format!("invalid ZPAQ entry path: {raw}"),
                ArchiveFormat::Zpaq,
            ));
        }

        // Register directory with trailing slash (matches reader convention)
        let dir_path = if normalized.ends_with('/') {
            normalized
        } else {
            format!("{normalized}/")
        };

        self.entries.push(BufferedZpaqEntry {
            path: dir_path,
            data: Vec::new(),
        });
        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::PathBuf;

    use zpaq_rs::{archive_from_entries, ArchiveEntry};

    fn build_test_zpaq(entries: &[ArchiveEntry<'_>]) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let archive_path = temp.path().join("fixture.zpaq");
        let archive = archive_from_entries(entries, "1").expect("zpaq fixture should be created");
        fs::write(&archive_path, archive).expect("fixture archive should be written");
        (temp, archive_path)
    }

    fn sample_entries<'a>() -> [ArchiveEntry<'a>; 3] {
        [
            ArchiveEntry {
                path: "hello.txt",
                data: b"hello zpaq\n",
                comment: None,
            },
            ArchiveEntry {
                path: "dir/",
                data: b"",
                comment: None,
            },
            ArchiveEntry {
                path: "dir/nested.txt",
                data: b"nested zpaq\n",
                comment: None,
            },
        ]
    }

    #[test]
    fn zpaq_entries_and_extract_file() {
        let (_temp, archive_path) = build_test_zpaq(&sample_entries());
        let mut reader = ZpaqReader::new(&archive_path);

        let entries = reader.entries().expect("entries should be listed");
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .any(|entry| entry.path == "dir/" && entry.is_dir));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "hello.txt" && entry.size == 11));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "dir/nested.txt" && entry.size == 12));

        let hello = entries
            .iter()
            .find(|entry| entry.path == "hello.txt")
            .expect("hello entry should exist")
            .clone();
        let mut out = Vec::new();
        let bytes = reader
            .extract(&hello, &mut out)
            .expect("file should extract successfully");
        assert_eq!(bytes, out.len() as u64);
        assert_eq!(out, b"hello zpaq\n");

        let dir = entries
            .iter()
            .find(|entry| entry.path == "dir/")
            .expect("dir entry should exist")
            .clone();
        let bytes = reader
            .extract(&dir, &mut std::io::sink())
            .expect("directory extract should be a no-op");
        assert_eq!(bytes, 0);
    }

    #[test]
    fn zpaq_extract_all_nested_paths() {
        let (_temp, archive_path) = build_test_zpaq(&sample_entries());
        let mut reader = ZpaqReader::new(&archive_path);
        let out_dir = tempfile::TempDir::new().expect("output dir should exist");

        let report = reader
            .extract_all(out_dir.path(), true)
            .expect("extract_all should succeed");
        assert_eq!(report.errors.len(), 0);
        assert_eq!(report.files_extracted, 3);
        assert_eq!(
            fs::read(out_dir.path().join("hello.txt")).expect("hello.txt should exist"),
            b"hello zpaq\n"
        );
        assert_eq!(
            fs::read(out_dir.path().join("dir/nested.txt")).expect("nested.txt should exist"),
            b"nested zpaq\n"
        );
    }

    #[test]
    fn zpaq_path_traversal_is_blocked_on_extract_all() {
        let entries = [ArchiveEntry {
            path: "../escape.txt",
            data: b"blocked",
            comment: None,
        }];
        let (_temp, archive_path) = build_test_zpaq(&entries);
        let mut reader = ZpaqReader::new(&archive_path);
        let out_dir = tempfile::TempDir::new().expect("output dir should exist");

        let report = reader
            .extract_all(out_dir.path(), true)
            .expect("extract_all should report path traversal without aborting");
        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(
            report.errors[0].1,
            GeeZipError::PathTraversal { .. }
        ));
        assert!(!out_dir.path().join("escape.txt").exists());
    }

    #[test]
    fn zpaq_invalid_archive_fails_cleanly() {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let archive_path = temp.path().join("broken.zpaq");
        fs::write(&archive_path, b"not a zpaq archive").expect("broken archive should be written");

        let mut reader = ZpaqReader::new(&archive_path);
        let err = reader.entries().expect_err("invalid archive should fail");
        assert!(matches!(err, GeeZipError::Format { .. }));
        let message = err.to_string();
        assert!(message.contains("zpaq"));
    }

    #[test]
    fn zpaq_missing_entry_reports_not_found() {
        let (_temp, archive_path) = build_test_zpaq(&sample_entries());
        let mut reader = ZpaqReader::new(&archive_path);
        let missing = Entry {
            path: "missing.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        };

        let err = reader
            .extract(&missing, &mut Vec::new())
            .expect_err("missing entry should fail");
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn zpaq_trait_object_is_supported() {
        let (_temp, archive_path) = build_test_zpaq(&sample_entries());
        let reader: Box<dyn ArchiveReader> = Box::new(ZpaqReader::new(&archive_path));
        assert_eq!(reader.format(), ArchiveFormat::Zpaq);
    }

    // -----------------------------------------------------------------------
    // ZPAQ writer tests
    // -----------------------------------------------------------------------

    #[test]
    fn zpaq_writer_single_file_roundtrip() {
        let content = b"hello zpaq writer";
        let mut writer = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(1));
        writer
            .add_entry_from_reader(
                std::path::Path::new("hello.txt"),
                &mut std::io::Cursor::new(content.to_vec()),
            )
            .expect("file should be added");

        let (bytes_written, cursor) = writer.finalize().expect("writer should finalize");
        let archive = cursor.into_inner();
        assert_eq!(bytes_written, archive.len() as u64);

        let temp = tempfile::TempDir::new().expect("tempdir");
        let archive_path = temp.path().join("out.zpaq");
        fs::write(&archive_path, &archive).expect("write");
        let mut reader = ZpaqReader::new(&archive_path);
        let entries = reader.entries().expect("entries should load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].size, content.len() as u64);
    }

    #[test]
    fn zpaq_writer_roundtrip_with_directories() {
        let mut writer = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(1));
        writer
            .add_directory(std::path::Path::new("subdir"))
            .expect("dir should be added");
        writer
            .add_entry_from_reader(
                std::path::Path::new("subdir/file.txt"),
                &mut std::io::Cursor::new(b"nested"),
            )
            .expect("nested file should be added");

        let (_, cursor) = writer.finalize().expect("writer should finalize");
        let archive = cursor.into_inner();

        let temp = tempfile::TempDir::new().expect("tempdir");
        let archive_path = temp.path().join("out.zpaq");
        fs::write(&archive_path, &archive).expect("write");
        let mut reader = ZpaqReader::new(&archive_path);
        let entries = reader.entries().expect("entries should load");
        assert!(
            entries.iter().any(|e| e.path == "subdir/" && e.is_dir),
            "expected subdir/ directory: {entries:?}"
        );
        assert!(
            entries.iter().any(|e| e.path == "subdir/file.txt"),
            "expected subdir/file.txt: {entries:?}"
        );
    }

    #[test]
    fn zpaq_writer_trait_object_finish_returns_byte_count() {
        let mut writer: Box<dyn ArchiveWriter> =
            Box::new(ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(1)));
        writer
            .add_entry_from_reader(
                std::path::Path::new("data.bin"),
                &mut std::io::Cursor::new(b"payload"),
            )
            .unwrap();
        let bytes = writer.finish().unwrap();
        assert!(bytes > 0);
    }

    #[test]
    fn zpaq_writer_rejects_absolute_paths() {
        let mut writer = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), None);
        let err = writer
            .add_entry_from_reader(
                std::path::Path::new("/etc/passwd"),
                &mut std::io::Cursor::new(b"bad"),
            )
            .expect_err("absolute path should be rejected");
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn zpaq_writer_level_method_mapping() {
        // Level 0 / None → "1" (fast)
        let w = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), None);
        assert_eq!(w.method, "1");
        let w = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(0));
        assert_eq!(w.method, "1");
        // Explicit levels
        let w = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(3));
        assert_eq!(w.method, "3");
        let w = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(5));
        assert_eq!(w.method, "5");
        // Above 5 → "5"
        let w = ZpaqWriter::new(std::io::Cursor::new(Vec::new()), Some(9));
        assert_eq!(w.method, "5");
    }
}
