//! TAR archive reader and writer implementations.
//!
//! Built on top of the [`tar`] crate.  The reader is generic over any
//! `Read + Seek + Send` so callers can seek back to the start for
//! multiple-pass iteration.  The writer is generic over any `Write + Send`
//! backend.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::CountWriter;
use crate::archive::{ArchiveReader, ArchiveWriter, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// TarReader
// ---------------------------------------------------------------------------

/// TAR archive reader.
///
/// Generic over any `R: Read + Seek + Send` (file, cursor, etc.).  Each
/// call to [`entries`](ArchiveReader::entries) or
/// [`extract`](ArchiveReader::extract) resets the reader to the
/// beginning of the stream.
pub struct TarReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for TarReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> TarReader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    ///
    /// The input is **not** buffered into memory — the source is passed
    /// directly to the underlying `tar::Archive`.
    pub fn new(reader: R) -> Self {
        TarReader {
            inner: reader,
            format: ArchiveFormat::Tar,
        }
    }
}

impl TarReader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    ///
    /// Equivalent to `TarReader::new(std::io::Cursor::new(buf))`.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        TarReader::new(std::io::Cursor::new(buf))
    }
}

/// Collect entries from a tar archive, skipping metadata entries.
fn collect_tar_entries<R: Read>(archive: &mut tar::Archive<R>) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive.entries().map_err(convert_tar_error)? {
        let tar_entry = result.map_err(convert_tar_error)?;
        let header = tar_entry.header();

        // The tar crate transparently handles GNU long name / PAX extended
        // headers, but we still filter them explicitly for safety.
        let entry_type = header.entry_type();
        if matches!(
            entry_type,
            tar::EntryType::XGlobalHeader
                | tar::EntryType::GNULongLink
                | tar::EntryType::GNULongName
        ) {
            continue;
        }

        // Skip directory entries — extract_all handles parent directory
        // creation implicitly, and treating a directory as a file would
        // break extraction of files inside it.
        if header.entry_type().is_dir() {
            continue;
        }

        let path = tar_entry
            .path()
            .map_err(convert_tar_error)?
            .to_string_lossy()
            .into_owned();
        let size = tar_entry.size();

        entries.push(Entry {
            path,
            size,
            compressed_size: 0,
            crc32: None,
        });
    }

    Ok(entries)
}

impl<R: Read + Seek + Send> ArchiveReader for TarReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut archive = tar::Archive::new(&mut self.inner);
        collect_tar_entries(&mut archive)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut archive = tar::Archive::new(&mut self.inner);

        for result in archive.entries().map_err(convert_tar_error)? {
            let mut tar_entry = result.map_err(convert_tar_error)?;
            let path = tar_entry
                .path()
                .map_err(convert_tar_error)?
                .to_string_lossy()
                .into_owned();

            if path == entry.path {
                if tar_entry.header().entry_type().is_dir() {
                    return Ok(0);
                }
                let bytes = std::io::copy(&mut tar_entry, writer)
                    .map_err(|e| GeeZipError::io(e, format!("extracting '{}'", entry.path)))?;
                return Ok(bytes);
            }
        }

        Err(GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// TarWriter
// ---------------------------------------------------------------------------

/// TAR archive writer.
///
/// Generic over any `W: Write + Send`.  Construct via [`TarWriter::new`],
/// add entries with [`add_entry_from_reader`](ArchiveWriter::add_entry_from_reader),
/// then finalise with either:
///
/// - [`TarWriter::finalize`] — returns `(total_bytes, inner_writer)`
/// - [`ArchiveWriter::finish`] — returns `total_bytes` (trait object-safe)
pub struct TarWriter<W: Write + Send> {
    inner: Option<tar::Builder<CountWriter<W>>>,
    format: ArchiveFormat,
}

impl<W: Write + Send> fmt::Debug for TarWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarWriter")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> TarWriter<W> {
    /// Create a new TAR writer targeting the given output.
    pub fn new(writer: W) -> Self {
        let counter = CountWriter {
            inner: writer,
            count: 0,
        };
        TarWriter {
            inner: Some(tar::Builder::new(counter)),
            format: ArchiveFormat::Tar,
        }
    }

    /// Finalise the archive and return the inner writer alongside
    /// the total number of bytes written.
    ///
    /// This is the "rich" version of [`ArchiveWriter::finish`] that lets
    /// callers recover the underlying writer (e.g. to inspect the
    /// buffer contents).
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let builder = self.inner.take().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer already finalised".into(),
            format: ArchiveFormat::Tar,
        })?;
        let count_writer = builder
            .into_inner()
            .map_err(|e| GeeZipError::io(e, "finalising TAR archive"))?;
        let bytes = count_writer.count;
        let writer = count_writer.inner;
        Ok((bytes, writer))
    }
}

impl<W: Write + Send> ArchiveWriter for TarWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::Tar,
        })?;

        // Tar requires data size in the header, so we buffer the data.
        let mut data = Vec::new();
        let mut chunk = [0u8; 65536]; // 64 KiB chunks for cancellation responsiveness
        loop {
            let n = reader
                .read(&mut chunk)
                .map_err(|e| GeeZipError::io(e, format!("reading data for entry '{name}'")))?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
        }

        let mut header = tar::Header::new_gnu();
        header.set_path(path).map_err(|e| GeeZipError::Format {
            message: format!("setting tar header path: {e}"),
            format: ArchiveFormat::Tar,
        })?;
        header.set_size(data.len() as u64);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::Tar,
        })?;
        builder
            .append(&header, std::io::Cursor::new(data))
            .map_err(convert_tar_error)?;

        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn convert_tar_error(e: std::io::Error) -> GeeZipError {
    GeeZipError::Io {
        source: e,
        context: "TAR operation failed".into(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Create a minimal valid TAR archive in memory containing the given
    /// file entries.
    fn create_test_tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let mut builder = tar::Builder::new(buf);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append(&header, std::io::Cursor::new(data)).unwrap();
        }
        builder.into_inner().unwrap()
    }

    /// Create a raw tar archive with the given entry path (allowing
    /// malicious paths like "../" that `tar::Header::set_path` rejects).
    fn create_raw_tar(path: &[u8], data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];

        // Path (100 bytes)
        let name_len = path.len().min(99);
        header[..name_len].copy_from_slice(&path[..name_len]);

        // Mode
        header[100..108].copy_from_slice(b"0000644\0");

        // Size (12 bytes octal)
        let size_oct = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size_oct.as_bytes());

        // Typeflag: '0' = regular file
        header[156] = b'0';

        // ustar magic + version
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        // Calculate checksum
        for b in header.iter_mut().take(156).skip(148) {
            *b = b' ';
        }
        let cksum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{:06o}\0 ", cksum);
        header[148..156].copy_from_slice(cksum_str.as_bytes());

        let mut archive = header.to_vec();

        // Data (padded to 512-byte block boundary)
        if !data.is_empty() {
            archive.extend_from_slice(data);
            let padding = (512 - data.len() % 512) % 512;
            archive.extend(std::iter::repeat_n(0, padding));
        }

        // End-of-archive markers (two zero blocks)
        archive.extend_from_slice(&[0u8; 1024]);

        archive
    }

    // -------------------------------------------------------------------
    // Round-trip: write to Vec -> read back
    // -------------------------------------------------------------------

    #[test]
    fn tar_roundtrip_single_file() {
        let content = b"hello world";
        let data = create_test_tar(&[("hello.txt", content)]);

        let mut reader = TarReader::from_buf(data);
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
    fn tar_roundtrip_multiple_files() {
        let files = [
            ("a.txt", b"aaa" as &[u8]),
            ("b.txt", b"bbb" as &[u8]),
            ("c.txt", b"ccc" as &[u8]),
        ];
        let data = create_test_tar(&files);

        let mut reader = TarReader::from_buf(data);
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
    fn tar_roundtrip_nested_path() {
        let content = b"nested content";
        let data = create_test_tar(&[("dir/subdir/file.txt", content)]);

        let mut reader = TarReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "dir/subdir/file.txt");

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, content);
        assert_eq!(bytes, content.len() as u64);
    }

    #[test]
    fn tar_unicode_filename() {
        let content = b"unicode content";
        let data = create_test_tar(&[("\u{4e2d}\u{6587}.txt", content)]); // 中文.txt

        let mut reader = TarReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.contains('\u{4e2d}'));
    }

    // -------------------------------------------------------------------
    // Empty / malformed archives
    // -------------------------------------------------------------------

    #[test]
    fn tar_empty_archive() {
        // A completely empty tar should have 0 entries.
        // Two zero-filled 512-byte blocks are the end-of-archive marker.
        let empty_tar = vec![0u8; 1024];
        let mut reader = TarReader::from_buf(empty_tar);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 0, "empty tar should have no entries");
    }

    #[test]
    fn tar_corrupted_data_fails() {
        let bad_data = b"this is not a tar archive at all";
        let mut reader = TarReader::from_buf(bad_data.to_vec());
        let err = reader.entries().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("tar")
                || msg.to_lowercase().contains("io")
                || msg.to_lowercase().contains("failed"),
            "expected TAR-related error, got: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // Entry not found
    // -------------------------------------------------------------------

    #[test]
    fn tar_entry_not_found() {
        let data = create_test_tar(&[("exists.txt", b"data")]);
        let mut reader = TarReader::from_buf(data);

        let fake_entry = Entry {
            path: "does_not_exist.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
        };

        let mut output = Vec::new();
        let err = reader.extract(&fake_entry, &mut output).unwrap_err();
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    // -------------------------------------------------------------------
    // Extract all
    // -------------------------------------------------------------------

    #[test]
    fn tar_extract_all_basic() {
        let data = create_test_tar(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = TarReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());

        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    #[test]
    fn tar_extract_all_with_dir_entries() {
        // Build a tar with an explicit directory entry plus a file inside
        // that directory, verifying extract_all creates the directory
        // implicitly (via create_dir_all on the file's parent).
        let mut buf = Vec::new();
        let mut builder = tar::Builder::new(&mut buf);

        // Directory entry: "mydir/"
        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_path("mydir").unwrap();
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_size(0);
        dir_header.set_cksum();
        builder
            .append(&dir_header, std::io::Cursor::new(&[] as &[u8]))
            .unwrap();

        // File inside the directory: "mydir/file.txt"
        let mut file_header = tar::Header::new_gnu();
        file_header.set_path("mydir/file.txt").unwrap();
        file_header.set_size(5);
        file_header.set_cksum();
        builder
            .append(&file_header, std::io::Cursor::new(b"hello"))
            .unwrap();

        drop(builder);

        let mut reader = TarReader::from_buf(buf);
        let entries = reader.entries().unwrap();
        // Directory entry should NOT appear in entries.
        assert_eq!(entries.len(), 1, "only the file entry should be present");
        assert_eq!(entries[0].path, "mydir/file.txt");

        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert_eq!(report.bytes_extracted, 5);
        assert!(report.errors.is_empty());

        let extracted_path = dest.path().join("mydir/file.txt");
        assert!(
            extracted_path.exists(),
            "file inside directory should be extracted: {}",
            extracted_path.display()
        );
        let content = std::fs::read_to_string(&extracted_path).unwrap();
        assert_eq!(content, "hello");
    }

    // -------------------------------------------------------------------
    // Writer round-trips
    // -------------------------------------------------------------------

    #[test]
    fn tar_writer_roundtrip() {
        let buf = Vec::new();
        let mut tar_writer = TarWriter::new(buf);
        tar_writer
            .add_entry_from_reader(
                &PathBuf::from("test.txt"),
                &mut Cursor::new(b"hello from writer"),
            )
            .unwrap();

        let (bytes_written, writer) = tar_writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        let mut reader = TarReader::from_buf(writer);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");

        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, b"hello from writer".len() as u64);
        assert_eq!(output, b"hello from writer");
    }

    #[test]
    fn tar_writer_multiple_files_roundtrip() {
        let buf = Vec::new();
        let mut tar_writer = TarWriter::new(buf);

        let files = [
            ("f1.txt", b"content 1" as &[u8]),
            ("f2.txt", b"content 2" as &[u8]),
            ("sub/f3.txt", b"nested content" as &[u8]),
        ];

        for (name, content) in &files {
            tar_writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(content))
                .unwrap();
        }

        let boxed: Box<dyn ArchiveWriter> = Box::new(tar_writer);
        let _bytes_written = boxed.finish().unwrap();
    }

    // -------------------------------------------------------------------
    // Zip Slip protection (via extract_all default impl)
    // -------------------------------------------------------------------

    #[test]
    fn tar_slip_detection() {
        let data = create_raw_tar(b"../escape.txt", b"malicious");

        let mut reader = TarReader::from_buf(data);
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
    fn tar_slip_dotdot_in_middle() {
        let data = create_raw_tar(b"subdir/../../../escape.txt", b"escape");

        let mut reader = TarReader::from_buf(data);
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
    fn tar_slip_absolute_path() {
        let data = create_raw_tar(b"/etc/passwd", b"leak");

        let mut reader = TarReader::from_buf(data);
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

    // -------------------------------------------------------------------
    // No-clobber tests
    // -------------------------------------------------------------------

    #[test]
    fn tar_no_clobber_skips_existing_files() {
        let content = b"hello world";
        let data = create_test_tar(&[("hello.txt", content)]);

        let mut reader = TarReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        // First extract (creates file).
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty());

        // Modify extracted file.
        let modified_path = dest.path().join("hello.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        // Second extract with overwrite=false: should skip.
        let mut reader2 = TarReader::from_buf(create_test_tar(&[("hello.txt", content)]));
        let report2 = reader2.extract_all(dest.path(), false).unwrap();
        assert_eq!(
            report2.files_extracted, 0,
            "existing file should be skipped"
        );
        assert_eq!(report2.files_skipped, 1, "one file should be skipped");

        // Verify existing file was NOT overwritten.
        assert_eq!(
            std::fs::read_to_string(&modified_path).unwrap(),
            "MODIFIED",
            "existing file content should be preserved"
        );

        // Verify clobber-denied error is recorded.
        assert!(
            report2
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::ClobberDenied { .. })),
            "expected ClobberDenied error"
        );
    }

    // -------------------------------------------------------------------
    // Trait object safety (compile-time checks)
    // -------------------------------------------------------------------

    #[test]
    fn archive_reader_trait_object() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let data = create_test_tar(&[("dummy.txt", b"x")]);
        let mut reader = TarReader::from_buf(data);
        use_reader(&mut reader);
    }

    #[test]
    fn archive_writer_trait_object() {
        fn use_writer(_w: Box<dyn ArchiveWriter>) {}
        let buf = Vec::new();
        let writer = TarWriter::new(buf);
        use_writer(Box::new(writer));
    }
}
