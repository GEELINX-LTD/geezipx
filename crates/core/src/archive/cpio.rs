//! CPIO (`.cpio`) archive reader.
//!
//! GeeZipX exposes CPIO support as a read-only archive view backed by the
//! [`cpio-archive`](https://crates.io/crates/cpio-archive) crate. The current
//! MVP supports `newc` and `odc` archives for listing, extraction, and
//! integrity verification.
//!
//! # Design notes
//!
//! - **Extension-only detection** — GeeZipX maps `.cpio` to this reader but does
//!   not auto-detect CPIO from leading bytes. CPIO has per-entry header magic,
//!   not a stable file-wide magic, so shallow sniffing is prone to false
//!   positives.
//! - **Read-only** — creation/writing is intentionally out of scope for the
//!   current product phase.
//! - **Path-based** — the reader stores the archive path and re-opens the file
//!   for each operation because the upstream reader is a forward-only cursor.
//! - **Filesystem safety first** — regular files and directories extract
//!   normally; symlinks, hard links, device nodes, FIFOs, sockets, and unknown
//!   special entries are reported as unsupported instead of being created on the
//!   host filesystem.

use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use cpio_archive::CpioHeader;

use crate::archive::{
    check_entry_path_safety, normalize_path, ArchiveReader, CancellableWriter, Entry, ExtractReport,
};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use crate::test::TestReport;

const S_IFMT: u32 = 0o170000;
const S_IFIFO: u32 = 0o010000;
const S_IFCHR: u32 = 0o020000;
const S_IFDIR: u32 = 0o040000;
const S_IFBLK: u32 = 0o060000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;
const S_IFSOCK: u32 = 0o140000;

type OpenCpioArchive = Box<cpio_archive::ChainedCpioReader<File>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpioEntryKind {
    Regular,
    Directory,
    Symlink,
    HardLink,
    CharacterDevice,
    BlockDevice,
    Fifo,
    Socket,
    UnknownSpecial,
}

impl CpioEntryKind {
    fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }

    fn is_extractable(self) -> bool {
        matches!(self, Self::Regular | Self::Directory)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Regular => "regular file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::HardLink => "hard link",
            Self::CharacterDevice => "character device",
            Self::BlockDevice => "block device",
            Self::Fifo => "FIFO",
            Self::Socket => "socket",
            Self::UnknownSpecial => "special entry",
        }
    }
}

#[derive(Debug, Clone)]
struct CpioEntryRecord {
    entry: Entry,
    kind: CpioEntryKind,
}

/// Read-only CPIO reader.
pub struct CpioReader {
    path: PathBuf,
    format: ArchiveFormat,
}

impl fmt::Debug for CpioReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpioReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl CpioReader {
    /// Create a new CPIO reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Cpio,
        }
    }

    fn open_archive(&self) -> GeeZipResult<OpenCpioArchive> {
        open_cpio_archive(&self.path)
    }

    fn scan_records(&self) -> GeeZipResult<Vec<CpioEntryRecord>> {
        let mut reader = self.open_archive()?;
        let mut records = Vec::new();

        while let Some(header) = reader
            .read_next()
            .map_err(|err| convert_cpio_error(err, format!("reading '{}'", self.path.display())))?
        {
            records.push(cpio_header_to_record(&*header)?);
        }

        Ok(records)
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        Ok(self
            .scan_records()?
            .into_iter()
            .map(|record| record.entry)
            .collect())
    }
}

impl ArchiveReader for CpioReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.collect_entries()
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut reader = self.open_archive()?;

        while let Some(header) = reader
            .read_next()
            .map_err(|err| convert_cpio_error(err, format!("reading '{}'", self.path.display())))?
        {
            let record = cpio_header_to_record(&*header)?;
            if record.entry.path != entry.path {
                continue;
            }

            return match record.kind {
                CpioEntryKind::Directory => Ok(0),
                CpioEntryKind::Regular => std::io::copy(&mut reader, writer).map_err(|err| {
                    GeeZipError::io(err, format!("extracting CPIO entry '{}'", entry.path))
                }),
                _ => Err(unsupported_extract_error(&record.entry.path, record.kind)),
            };
        }

        Err(GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }

    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport> {
        self.extract_all_with_cancel(dest, overwrite, &|| false)
    }

    fn extract_all_with_cancel(
        &mut self,
        dest: &Path,
        overwrite: bool,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<ExtractReport> {
        let records = self.scan_records()?;
        let mut report = ExtractReport::default();
        let dest = normalize_path(dest);

        for record in &records {
            if is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }

            let entry = &record.entry;
            let entry_path = Path::new(&entry.path);
            let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                Ok(target) => target,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    continue;
                }
            };

            if record.kind.is_directory() {
                if let Err(err) = std::fs::create_dir_all(&target) {
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::io(err, "creating directory"),
                    ));
                    continue;
                }
                report.files_extracted += 1;
                continue;
            }

            if !record.kind.is_extractable() {
                report.errors.push((
                    entry.path.clone(),
                    unsupported_extract_error(&entry.path, record.kind),
                ));
                continue;
            }

            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(err, "creating parent directory"),
                        ));
                        continue;
                    }
                }
            }

            let output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(file) => file,
                    Err(err) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(
                                err,
                                format!("creating output file '{}'", target.display()),
                            ),
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
                    Ok(file) => file,
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                        report.files_skipped += 1;
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        continue;
                    }
                    Err(err) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(
                                err,
                                format!("creating output file '{}'", target.display()),
                            ),
                        ));
                        continue;
                    }
                }
            };

            let mut output = CancellableWriter::new(output, is_cancelled);
            let extract_result = self.extract(entry, &mut output);
            let was_cancelled = output.was_cancelled();
            drop(output);

            match extract_result {
                Ok(bytes) => {
                    if was_cancelled {
                        let _ = std::fs::remove_file(&target);
                        return Err(GeeZipError::Cancelled);
                    }
                    report.files_extracted += 1;
                    report.bytes_extracted += bytes;
                }
                Err(err) => {
                    let _ = std::fs::remove_file(&target);
                    if was_cancelled {
                        return Err(GeeZipError::Cancelled);
                    }
                    report.errors.push((entry.path.clone(), err));
                }
            }
        }

        Ok(report)
    }
}

/// Verify a CPIO archive by streaming every entry payload to `sink()`.
///
/// This helper is used by CLI/GUI `test` flows so archives containing
/// symlinks or other special entries can still be integrity-checked without
/// requiring filesystem creation semantics.
pub fn verify_cpio_archive(path: &Path) -> GeeZipResult<TestReport> {
    let mut reader = open_cpio_archive(path)?;
    let mut entry_count = 0u64;
    let mut bytes_read = 0u64;

    while let Some(header) = reader
        .read_next()
        .map_err(|err| convert_cpio_error(err, format!("verifying '{}'", path.display())))?
    {
        let record = cpio_header_to_record(&*header)?;
        entry_count += 1;

        if record.kind.is_directory() {
            continue;
        }

        let bytes = std::io::copy(&mut reader, &mut std::io::sink()).map_err(|err| {
            GeeZipError::io(err, format!("verifying CPIO entry '{}'", record.entry.path))
        })?;
        bytes_read += bytes;
    }

    Ok(TestReport {
        format: ArchiveFormat::Cpio,
        entry_count,
        bytes_read,
        crc32_verified: false,
    })
}

fn open_cpio_archive(path: &Path) -> GeeZipResult<OpenCpioArchive> {
    let file = File::open(path)
        .map_err(|err| GeeZipError::io(err, format!("opening '{}'", path.display())))?;
    cpio_archive::reader(file)
        .map_err(|err| convert_cpio_error(err, format!("reading '{}'", path.display())))
}

fn cpio_header_to_record(header: &dyn CpioHeader) -> GeeZipResult<CpioEntryRecord> {
    let kind = entry_kind_from_header(header);
    let path = normalize_cpio_entry_path(header.name(), kind.is_directory());
    if path.is_empty() {
        return Err(GeeZipError::format(
            "CPIO entry is missing a pathname",
            ArchiveFormat::Cpio,
        ));
    }

    Ok(CpioEntryRecord {
        entry: Entry {
            path,
            size: header.file_size(),
            compressed_size: 0,
            crc32: None,
            modified: Some(u64::from(header.mtime())),
            is_dir: kind.is_directory(),
        },
        kind,
    })
}

fn entry_kind_from_header(header: &dyn CpioHeader) -> CpioEntryKind {
    let mode = header.mode();
    let file_type = mode & S_IFMT;

    match file_type {
        S_IFDIR => CpioEntryKind::Directory,
        S_IFLNK => CpioEntryKind::Symlink,
        S_IFCHR => CpioEntryKind::CharacterDevice,
        S_IFBLK => CpioEntryKind::BlockDevice,
        S_IFIFO => CpioEntryKind::Fifo,
        S_IFSOCK => CpioEntryKind::Socket,
        S_IFREG => {
            if header.nlink() > 1 && header.file_size() == 0 {
                CpioEntryKind::HardLink
            } else {
                CpioEntryKind::Regular
            }
        }
        _ if header.name().ends_with('/') => CpioEntryKind::Directory,
        _ if header.nlink() > 1 && header.file_size() == 0 => CpioEntryKind::HardLink,
        _ if file_type == 0 => CpioEntryKind::Regular,
        _ => CpioEntryKind::UnknownSpecial,
    }
}

fn normalize_cpio_entry_path(name: &str, is_dir: bool) -> String {
    let mut path = name.replace('\\', "/");
    if is_dir {
        while path.len() > 1 && path.ends_with('/') {
            path.pop();
        }
    }
    path
}

fn unsupported_extract_error(path: &str, kind: CpioEntryKind) -> GeeZipError {
    GeeZipError::format(
        format!(
            "CPIO entry '{}' is a {}; GeeZipX's read-only CPIO MVP does not create these filesystem objects during extraction",
            path,
            kind.label()
        ),
        ArchiveFormat::Cpio,
    )
}

fn convert_cpio_error(err: cpio_archive::Error, context: impl Into<String>) -> GeeZipError {
    let context = context.into();
    match err {
        cpio_archive::Error::Io(io_err) => match io_err.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => GeeZipError::format(
                format!("invalid CPIO archive: {io_err}"),
                ArchiveFormat::Cpio,
            ),
            _ => GeeZipError::io(io_err, context),
        },
        cpio_archive::Error::BadMagic => GeeZipError::format(
            "unsupported or invalid CPIO archive: GeeZipX currently supports only newc and odc streams",
            ArchiveFormat::Cpio,
        ),
        cpio_archive::Error::BadHeaderString
        | cpio_archive::Error::BadHeaderHex(_)
        | cpio_archive::Error::FilenameDecode
        | cpio_archive::Error::ValueTooLarge
        | cpio_archive::Error::SizeMismatch
        | cpio_archive::Error::NotAFile(_) => {
            GeeZipError::format(format!("invalid CPIO archive: {err}"), ArchiveFormat::Cpio)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cpio_archive::OdcBuilder;

    use std::io::Cursor;

    #[derive(Clone, Copy)]
    struct FixtureEntry<'a> {
        path: &'a str,
        data: &'a [u8],
        mode: u32,
        nlink: u32,
    }

    impl<'a> FixtureEntry<'a> {
        fn file(path: &'a str, data: &'a [u8]) -> Self {
            Self {
                path,
                data,
                mode: S_IFREG | 0o644,
                nlink: 1,
            }
        }

        fn directory(path: &'a str) -> Self {
            Self {
                path,
                data: b"",
                mode: S_IFDIR | 0o755,
                nlink: 2,
            }
        }

        fn symlink(path: &'a str, target: &'a [u8]) -> Self {
            Self {
                path,
                data: target,
                mode: S_IFLNK | 0o777,
                nlink: 1,
            }
        }
    }

    fn push_hex(out: &mut Vec<u8>, value: u64, width: usize) {
        out.extend_from_slice(format!("{value:0width$X}", width = width).as_bytes());
    }

    fn append_newc_entry(out: &mut Vec<u8>, inode: u32, entry: FixtureEntry<'_>) {
        out.extend_from_slice(b"070701");
        push_hex(out, u64::from(inode), 8);
        push_hex(out, u64::from(entry.mode), 8);
        push_hex(out, 0, 8);
        push_hex(out, 0, 8);
        push_hex(out, u64::from(entry.nlink), 8);
        push_hex(out, 0, 8);
        push_hex(out, entry.data.len() as u64, 8);
        push_hex(out, 0, 8);
        push_hex(out, 0, 8);
        push_hex(out, 0, 8);
        push_hex(out, 0, 8);
        push_hex(out, (entry.path.len() + 1) as u64, 8);
        push_hex(out, 0, 8);
        out.extend_from_slice(entry.path.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(entry.data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    fn build_newc(entries: &[FixtureEntry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        for (index, entry) in entries.iter().copied().enumerate() {
            append_newc_entry(&mut out, (index + 1) as u32, entry);
        }
        append_newc_entry(
            &mut out,
            0,
            FixtureEntry {
                path: "TRAILER!!!",
                data: b"",
                mode: S_IFREG,
                nlink: 1,
            },
        );
        out
    }

    fn build_odc(entries: &[FixtureEntry<'_>]) -> Vec<u8> {
        let mut builder = OdcBuilder::new(Cursor::new(Vec::new()));
        builder.auto_write_dirs(false);
        for (index, entry) in entries.iter().copied().enumerate() {
            let mut header = builder.next_header();
            header.inode = (index + 1) as u32;
            header.name = entry.path.to_string();
            header.mode = entry.mode;
            header.nlink = entry.nlink;
            header.file_size = entry.data.len() as u64;
            builder
                .append_header_with_data(header, entry.data)
                .expect("ODC fixture entry should be written");
        }
        builder
            .finish()
            .expect("ODC fixture trailer should be written");
        builder
            .into_inner()
            .expect("ODC fixture writer should unwrap")
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

    fn assert_cpio_extract_all_rejects_dangerous_path(raw_path: &str, expected_path: &str) {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "dangerous.cpio",
            &build_newc(&[FixtureEntry::file(raw_path, b"bad")]),
        );
        let out = tempfile::tempdir().unwrap();

        let mut reader = CpioReader::new(&archive);
        let entries = reader.entries().expect("entries should still be listable");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, expected_path);

        let escaped_target =
            crate::archive::normalize_path(&out.path().join(Path::new(expected_path)));
        let _cleanup = CleanupPathGuard::new(escaped_target.clone());

        let report = reader
            .extract_all(out.path(), true)
            .expect("dangerous CPIO should report per-file errors");

        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, expected_path);
        assert!(matches!(
            report.errors[0].1,
            GeeZipError::PathTraversal { .. }
        ));
        assert!(
            !escaped_target.exists(),
            "dangerous CPIO entry should not create '{}'",
            escaped_target.display()
        );
        assert!(
            std::fs::read_dir(out.path()).unwrap().next().is_none(),
            "dangerous CPIO entry should not write anything under '{}'",
            out.path().display()
        );
    }

    #[test]
    fn cpio_entries_extract_newc_file() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "sample.cpio",
            &build_newc(&[
                FixtureEntry::directory("docs/"),
                FixtureEntry::file("docs/hello.txt", b"hello"),
                FixtureEntry::file("readme.txt", b"readme"),
            ]),
        );

        let mut reader = CpioReader::new(&archive);
        let entries = reader.entries().expect("entries should load");
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .any(|entry| entry.path == "docs" && entry.is_dir));
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
    fn cpio_extract_all_odc_nested_paths_creates_directories() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "nested.cpio",
            &build_odc(&[
                FixtureEntry::directory("docs/"),
                FixtureEntry::file("docs/readme.txt", b"docs"),
                FixtureEntry::file("nested/deep/file.txt", b"deep"),
            ]),
        );
        let out = tempfile::tempdir().unwrap();

        let mut reader = CpioReader::new(&archive);
        let report = reader
            .extract_all(out.path(), true)
            .expect("archive should extract");

        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert_eq!(report.files_extracted, 3);
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
    fn cpio_extract_all_rejects_parent_dir_paths() {
        assert_cpio_extract_all_rejects_dangerous_path("../evil.txt", "../evil.txt");
    }

    #[test]
    fn cpio_extract_all_rejects_absolute_paths() {
        assert_cpio_extract_all_rejects_dangerous_path("/tmp/evil.txt", "/tmp/evil.txt");
    }

    #[test]
    fn cpio_extract_all_rejects_windows_drive_paths() {
        assert_cpio_extract_all_rejects_dangerous_path("C:\\evil.txt", "C:/evil.txt");
    }

    #[test]
    fn cpio_extract_all_rejects_unc_paths() {
        assert_cpio_extract_all_rejects_dangerous_path(
            "\\\\server\\share\\evil.txt",
            "//server/share/evil.txt",
        );
    }

    #[test]
    fn cpio_extract_all_rejects_windows_device_paths() {
        assert_cpio_extract_all_rejects_dangerous_path("\\\\.\\NUL", "//./NUL");
    }

    #[test]
    fn cpio_symlink_extract_is_unsupported_and_extract_all_skips_creation() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "symlink.cpio",
            &build_newc(&[
                FixtureEntry::file("regular.txt", b"ok"),
                FixtureEntry::symlink("link.txt", b"regular.txt"),
            ]),
        );
        let out = tempfile::tempdir().unwrap();

        let mut reader = CpioReader::new(&archive);
        let entries = reader.entries().unwrap();
        let link = entries
            .iter()
            .find(|entry| entry.path == "link.txt")
            .unwrap();
        let err = reader.extract(link, &mut Vec::new()).unwrap_err();
        assert!(err
            .to_string()
            .contains("does not create these filesystem objects"));

        let report = reader.extract_all(out.path(), true).unwrap();
        assert_eq!(report.files_extracted, 1);
        assert_eq!(
            std::fs::read_to_string(out.path().join("regular.txt")).unwrap(),
            "ok"
        );
        assert!(!out.path().join("link.txt").exists());
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, "link.txt");
    }

    #[test]
    fn verify_cpio_archive_reads_special_entry_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "verify.cpio",
            &build_newc(&[
                FixtureEntry::file("regular.txt", b"ok"),
                FixtureEntry::symlink("link.txt", b"regular.txt"),
            ]),
        );

        let report = verify_cpio_archive(&archive).expect("verification should pass");
        assert_eq!(report.format, ArchiveFormat::Cpio);
        assert_eq!(report.entry_count, 2);
        assert_eq!(report.bytes_read, 13);
        assert!(!report.crc32_verified);
    }

    #[test]
    fn cpio_invalid_magic_reports_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(&temp, "invalid.cpio", b"070702bad");
        let mut reader = CpioReader::new(&archive);
        let err = reader.entries().unwrap_err();
        assert!(err.to_string().contains("newc and odc"));
    }

    #[test]
    fn cpio_missing_entry_returns_entry_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "missing.cpio",
            &build_newc(&[FixtureEntry::file("present.txt", b"hello")]),
        );
        let mut reader = CpioReader::new(&archive);
        let missing = Entry {
            path: "missing.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        };

        let err = reader.extract(&missing, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn cpio_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = write_archive(
            &temp,
            "trait-object.cpio",
            &build_newc(&[FixtureEntry::file("hello.txt", b"hello world")]),
        );

        let mut reader: Box<dyn ArchiveReader> = Box::new(CpioReader::new(&archive));
        let entries = reader
            .entries()
            .expect("entries should load through trait object");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");

        let mut out = Vec::new();
        let bytes = reader
            .extract(&entries[0], &mut out)
            .expect("extract should work through trait object");
        assert_eq!(bytes, 11);
        assert_eq!(out, b"hello world");
    }
}
