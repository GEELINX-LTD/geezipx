//! Tar.xz (tarball compressed with XZ) reader and writer.
//!
//! Combines [`tar`] and [`xz2`] to produce/consume an xz-compressed
//! tar archive.  The reader is generic over any `Read + Seek + Send`
//! source; the writer is generic over any `Write + Send` sink.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::CountWriter;
use crate::archive::{ArchiveReader, ArchiveWriter, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// TarXzReader
// ---------------------------------------------------------------------------

/// XZ-compressed TAR archive reader.
///
/// Generic over any `R: Read + Seek + Send` (file, cursor, etc.).
/// Each access to the archive resets the reader to the start of the
/// stream and re-creates the xz decoder, so a `Seek` source is
/// required.
pub struct TarXzReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for TarXzReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarXzReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> TarXzReader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    ///
    /// The input is **not** buffered into memory; the source is wrapped
    /// in a new xz decoder on each pass.
    pub fn new(reader: R) -> Self {
        TarXzReader {
            inner: reader,
            format: ArchiveFormat::TarXz,
        }
    }
}

impl TarXzReader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        TarXzReader::new(std::io::Cursor::new(buf))
    }
}

/// Collect tar entries from an xz-decoded stream.
fn collect_tarxz_entries<R: Read>(archive: &mut tar::Archive<R>) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive.entries().map_err(convert_tarxz_error)? {
        let tar_entry = result.map_err(convert_tarxz_error)?;
        let header = tar_entry.header();

        let entry_type = header.entry_type();
        if matches!(
            entry_type,
            tar::EntryType::XGlobalHeader
                | tar::EntryType::GNULongLink
                | tar::EntryType::GNULongName
        ) {
            continue;
        }

        let path = tar_entry
            .path()
            .map_err(convert_tarxz_error)?
            .to_string_lossy()
            .into_owned();
        let size = tar_entry.size();
        let is_dir = header.entry_type().is_dir();

        entries.push(Entry {
            path,
            size,
            compressed_size: 0,
            crc32: None,
            modified: header.mtime().ok(),
            is_dir,
        });
    }

    Ok(entries)
}

impl<R: Read + Seek + Send> ArchiveReader for TarXzReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = xz2::read::XzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);
        collect_tarxz_entries(&mut archive)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = xz2::read::XzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);

        for result in archive.entries().map_err(convert_tarxz_error)? {
            let mut tar_entry = result.map_err(convert_tarxz_error)?;
            let path = tar_entry
                .path()
                .map_err(convert_tarxz_error)?
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
// TarXzWriter
// ---------------------------------------------------------------------------

/// XZ-compressed TAR archive writer.
///
/// Generic over any `W: Write + Send`.  Data is first tar-ed, then
/// xz-compressed, and finally written to the underlying writer.
/// Construct via [`TarXzWriter::new`], add entries with
/// [`add_entry_from_reader`](ArchiveWriter::add_entry_from_reader),
/// then finalise with either:
///
/// - [`TarXzWriter::finalize`] — returns `(total_bytes, inner_writer)`
/// - [`ArchiveWriter::finish`] — returns `total_bytes` (trait object-safe)
pub struct TarXzWriter<W: Write + Send> {
    // Chained: tar::Builder writes into XzEncoder, which writes into
    // CountWriter, which writes into the user-supplied writer.
    inner: Option<tar::Builder<xz2::write::XzEncoder<CountWriter<W>>>>,
    format: ArchiveFormat,
}

impl<W: Write + Send> fmt::Debug for TarXzWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarXzWriter")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> TarXzWriter<W> {
    /// Create a new tar.xz writer targeting the given output with the
    /// specified xz compression level.
    ///
    /// `level` controls the xz compression strength (0..=9). `None` uses the
    /// default level (6).
    pub fn new_with_level(writer: W, level: Option<u32>) -> Self {
        let lvl = level.unwrap_or(6);
        let counter = CountWriter {
            inner: writer,
            count: 0,
        };
        let encoder = xz2::write::XzEncoder::new(counter, lvl);
        TarXzWriter {
            inner: Some(tar::Builder::new(encoder)),
            format: ArchiveFormat::TarXz,
        }
    }

    /// Create a new tar.xz writer targeting the given output using the
    /// default compression level.
    ///
    /// This is a convenience wrapper around [`Self::new_with_level`] with
    /// `level: None`.
    pub fn new(writer: W) -> Self {
        Self::new_with_level(writer, None)
    }

    /// Finalise the archive and return the inner writer alongside
    /// the total number of bytes written (compressed size).
    ///
    /// This is the "rich" version of [`ArchiveWriter::finish`] that lets
    /// callers recover the underlying writer.
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let builder = self.inner.take().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer already finalised".into(),
            format: ArchiveFormat::TarXz,
        })?;
        let encoder = builder
            .into_inner()
            .map_err(|e| GeeZipError::io(e, "finalising TAR stream"))?;
        let count_writer = encoder
            .finish()
            .map_err(|e| GeeZipError::io(e, "finalising XZ stream"))?;
        let bytes = count_writer.count;
        let writer = count_writer.inner;
        Ok((bytes, writer))
    }
}

impl<W: Write + Send> ArchiveWriter for TarXzWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::TarXz,
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
            format: ArchiveFormat::TarXz,
        })?;
        header.set_size(data.len() as u64);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarXz,
        })?;
        builder
            .append(&header, std::io::Cursor::new(data))
            .map_err(convert_tarxz_error)?;

        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_path(path).map_err(|e| GeeZipError::Format {
            message: format!("setting tar header path: {e}"),
            format: ArchiveFormat::TarXz,
        })?;
        header.set_size(0);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarXz,
        })?;
        builder
            .append(&header, std::io::Cursor::new(&[] as &[u8]))
            .map_err(convert_tarxz_error)?;

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

fn convert_tarxz_error(e: std::io::Error) -> GeeZipError {
    GeeZipError::Io {
        source: e,
        context: "tar.xz operation failed".into(),
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

    /// Create a minimal valid tar.xz archive in memory.
    fn create_test_tarxz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = TarXzWriter::new(Vec::new());
        for (name, data) in files {
            writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(data))
                .unwrap();
        }
        let (_bytes, data) = writer.finalize().unwrap();
        data
    }

    /// Create a raw tar.xz archive where the tar entry has `path`
    /// (allowing malicious paths that `tar::Header::set_path` rejects).
    /// We construct the tar bytes manually, then xz-compress them.
    fn create_raw_tarxz(path: &[u8], data: &[u8]) -> Vec<u8> {
        // Build the raw tar archive with one entry.
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

        let mut raw_tar = header.to_vec();
        if !data.is_empty() {
            raw_tar.extend_from_slice(data);
            let padding = (512 - data.len() % 512) % 512;
            raw_tar.extend(std::iter::repeat_n(0, padding));
        }
        raw_tar.extend_from_slice(&[0u8; 1024]);

        // XZ compress it
        let mut compressed = Vec::new();
        {
            let mut encoder = xz2::write::XzEncoder::new(&mut compressed, 6);
            encoder.write_all(&raw_tar).unwrap();
            encoder.finish().unwrap();
        }
        compressed
    }

    // -------------------------------------------------------------------
    // Round-trip: write to Vec -> read back
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_roundtrip_single_file() {
        let content = b"hello world from tar.xz";
        let data = create_test_tarxz(&[("hello.txt", content)]);

        let mut reader = TarXzReader::from_buf(data);
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
    fn tarxz_roundtrip_multiple_files() {
        let files = [
            ("a.txt", b"aaa" as &[u8]),
            ("b.txt", b"bbb" as &[u8]),
            ("c.txt", b"ccc" as &[u8]),
        ];
        let data = create_test_tarxz(&files);

        let mut reader = TarXzReader::from_buf(data);
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
    fn tarxz_roundtrip_nested_path() {
        let content = b"nested content";
        let data = create_test_tarxz(&[("dir/subdir/file.txt", content)]);

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "dir/subdir/file.txt");

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, content);
        assert_eq!(bytes, content.len() as u64);
    }

    #[test]
    fn tarxz_unicode_filename() {
        let content = b"unicode content";
        let data = create_test_tarxz(&[("\u{4e2d}\u{6587}.txt", content)]); // 中文.txt

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.contains('\u{4e2d}'));
    }

    // -------------------------------------------------------------------
    // Empty / malformed archives
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_empty_archive() {
        let data = create_test_tarxz(&[] as &[(&str, &[u8])]);

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 0, "empty tar.xz should have no entries");
    }

    #[test]
    fn tarxz_corrupted_data_fails() {
        let bad_data = b"this is not a tar.xz archive at all";
        let mut reader = TarXzReader::from_buf(bad_data.to_vec());
        let err = reader.entries().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("tar")
                || msg.to_lowercase().contains("xz")
                || msg.to_lowercase().contains("io")
                || msg.to_lowercase().contains("failed"),
            "expected tar/xz/io error, got: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // Extract all
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_extract_all_basic() {
        let data = create_test_tarxz(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = TarXzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());

        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    // -------------------------------------------------------------------
    // Path traversal protection
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_slip_detection() {
        let data = create_raw_tarxz(b"../escape.txt", b"malicious");

        let mut reader = TarXzReader::from_buf(data);
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
    fn tarxz_slip_dotdot_in_middle() {
        let data = create_raw_tarxz(b"subdir/../../../escape.txt", b"escape");
        let mut reader = TarXzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert!(
            report
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::PathTraversal { .. })),
            "expected PathTraversal for subdir/../../../escape.txt, got: {report:?}"
        );
        assert_eq!(report.files_extracted, 0);
    }

    #[test]
    fn tarxz_slip_absolute_path() {
        let data = create_raw_tarxz(b"/etc/passwd", b"leak");

        let mut reader = TarXzReader::from_buf(data);
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
    fn tarxz_extract_all_with_dir_entries() {
        // Build a tar.xz with an explicit directory entry plus a file inside
        // that directory, verifying extract_all creates the directory
        // implicitly (via create_dir_all on the file's parent).
        let mut buf = Vec::new();
        {
            let mut encoder = xz2::write::XzEncoder::new(&mut buf, 6);
            let mut builder = tar::Builder::new(&mut encoder);

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

            builder.finish().unwrap();
            drop(builder);
            encoder.finish().unwrap();
        }

        let mut reader = TarXzReader::from_buf(buf);
        let entries = reader.entries().unwrap();
        assert_eq!(
            entries.len(),
            2,
            "both directory and file entries should be present"
        );
        assert!(
            entries.iter().any(|e| e.is_dir && e.path == "mydir"),
            "expected directory entry 'mydir'"
        );
        assert!(
            entries
                .iter()
                .any(|e| !e.is_dir && e.path == "mydir/file.txt"),
            "expected file entry 'mydir/file.txt'"
        );

        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
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
    // No-clobber tests
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_no_clobber_skips_existing_files() {
        let content = b"hello world";
        let data = create_test_tarxz(&[("hello.txt", content)]);

        let mut reader = TarXzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        // First extract (creates file).
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty());

        // Modify extracted file.
        let modified_path = dest.path().join("hello.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        // Second extract with overwrite=false: should skip.
        let mut reader2 = TarXzReader::from_buf(create_test_tarxz(&[("hello.txt", content)]));
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
    // Writer round-trip
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_writer_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new(buf);
        writer
            .add_entry_from_reader(&PathBuf::from("test.txt"), &mut Cursor::new(b"hello"))
            .unwrap();

        let (bytes_written, data) = writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");

        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, 5);
        assert_eq!(output, b"hello");
    }

    #[test]
    fn tarxz_writer_multiple_files_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new(buf);

        let files = [
            ("f1.txt", b"content 1" as &[u8]),
            ("f2.txt", b"content 2" as &[u8]),
            ("sub/f3.txt", b"nested content" as &[u8]),
        ];

        for (name, content) in &files {
            writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(content))
                .unwrap();
        }

        let boxed: Box<dyn ArchiveWriter> = Box::new(writer);
        let _bytes_written = boxed.finish().unwrap();
    }

    #[test]
    fn tarxz_writer_add_directory_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new(buf);

        // Add a regular file.
        writer
            .add_entry_from_reader(
                &PathBuf::from("file.txt"),
                &mut Cursor::new(b"hello from file"),
            )
            .unwrap();

        // Add an empty directory.
        writer.add_directory(Path::new("emptydir")).unwrap();

        let (bytes_written, data) = writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 2, "should have file + directory entries");

        // Verify the directory entry.
        let dir_entry = entries.iter().find(|e| e.is_dir).expect("directory entry");
        assert_eq!(dir_entry.path, "emptydir");
        assert!(dir_entry.is_dir);

        // Verify the file entry.
        let file_entry = entries.iter().find(|e| !e.is_dir).expect("file entry");
        assert_eq!(file_entry.path, "file.txt");
        assert!(!file_entry.is_dir);

        // Extract all to a tempdir.
        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert!(report.errors.is_empty(), "extract_all errors: {report:?}");

        // Verify directory exists.
        assert!(dest.path().join("emptydir").is_dir());

        // Verify file content.
        let file_content = std::fs::read_to_string(dest.path().join("file.txt")).unwrap();
        assert_eq!(file_content, "hello from file");
    }

    // -------------------------------------------------------------------
    // Trait object safety (compile-time checks)
    // -------------------------------------------------------------------

    #[test]
    fn archive_reader_trait_object() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let data = create_test_tarxz(&[("dummy.txt", b"x")]);
        let mut reader = TarXzReader::from_buf(data);
        use_reader(&mut reader);
    }

    #[test]
    fn archive_writer_trait_object() {
        fn use_writer(_w: Box<dyn ArchiveWriter>) {}
        let buf = Vec::new();
        let writer = TarXzWriter::new(buf);
        use_writer(Box::new(writer));
    }

    #[test]
    fn tarxz_reader_format() {
        let data = create_test_tarxz(&[("dummy.txt", b"x")]);
        let reader = TarXzReader::from_buf(data);
        assert_eq!(reader.format(), ArchiveFormat::TarXz);
    }

    #[test]
    fn tarxz_writer_format() {
        let buf = Vec::new();
        let writer = TarXzWriter::new(buf);
        assert_eq!(writer.format(), ArchiveFormat::TarXz);
    }

    // -------------------------------------------------------------------
    // Level tests
    // -------------------------------------------------------------------

    #[test]
    fn tarxz_level_1() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new_with_level(buf, Some(1));
        writer
            .add_entry_from_reader(
                &PathBuf::from("test.txt"),
                &mut Cursor::new(b"hello level 1"),
            )
            .unwrap();
        let (_bytes, data) = writer.finalize().unwrap();
        assert!(!data.is_empty());

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        let mut output = Vec::new();
        reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, b"hello level 1");
    }

    #[test]
    fn tarxz_level_9() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new_with_level(buf, Some(9));
        writer
            .add_entry_from_reader(
                &PathBuf::from("test.txt"),
                &mut Cursor::new(b"hello level 9"),
            )
            .unwrap();
        let (_bytes, data) = writer.finalize().unwrap();
        assert!(!data.is_empty());

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        let mut output = Vec::new();
        reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, b"hello level 9");
    }

    #[test]
    fn tarxz_level_0() {
        let buf = Vec::new();
        let mut writer = TarXzWriter::new_with_level(buf, Some(0));
        writer
            .add_entry_from_reader(
                &PathBuf::from("test.txt"),
                &mut Cursor::new(b"hello level 0"),
            )
            .unwrap();
        let (_bytes, data) = writer.finalize().unwrap();
        assert!(!data.is_empty());

        let mut reader = TarXzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        let mut output = Vec::new();
        reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, b"hello level 0");
    }
}

#[test]
fn tarxz_truncated_not_panic() {
    // Only valid XZ magic bytes (FD 37 7A 58 5A 00), no tar data at all.
    // Must NOT panic; should return a proper error.
    let truncated = vec![0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00];
    let mut reader = TarXzReader::from_buf(truncated);
    let err = reader.entries().unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("tar") || msg.contains("xz") || msg.contains("io") || msg.contains("failed"),
        "expected tar/xz/io error for truncated tarxz, got: {err}"
    );
}
