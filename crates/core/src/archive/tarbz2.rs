//! Tar.bz2 (tarball compressed with bzip2) reader and writer.
//!
//! Combines [`tar`] and [`bzip2`] to produce/consume a bzip2-compressed
//! tar archive. The reader is generic over any `Read + Seek + Send`
//! source; the writer is generic over any `Write + Send` sink.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::CountWriter;
use crate::archive::{ArchiveReader, ArchiveWriter, Entry};
use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

fn level_to_compression(level: Option<u32>) -> ::bzip2::Compression {
    match level {
        None | Some(0) => ::bzip2::Compression::default(),
        Some(l @ 1..=9) => ::bzip2::Compression::new(l),
        Some(l) => panic!("expected bzip2 compression level in 0..=9, got {l}"),
    }
}

/// Bzip2-compressed TAR archive reader.
pub struct TarBz2Reader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for TarBz2Reader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarBz2Reader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> TarBz2Reader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        TarBz2Reader {
            inner: reader,
            format: ArchiveFormat::TarBz2,
        }
    }
}

impl TarBz2Reader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        TarBz2Reader::new(std::io::Cursor::new(buf))
    }
}

fn collect_tarbz2_entries<R: Read>(archive: &mut tar::Archive<R>) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive.entries().map_err(convert_tarbz2_error)? {
        let tar_entry = result.map_err(convert_tarbz2_error)?;
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
            .map_err(convert_tarbz2_error)?
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

impl<R: Read + Seek + Send> ArchiveReader for TarBz2Reader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = ::bzip2::read::MultiBzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);
        collect_tarbz2_entries(&mut archive)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = ::bzip2::read::MultiBzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);

        for result in archive.entries().map_err(convert_tarbz2_error)? {
            let mut tar_entry = result.map_err(convert_tarbz2_error)?;
            let path = tar_entry
                .path()
                .map_err(convert_tarbz2_error)?
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

/// Bzip2-compressed TAR archive writer.
pub struct TarBz2Writer<W: Write + Send> {
    inner: Option<tar::Builder<::bzip2::write::BzEncoder<CountWriter<W>>>>,
    format: ArchiveFormat,
}

impl<W: Write + Send> fmt::Debug for TarBz2Writer<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarBz2Writer")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> TarBz2Writer<W> {
    /// Create a new tar.bz2 writer targeting the given output with the
    /// specified bzip2 compression level.
    ///
    /// `level` controls the bzip2 compression strength (0..=9). `None` and
    /// `Some(0)` use the default level because libbz2 has no store-only mode.
    pub fn new_with_level(writer: W, level: Option<u32>) -> Self {
        let compression = level_to_compression(level);
        let counter = CountWriter {
            inner: writer,
            count: 0,
        };
        let encoder = ::bzip2::write::BzEncoder::new(counter, compression);
        TarBz2Writer {
            inner: Some(tar::Builder::new(encoder)),
            format: ArchiveFormat::TarBz2,
        }
    }

    /// Create a new tar.bz2 writer targeting the given output using the
    /// default compression level.
    pub fn new(writer: W) -> Self {
        Self::new_with_level(writer, None)
    }

    /// Create a new tar.bz2 writer with the given compression options.
    ///
    /// Currently only `options.level` is applied; `options.jobs` is accepted
    /// but ignored because the bzip2 crate does not expose a stable
    /// multi-threaded encoder API.
    pub fn new_with_options(writer: W, options: CompressOptions) -> Self {
        Self::new_with_level(writer, options.level)
    }

    /// Finalise the archive and return the inner writer alongside
    /// the total number of bytes written (compressed size).
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let builder = self.inner.take().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer already finalised".into(),
            format: ArchiveFormat::TarBz2,
        })?;
        let encoder = builder
            .into_inner()
            .map_err(|e| GeeZipError::io(e, "finalising TAR stream"))?;
        let count_writer = encoder
            .finish()
            .map_err(|e| GeeZipError::io(e, "finalising bzip2 stream"))?;
        let bytes = count_writer.count;
        let writer = count_writer.inner;
        Ok((bytes, writer))
    }
}

impl<W: Write + Send> ArchiveWriter for TarBz2Writer<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::TarBz2,
        })?;

        let mut data = Vec::new();
        let mut chunk = [0u8; 65536];
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
            format: ArchiveFormat::TarBz2,
        })?;
        header.set_size(data.len() as u64);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarBz2,
        })?;
        builder
            .append(&header, std::io::Cursor::new(data))
            .map_err(convert_tarbz2_error)?;

        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_path(path).map_err(|e| GeeZipError::Format {
            message: format!("setting tar header path: {e}"),
            format: ArchiveFormat::TarBz2,
        })?;
        header.set_size(0);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarBz2,
        })?;
        builder
            .append(&header, std::io::Cursor::new(&[] as &[u8]))
            .map_err(convert_tarbz2_error)?;

        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

fn convert_tarbz2_error(e: std::io::Error) -> GeeZipError {
    GeeZipError::Io {
        source: e,
        context: "tar.bz2 operation failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    fn create_test_tarbz2(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = TarBz2Writer::new(Vec::new());
        for (name, data) in files {
            writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(data))
                .unwrap();
        }
        let (_bytes, data) = writer.finalize().unwrap();
        data
    }

    fn create_raw_tarbz2(path: &[u8], data: &[u8]) -> Vec<u8> {
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

        let mut compressed = Vec::new();
        {
            let mut encoder =
                ::bzip2::write::BzEncoder::new(&mut compressed, ::bzip2::Compression::default());
            encoder.write_all(&raw_tar).unwrap();
            encoder.finish().unwrap();
        }
        compressed
    }

    #[test]
    fn tarbz2_roundtrip_single_file() {
        let content = b"hello world from tar.bz2";
        let data = create_test_tarbz2(&[("hello.txt", content)]);

        let mut reader = TarBz2Reader::from_buf(data);
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
    fn tarbz2_roundtrip_nested_path() {
        let content = b"nested tar.bz2 content";
        let data = create_test_tarbz2(&[("dir/subdir/file.txt", content)]);

        let mut reader = TarBz2Reader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "dir/subdir/file.txt");

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(bytes, content.len() as u64);
        assert_eq!(output, content);
    }

    #[test]
    fn tarbz2_unicode_filename() {
        let content = b"unicode tar.bz2 content";
        let data = create_test_tarbz2(&[("中文.txt", content)]);

        let mut reader = TarBz2Reader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "中文.txt");
    }

    #[test]
    fn tarbz2_empty_archive() {
        let data = create_test_tarbz2(&[] as &[(&str, &[u8])]);
        let mut reader = TarBz2Reader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 0, "empty tar.bz2 should have no entries");
    }

    #[test]
    fn tarbz2_corrupted_data_fails() {
        let bad_data = b"this is not a tar.bz2 archive at all";
        let mut reader = TarBz2Reader::from_buf(bad_data.to_vec());
        let err = reader.entries().unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("tar")
                || msg.contains("bz")
                || msg.contains("bzip2")
                || msg.contains("io"),
            "expected tar/bz/io error, got: {msg}"
        );
    }

    #[test]
    fn tarbz2_extract_all_basic() {
        let data = create_test_tarbz2(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = TarBz2Reader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());
        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    #[test]
    fn tarbz2_slip_detection() {
        let data = create_raw_tarbz2(b"../escape.txt", b"malicious");
        let mut reader = TarBz2Reader::from_buf(data);
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
    fn tarbz2_slip_absolute_path() {
        let data = create_raw_tarbz2(b"/etc/passwd", b"leak");
        let mut reader = TarBz2Reader::from_buf(data);
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
    fn tarbz2_no_clobber_skips_existing_files() {
        let content = b"hello world";
        let data = create_test_tarbz2(&[("hello.txt", content)]);

        let mut reader = TarBz2Reader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty());

        let modified_path = dest.path().join("hello.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        let mut reader2 = TarBz2Reader::from_buf(create_test_tarbz2(&[("hello.txt", content)]));
        let report2 = reader2.extract_all(dest.path(), false).unwrap();
        assert_eq!(
            report2.files_extracted, 0,
            "existing file should be skipped"
        );
        assert_eq!(report2.files_skipped, 1, "one file should be skipped");
        assert_eq!(std::fs::read_to_string(&modified_path).unwrap(), "MODIFIED");
        assert!(
            report2
                .errors
                .iter()
                .any(|(_, e)| matches!(e, GeeZipError::ClobberDenied { .. })),
            "expected ClobberDenied error"
        );
    }
}
