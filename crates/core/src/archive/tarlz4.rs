//! Tar.lz4 (tarball compressed with an LZ4 frame) reader and writer.
//!
//! Combines [`tar`] with the standard LZ4 frame format from [`lz4_flex`].

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::CountWriter;
use crate::archive::{ArchiveReader, ArchiveWriter, Entry};
use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

fn validate_level(level: Option<u32>) -> GeeZipResult<()> {
    match level {
        None | Some(0) => Ok(()),
        Some(l) => Err(GeeZipError::format(
            format!(
                "tar.lz4 compression level is not configurable in the current encoder; use 0 or omit the level (got {l})"
            ),
            ArchiveFormat::TarLz4,
        )),
    }
}

/// LZ4-frame-compressed TAR archive reader.
pub struct TarLz4Reader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for TarLz4Reader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarLz4Reader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> TarLz4Reader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        TarLz4Reader {
            inner: reader,
            format: ArchiveFormat::TarLz4,
        }
    }
}

impl TarLz4Reader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        TarLz4Reader::new(std::io::Cursor::new(buf))
    }
}

fn collect_tarlz4_entries<R: Read>(archive: &mut tar::Archive<R>) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive.entries().map_err(convert_tarlz4_error)? {
        let tar_entry = result.map_err(convert_tarlz4_error)?;
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
            .map_err(convert_tarlz4_error)?
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

impl<R: Read + Seek + Send> ArchiveReader for TarLz4Reader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = lz4_flex::frame::FrameDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);
        collect_tarlz4_entries(&mut archive)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = lz4_flex::frame::FrameDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);

        for result in archive.entries().map_err(convert_tarlz4_error)? {
            let mut tar_entry = result.map_err(convert_tarlz4_error)?;
            let path = tar_entry
                .path()
                .map_err(convert_tarlz4_error)?
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

/// LZ4-frame-compressed TAR archive writer.
pub struct TarLz4Writer<W: Write + Send> {
    inner: Option<tar::Builder<lz4_flex::frame::FrameEncoder<CountWriter<W>>>>,
    format: ArchiveFormat,
}

impl<W: Write + Send> fmt::Debug for TarLz4Writer<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarLz4Writer")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> TarLz4Writer<W> {
    /// Create a new tar.lz4 writer targeting the given output with the
    /// selected frame settings.
    pub fn new_with_level(writer: W, level: Option<u32>) -> GeeZipResult<Self> {
        validate_level(level)?;
        let counter = CountWriter {
            inner: writer,
            count: 0,
        };
        let encoder = lz4_flex::frame::FrameEncoder::new(counter);
        Ok(TarLz4Writer {
            inner: Some(tar::Builder::new(encoder)),
            format: ArchiveFormat::TarLz4,
        })
    }

    /// Create a new tar.lz4 writer targeting the given output using the
    /// default frame settings.
    pub fn new(writer: W) -> GeeZipResult<Self> {
        Self::new_with_level(writer, None)
    }

    /// Create a new tar.lz4 writer with the given compression options.
    ///
    /// Currently only `options.level` is validated; `options.jobs` is accepted
    /// but ignored because the selected LZ4 frame encoder is single-threaded.
    pub fn new_with_options(writer: W, options: CompressOptions) -> GeeZipResult<Self> {
        Self::new_with_level(writer, options.level)
    }

    /// Finalise the archive and return the inner writer alongside the total
    /// number of bytes written (compressed size).
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let builder = self.inner.take().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer already finalised".into(),
            format: ArchiveFormat::TarLz4,
        })?;
        let encoder = builder
            .into_inner()
            .map_err(|e| GeeZipError::io(e, "finalising TAR stream"))?;
        let count_writer = encoder
            .finish()
            .map_err(|e| GeeZipError::io(e.into(), "finalising lz4 frame"))?;
        let bytes = count_writer.count;
        let writer = count_writer.inner;
        Ok((bytes, writer))
    }
}

impl<W: Write + Send> ArchiveWriter for TarLz4Writer<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::TarLz4,
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
            format: ArchiveFormat::TarLz4,
        })?;
        header.set_size(data.len() as u64);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarLz4,
        })?;
        builder
            .append(&header, std::io::Cursor::new(data))
            .map_err(convert_tarlz4_error)?;

        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_path(path).map_err(|e| GeeZipError::Format {
            message: format!("setting tar header path: {e}"),
            format: ArchiveFormat::TarLz4,
        })?;
        header.set_size(0);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarLz4,
        })?;
        builder
            .append(&header, std::io::Cursor::new(&[] as &[u8]))
            .map_err(convert_tarlz4_error)?;

        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

fn convert_tarlz4_error(e: std::io::Error) -> GeeZipError {
    GeeZipError::Io {
        source: e,
        context: "tar.lz4 operation failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    fn create_test_tarlz4(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = TarLz4Writer::new(Vec::new()).unwrap();
        for (name, data) in files {
            writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(data))
                .unwrap();
        }
        let (_bytes, data) = writer.finalize().unwrap();
        data
    }

    fn create_raw_tarlz4(path: &[u8], data: &[u8]) -> Vec<u8> {
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
            let mut encoder = lz4_flex::frame::FrameEncoder::new(&mut compressed);
            encoder.write_all(&raw_tar).unwrap();
            encoder.finish().unwrap();
        }
        compressed
    }

    #[test]
    fn tarlz4_roundtrip_single_file() {
        let content = b"hello world from tar.lz4";
        let data = create_test_tarlz4(&[("hello.txt", content)]);

        let mut reader = TarLz4Reader::from_buf(data);
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
    fn tarlz4_roundtrip_nested_and_unicode_paths() {
        let content_a = b"nested tar.lz4 content";
        let content_b = b"unicode tar.lz4 content";
        let data =
            create_test_tarlz4(&[("dir/subdir/file.txt", content_a), ("中文.txt", content_b)]);

        let mut reader = TarLz4Reader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.path == "dir/subdir/file.txt"));
        assert!(entries.iter().any(|e| e.path == "中文.txt"));
    }

    #[test]
    fn tarlz4_empty_archive() {
        let data = create_test_tarlz4(&[] as &[(&str, &[u8])]);
        let mut reader = TarLz4Reader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 0, "empty tar.lz4 should have no entries");
    }

    #[test]
    fn tarlz4_corrupted_data_fails() {
        let bad_data = b"this is not a tar.lz4 archive at all";
        let mut reader = TarLz4Reader::from_buf(bad_data.to_vec());
        let err = reader.entries().unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("tar")
                || msg.contains("lz4")
                || msg.contains("frame")
                || msg.contains("io"),
            "expected tar/lz4/io error, got: {msg}"
        );
    }

    #[test]
    fn tarlz4_extract_all_basic() {
        let data = create_test_tarlz4(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = TarLz4Reader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());
        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    #[test]
    fn tarlz4_slip_detection() {
        let data = create_raw_tarlz4(b"../escape.txt", b"malicious");
        let mut reader = TarLz4Reader::from_buf(data);
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
    fn tarlz4_slip_absolute_path() {
        let data = create_raw_tarlz4(b"/etc/passwd", b"leak");
        let mut reader = TarLz4Reader::from_buf(data);
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
    fn tarlz4_no_clobber_skips_existing_files() {
        let content = b"hello world";
        let data = create_test_tarlz4(&[("hello.txt", content)]);

        let mut reader = TarLz4Reader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty());

        let modified_path = dest.path().join("hello.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        let mut reader2 = TarLz4Reader::from_buf(create_test_tarlz4(&[("hello.txt", content)]));
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

    #[test]
    fn tarlz4_invalid_level_returns_error() {
        let err = TarLz4Writer::new_with_level(Vec::new(), Some(1)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("use 0 or omit"), "unexpected message: {msg}");
    }

    #[test]
    fn tarlz4_writer_reports_format() {
        let writer = TarLz4Writer::new(Vec::new()).unwrap();
        assert_eq!(writer.format(), ArchiveFormat::TarLz4);
    }
}
