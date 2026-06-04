//! RAR archive reader implementation.
//!
//! Built on top of the [`unrar`] crate.  This module provides
//! read-only support for RAR archives (both RAR4 and RAR5),
//! including encrypted archives.
//!
//! # Design notes
//!
//! - **Read-only** — RAR creation is not supported.
//! - **Feature-gated** — Behind the `rar` feature (default: off) because
//!   the `unrar` crate links to a C++ vendor library (RARLAB freeware UnRAR).
//! - **Path-based** — The [`unrar`] crate opens files by path, so the reader
//!   stores a `PathBuf` rather than a `File` handle.
//! - **Single-entry extract** — Re-opens the archive and scans entries
//!   sequentially.  Callers that need to extract many entries should use
//!   [`extract_all`] which processes all entries in one pass.
//! - **Password safety** — The password is stored in the reader and
//!   never logged or printed.  Empty passwords are rejected at the
//!   CLI layer.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use unrar::Archive as UnrarArchive;

use crate::archive::{
    check_entry_path_safety, normalize_path, ArchiveReader, Entry, ExtractReport,
};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// RarReader
// ---------------------------------------------------------------------------

/// RAR archive reader.
///
/// Opens the file on each read operation.  The underlying [`unrar`]
/// crate operates by path and does not support random access, so
/// `[extract]` re-opens the archive and scans to the desired entry.
pub struct RarReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Option<String>,
}

impl fmt::Debug for RarReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RarReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl RarReader {
    /// Create a new RAR reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        RarReader {
            path: path.into(),
            format: ArchiveFormat::Rar,
            password: None,
        }
    }

    /// Open the archive for listing (skips payload).
    fn open_list(
        &self,
    ) -> GeeZipResult<impl Iterator<Item = Result<unrar::FileHeader, unrar::error::UnrarError>>>
    {
        let archive = match &self.password {
            Some(pwd) => UnrarArchive::with_password(&self.path, pwd),
            None => UnrarArchive::new(&self.path),
        };
        archive.open_for_listing().map_err(convert_unrar_error)
    }

    /// Open the archive for processing (can read payload).
    fn open_process(
        &self,
    ) -> GeeZipResult<unrar::OpenArchive<unrar::Process, unrar::CursorBeforeHeader>> {
        let archive = match &self.password {
            Some(pwd) => UnrarArchive::with_password(&self.path, pwd),
            None => UnrarArchive::new(&self.path),
        };
        archive.open_for_processing().map_err(convert_unrar_error)
    }

    /// Convert a [`unrar::FileHeader`] to a GeeZipX [`Entry`].
    fn to_entry(header: &unrar::FileHeader) -> Entry {
        let path_str = header.filename.to_string_lossy().replace('\\', "/");
        Entry {
            path: path_str,
            size: header.unpacked_size,
            compressed_size: 0, // unrar does not expose compressed size at the header level
            crc32: Some(header.file_crc),
            modified: None, // unrar exposes file_time but as DOS-format; skip conversion
            is_dir: header.is_directory(),
        }
    }
}

impl ArchiveReader for RarReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn set_password(&mut self, password: &str) -> GeeZipResult<()> {
        self.password = Some(password.to_owned());
        Ok(())
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        let list = self.open_list()?;
        let mut entries = Vec::new();
        for result in list {
            let header = result.map_err(convert_unrar_error)?;
            entries.push(Self::to_entry(&header));
        }
        Ok(entries)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut archive = self.open_process()?;
        let target_path = &entry.path;

        while let Some(header) = archive.read_header().map_err(convert_unrar_error)? {
            let entry_path = header.entry().filename.to_string_lossy().replace('\\', "/");
            if entry_path == *target_path {
                if header.entry().is_directory() {
                    return Ok(0);
                }
                let (data, _rest) = header.read().map_err(convert_unrar_error)?;
                let len = data.len() as u64;
                if !data.is_empty() {
                    writer.write_all(&data).map_err(|e| {
                        GeeZipError::io(e, format!("writing entry '{}'", entry.path))
                    })?;
                }
                return Ok(len);
            } else {
                archive = header.skip().map_err(convert_unrar_error)?;
            }
        }

        Err(GeeZipError::EntryNotFound {
            name: target_path.clone(),
        })
    }

    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport> {
        let dest = normalize_path(dest);
        let mut report = ExtractReport::default();
        let mut archive = self.open_process()?;

        while let Some(header) = archive.read_header().map_err(convert_unrar_error)? {
            let entry_path_str = header.entry().filename.to_string_lossy().replace('\\', "/");
            let entry_path = Path::new(&entry_path_str);
            let is_dir = header.entry().is_directory();
            let size = header.entry().unpacked_size;

            // Path safety check
            let target = match check_entry_path_safety(entry_path, &entry_path_str, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    archive = header.skip().map_err(convert_unrar_error)?;
                    continue;
                }
            };

            if is_dir {
                if let Err(e) = std::fs::create_dir_all(&target) {
                    report.errors.push((
                        entry_path_str.clone(),
                        GeeZipError::io(e, "creating directory"),
                    ));
                } else {
                    report.files_extracted += 1;
                }
                archive = header.skip().map_err(convert_unrar_error)?;
                continue;
            }

            // Create parent directory
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, "creating parent directory"),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                }
            }

            // Open output file
            let mut output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
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
                            entry_path_str.clone(),
                            GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                }
            };

            // Read entry data and write
            let (data, rest) = header.read().map_err(convert_unrar_error)?;
            archive = rest;
            if !data.is_empty() {
                match output.write_all(&data) {
                    Ok(()) => {
                        report.files_extracted += 1;
                        report.bytes_extracted += size;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("extracting '{}'", entry_path_str)),
                        ));
                    }
                }
            }
        }

        Ok(report)
    }

    fn extract_all_with_cancel(
        &mut self,
        dest: &Path,
        overwrite: bool,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<ExtractReport> {
        let dest = normalize_path(dest);

        if is_cancelled() {
            return Err(GeeZipError::Cancelled);
        }

        let mut report = ExtractReport::default();
        let mut archive = self.open_process()?;

        while let Some(header) = archive.read_header().map_err(convert_unrar_error)? {
            if is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }

            let entry_path_str = header.entry().filename.to_string_lossy().replace('\\', "/");
            let entry_path = Path::new(&entry_path_str);
            let is_dir = header.entry().is_directory();
            let size = header.entry().unpacked_size;

            // Path safety check
            let target = match check_entry_path_safety(entry_path, &entry_path_str, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    archive = header.skip().map_err(convert_unrar_error)?;
                    continue;
                }
            };

            if is_dir {
                if let Err(e) = std::fs::create_dir_all(&target) {
                    report.errors.push((
                        entry_path_str.clone(),
                        GeeZipError::io(e, "creating directory"),
                    ));
                } else {
                    report.files_extracted += 1;
                }
                archive = header.skip().map_err(convert_unrar_error)?;
                continue;
            }

            // Create parent directory
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, "creating parent directory"),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                }
            }

            // Open output file
            let mut output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
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
                            entry_path_str.clone(),
                            GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("creating '{}'", target.display())),
                        ));
                        archive = header.skip().map_err(convert_unrar_error)?;
                        continue;
                    }
                }
            };

            // Check cancellation before reading entry data
            if is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }

            // Read entry data and write
            let (data, rest) = header.read().map_err(convert_unrar_error)?;
            archive = rest;
            if !data.is_empty() {
                match output.write_all(&data) {
                    Ok(()) => {
                        report.files_extracted += 1;
                        report.bytes_extracted += size;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry_path_str.clone(),
                            GeeZipError::io(e, format!("extracting '{}'", entry_path_str)),
                        ));
                    }
                }
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn convert_unrar_error(e: unrar::error::UnrarError) -> GeeZipError {
    let msg = e.to_string().to_lowercase();
    if msg.contains("password") || msg.contains("encrypted") {
        GeeZipError::Crypto {
            message: format!("RAR error: {e}"),
        }
    } else if msg.contains("unsupported")
        || msg.contains("corrupt")
        || msg.contains("bad data")
        || msg.contains("unknown")
        || msg.contains("header")
    {
        GeeZipError::Format {
            message: format!("RAR error: {e}"),
            format: ArchiveFormat::Rar,
        }
    } else {
        GeeZipError::Io {
            source: std::io::Error::other(e.to_string()),
            context: "RAR operation failed".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a RAR archive with files using the `rar` CLI (from rarlab).
    /// Returns `None` if the CLI is not available.

    #[test]
    fn rar_detect_magic() {
        // Verify the magic constant is correct
        assert_eq!(crate::detect::MAGIC_RAR, b"Rar!\x1A\x07");
    }

    #[test]
    fn rar_entries_empty_archive() {
        // A completely empty file should produce an IO error
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.rar");
        std::fs::write(&path, b"").unwrap();

        // Convert the UnrarError -> GeeZipError
        let result = UnrarArchive::new(&path).open_for_listing();
        assert!(result.is_err(), "empty file should fail");
    }

    #[test]
    fn rar_corrupted_fails() {
        let bad_data = b"this is not a valid rar archive";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.rar");
        std::fs::write(&path, bad_data).unwrap();

        let result = UnrarArchive::new(&path).open_for_listing();
        assert!(result.is_err(), "corrupted data should fail");
    }

    /// Helper: generate a minimal RAR archive with a single file.
    /// Uses the `rar` CLI if available, otherwise panics (tests skip).
    fn create_test_rar(files: &[(&str, &[u8])]) -> Option<Vec<u8>> {
        let src_dir = tempfile::tempdir().ok()?;
        let src_path = src_dir.path();

        for (name, data) in files {
            let path = src_path.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok()?;
            }
            std::fs::write(path, data).ok()?;
        }

        let out_dir = tempfile::tempdir().ok()?;
        let archive_path = out_dir.path().join("test.rar");

        let ok = std::process::Command::new("rar")
            .arg("a")
            .arg("-ep1")
            .arg(&archive_path)
            .arg(src_path)
            .arg("-idq")
            .status()
            .ok()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            std::fs::read(&archive_path).ok()
        } else {
            None
        }
    }

    #[ignore = "requires `rar` CLI (from rarlab.com)"]
    #[test]
    fn rar_list_entries() {
        let Some(data) = create_test_rar(&[("hello.txt", b"hello world\n")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        std::fs::write(&path, &data).unwrap();

        let mut reader = RarReader::new(&path);
        let entries = reader.entries().unwrap();

        assert!(!entries.is_empty(), "should have at least one entry");
        let names: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("hello.txt") || *n == "hello.txt"),
            "names: {names:?}"
        );
    }

    #[ignore = "requires `rar` CLI (from rarlab.com)"]
    #[test]
    fn rar_extract_entry() {
        let Some(data) = create_test_rar(&[("file.txt", b"Hello from RAR!\n")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        std::fs::write(&path, &data).unwrap();

        let mut reader = RarReader::new(&path);
        let entries = reader.entries().unwrap();
        let file_entry = entries.iter().find(|e| !e.is_dir).expect("file entry");

        let mut output = Vec::new();
        let bytes = reader.extract(file_entry, &mut output).unwrap();
        assert!(bytes > 0, "should extract some bytes");
        let out_str = String::from_utf8_lossy(&output);
        assert!(out_str.contains("Hello from RAR"), "got: {out_str}");
    }

    #[ignore = "requires `rar` CLI (from rarlab.com)"]
    #[test]
    fn rar_extract_all_basic() {
        let Some(data) = create_test_rar(&[("a.txt", b"AAA"), ("b.txt", b"BBB")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        std::fs::write(&path, &data).unwrap();

        let mut reader = RarReader::new(&path);
        let dest = tempfile::tempdir().unwrap();
        let report = reader.extract_all(dest.path(), true).unwrap();

        assert!(
            report.files_extracted >= 2,
            "should extract at least 2 files, got {}",
            report.files_extracted
        );
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[ignore = "requires `rar` CLI (from rarlab.com)"]
    #[test]
    fn rar_no_clobber_skips_existing() {
        let Some(data) = create_test_rar(&[("unique.txt", b"ORIGINAL")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        std::fs::write(&path, &data).unwrap();

        let mut reader = RarReader::new(&path);
        let dest = tempfile::tempdir().unwrap();

        let report = reader.extract_all(dest.path(), true).unwrap();
        assert!(report.files_extracted >= 1, "first extract");

        std::fs::write(dest.path().join("unique.txt"), b"MODIFIED").unwrap();

        let Some(data2) = create_test_rar(&[("unique.txt", b"ORIGINAL")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let path2 = dir.path().join("test2.rar");
        std::fs::write(&path2, &data2).unwrap();
        let mut reader2 = RarReader::new(&path2);
        let report2 = reader2.extract_all(dest.path(), false).unwrap();
        assert_eq!(report2.files_skipped, 1, "should skip existing file");

        let content = std::fs::read_to_string(dest.path().join("unique.txt")).unwrap();
        assert_eq!(content, "MODIFIED");
    }

    #[ignore = "requires `rar` CLI (from rarlab.com)"]
    #[test]
    fn rar_trait_object_safety() {
        fn use_reader(_r: &mut dyn ArchiveReader) {}
        let Some(data) = create_test_rar(&[("dummy.txt", b"x")]) else {
            eprintln!("skipped: `rar` CLI not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        std::fs::write(&path, &data).unwrap();

        let mut reader = RarReader::new(&path);
        use_reader(&mut reader);
    }
}
