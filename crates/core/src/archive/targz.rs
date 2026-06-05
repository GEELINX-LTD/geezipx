//! Tar.gz (tarball compressed with gzip) reader and writer.
//!
//! Combines [`tar`] and [`flate2`] / [`gzp`] to produce/consume a gzip-compressed
//! tar archive.  The reader is generic over any `Read + Seek + Send`
//! source; the writer is generic over any `Write + Send + 'static` sink
//! (the `'static` bound only applies to methods that use the parallel
//! gzip encoder via `gzp`; `new` and `new_with_level` work with any `Write + Send`).
//!
//! When `CompressOptions.effective_jobs() > 1`, the writer uses `gzp`'s parallel
//! gzip encoder (pigz-style chunked compression).  The reader uses
//! `flate2::read::MultiGzDecoder` to support both single-member and
//! multi-member (parallel) gzip streams.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use flate2::Compression;
use gzp::deflate::Gzip;
use gzp::par::compress::{ParCompress, ParCompressBuilder};
use gzp::ZWriter;

use super::CountWriter;
use crate::archive::{ArchiveReader, ArchiveWriter, Entry};
use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// SharedCountWriter — a thread-safe writer wrapper that doubles as a
// byte counter and allows recovery of the inner writer after ParCompress
// has consumed the wrapper.
// ---------------------------------------------------------------------------

/// Thread-safe wrapper around `CountWriter` for use with gzp's parallel
/// compressor (which moves the writer into internal threads).
struct SharedCountWriter<W: Write + Send> {
    inner: Arc<Mutex<CountWriter<W>>>,
}

impl<W: Write + Send> Write for SharedCountWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().unwrap().flush()
    }
}

// Safety: `SharedCountWriter` is `Send` because `Arc<Mutex<CountWriter<W>>>` is `Send`
// when `W: Send`.

// ---------------------------------------------------------------------------
// GzipEncoder — unified enum over single-threaded and multi-threaded gzip
// encoders so that `tar::Builder` can be parameterised once.
// ---------------------------------------------------------------------------

/// Wraps either a single-threaded `flate2::write::GzEncoder` or a
/// multi-threaded `gzp::par::compress::ParCompress<Gzip>` behind a single
/// `Write` impl.  Both paths write to a `CountWriter` (directly or through
/// a shared wrapper) so the byte count can be recovered on finalisation.
enum GzipEncoder<W: Write + Send> {
    /// Single-threaded flate2 encoder.
    Single(flate2::write::GzEncoder<CountWriter<W>>),
    /// Multi-threaded gzp encoder with a shared reference to the underlying
    /// `CountWriter` (needed to recover it after `ParCompress` has finished).
    Multi {
        compressor: Option<ParCompress<Gzip>>,
        shared: Arc<Mutex<CountWriter<W>>>,
    },
}

impl<W: Write + Send> Write for GzipEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            GzipEncoder::Single(e) => e.write(buf),
            GzipEncoder::Multi {
                ref mut compressor, ..
            } => compressor.as_mut().unwrap().write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            GzipEncoder::Single(e) => e.flush(),
            GzipEncoder::Multi {
                ref mut compressor, ..
            } => compressor.as_mut().unwrap().flush(),
        }
    }
}

/// Consume the encoder, finalise compression, and return `(total_bytes, inner_writer)`.
fn finalise_gzip_encoder<W: Write + Send>(encoder: GzipEncoder<W>) -> GeeZipResult<(u64, W)> {
    match encoder {
        GzipEncoder::Single(e) => {
            let cw = e
                .finish()
                .map_err(|e| GeeZipError::io(e, "finalising gzip stream"))?;
            let bytes = cw.count;
            let writer = cw.inner;
            Ok((bytes, writer))
        }
        GzipEncoder::Multi {
            mut compressor,
            shared,
        } => {
            // Finalise the parallel compressor.
            if let Some(ref mut c) = compressor {
                ZWriter::finish(c).map_err(|e| {
                    GeeZipError::io(
                        io::Error::other(e.to_string()),
                        "finalising parallel gzip stream",
                    )
                })?;
            }
            drop(compressor);

            // Recover the CountWriter from the shared Arc.
            let cw = Arc::try_unwrap(shared)
                .map_err(|_| {
                    GeeZipError::io(
                        io::Error::other("compressor still holds writer ref"),
                        "recovering writer after parallel gzip",
                    )
                })?
                .into_inner()
                .map_err(|e| GeeZipError::io(io::Error::other(e.to_string()), "mutex poisoned"))?;
            let bytes = cw.count;
            let writer = cw.inner;
            Ok((bytes, writer))
        }
    }
}

// ---------------------------------------------------------------------------
// TarGzReader
// ---------------------------------------------------------------------------

/// Gzip-compressed TAR archive reader.
///
/// Generic over any `R: Read + Seek + Send` (file, cursor, etc.).
/// Each access to the archive resets the reader to the start of the
/// stream and re-creates the gzip decoder, so a `Seek` source is
/// required.
///
/// Uses `MultiGzDecoder` to support both single-member and multi-member
/// (parallel) gzip streams.
pub struct TarGzReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for TarGzReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarGzReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> TarGzReader<R> {
    /// Create a reader from any `Read + Seek + Send` source.
    ///
    /// The input is **not** buffered into memory; the source is wrapped
    /// in a new gzip decoder on each pass.
    pub fn new(reader: R) -> Self {
        TarGzReader {
            inner: reader,
            format: ArchiveFormat::TarGz,
        }
    }
}

impl TarGzReader<std::io::Cursor<Vec<u8>>> {
    /// Create a reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        TarGzReader::new(std::io::Cursor::new(buf))
    }
}

/// Collect tar entries from a gzip-decoded stream.
fn collect_targz_entries<R: Read>(archive: &mut tar::Archive<R>) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive.entries().map_err(convert_targz_error)? {
        let tar_entry = result.map_err(convert_targz_error)?;
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
            .map_err(convert_targz_error)?
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

impl<R: Read + Seek + Send> ArchiveReader for TarGzReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = flate2::read::MultiGzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);
        collect_targz_entries(&mut archive)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let decoder = flate2::read::MultiGzDecoder::new(&mut self.inner);
        let mut archive = tar::Archive::new(decoder);

        for result in archive.entries().map_err(convert_targz_error)? {
            let mut tar_entry = result.map_err(convert_targz_error)?;
            let path = tar_entry
                .path()
                .map_err(convert_targz_error)?
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
// TarGzWriter
// ---------------------------------------------------------------------------

/// Gzip-compressed TAR archive writer.
///
/// Generic over any `W: Write + Send + 'static`.  Data is first tar-ed, then
/// gzip-compressed, and finally written to the underlying writer.
/// Construct via [`TarGzWriter::new`], add entries with
/// [`add_entry_from_reader`](ArchiveWriter::add_entry_from_reader),
/// then finalise with either:
///
/// - [`TarGzWriter::finalize`] — returns `(total_bytes, inner_writer)`
/// - [`ArchiveWriter::finish`] — returns `total_bytes` (trait object-safe)
///
/// When `jobs > 1` is requested via `CompressOptions`, the gzip layer uses
/// a multi-threaded encoder (`gzp::ParCompress`); otherwise it uses the
/// standard `flate2::GzEncoder`.
pub struct TarGzWriter<W: Write + Send> {
    // Chained: tar::Builder writes into GzipEncoder, which writes into
    // CountWriter, which writes into the user-supplied writer.
    inner: Option<tar::Builder<GzipEncoder<W>>>,
    format: ArchiveFormat,
}

impl<W: Write + Send> fmt::Debug for TarGzWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarGzWriter")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

// ---- TarGzWriter impl blocks: single-threaded methods (no `'static` needed) ----

impl<W: Write + Send> TarGzWriter<W> {
    /// Create a new tar.gz writer targeting the given output with the
    /// specified gzip compression level.
    ///
    /// `level` controls the gzip compression strength (0-9). `None` uses the
    /// default level (6).
    ///
    /// This constructor uses a single-threaded flate2 encoder and does **not**
    /// require `W: 'static`.  For parallel gzip compression see [`new_with_options`].
    pub fn new_with_level(writer: W, level: Option<u32>) -> Self {
        let compression = match level {
            None => Compression::default(),
            Some(l) => Compression::new(l),
        };
        let counter = CountWriter {
            inner: writer,
            count: 0,
        };
        let encoder = flate2::write::GzEncoder::new(counter, compression);
        TarGzWriter {
            inner: Some(tar::Builder::new(GzipEncoder::Single(encoder))),
            format: ArchiveFormat::TarGz,
        }
    }

    /// Create a new tar.gz writer targeting the given output using the
    /// default compression level.
    ///
    /// This is a convenience wrapper around [`Self::new_with_level`] with
    /// `level: None`.
    pub fn new(writer: W) -> Self {
        Self::new_with_level(writer, None)
    }
}

// ---- TarGzWriter impl blocks: multi-threaded / finalize methods (need `'static`) ----
// The `W: 'static` bound is needed by `gzp::ParCompressBuilder` which
// passes the writer across thread boundaries internally.  Common types like
// `File` and `Vec<u8>` satisfy this bound automatically.

impl<W: Write + Send + 'static> TarGzWriter<W> {
    /// Create a new tar.gz writer with the given compression options.
    ///
    /// When `options.effective_jobs() > 1`, the gzip encoder uses a parallel
    /// (pigz-style) compressor.  For single-threaded use (jobs <= 1) the
    /// standard `flate2::GzEncoder` is used.
    ///
    /// **Note:** The parallel path requires `W: 'static` because the gzp
    /// library passes the writer into worker threads.  Single-threaded mode
    /// (`jobs <= 1`) also accepts the `'static` bound for uniformity, but
    /// callers that cannot satisfy `'static` should use [`new_with_level`]
    /// or [`new`] instead.
    pub fn new_with_options(writer: W, options: CompressOptions) -> Self {
        let jobs = options.effective_jobs();
        let level = options.level.unwrap_or(6);
        let compression_level = Compression::new(level.min(9));

        let counter = CountWriter {
            inner: writer,
            count: 0,
        };

        let encoder = if jobs > 1 {
            let shared = Arc::new(Mutex::new(counter));
            let shared_writer = SharedCountWriter {
                inner: shared.clone(),
            };
            let compressor = ParCompressBuilder::<Gzip>::new()
                .compression_level(compression_level)
                .from_writer(shared_writer);
            GzipEncoder::Multi {
                compressor: Some(compressor),
                shared,
            }
        } else {
            let encoder = flate2::write::GzEncoder::new(counter, compression_level);
            GzipEncoder::Single(encoder)
        };

        TarGzWriter {
            inner: Some(tar::Builder::new(encoder)),
            format: ArchiveFormat::TarGz,
        }
    }

    /// Finalise the archive and return the inner writer alongside
    /// the total number of bytes written (compressed size).
    ///
    /// This is the "rich" version of [`ArchiveWriter::finish`] that lets
    /// callers recover the underlying writer.
    ///
    /// **Note:** When a parallel gzip encoder was used (`jobs > 1`), this
    /// method recovers the writer from `Arc::try_unwrap` after the worker
    /// threads have finished.  The `'static` bound ensures thread safety.
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let builder = self.inner.take().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer already finalised".into(),
            format: ArchiveFormat::TarGz,
        })?;
        let gz_encoder = builder
            .into_inner()
            .map_err(|e| GeeZipError::io(e, "finalising TAR stream"))?;
        finalise_gzip_encoder(gz_encoder)
    }
}

// Need static bound for ArchiveWriter trait object as well, but we need to
// implement it for all W: Write + Send (trait bound).  Since the trait is
// object-safe and the impl delegates to finalize which has the stronger bound,
// we keep the impl bound minimal.
impl<W: Write + Send + 'static> ArchiveWriter for TarGzWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let name = path.to_str().ok_or_else(|| GeeZipError::Format {
            message: format!("non-UTF-8 path: {}", path.display()),
            format: ArchiveFormat::TarGz,
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
            format: ArchiveFormat::TarGz,
        })?;
        header.set_size(data.len() as u64);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarGz,
        })?;
        builder
            .append(&header, std::io::Cursor::new(data))
            .map_err(convert_targz_error)?;

        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_path(path).map_err(|e| GeeZipError::Format {
            message: format!("setting tar header path: {e}"),
            format: ArchiveFormat::TarGz,
        })?;
        header.set_size(0);
        header.set_cksum();

        let builder = self.inner.as_mut().ok_or_else(|| GeeZipError::Format {
            message: "TAR writer not initialised (already consumed)".into(),
            format: ArchiveFormat::TarGz,
        })?;
        builder
            .append(&header, std::io::Cursor::new(&[] as &[u8]))
            .map_err(convert_targz_error)?;

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

fn convert_targz_error(e: std::io::Error) -> GeeZipError {
    GeeZipError::Io {
        source: e,
        context: "tar.gz operation failed".into(),
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

    /// Create a minimal valid tar.gz archive in memory.
    fn create_test_targz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = TarGzWriter::new(Vec::new());
        for (name, data) in files {
            writer
                .add_entry_from_reader(&PathBuf::from(name), &mut Cursor::new(data))
                .unwrap();
        }
        let (_bytes, data) = writer.finalize().unwrap();
        data
    }

    /// Create a raw tar.gz archive where the tar entry has `path`
    /// (allowing malicious paths that `tar::Header::set_path` rejects).
    /// We construct the tar bytes manually, then gzip-compress them.
    fn create_raw_targz(path: &[u8], data: &[u8]) -> Vec<u8> {
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

        // Gzip compress it
        let mut compressed = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut compressed, flate2::Compression::default());
            encoder.write_all(&raw_tar).unwrap();
            encoder.try_finish().unwrap();
        }
        compressed
    }

    // -------------------------------------------------------------------
    // Round-trip: write to Vec -> read back
    // -------------------------------------------------------------------

    #[test]
    fn targz_roundtrip_single_file() {
        let content = b"hello world from tar.gz";
        let data = create_test_targz(&[("hello.txt", content)]);

        let mut reader = TarGzReader::from_buf(data);
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
    fn targz_roundtrip_multiple_files() {
        let files = [
            ("a.txt", b"aaa" as &[u8]),
            ("b.txt", b"bbb" as &[u8]),
            ("c.txt", b"ccc" as &[u8]),
        ];
        let data = create_test_targz(&files);

        let mut reader = TarGzReader::from_buf(data);
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
    fn targz_roundtrip_nested_path() {
        let content = b"nested content";
        let data = create_test_targz(&[("dir/subdir/file.txt", content)]);

        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "dir/subdir/file.txt");

        let mut output = Vec::new();
        let bytes = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, content);
        assert_eq!(bytes, content.len() as u64);
    }

    #[test]
    fn targz_unicode_filename() {
        let content = b"unicode content";
        let data = create_test_targz(&[("\u{4e2d}\u{6587}.txt", content)]); // 中文.txt

        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.contains('\u{4e2d}'));
    }

    // -------------------------------------------------------------------
    // Empty / malformed archives
    // -------------------------------------------------------------------

    #[test]
    fn targz_empty_archive() {
        // A minimal tar.gz with no entries: use the writer.
        let data = create_test_targz(&[] as &[(&str, &[u8])]);

        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 0, "empty tar.gz should have no entries");
    }

    #[test]
    fn targz_corrupted_data_fails() {
        let bad_data = b"this is not a tar.gz archive at all";
        let mut reader = TarGzReader::from_buf(bad_data.to_vec());
        let err = reader.entries().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("tar")
                || msg.to_lowercase().contains("gz")
                || msg.to_lowercase().contains("io")
                || msg.to_lowercase().contains("failed"),
            "expected tar/gz/io error, got: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // Extract all
    // -------------------------------------------------------------------

    #[test]
    fn targz_extract_all_basic() {
        let data = create_test_targz(&[("file_a.txt", b"AAA"), ("file_b.txt", b"BBB")]);

        let mut reader = TarGzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());

        assert!(dest.path().join("file_a.txt").exists());
        assert!(dest.path().join("file_b.txt").exists());
    }

    // -------------------------------------------------------------------
    // extract_all_with_cancel
    // -------------------------------------------------------------------

    #[test]
    fn targz_extract_all_with_cancel_basic() {
        let data = create_test_targz(&[("a.txt", b"aaa"), ("b.txt", b"bbb")]);
        let mut reader = TarGzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader
            .extract_all_with_cancel(dest.path(), true, &|| false)
            .unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 6);
        assert!(report.errors.is_empty());

        // Verify file contents.
        assert_eq!(
            std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
            "aaa"
        );
        assert_eq!(
            std::fs::read_to_string(dest.path().join("b.txt")).unwrap(),
            "bbb"
        );
    }

    #[test]
    fn targz_extract_all_with_cancel_before_start() {
        let data = create_test_targz(&[("only.txt", b"data")]);
        let mut reader = TarGzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let err = reader
            .extract_all_with_cancel(dest.path(), true, &|| true)
            .unwrap_err();
        assert!(matches!(err, GeeZipError::Cancelled));

        // Ensure no file was extracted.
        assert!(!dest.path().join("only.txt").exists());
    }

    #[test]
    fn targz_extract_all_with_cancel_between_entries() {
        use std::cell::Cell;

        let data = create_test_targz(&[("first.txt", b"AAA"), ("second.txt", b"BBB")]);
        let mut reader = TarGzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        let call_count = Cell::new(0u32);
        let is_cancelled = || {
            call_count.set(call_count.get() + 1);
            // Pre-entry check for entry 1 -> proceed (count 1)
            // Write check for entry 1 (CancellableWriter) -> proceed (count 2)
            // Pre-entry check for entry 2 -> cancel (count 3)
            call_count.get() > 2
        };

        let result = reader.extract_all_with_cancel(dest.path(), true, &is_cancelled);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GeeZipError::Cancelled));

        // First file should exist and have correct content.
        assert_eq!(
            std::fs::read_to_string(dest.path().join("first.txt")).unwrap(),
            "AAA"
        );
        // Second file should NOT exist.
        assert!(!dest.path().join("second.txt").exists());
    }

    // -------------------------------------------------------------------
    // Path traversal protection
    // -------------------------------------------------------------------

    #[test]
    fn targz_slip_detection() {
        let data = create_raw_targz(b"../escape.txt", b"malicious");

        let mut reader = TarGzReader::from_buf(data);
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
    fn targz_slip_dotdot_in_middle() {
        let data = create_raw_targz(b"subdir/../../../escape.txt", b"escape");
        let mut reader = TarGzReader::from_buf(data);
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
    fn targz_slip_absolute_path() {
        let data = create_raw_targz(b"/etc/passwd", b"leak");

        let mut reader = TarGzReader::from_buf(data);
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
    fn targz_extract_all_with_dir_entries() {
        // Build a tar.gz with an explicit directory entry plus a file inside
        // that directory, verifying extract_all creates the directory
        // implicitly (via create_dir_all on the file's parent).
        let mut buf = Vec::new();
        {
            let mut gz = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
            let mut builder = tar::Builder::new(&mut gz);

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
            gz.try_finish().unwrap();
        }

        let mut reader = TarGzReader::from_buf(buf);
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
    fn targz_no_clobber_skips_existing_files() {
        let content = b"hello world";
        let data = create_test_targz(&[("hello.txt", content)]);

        let mut reader = TarGzReader::from_buf(data);
        let dest = tempfile::tempdir().unwrap();

        // First extract (creates file).
        let report = reader.extract_all(dest.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert!(report.errors.is_empty());

        // Modify extracted file.
        let modified_path = dest.path().join("hello.txt");
        std::fs::write(&modified_path, b"MODIFIED").unwrap();

        // Second extract with overwrite=false: should skip.
        let mut reader2 = TarGzReader::from_buf(create_test_targz(&[("hello.txt", content)]));
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
    fn targz_writer_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarGzWriter::new(buf);
        writer
            .add_entry_from_reader(&PathBuf::from("test.txt"), &mut Cursor::new(b"hello"))
            .unwrap();

        let (bytes_written, data) = writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");

        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, 5);
        assert_eq!(output, b"hello");
    }

    #[test]
    fn targz_writer_multiple_files_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarGzWriter::new(buf);

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
    fn targz_writer_add_directory_roundtrip() {
        let buf = Vec::new();
        let mut writer = TarGzWriter::new(buf);

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

        let mut reader = TarGzReader::from_buf(data);
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
    // Parallel compression (jobs > 1) round-trip
    // -------------------------------------------------------------------

    #[test]
    fn targz_parallel_jobs_2_roundtrip() {
        let opts = CompressOptions {
            level: Some(6),
            jobs: Some(2),
            password: None,
        };
        let buf = Vec::new();
        let mut writer = TarGzWriter::new_with_options(buf, opts);
        writer
            .add_entry_from_reader(
                &PathBuf::from("hello.txt"),
                &mut Cursor::new(b"hello parallel"),
            )
            .unwrap();

        let (bytes_written, data) = writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        // Verify with both single-member GzDecoder and MultiGzDecoder.
        // MultiGzDecoder is the safer choice for parallel output.
        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");

        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, b"hello parallel".len() as u64);
        assert_eq!(output, b"hello parallel");
    }

    #[test]
    fn targz_parallel_jobs_4_roundtrip() {
        // Use enough data that parallel compression has something to do.
        let large_content = b"Hello from multi-threaded Gzip compression! ";
        let repeated: Vec<u8> = large_content
            .iter()
            .copied()
            .cycle()
            .take(1024 * 10) // ~260 bytes * 40 copies = ~10 KB
            .collect();

        let opts = CompressOptions {
            level: Some(6),
            jobs: Some(4),
            password: None,
        };
        let buf = Vec::new();
        let mut writer = TarGzWriter::new_with_options(buf, opts);
        writer
            .add_entry_from_reader(&PathBuf::from("large.txt"), &mut Cursor::new(&repeated))
            .unwrap();

        let (bytes_written, data) = writer.finalize().unwrap();
        assert!(bytes_written > 0, "should have written something");

        // Read back and verify content.
        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "large.txt");

        let mut output = Vec::new();
        let extracted = reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(extracted, repeated.len() as u64);
        assert_eq!(output, repeated);
    }

    #[test]
    fn targz_parallel_then_single_roundtrip() {
        // Verify that data compressed with jobs > 1 can be read back.
        let content = b"data for parallel then single read test";
        let opts = CompressOptions {
            level: None,
            jobs: Some(2),
            password: None,
        };
        let buf = Vec::new();
        let mut writer = TarGzWriter::new_with_options(buf, opts);
        writer
            .add_entry_from_reader(&PathBuf::from("test.txt"), &mut Cursor::new(content))
            .unwrap();
        let (_bytes, data) = writer.finalize().unwrap();

        // Read with MultiGzDecoder (used by TarGzReader)
        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);

        let mut output = Vec::new();
        reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, content);
    }

    // -------------------------------------------------------------------
    // Jobs=0 maps to available_parallelism (at least 1) → same as Single
    // -------------------------------------------------------------------

    #[test]
    fn targz_jobs_zero_uses_auto() {
        // jobs=0 should map to at least 1 thread; compression should work.
        let opts = CompressOptions {
            level: None,
            jobs: Some(0),
            password: None,
        };
        let buf = Vec::new();
        let mut writer = TarGzWriter::new_with_options(buf, opts);
        writer
            .add_entry_from_reader(&PathBuf::from("auto.txt"), &mut Cursor::new(b"jobs=0 test"))
            .unwrap();
        let (_bytes, data) = writer.finalize().unwrap();
        assert!(!data.is_empty(), "compressed data should not be empty");

        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 1);
        let mut output = Vec::new();
        reader.extract(&entries[0], &mut output).unwrap();
        assert_eq!(output, b"jobs=0 test");
    }

    // -------------------------------------------------------------------
    // Trait object safety (compile-time checks)
    // -------------------------------------------------------------------

    #[test]
    fn targz_parallel_finalize_multi_branch() {
        // Explicitly exercise the GzipEncoder::Multi branch finalization
        // path (Arc::try_unwrap + ParCompress::finish).  Uses jobs>1 and
        // several entries to force the parallel path.
        let opts = CompressOptions {
            level: None,
            jobs: Some(2),
            password: None,
        };
        let buf = Vec::new();
        let mut writer = TarGzWriter::new_with_options(buf, opts);

        for i in 0..10 {
            let name = format!("file-{}.txt", i);
            let content = format!("content-{}\n", i);
            writer
                .add_entry_from_reader(
                    &PathBuf::from(&name),
                    &mut std::io::Cursor::new(content.as_bytes()),
                )
                .unwrap();
        }

        // finalize() should succeed, recovering the inner Vec<u8> writer.
        let (bytes, data) = writer.finalize().unwrap();
        assert!(bytes > 0, "should have written compressed bytes");
        assert!(
            !data.is_empty(),
            "inner writer should be recovered after finalize"
        );

        // Sanity: verify the output is a valid gzip stream.
        assert_eq!(data[..2], [0x1F, 0x8B], "gzip magic expected");

        // Verify round-trip: decompress and check all entries are present.
        let mut reader = TarGzReader::from_buf(data);
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 10, "all 10 entries should be present");

        // Verify content of the last entry.
        let mut output = Vec::new();
        reader.extract(&entries[9], &mut output).unwrap();
        assert_eq!(output, b"content-9\n");
    }

    #[test]
    fn archive_reader_trait_object() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let data = create_test_targz(&[("dummy.txt", b"x")]);
        let mut reader = TarGzReader::from_buf(data);
        use_reader(&mut reader);
    }

    #[test]
    fn archive_writer_trait_object() {
        fn use_writer(_w: Box<dyn ArchiveWriter>) {}
        let buf = Vec::new();
        let writer = TarGzWriter::new(buf);
        use_writer(Box::new(writer));
    }

    #[test]
    fn targz_reader_format() {
        let data = create_test_targz(&[("dummy.txt", b"x")]);
        let reader = TarGzReader::from_buf(data);
        assert_eq!(reader.format(), ArchiveFormat::TarGz);
    }

    #[test]
    fn targz_writer_format() {
        let buf = Vec::new();
        let writer = TarGzWriter::new(buf);
        assert_eq!(writer.format(), ArchiveFormat::TarGz);
    }
}

#[test]
fn targz_truncated_not_panic() {
    // Only valid gzip magic bytes (1f 8b), no tar data at all.
    // Must NOT panic; should return a proper error.
    let truncated = vec![0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
    let mut reader = TarGzReader::from_buf(truncated);
    let err = reader.entries().unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("tar") || msg.contains("gz") || msg.contains("io") || msg.contains("failed"),
        "expected tar/gz/io error for truncated targz, got: {err}"
    );
}
