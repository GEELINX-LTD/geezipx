//! ZIP archive reader and writer implementations.
//!
//! Built on top of the [`zip`] crate (v2.x).  The reader is generic over
//! any `Read + Seek + Send` backend so callers can pass a file or a
//! memory buffer; individual entry extraction is streaming via
//! `std::io::copy`.  The writer is generic over any `Write + Seek` backend.

use std::fmt;
use std::io::{Read, Seek, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;

use crate::archive::{
    check_entry_path_safety, normalize_path, ArchiveReader, ArchiveWriter, Entry, ExtractReport,
};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// ZipReader
// ---------------------------------------------------------------------------

/// ZIP archive reader.
///
/// Generic over any `R: Read + Seek + Send` (file, cursor, etc.).  The
/// [`ZipReader::from_buf`] convenience constructor is provided for the
/// common case of reading from an already-loaded byte buffer.
pub struct ZipReader<R: Read + Seek + Send> {
    archive: zip::ZipArchive<R>,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for ZipReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZipReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> ZipReader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    ///
    /// The input is **not** buffered into memory — the source is passed
    /// directly to the underlying `zip::ZipArchive`.
    pub fn new(reader: R) -> GeeZipResult<Self> {
        let archive = zip::ZipArchive::new(reader).map_err(convert_zip_error)?;
        Ok(ZipReader {
            archive,
            format: ArchiveFormat::Zip,
        })
    }
}

impl ZipReader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    ///
    /// Equivalent to `ZipReader::new(std::io::Cursor::new(buf))`.
    pub fn from_buf(buf: Vec<u8>) -> GeeZipResult<Self> {
        ZipReader::new(std::io::Cursor::new(buf))
    }
}

impl<R: Read + Seek + Send> ArchiveReader for ZipReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        let len = self.archive.len();
        let mut entries = Vec::with_capacity(len);

        for i in 0..len {
            let file = self.archive.by_index(i).map_err(convert_zip_error)?;
            let modified = file.last_modified().map(|dt| {
                crate::archive::datetime_to_timestamp(
                    dt.year() as u64,
                    dt.month() as u64,
                    dt.day() as u64,
                    dt.hour() as u64,
                    dt.minute() as u64,
                    dt.second() as u64,
                )
            });
            entries.push(Entry {
                path: file.name().to_owned(),
                size: file.size(),
                compressed_size: file.compressed_size(),
                crc32: Some(file.crc32()),
                modified,
            });
        }

        Ok(entries)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut file = self.archive.by_name(&entry.path).map_err(|e| match e {
            zip::result::ZipError::FileNotFound => GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            },
            other => convert_zip_error(other),
        })?;

        let bytes = std::io::copy(&mut file, writer)
            .map_err(|e| GeeZipError::io(e, format!("extracting '{}'", entry.path)))?;

        Ok(bytes)
    }

    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport> {
        let entries = self.entries()?;
        let mut report = ExtractReport::default();

        let dest = normalize_path(dest);

        for entry in &entries {
            let entry_path = Path::new(&entry.path);

            // --- Path safety checks (Zip Slip protection) ---
            let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    continue;
                }
            };

            // Create parent directory.
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, "creating parent directory"),
                        ));
                        continue;
                    }
                }
            }

            // Write entry content — atomically create or fail (avoids TOCTOU
            // between path-exists check and file creation for the no-clobber path).
            let mut output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        continue;
                    }
                }
            } else {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        report.files_skipped += 1;
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        continue;
                    }
                }
            };

            match self.extract(entry, &mut output) {
                Ok(bytes) => {
                    report.files_extracted += 1;
                    report.bytes_extracted += bytes;
                }
                Err(e) => {
                    report.errors.push((entry.path.clone(), e));
                }
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// ZipWriter
// ---------------------------------------------------------------------------

/// ZIP archive writer.
///
/// Generic over any `W: Write + Seek + Send`.  Construct via
/// [`ZipWriter::new`], add entries with
/// [`add_entry_from_reader`](ArchiveWriter::add_entry_from_reader), then
/// finalise with either:
///
/// - [`ZipWriter::finalize`] — returns `(total_bytes, inner_writer)`
/// - [`ArchiveWriter::finish`] — returns `total_bytes` (trait object-safe)
pub struct ZipWriter<W: Write + Seek> {
    inner: zip::ZipWriter<W>,
    start_pos: u64,
    format: ArchiveFormat,
}

impl<W: Write + Seek> ZipWriter<W> {
    /// Create a new ZIP writer targeting the given output.
    pub fn new(mut writer: W) -> Self {
        let start_pos = writer.stream_position().unwrap_or(0);
        ZipWriter {
            inner: zip::ZipWriter::new(writer),
            start_pos,
            format: ArchiveFormat::Zip,
        }
    }

    /// Finalize the ZIP archive and return the inner writer alongside
    /// the total number of bytes written.
    ///
    /// This is the "rich" version of [`ArchiveWriter::finish`] that lets
    /// callers recover the underlying writer (e.g. to inspect the
    /// buffer contents).
    pub fn finalize(self) -> GeeZipResult<(u64, W)> {
        let start_pos = self.start_pos;
        let mut writer = self.inner.finish().map_err(convert_zip_error)?;
        let end_pos = writer
            .stream_position()
            .map_err(|e| GeeZipError::io(e, "getting final archive size"))?;
        Ok((end_pos - start_pos, writer))
    }
}

impl<W: Write + Seek + Send> ArchiveWriter for ZipWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::Zip,
        })?;

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::DEFLATE);

        self.inner
            .start_file(name, options)
            .map_err(convert_zip_error)?;

        std::io::copy(reader, &mut self.inner)
            .map_err(|e| GeeZipError::io(e, format!("writing entry '{}'", name)))?;

        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        // Deref the box and call the inherent finalize, discarding
        // the writer.
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn convert_zip_error(e: zip::result::ZipError) -> GeeZipError {
    match e {
        zip::result::ZipError::Io(inner) => GeeZipError::Io {
            source: inner,
            context: "ZIP operation failed".into(),
        },
        zip::result::ZipError::InvalidArchive(msg) => GeeZipError::Format {
            message: format!("invalid ZIP archive: {msg}"),
            format: ArchiveFormat::Zip,
        },
        zip::result::ZipError::FileNotFound => GeeZipError::EntryNotFound {
            name: "(unknown)".into(),
        },
        zip::result::ZipError::UnsupportedArchive(msg) => GeeZipError::Format {
            message: format!("unsupported ZIP feature: {msg}"),
            format: ArchiveFormat::Zip,
        },
        zip::result::ZipError::InvalidPassword => GeeZipError::Crypto {
            message: "invalid ZIP password".into(),
        },
        _ => GeeZipError::Format {
            message: "unknown ZIP error".into(),
            format: ArchiveFormat::Zip,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Create a minimal valid ZIP archive in memory containing the given
    /// file entries (stored, not compressed).
    fn create_test_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            for (name, data) in files {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    // -------------------------------------------------------------------
    // Round-trip: write to Cursor -> read back from Vec
    // -------------------------------------------------------------------

    #[test]
    fn zip_roundtrip_single_file() {
        let content = b"hello world";
        let data = create_test_zip(&[("hello.txt", content)]);

        let mut reader = ZipReader::from_buf(data).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
        assert_eq!(entries[0].size, content.len() as u64);

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(bytes, content.len() as u64);
        assert_eq!(output, content);
    }

    #[test]
    fn zip_roundtrip_multiple_files() {
        let files = [
            ("a.txt", b"aaa" as &[u8]),
            ("b.txt", b"bbb" as &[u8]),
            ("c.txt", b"ccc" as &[u8]),
        ];
        let data = create_test_zip(&files);

        let mut reader = ZipReader::from_buf(data).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 3);

        for (i, (name, content)) in files.iter().enumerate() {
            assert_eq!(entries[i].path, *name);
            let mut output = Vec::new();
            reader.extract(&entries[i], &mut output).unwrap();
            assert_eq!(output, *content);
        }
    }

    #[test]
    fn zip_roundtrip_nested_path() {
        let content = b"nested content";
        let data = create_test_zip(&[("dir/subdir/file.txt", content)]);

        let mut reader = ZipReader::from_buf(data).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "dir/subdir/file.txt");

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, content);
        assert_eq!(bytes, content.len() as u64);
    }

    #[test]
    fn zip_unicode_filename() {
        let content = b"unicode content";
        let data = create_test_zip(&[("\u{4e2d}\u{6587}.txt", content)]); // 中文.txt

        let mut reader = ZipReader::from_buf(data).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.contains('\u{4e2d}'));
    }

    // -------------------------------------------------------------------
    // Empty / malformed archives
    // -------------------------------------------------------------------

    #[test]
    fn zip_empty_archive_fails() {
        let err = ZipReader::from_buf(vec![]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("zip") || msg.to_lowercase().contains("invalid"),
            "expected ZIP-related error, got: {msg}"
        );
    }

    #[test]
    fn zip_corrupted_archive_fails() {
        let bad_data = b"this is not a zip file at all";
        let err = ZipReader::from_buf(bad_data.to_vec()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("zip") || msg.to_lowercase().contains("invalid"),
            "expected ZIP-related error, got: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // Zip Slip protection
    // -------------------------------------------------------------------

    #[test]
    fn zip_slip_detection() {
        use std::io::Write as IoWrite;

        // Create a ZIP with a path-traversal entry manually.
        let mut buf = Cursor::new(Vec::new());
        {
            let inner = Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(inner);
            let name = "../escape.txt";
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file(name, options).unwrap();
            zip.write_all(b"malicious").unwrap();
            let inner = zip.finish().unwrap();
            buf.write_all(&inner.into_inner()).unwrap();
        }

        let mut reader = ZipReader::from_buf(buf.into_inner()).unwrap();
        let entries = reader.entries().unwrap();
        assert!(entries[0].path.contains(".."));

        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::PathTraversal { .. })),
            "expected PathTraversal error, got: {report:?}"
        );
        assert_eq!(report.files_extracted, 0);
    }

    #[test]
    fn zip_slip_dotdot_in_middle() {
        use std::io::Write as IoWrite;

        // foo/../../bar must be detected as PathTraversal.
        let mut buf = Cursor::new(Vec::new());
        {
            let inner = Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(inner);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("subdir/../../../escape.txt", options)
                .unwrap();
            zip.write_all(b"escape").unwrap();
            let inner = zip.finish().unwrap();
            buf.write_all(&inner.into_inner()).unwrap();
        }

        let mut reader = ZipReader::from_buf(buf.into_inner()).unwrap();
        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::PathTraversal { .. })),
            "expected PathTraversal for foo/../../bar, got: {report:?}"
        );
        assert_eq!(report.files_extracted, 0);
    }

    #[test]
    fn zip_slip_absolute_path() {
        use std::io::Write as IoWrite;

        // Entries with absolute paths must be rejected.
        let mut buf = Cursor::new(Vec::new());
        {
            let inner = Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(inner);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("/etc/passwd", options).unwrap();
            zip.write_all(b"leak").unwrap();
            let inner = zip.finish().unwrap();
            buf.write_all(&inner.into_inner()).unwrap();
        }

        let mut reader = ZipReader::from_buf(buf.into_inner()).unwrap();
        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::PathTraversal { .. })),
            "expected PathTraversal for absolute path, got: {report:?}"
        );
        assert_eq!(report.files_extracted, 0);
    }

    #[test]
    fn zip_extract_all_to_curdir() {
        use std::io::Write as IoWrite;

        let mut buf = Cursor::new(Vec::new());
        {
            let inner = Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(inner);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("file_a.txt", options).unwrap();
            zip.write_all(b"AAA").unwrap();
            let inner = zip.finish().unwrap();
            buf.write_all(&inner.into_inner()).unwrap();
        }

        let tmp = tempfile::tempdir().unwrap();
        let orig_cwd = std::env::current_dir().unwrap();

        std::env::set_current_dir(tmp.path()).unwrap();
        let mut reader = ZipReader::from_buf(buf.into_inner()).unwrap();
        let report = reader.extract_all(Path::new("."), true).unwrap();
        std::env::set_current_dir(orig_cwd).unwrap();

        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty(), "errors: {report:#?}");
        assert!(
            tmp.path().join("file_a.txt").exists(),
            "file_a.txt should exist in {}",
            tmp.path().display()
        );
    }

    // -------------------------------------------------------------------
    // Writer
    // -------------------------------------------------------------------

    #[test]
    fn zip_writer_roundtrip() {
        let buf = Cursor::new(Vec::new());
        let mut zip_writer = ZipWriter::new(buf);
        zip_writer
            .add_entry_from_reader(
                &PathBuf::from("test.txt"),
                &mut Cursor::new(b"hello from writer"),
            )
            .unwrap();

        // Use inherent finalize to get the writer back, then read back.
        let (bytes_written, writer) = zip_writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        let data = writer.into_inner();
        let mut reader = ZipReader::from_buf(data).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");
        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, b"hello from writer".len() as u64);
        assert_eq!(output, b"hello from writer");
    }

    #[test]
    fn zip_writer_multiple_files_roundtrip() {
        let buf = Cursor::new(Vec::new());
        let mut zip_writer = ZipWriter::new(buf);

        let files = [
            ("f1.txt", b"content 1" as &[u8]),
            ("f2.txt", b"content 2" as &[u8]),
            ("sub/f3.txt", b"nested content" as &[u8]),
        ];

        for (name, content) in &files {
            zip_writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(content))
                .unwrap();
        }

        // finalize through trait to exercise that path too
        let boxed: Box<dyn ArchiveWriter> = Box::new(zip_writer);
        let _bytes_written = boxed.finish().unwrap();
    }

    #[test]
    fn zip_writer_finish_returns_bytes() {
        let buf = Cursor::new(Vec::new());
        let mut zip_writer = ZipWriter::new(buf);
        zip_writer
            .add_entry_from_reader(&PathBuf::from("data.bin"), &mut Cursor::new(b"data"))
            .unwrap();
        let boxed: Box<dyn ArchiveWriter> = Box::new(zip_writer);
        let bytes = boxed.finish().unwrap();
        assert!(bytes > 0, "should report bytes written");
    }

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------

    #[test]
    fn zip_entry_not_found() {
        let data = create_test_zip(&[("exists.txt", b"data")]);
        let mut reader = ZipReader::from_buf(data).unwrap();

        let fake_entry = Entry {
            path: "does_not_exist.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
        };

        let mut output = Vec::new();
        let err = reader.extract(&fake_entry, &mut output).unwrap_err();
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn zip_extract_all_basic() {
        let data = create_test_zip(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = ZipReader::from_buf(data).unwrap();
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());

        // Verify files exist on disk.
        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    // -------------------------------------------------------------------
    // No-clobber tests
    // -------------------------------------------------------------------

    #[test]
    fn zip_no_clobber_skips_existing_files() {
        let data = create_test_zip(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = ZipReader::from_buf(data).unwrap();
        let dest = tempfile::tempdir().unwrap();

        // First extract (creates files).
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert!(report.errors.is_empty());

        // Modify one extracted file.
        let modified_path = dest.path().join("file_a.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        // Second extract with overwrite=false: should skip existing files.
        let mut reader2 = ZipReader::from_buf(create_test_zip(&[
            ("file_a.txt", b"AAA"),
            ("file_b.txt", b"BBB"),
        ]))
        .unwrap();
        let report2 = reader2.extract_all(dest.path(), false).unwrap();
        assert_eq!(
            report2.files_extracted, 0,
            "existing files should be skipped"
        );
        assert_eq!(
            report2.files_skipped, 2,
            "both files should be counted as skipped"
        );

        // Verify existing file was NOT overwritten.
        assert_eq!(
            std::fs::read_to_string(&modified_path).unwrap(),
            "MODIFIED",
            "existing file content should be preserved"
        );

        // Verify clobber-denied errors are recorded.
        assert!(
            report2
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::ClobberDenied { .. })),
            "expected at least one ClobberDenied error"
        );
    }

    // -------------------------------------------------------------------
    // Trait object safety (compile-time checks)
    // -------------------------------------------------------------------

    #[test]
    fn archive_reader_trait_object() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let data = create_test_zip(&[("dummy.txt", b"x")]);
        let mut reader = ZipReader::from_buf(data).unwrap();
        use_reader(&mut reader);
    }

    #[test]
    fn archive_writer_trait_object() {
        fn use_writer(_w: Box<dyn ArchiveWriter>) {}
        let buf = Cursor::new(Vec::new());
        let writer = ZipWriter::new(buf);
        use_writer(Box::new(writer));
    }
}
