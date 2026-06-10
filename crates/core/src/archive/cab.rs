//! Microsoft Cabinet (`.cab`) archive reader.
//!
//! GeeZipX exposes CAB as a read-only archive format backed by the [`cab`]
//! crate. The current MVP supports listing, extraction, and integrity
//! verification for single-volume cabinet files; writing, multi-volume cabinet
//! sets, encryption, and password flows remain out of scope.
//!
//! # Design notes
//!
//! - **Read-only** — GeeZipX does not expose CAB creation in the product; the
//!   upstream writer API is only used in unit tests to generate portable
//!   fixtures.
//! - **Magic + extension detection** — CAB archives are detected via the `MSCF`
//!   signature and `.cab` extension.
//! - **Path-based** — The reader stores the cabinet path and re-opens the file
//!   on each operation because the upstream [`cab::Cabinet`] consumes a
//!   `Read + Seek` source.
//! - **Windows path normalization** — Stored file names may use backslashes.
//!   GeeZipX normalizes them to forward slashes for listing and extraction so
//!   shared Zip Slip protection catches `..\\evil.txt`, `C:\\foo`, and UNC
//!   paths consistently across platforms.

use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only Microsoft Cabinet reader.
pub struct CabReader {
    path: PathBuf,
    format: ArchiveFormat,
}

impl fmt::Debug for CabReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CabReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl CabReader {
    /// Create a new CAB reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Cab,
        }
    }

    fn open_cabinet(&self) -> GeeZipResult<cab::Cabinet<File>> {
        let file = File::open(&self.path)
            .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
        cab::Cabinet::new(file).map_err(convert_cab_error)
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        let cabinet = self.open_cabinet()?;
        let mut entries = Vec::new();
        for folder in cabinet.folder_entries() {
            for file in folder.file_entries() {
                entries.push(cab_file_to_entry(file)?);
            }
        }
        Ok(entries)
    }

    fn find_original_name(cabinet: &cab::Cabinet<File>, normalized_path: &str) -> Option<String> {
        for folder in cabinet.folder_entries() {
            for file in folder.file_entries() {
                if normalize_cab_entry_path(file.name()) == normalized_path {
                    return Some(file.name().to_owned());
                }
            }
        }
        None
    }
}

impl ArchiveReader for CabReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.collect_entries()
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }

        let mut cabinet = self.open_cabinet()?;
        let original_name = Self::find_original_name(&cabinet, &entry.path).ok_or_else(|| {
            GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            }
        })?;
        let mut reader = cabinet
            .read_file(&original_name)
            .map_err(|e| convert_cab_extract_error(e, &entry.path))?;
        std::io::copy(&mut reader, writer)
            .map_err(|e| GeeZipError::io(e, format!("writing CAB entry '{}'", entry.path)))
    }
}

fn cab_file_to_entry(file: &cab::FileEntry) -> GeeZipResult<Entry> {
    let path = normalize_cab_entry_path(file.name());
    if path.is_empty() {
        return Err(GeeZipError::format(
            "CAB entry is missing a pathname",
            ArchiveFormat::Cab,
        ));
    }

    let modified = file.datetime().map(|dt| {
        crate::archive::datetime_to_timestamp(
            dt.year() as u64,
            dt.month() as u64,
            dt.day() as u64,
            dt.hour() as u64,
            dt.minute() as u64,
            dt.second() as u64,
        )
    });

    Ok(Entry {
        path,
        size: file.uncompressed_size() as u64,
        compressed_size: 0,
        crc32: None,
        modified,
        is_dir: false,
    })
}

fn normalize_cab_entry_path(name: &str) -> String {
    name.replace('\\', "/")
}

fn convert_cab_error(err: std::io::Error) -> GeeZipError {
    match err.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            GeeZipError::format(format!("invalid CAB archive: {err}"), ArchiveFormat::Cab)
        }
        _ => GeeZipError::io(err, "reading CAB archive"),
    }
}

fn convert_cab_extract_error(err: std::io::Error, entry_name: &str) -> GeeZipError {
    match err.kind() {
        std::io::ErrorKind::NotFound => GeeZipError::EntryNotFound {
            name: entry_name.to_owned(),
        },
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => GeeZipError::format(
            format!("failed to extract CAB entry '{entry_name}': {err}"),
            ArchiveFormat::Cab,
        ),
        _ => GeeZipError::io(err, format!("extracting CAB entry '{entry_name}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab::{CabinetBuilder, CompressionType};

    fn create_test_cab(entries: &[(&str, &[u8])], compression: CompressionType) -> Vec<u8> {
        let mut builder = CabinetBuilder::new();
        {
            let folder = builder.add_folder(compression);
            for (path, _) in entries {
                folder.add_file(*path);
            }
        }

        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = builder.build(cursor).expect("CAB writer should be created");
        let mut index = 0usize;
        while let Some(mut file_writer) = writer.next_file().expect("next_file should succeed") {
            assert_eq!(
                file_writer.file_name(),
                entries[index].0,
                "unexpected CAB writer file order"
            );
            file_writer
                .write_all(entries[index].1)
                .expect("fixture file write should succeed");
            index += 1;
        }
        assert_eq!(
            index,
            entries.len(),
            "all CAB fixture entries should be written"
        );
        writer
            .finish()
            .expect("CAB writer should finish")
            .into_inner()
    }

    fn write_archive(temp_dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = temp_dir.path().join(name);
        std::fs::write(&path, bytes).expect("fixture should be written");
        path
    }

    struct CleanupPathGuard {
        target: PathBuf,
        stop_at: PathBuf,
    }

    impl CleanupPathGuard {
        fn new(target: PathBuf) -> Self {
            assert!(
                !target.exists(),
                "test precondition failed: dangerous target already exists: {}",
                target.display()
            );

            let mut stop_at = PathBuf::from("/");
            let mut cursor = target.parent();
            while let Some(parent) = cursor {
                if parent.exists() {
                    stop_at = parent.to_path_buf();
                    break;
                }
                cursor = parent.parent();
            }

            Self { target, stop_at }
        }
    }

    impl Drop for CleanupPathGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.target);

            let mut cursor = self.target.parent().map(|parent| parent.to_path_buf());
            while let Some(parent) = cursor {
                if parent == self.stop_at {
                    break;
                }
                if std::fs::remove_dir(&parent).is_err() {
                    break;
                }
                cursor = parent.parent().map(|ancestor| ancestor.to_path_buf());
            }
        }
    }

    fn assert_cab_extract_all_rejects_dangerous_path(raw_path: &str, expected_path: &str) {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "dangerous.cab",
            &create_test_cab(&[(raw_path, b"bad")], CompressionType::None),
        );
        let out = tempfile::tempdir().unwrap();

        let mut reader = CabReader::new(&archive);
        let entries = reader.entries().expect("entries should still be listable");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, expected_path);

        let escaped_target =
            crate::archive::normalize_path(&out.path().join(std::path::Path::new(expected_path)));
        let _cleanup = CleanupPathGuard::new(escaped_target.clone());

        let report = reader
            .extract_all(out.path(), true)
            .expect("dangerous CAB should report per-file errors");

        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, expected_path);
        assert!(matches!(
            report.errors[0].1,
            GeeZipError::PathTraversal { .. }
        ));
        assert!(
            !escaped_target.exists(),
            "dangerous CAB entry should not create '{}'",
            escaped_target.display()
        );
        assert!(
            std::fs::read_dir(out.path()).unwrap().next().is_none(),
            "dangerous CAB entry should not write anything under '{}'",
            out.path().display()
        );
    }

    #[test]
    fn cab_entries_normalize_paths_and_extract_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "sample.cab",
            &create_test_cab(
                &[("docs\\hello.txt", b"hello"), ("readme.txt", b"readme")],
                CompressionType::MsZip,
            ),
        );

        let mut reader = CabReader::new(&archive);
        let entries = reader.entries().expect("entries should load");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.path == "docs/hello.txt"));
        assert!(entries.iter().any(|entry| entry.path == "readme.txt"));

        let hello = entries
            .iter()
            .find(|entry| entry.path == "docs/hello.txt")
            .unwrap();
        let mut out = Vec::new();
        let bytes = reader
            .extract(hello, &mut out)
            .expect("entry should extract");
        assert_eq!(bytes, 5);
        assert_eq!(out, b"hello");
    }

    #[test]
    fn cab_extract_all_nested_paths_creates_directories() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "nested.cab",
            &create_test_cab(
                &[
                    ("docs/readme.txt", b"docs"),
                    ("nested/deep/file.txt", b"deep"),
                ],
                CompressionType::None,
            ),
        );
        let out = tempfile::tempdir().unwrap();

        let mut reader = CabReader::new(&archive);
        let report = reader
            .extract_all(out.path(), true)
            .expect("archive should extract");

        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert_eq!(report.files_extracted, 2);
        assert_eq!(
            std::fs::read_to_string(out.path().join("docs/readme.txt")).unwrap(),
            "docs"
        );
        assert_eq!(
            std::fs::read_to_string(out.path().join("nested/deep/file.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn cab_extract_all_rejects_parent_dir_paths() {
        assert_cab_extract_all_rejects_dangerous_path("../evil.txt", "../evil.txt");
    }

    #[test]
    fn cab_extract_all_rejects_backslash_parent_dir_paths() {
        assert_cab_extract_all_rejects_dangerous_path("..\\evil.txt", "../evil.txt");
    }

    #[test]
    fn cab_extract_all_rejects_windows_drive_paths() {
        assert_cab_extract_all_rejects_dangerous_path(r"C:\escape.txt", "C:/escape.txt");
    }

    #[test]
    fn cab_extract_all_rejects_unc_paths() {
        assert_cab_extract_all_rejects_dangerous_path(
            r"\\server\share\escape.txt",
            "//server/share/escape.txt",
        );
    }

    #[test]
    fn cab_extract_all_rejects_windows_device_paths() {
        assert_cab_extract_all_rejects_dangerous_path(r"\\?\C:\escape.txt", "//?/C:/escape.txt");
    }

    #[test]
    fn cab_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(&temp, "broken.cab", b"MSCF\0\0\0\0");
        let mut reader = CabReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Cab,
                ..
            }
        ));
        assert!(err.to_string().contains("invalid CAB archive"));
    }

    #[test]
    fn cab_missing_entry_returns_entry_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "missing.cab",
            &create_test_cab(&[("hello.txt", b"hello")], CompressionType::None),
        );
        let mut reader = CabReader::new(&archive);
        let mut sink = Vec::new();

        let err = reader
            .extract(
                &Entry {
                    path: "missing.txt".into(),
                    size: 0,
                    compressed_size: 0,
                    crc32: None,
                    modified: None,
                    is_dir: false,
                },
                &mut sink,
            )
            .unwrap_err();

        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn cab_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "trait.cab",
            &create_test_cab(&[("hello.txt", b"hello")], CompressionType::None),
        );
        let mut reader: Box<dyn ArchiveReader> = Box::new(CabReader::new(&archive));

        assert_eq!(reader.format(), ArchiveFormat::Cab);
        let entries = reader
            .entries()
            .expect("entries should load through trait object");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }
}
