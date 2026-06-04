//! 7z archive reader implementation.
//!
//! Built on top of the [`sevenz_rust2`] crate.  This module provides
//! read-only support for 7z archives, including AES-256 encrypted
//! archives, LZMA/LZMA2/BZIP2/PPMD/DEFLATE compressed entries,
//! and solid archives.
//!
//! # Design notes
//!
//! - **Read-only** — 7z writing is not supported (Phase 2.3 scope).
//! - **Single-entry extract** re-opens the archive and uses
//!   `sevenz_rust2::ArchiveReader::read_file()`, which is O(n) for
//!   solid archives (decodes all preceding data).  Callers that need
//!   to extract many entries should use [`extract_all`] which decodes
//!   the entire archive in one pass via
//!   [`sevenz_rust2::ArchiveReader::for_each_entries()`].
//! - **Password safety** — The password is stored in the reader and
//!   never logged or printed.  Empty passwords are rejected at the
//!   CLI layer.

use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use sevenz_rust2::Password;

use crate::archive::{
    check_entry_path_safety, normalize_path, ArchiveReader, CancellableWriter, Entry, ExtractReport,
};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// SevenZipReader
// ---------------------------------------------------------------------------

/// 7z archive reader.
///
/// Opens the file on each read operation.  This is acceptable because
/// 7z metadata is small and the underlying
/// [`sevenz_rust2::ArchiveReader`] needs ownership of the source
/// reader.
pub struct SevenZipReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Password,
}

impl fmt::Debug for SevenZipReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SevenZipReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SevenZipReader {
    /// Create a new 7z reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        SevenZipReader {
            path: path.into(),
            format: ArchiveFormat::SevenZip,
            password: Password::empty(),
        }
    }

    /// Set a password for decrypting encrypted entries.
    pub fn set_password(&mut self, password: &str) {
        self.password = Password::from(password);
    }

    /// Open an [`sevenz_rust2::ArchiveReader`] for the stored file.
    fn open_reader(&self) -> GeeZipResult<sevenz_rust2::ArchiveReader<File>> {
        let reader = sevenz_rust2::ArchiveReader::open(&self.path, self.password.clone())
            .map_err(convert_7z_error)?;
        Ok(reader)
    }

    /// Convert a [`sevenz_rust2::ArchiveEntry`] to a GeeZipX [`Entry`].
    fn to_entry(entry: &sevenz_rust2::ArchiveEntry) -> Entry {
        let modified = if entry.has_last_modified_date {
            // NtTime access is not publicly exposed, so we skip timestamp
            // conversion for 7z entries.
            None
        } else {
            None
        };

        Entry {
            path: entry.name.clone(),
            size: entry.size,
            compressed_size: entry.compressed_size,
            crc32: if entry.has_crc {
                Some(entry.crc as u32)
            } else {
                None
            },
            modified,
            is_dir: entry.is_directory,
        }
    }
}

impl ArchiveReader for SevenZipReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn set_password(&mut self, password: &str) -> GeeZipResult<()> {
        self.password = Password::from(password);
        Ok(())
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        let reader = self.open_reader()?;
        let archive = reader.archive();

        let entries: Vec<Entry> = archive.files.iter().map(Self::to_entry).collect();

        Ok(entries)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut reader = self.open_reader()?;
        let data = reader
            .read_file(&entry.path)
            .map_err(|e| convert_7z_entry_error(e, &entry.path))?;

        if data.is_empty() {
            return Ok(0);
        }

        writer
            .write_all(&data)
            .map_err(|e| GeeZipError::io(e, format!("writing entry '{}'", entry.path)))?;

        Ok(data.len() as u64)
    }

    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport> {
        let dest = normalize_path(dest);
        let password = self.password.clone();
        let path = self.path.clone();

        // Use for_each_entries for single-pass decoding
        let mut reader =
            sevenz_rust2::ArchiveReader::open(&path, password).map_err(convert_7z_error)?;

        let mut report = ExtractReport::default();

        reader
            .for_each_entries(|sz_entry, data_reader| {
                let entry = Self::to_entry(sz_entry);

                // Path safety check
                let entry_path = Path::new(&entry.path);
                let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                    Ok(t) => t,
                    Err((name, err)) => {
                        report.errors.push((name, err));
                        return Ok(true);
                    }
                };

                // Directory entry
                if entry.is_dir {
                    if let Err(e) = std::fs::create_dir_all(&target) {
                        report
                            .errors
                            .push((entry.path.clone(), GeeZipError::io(e, "creating directory")));
                    } else {
                        report.files_extracted += 1;
                    }
                    return Ok(true);
                }

                // Create parent directory
                if let Some(parent) = target.parent() {
                    if !parent.exists() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, "creating parent directory"),
                            ));
                            return Ok(true);
                        }
                    }
                }

                // Open output file
                let mut output = if overwrite {
                    match std::fs::File::create(&target) {
                        Ok(f) => f,
                        Err(e) => {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, format!("creating '{}'", target.display())),
                            ));
                            return Ok(true);
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
                            return Ok(true);
                        }
                        Err(e) => {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, format!("creating '{}'", target.display())),
                            ));
                            return Ok(true);
                        }
                    }
                };

                // Copy data from the decoded stream
                match std::io::copy(data_reader, &mut output) {
                    Ok(bytes) => {
                        report.files_extracted += 1;
                        report.bytes_extracted += bytes;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, format!("extracting '{}'", entry.path)),
                        ));
                    }
                }

                Ok(true)
            })
            .map_err(convert_7z_error)?;

        Ok(report)
    }

    fn extract_all_with_cancel(
        &mut self,
        dest: &Path,
        overwrite: bool,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<ExtractReport> {
        let dest = normalize_path(dest);
        let password = self.password.clone();
        let path = self.path.clone();

        let reader =
            sevenz_rust2::ArchiveReader::open(&path, password).map_err(convert_7z_error)?;

        let mut report = ExtractReport::default();

        // Check cancellation before starting
        if is_cancelled() {
            return Err(GeeZipError::Cancelled);
        }

        // Pre-fetch entries for cancellation checking between entries
        let entries: Vec<Entry> = {
            let archive = reader.archive();
            archive.files.iter().map(Self::to_entry).collect()
        };

        // Use a second reader for actual extraction
        let mut extract_reader = sevenz_rust2::ArchiveReader::open(&path, self.password.clone())
            .map_err(convert_7z_error)?;

        let mut entry_index = 0usize;

        extract_reader
            .for_each_entries(|_sz_entry, data_reader| {
                // Check cancellation before each entry
                if is_cancelled() {
                    return Err(sevenz_rust2::Error::Io(
                        std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled by user"),
                        Cow::Borrowed("cancelled"),
                    ));
                }

                let entry = if entry_index < entries.len() {
                    &entries[entry_index]
                } else {
                    // Fallback: create from sz_entry
                    // Should not happen, but be safe
                    entry_index += 1;
                    return Ok(true);
                };
                entry_index += 1;

                // Path safety check
                let entry_path = Path::new(&entry.path);
                let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                    Ok(t) => t,
                    Err((name, err)) => {
                        report.errors.push((name, err));
                        return Ok(true);
                    }
                };

                // Directory entry
                if entry.is_dir {
                    if let Err(e) = std::fs::create_dir_all(&target) {
                        report
                            .errors
                            .push((entry.path.clone(), GeeZipError::io(e, "creating directory")));
                    } else {
                        report.files_extracted += 1;
                    }
                    return Ok(true);
                }

                // Create parent directory
                if let Some(parent) = target.parent() {
                    if !parent.exists() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, "creating parent directory"),
                            ));
                            return Ok(true);
                        }
                    }
                }

                // Open output file
                let mut output = if overwrite {
                    match std::fs::File::create(&target) {
                        Ok(f) => f,
                        Err(e) => {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, format!("creating '{}'", target.display())),
                            ));
                            return Ok(true);
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
                            return Ok(true);
                        }
                        Err(e) => {
                            report.errors.push((
                                entry.path.clone(),
                                GeeZipError::io(e, format!("creating '{}'", target.display())),
                            ));
                            return Ok(true);
                        }
                    }
                };

                // Wrap with cancellation check
                let mut canceller = CancellableWriter::new(&mut output, is_cancelled);

                // Copy data
                match std::io::copy(data_reader, &mut canceller) {
                    Ok(bytes) => {
                        if canceller.was_cancelled() {
                            return Err(sevenz_rust2::Error::Io(
                                std::io::Error::new(
                                    std::io::ErrorKind::Interrupted,
                                    "cancelled by user",
                                ),
                                Cow::Borrowed("cancelled"),
                            ));
                        }
                        report.files_extracted += 1;
                        report.bytes_extracted += bytes;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, format!("extracting '{}'", entry.path)),
                        ));
                    }
                }

                Ok(true)
            })
            .map_err(|e| {
                if let sevenz_rust2::Error::Io(ref inner, ref msg) = e {
                    if msg.contains("cancelled") && inner.kind() == std::io::ErrorKind::Interrupted
                    {
                        return GeeZipError::Cancelled;
                    }
                }
                convert_7z_error(e)
            })?;

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn convert_7z_error(e: sevenz_rust2::Error) -> GeeZipError {
    match e {
        sevenz_rust2::Error::Io(inner, _) => GeeZipError::Io {
            source: inner,
            context: "7z I/O operation failed".into(),
        },
        sevenz_rust2::Error::BadSignature(sig) => GeeZipError::Format {
            message: format!("invalid 7z signature: {:02X?}", sig),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::UnsupportedVersion { major, minor } => GeeZipError::Format {
            message: format!("unsupported 7z version {}.{}", major, minor),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::ChecksumVerificationFailed => GeeZipError::Format {
            message: "7z checksum verification failed (archive may be corrupted)".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::NextHeaderCrcMismatch => GeeZipError::Format {
            message: "7z header CRC mismatch (archive may be corrupted)".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::UnsupportedCompressionMethod(m) => GeeZipError::Format {
            message: format!("unsupported 7z compression method: {}", m),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::PasswordRequired => GeeZipError::Crypto {
            message: "encrypted 7z archive requires a password (use --password)".into(),
        },
        sevenz_rust2::Error::MaybeBadPassword(_) => GeeZipError::Crypto {
            message: "invalid password for 7z archive".into(),
        },
        sevenz_rust2::Error::MaxMemLimited { .. } => GeeZipError::Format {
            message: "7z archive requires too much memory to decode".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::Unsupported(msg) => GeeZipError::Format {
            message: format!("unsupported 7z feature: {}", msg),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::FileNotFound => GeeZipError::EntryNotFound {
            name: "(unknown)".into(),
        },
        sevenz_rust2::Error::ExternalUnsupported => GeeZipError::Format {
            message: "external compression method not supported in 7z".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::FileOpen(inner, _) => GeeZipError::Io {
            source: inner,
            context: "opening 7z archive".into(),
        },
        // Terminal errors - unlikely to be seen during normal operation
        sevenz_rust2::Error::BadTerminatedPackInfo(n) => GeeZipError::Format {
            message: format!("bad 7z pack info terminator: 0x{n:02X}"),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::BadTerminatedUnpackInfo => GeeZipError::Format {
            message: "bad 7z unpack info terminator".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::BadTerminatedStreamsInfo(n) => GeeZipError::Format {
            message: format!("bad 7z streams info terminator: 0x{n:02X}"),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::BadTerminatedSubStreamsInfo => GeeZipError::Format {
            message: "bad 7z sub-streams info terminator".into(),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::BadTerminatedHeader(n) => GeeZipError::Format {
            message: format!("bad 7z header terminator: 0x{n:02X}"),
            format: ArchiveFormat::SevenZip,
        },
        sevenz_rust2::Error::Other(msg) => GeeZipError::Format {
            message: format!("7z error: {msg}"),
            format: ArchiveFormat::SevenZip,
        },
    }
}

/// Like `convert_7z_error` but may remap `FileNotFound` to an
/// entry-specific message.
fn convert_7z_entry_error(e: sevenz_rust2::Error, entry_name: &str) -> GeeZipError {
    if matches!(e, sevenz_rust2::Error::FileNotFound) {
        GeeZipError::EntryNotFound {
            name: entry_name.to_owned(),
        }
    } else {
        convert_7z_error(e)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a simple test 7z archive with files.
    /// Uses the sevenz-rust2 compress utilities to generate a valid .7z.
    fn create_test_7z(files: &[(&str, &[u8])]) -> Vec<u8> {
        use sevenz_rust2::compress_to_path;

        // Source directory: files to compress
        let src_dir = tempfile::tempdir().unwrap();
        let src_path = src_dir.path();

        // Write each file to the source dir
        for (name, data) in files {
            let path = src_path.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, data).unwrap();
        }

        // Output directory: archive goes here (outside src_dir to avoid
        // including the archive itself in the entries)
        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join("test.7z");
        compress_to_path(src_path, &archive_path).expect("failed to compress test 7z");

        std::fs::read(archive_path).unwrap()
    }

    macro_rules! assert_contains {
        ($msg:expr, $sub:expr) => {
            assert!(
                $msg.contains($sub),
                "expected '{}' to contain '{}'",
                $msg,
                $sub
            );
        };
    }

    fn buf_reader(data: Vec<u8>) -> (SevenZipReader, tempfile::TempDir) {
        // We need to write to a temp file since sevenz_rust2::ArchiveReader
        // works with File for the read_file method.
        // Returning the TempDir keeps it alive so the file remains accessible.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.7z");
        std::fs::write(&path, data).unwrap();
        (SevenZipReader::new(path), dir)
    }

    // -------------------------------------------------------------------
    // Detection
    // -------------------------------------------------------------------

    #[test]
    fn detect_sevenzip_magic() {
        let magic = crate::detect::MAGIC_SEVENZIP;
        assert_eq!(magic, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
        assert_eq!(
            crate::detect::detect_format(magic),
            Some(ArchiveFormat::SevenZip)
        );
    }

    #[test]
    fn detect_sevenzip_extension() {
        assert_eq!(
            crate::detect::detect_from_extension(Path::new("test.7z")),
            Some(ArchiveFormat::SevenZip)
        );
    }

    #[test]
    fn detect_sevenzip_display() {
        assert_eq!(ArchiveFormat::SevenZip.to_string(), "7z");
    }

    // -------------------------------------------------------------------
    // Basic
    // -------------------------------------------------------------------

    #[test]
    fn sevenzip_list_entries() {
        let data = create_test_7z(&[
            ("hello.txt", b"hello world"),
            ("nested/data.txt", b"nested content"),
        ]);

        let (mut reader, _dir) = buf_reader(data);
        let entries = reader.entries().unwrap();

        // Finder order depends on the compressor, just check we get both
        let names: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"hello.txt"), "entries: {names:?}");
        assert!(names.contains(&"nested/data.txt"), "entries: {names:?}");
        for e in &entries {
            if e.path == "hello.txt" {
                assert_eq!(e.size, 11);
                assert!(!e.is_dir);
            }
        }
    }

    #[test]
    fn sevenzip_extract_entry() {
        let data = create_test_7z(&[("file.txt", b"Hello from 7z!")]);

        let (mut reader, _dir) = buf_reader(data);
        let entries = reader.entries().unwrap();

        // Find the file entry (index 0 might be the root directory entry)
        let file_entry = entries.iter().find(|e| !e.is_dir).expect("file entry");
        assert_eq!(file_entry.path, "file.txt", "entries: {entries:#?}");

        let mut output = Vec::new();
        let bytes = reader.extract(file_entry, &mut output).unwrap();
        assert_eq!(bytes, 14);
        assert_eq!(&output, b"Hello from 7z!");
    }

    #[test]
    fn sevenzip_extract_all_basic() {
        let data = create_test_7z(&[("a.txt", b"AAA"), ("b.txt", b"BBB")]);

        let (mut reader, _dir) = buf_reader(data);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        // Expect 3 entries: a.txt, b.txt, and the root directory entry
        // Skip exact count (compressor may add directory entries)
        assert!(
            report.files_extracted >= 2,
            "should extract at least the files, got {}",
            report.files_extracted
        );
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        assert_eq!(
            std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
            "AAA"
        );
        assert_eq!(
            std::fs::read_to_string(dest.path().join("b.txt")).unwrap(),
            "BBB"
        );
    }

    #[test]
    fn sevenzip_entry_not_found() {
        let data = create_test_7z(&[("exists.txt", b"data")]);

        let (mut reader, _dir) = buf_reader(data);
        let fake_entry = Entry {
            path: "does_not_exist.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        };

        let mut output = Vec::new();
        let err = reader.extract(&fake_entry, &mut output).unwrap_err();
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn sevenzip_empty_archive_fails() {
        let result = buf_reader(vec![]).0.open_reader();
        let err = match result {
            Ok(_) => panic!("expected error for empty archive"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert_contains!(msg.to_lowercase(), "io");
    }

    #[test]
    fn sevenzip_corrupted_fails() {
        let bad_data = b"this is not a valid 7z file";
        let result = buf_reader(bad_data.to_vec()).0.open_reader();
        let err = match result {
            Ok(_) => panic!("expected error for corrupted data"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert_contains!(msg.to_lowercase(), "7z");
    }

    // -------------------------------------------------------------------
    // No-clobber
    // -------------------------------------------------------------------

    #[test]
    fn sevenzip_no_clobber_skips_existing() {
        let data = create_test_7z(&[("file.txt", b"DATA")]);

        let (mut reader, _dir) = buf_reader(data);
        let dest = tempfile::tempdir().unwrap();

        // First extract
        let report = reader.extract_all(dest.path(), true).unwrap();
        // Expect 2 entries: file.txt and root directory entry
        // Skip exact count (compressor may add directory entries)
        assert!(
            report.files_extracted >= 1,
            "should extract at least the file, got {}",
            report.files_extracted
        );
        assert!(report.errors.is_empty());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("file.txt")).unwrap(),
            "DATA"
        );

        // Modify
        std::fs::write(dest.path().join("file.txt"), b"MODIFIED").unwrap();

        // Second extract with no-clobber
        let data2 = create_test_7z(&[("file.txt", b"DATA")]);
        let (mut reader2, _dir2) = buf_reader(data2);
        let report2 = reader2.extract_all(dest.path(), false).unwrap();
        // Skip files_extracted assertion (compressor may add directory
        // entries); just verify files_skipped below.
        assert_eq!(report2.files_extracted, 0);
        assert_eq!(report2.files_skipped, 1);

        // Verify not overwritten
        assert_eq!(
            std::fs::read_to_string(dest.path().join("file.txt")).unwrap(),
            "MODIFIED"
        );
    }

    // -------------------------------------------------------------------
    // Trait object safety
    // -------------------------------------------------------------------

    #[test]
    fn archive_reader_trait_object() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let data = create_test_7z(&[("dummy.txt", b"x")]);
        let (mut reader, _dir) = buf_reader(data);
        use_reader(&mut reader);
    }
}
