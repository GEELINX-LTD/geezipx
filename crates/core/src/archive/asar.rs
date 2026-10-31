//! ASAR archive reader and writer implementation.
//!
//! Built on top of the [`asar`] crate's header parser. GeeZipX keeps the
//! extraction logic in-house so it can enforce the same path-safety guarantees
//! across packed entries, unpacked entries (`.asar.unpacked`), and ASAR
//! symlink metadata.
//!
//! # Design notes
//!
//! - **Disk-backed creation** — Inputs are snapshotted and indexed without
//!   retaining payloads in RAM; explicit empty directories are preserved.
//! - **Extension / explicit-format only** — ASAR does not have a reliable fixed
//!   magic header, so detection is handled by `detect_from_extension()` and
//!   explicit `--format asar` parsing.
//! - **Seekable payloads** — Only the JSON index is loaded and cached. Packed
//!   entries stream from a file handle at their recorded offsets.
//! - **Safe unpacked handling** — unpacked entries are read from the sibling
//!   `.asar.unpacked` directory only after validating that the path stays under
//!   that directory and that no symlink component is traversed.
//! - **Safe symlink handling** — symlink entries are treated as aliases to other
//!   in-archive entries only when their targets resolve to a safe in-archive
//!   path. Unsafe or cyclic targets fail closed.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs::File as FsFile;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use asar::{header::FileLocation, Header};

use crate::archive::{is_entry_path_dangerous, normalize_path, ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only ASAR archive reader.
pub struct AsarReader {
    path: PathBuf,
    format: ArchiveFormat,
    loaded: Option<LoadedAsar>,
}

impl fmt::Debug for AsarReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsarReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl AsarReader {
    /// Create a new ASAR reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Asar,
            loaded: None,
        }
    }

    fn load_archive(&mut self) -> GeeZipResult<&mut LoadedAsar> {
        if self.loaded.is_none() {
            let mut file = FsFile::open(&self.path)
                .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
            let length = usize::try_from(file.metadata()?.len()).map_err(|_| {
                GeeZipError::format("ASAR exceeds addressable size", ArchiveFormat::Asar)
            })?;
            // Validate untrusted JSON lengths before the upstream parser allocates.
            let mut prefix = [0u8; 16];
            file.read_exact(&mut prefix)?;
            let header_size = u32::from_le_bytes(prefix[4..8].try_into().unwrap()) as usize;
            let json_size = u32::from_le_bytes(prefix[12..16].try_into().unwrap()) as usize;
            if prefix[..4] != 4u32.to_le_bytes()
                || json_size > 64 * 1024 * 1024
                || header_size < 8
                || json_size > header_size - 8
                || header_size.checked_add(8).is_none_or(|end| end > length)
            {
                return Err(GeeZipError::format(
                    "invalid or oversized ASAR header (maximum JSON index: 64 MiB)",
                    ArchiveFormat::Asar,
                ));
            }
            file.seek(SeekFrom::Start(0))?;
            let (header, payload_offset) = Header::read(&mut std::io::BufReader::new(&mut file))
                .map_err(convert_asar_error)?;
            let mut records = BTreeMap::new();
            walk_header(Path::new(""), &header, payload_offset, length, &mut records)?;
            self.loaded = Some(LoadedAsar { file, records });
        }
        Ok(self.loaded.as_mut().expect("index initialized"))
    }

    fn read_unpacked_entry(
        &self,
        entry_path: &str,
        writer: &mut dyn Write,
        integrity: Option<&asar::header::FileIntegrity>,
    ) -> GeeZipResult<u64> {
        let unpacked_root = self.path.with_extension("asar.unpacked");
        let root_meta = std::fs::symlink_metadata(&unpacked_root).map_err(|e| {
            GeeZipError::io(
                e,
                format!(
                    "opening unpacked ASAR directory '{}' for '{}'",
                    unpacked_root.display(),
                    entry_path
                ),
            )
        })?;

        if root_meta.file_type().is_symlink() {
            return Err(GeeZipError::format(
                format!(
                    "unsafe unpacked ASAR root '{}' is a symlink",
                    unpacked_root.display()
                ),
                ArchiveFormat::Asar,
            ));
        }
        if !root_meta.is_dir() {
            return Err(GeeZipError::format(
                format!(
                    "unpacked ASAR root '{}' is not a directory",
                    unpacked_root.display()
                ),
                ArchiveFormat::Asar,
            ));
        }

        let normalized = normalize_path(Path::new(entry_path));
        if is_entry_path_dangerous(&normalized) {
            return Err(GeeZipError::format(
                format!("unsafe unpacked ASAR entry path '{}'", entry_path),
                ArchiveFormat::Asar,
            ));
        }

        let mut current = unpacked_root.clone();
        let mut components = normalized.components().peekable();
        while let Some(component) = components.next() {
            match component {
                Component::CurDir => continue,
                Component::Normal(name) => {
                    current.push(name);
                    let meta = std::fs::symlink_metadata(&current).map_err(|e| {
                        GeeZipError::io(
                            e,
                            format!(
                                "reading unpacked ASAR entry component '{}' for '{}'",
                                current.display(),
                                entry_path
                            ),
                        )
                    })?;
                    if meta.file_type().is_symlink() {
                        return Err(GeeZipError::format(
                            format!(
                                "unsafe unpacked ASAR entry '{}' traverses symlink '{}'",
                                entry_path,
                                current.display()
                            ),
                            ArchiveFormat::Asar,
                        ));
                    }
                    if components.peek().is_some() && !meta.is_dir() {
                        return Err(GeeZipError::format(
                            format!(
                                "invalid unpacked ASAR path '{}' stops at non-directory '{}'",
                                entry_path,
                                current.display()
                            ),
                            ArchiveFormat::Asar,
                        ));
                    }
                }
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(GeeZipError::format(
                        format!("unsafe unpacked ASAR entry path '{}'", entry_path),
                        ArchiveFormat::Asar,
                    ));
                }
            }
        }

        let file_meta = std::fs::metadata(&current).map_err(|e| {
            GeeZipError::io(
                e,
                format!("statting unpacked ASAR entry '{}'", current.display()),
            )
        })?;
        if file_meta.is_dir() {
            return Err(GeeZipError::format(
                format!(
                    "unpacked ASAR entry '{}' resolves to a directory, not a file",
                    entry_path
                ),
                ArchiveFormat::Asar,
            ));
        }

        let mut input = FsFile::open(&current).map_err(|e| {
            GeeZipError::io(
                e,
                format!("opening unpacked ASAR entry '{}'", current.display()),
            )
        })?;
        integrity::copy_verified(&mut input, writer, integrity)
    }
}

impl ArchiveReader for AsarReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        let loaded = self.load_archive()?;
        Ok(loaded
            .records
            .values()
            .map(|record| record.entry.clone())
            .collect())
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let loaded = self.load_archive()?;
        let record = resolve_record(&entry.path, &loaded.records, &mut HashSet::new())?.clone();

        match &record.source {
            AsarSource::Directory => {
                if entry.is_dir {
                    Ok(0)
                } else {
                    Err(GeeZipError::format(
                        format!("ASAR entry '{}' resolves to a directory", entry.path),
                        ArchiveFormat::Asar,
                    ))
                }
            }
            AsarSource::Packed {
                absolute_offset,
                size,
            } => {
                loaded.file.seek(SeekFrom::Start(*absolute_offset as u64))?;
                let bytes = integrity::copy_verified(
                    &mut (&mut loaded.file).take(*size as u64),
                    writer,
                    record.integrity.as_ref(),
                )?;
                if bytes != *size as u64 {
                    return Err(GeeZipError::format(
                        "truncated ASAR payload",
                        ArchiveFormat::Asar,
                    ));
                }
                Ok(bytes)
            }
            AsarSource::Unpacked => {
                self.read_unpacked_entry(&record.entry.path, writer, record.integrity.as_ref())
            }
            AsarSource::Symlink { .. } => Err(GeeZipError::format(
                format!("ASAR symlink entry '{}' could not be resolved", entry.path),
                ArchiveFormat::Asar,
            )),
        }
    }
}

// ---------------------------------------------------------------------------
mod integrity;
mod writer;
pub use writer::AsarWriter;

#[derive(Debug)]
struct LoadedAsar {
    file: FsFile,
    records: BTreeMap<String, AsarEntryRecord>,
}

#[derive(Debug, Clone)]
struct AsarEntryRecord {
    entry: Entry,
    source: AsarSource,
    integrity: Option<asar::header::FileIntegrity>,
}

impl AsarEntryRecord {
    fn directory(path: String) -> Self {
        Self {
            entry: Entry {
                path,
                size: 0,
                compressed_size: 0,
                crc32: None,
                modified: None,
                is_dir: true,
            },
            source: AsarSource::Directory,
            integrity: None,
        }
    }

    fn file(
        path: String,
        size: u64,
        source: AsarSource,
        integrity: Option<asar::header::FileIntegrity>,
    ) -> Self {
        Self {
            entry: Entry {
                path,
                size,
                compressed_size: size,
                crc32: None,
                modified: None,
                is_dir: false,
            },
            source,
            integrity,
        }
    }

    fn symlink(path: String, raw_target: PathBuf) -> Self {
        Self {
            entry: Entry {
                path,
                size: 0,
                compressed_size: 0,
                crc32: None,
                modified: None,
                is_dir: false,
            },
            source: AsarSource::Symlink { raw_target },
            integrity: None,
        }
    }
}

#[derive(Debug, Clone)]
enum AsarSource {
    Directory,
    Packed { absolute_offset: usize, size: usize },
    Unpacked,
    Symlink { raw_target: PathBuf },
}

fn walk_header(
    current_path: &Path,
    header: &Header,
    payload_offset: usize,
    data_len: usize,
    records: &mut BTreeMap<String, AsarEntryRecord>,
) -> GeeZipResult<()> {
    match header {
        Header::Directory { files } => {
            let dir_path = normalize_entry_path(current_path);
            if !dir_path.is_empty() {
                insert_parent_directories(&dir_path, records);
                records
                    .entry(dir_path.clone())
                    .or_insert_with(|| AsarEntryRecord::directory(dir_path));
            }

            let mut children: Vec<_> = files.iter().collect();
            children.sort_by(|a, b| a.0.cmp(b.0));
            for (name, child) in children {
                let child_path = if current_path.as_os_str().is_empty() {
                    PathBuf::from(name)
                } else {
                    current_path.join(name)
                };
                walk_header(&child_path, child, payload_offset, data_len, records)?;
            }
        }
        Header::File(file) => {
            let entry_path = normalize_entry_path(current_path);
            insert_parent_directories(&entry_path, records);
            let size = file.size() as u64;
            let source = match file.location() {
                FileLocation::Offset { offset } => {
                    let absolute_offset = payload_offset.checked_add(offset).ok_or_else(|| {
                        GeeZipError::format(
                            format!("ASAR entry '{}' offset overflowed", entry_path),
                            ArchiveFormat::Asar,
                        )
                    })?;
                    let end = absolute_offset.checked_add(file.size()).ok_or_else(|| {
                        GeeZipError::format(
                            format!("ASAR entry '{}' size overflowed", entry_path),
                            ArchiveFormat::Asar,
                        )
                    })?;
                    if end > data_len {
                        return Err(GeeZipError::format(
                            format!(
                                "truncated ASAR entry '{}' (offset {}, size {}, archive {})",
                                entry_path,
                                offset,
                                file.size(),
                                data_len
                            ),
                            ArchiveFormat::Asar,
                        ));
                    }
                    AsarSource::Packed {
                        absolute_offset,
                        size: file.size(),
                    }
                }
                FileLocation::Unpacked { .. } => AsarSource::Unpacked,
            };
            records.insert(
                entry_path.clone(),
                AsarEntryRecord::file(entry_path, size, source, file.integrity().cloned()),
            );
        }
        Header::Link { link } => {
            let entry_path = normalize_entry_path(current_path);
            insert_parent_directories(&entry_path, records);
            records.insert(
                entry_path.clone(),
                AsarEntryRecord::symlink(entry_path, link.clone()),
            );
        }
    }

    Ok(())
}

fn resolve_record<'a>(
    entry_path: &str,
    records: &'a BTreeMap<String, AsarEntryRecord>,
    seen: &mut HashSet<String>,
) -> GeeZipResult<&'a AsarEntryRecord> {
    if !seen.insert(entry_path.to_owned()) {
        return Err(GeeZipError::format(
            format!(
                "cyclic ASAR symlink resolution detected at '{}'",
                entry_path
            ),
            ArchiveFormat::Asar,
        ));
    }

    let record = records
        .get(entry_path)
        .ok_or_else(|| GeeZipError::EntryNotFound {
            name: entry_path.to_owned(),
        })?;

    match &record.source {
        AsarSource::Symlink { raw_target } => {
            let resolved_target = resolve_symlink_target(entry_path, raw_target)?;
            resolve_record(&resolved_target, records, seen)
        }
        _ => Ok(record),
    }
}

fn resolve_symlink_target(entry_path: &str, raw_target: &Path) -> GeeZipResult<String> {
    let parent = Path::new(entry_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let raw_target = raw_target.to_string_lossy().replace('\\', "/");
    if looks_like_windows_absolute_or_unc(&raw_target) {
        return Err(GeeZipError::format(
            format!(
                "unsafe ASAR symlink target '{}' for '{}'",
                raw_target, entry_path
            ),
            ArchiveFormat::Asar,
        ));
    }
    let joined = normalize_path(&parent.join(&raw_target));
    if is_entry_path_dangerous(&joined) {
        return Err(GeeZipError::format(
            format!(
                "unsafe ASAR symlink target '{}' for '{}'",
                raw_target, entry_path
            ),
            ArchiveFormat::Asar,
        ));
    }

    let resolved = normalize_entry_path(&joined);
    if resolved.is_empty() {
        return Err(GeeZipError::format(
            format!("empty ASAR symlink target for '{}'", entry_path),
            ArchiveFormat::Asar,
        ));
    }
    Ok(resolved)
}

fn normalize_entry_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn insert_parent_directories(path: &str, records: &mut BTreeMap<String, AsarEntryRecord>) {
    if path.is_empty() || path.starts_with('/') || looks_like_windows_absolute_or_unc(path) {
        return;
    }

    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() <= 1 {
        return;
    }

    let mut current = String::new();
    for part in parts.iter().take(parts.len() - 1) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        let dir = current.clone();
        records
            .entry(dir.clone())
            .or_insert_with(|| AsarEntryRecord::directory(dir));
    }
}

fn looks_like_windows_absolute_or_unc(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("//")
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\'))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn convert_asar_error(err: asar::Error) -> GeeZipError {
    match err {
        asar::Error::Io(e) => GeeZipError::io(e, "reading ASAR archive"),
        asar::Error::UnpackedIoError { path, err } => GeeZipError::io(
            err,
            format!("reading unpacked ASAR entry '{}'", path.display()),
        ),
        asar::Error::Json(e) => GeeZipError::format(
            format!("invalid ASAR header JSON: {e}"),
            ArchiveFormat::Asar,
        ),
        asar::Error::Truncated => {
            GeeZipError::format("truncated ASAR archive", ArchiveFormat::Asar)
        }
        asar::Error::HashMismatch {
            file,
            block,
            expected,
            actual,
        } => GeeZipError::format(
            format!(
                "ASAR integrity mismatch for '{}'{} (expected {}, got {})",
                file.display(),
                block
                    .map(|idx| format!(", block #{idx}"))
                    .unwrap_or_default(),
                bytes_to_hex(&expected),
                bytes_to_hex(&actual)
            ),
            ArchiveFormat::Asar,
        ),
        asar::Error::FileAlreadyWritten(path) => GeeZipError::format(
            format!("duplicate ASAR file '{}'", path.display()),
            ArchiveFormat::Asar,
        ),
        asar::Error::InvalidHashAlgorithm(alg) => GeeZipError::format(
            format!("invalid ASAR hash algorithm '{alg}'"),
            ArchiveFormat::Asar,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use crate::archive::{ArchiveReader, ArchiveWriter};

    fn create_basic_asar() -> Vec<u8> {
        let mut writer = asar::AsarWriter::new();
        writer
            .write_file("dir/hello.txt", b"hello from asar", false)
            .expect("write_file should work");
        writer
            .write_file("root.txt", b"root file", false)
            .expect("write_file should work");
        let mut out = Cursor::new(Vec::<u8>::new());
        writer.finalize(&mut out).expect("finalize should work");
        out.into_inner()
    }

    fn create_symlink_asar() -> Vec<u8> {
        let mut writer = asar::AsarWriter::new();
        writer
            .write_file("dir/hello.txt", b"hello from asar", false)
            .expect("write_file should work");
        writer
            .write_symlink("alias.txt", "dir/hello.txt")
            .expect("write_symlink should work");
        let mut out = Cursor::new(Vec::<u8>::new());
        writer.finalize(&mut out).expect("finalize should work");
        out.into_inner()
    }

    fn build_raw_asar(header_json: &str, payload: &[u8]) -> Vec<u8> {
        let mut json = header_json.as_bytes().to_vec();
        let json_size = json.len() as u32;
        let aligned_json_size = json_size + (4 - (json_size % 4)) % 4;
        json.resize(aligned_json_size as usize, 0);

        let mut out = Vec::with_capacity(16 + json.len() + payload.len());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&(aligned_json_size + 8).to_le_bytes());
        out.extend_from_slice(&(aligned_json_size + 4).to_le_bytes());
        out.extend_from_slice(&json_size.to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(payload);
        out
    }

    fn write_archive(temp_dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = temp_dir.path().join(name);
        fs::write(&path, bytes).expect("archive should be written");
        path
    }

    #[test]
    fn asar_sparse_payload_indexes_without_loading_file_and_detects_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.asar");
        let header = r#"{"files":{"large.bin":{"size":536870912,"offset":"0"}}}"#;
        let bytes = build_raw_asar(header, &[]);
        let mut file = FsFile::create(&path).unwrap();
        file.write_all(&bytes).unwrap();
        file.set_len(bytes.len() as u64 + 536870912).unwrap();
        let mut reader = AsarReader::new(&path);
        let entries = reader.entries().unwrap();
        assert_eq!(entries[0].size, 536870912);
        // The cached handle must still detect payload truncation after indexing.
        file.set_len(bytes.len() as u64).unwrap();
        assert!(reader.extract(&entries[0], &mut std::io::sink()).is_err());
        let mut invalid = bytes;
        invalid[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&path, invalid).unwrap();
        assert!(AsarReader::new(&path).entries().is_err());
    }

    #[test]
    fn asar_list_entries_and_extract_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "app.asar", &create_basic_asar());

        let mut reader = AsarReader::new(&archive);
        let entries = reader.entries().expect("entries should load");
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert!(paths.contains(&"dir"));
        assert!(paths.contains(&"dir/hello.txt"));
        assert!(paths.contains(&"root.txt"));

        let hello = entries
            .iter()
            .find(|entry| entry.path == "dir/hello.txt")
            .expect("hello entry should exist")
            .clone();
        let mut out = Vec::new();
        let bytes = reader
            .extract(&hello, &mut out)
            .expect("extract should work");
        assert_eq!(bytes, b"hello from asar".len() as u64);
        assert_eq!(out, b"hello from asar");
    }

    #[test]
    fn asar_extract_all_basic() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "app.asar", &create_basic_asar());
        let dest = temp.path().join("out");

        let mut reader = AsarReader::new(&archive);
        let report = reader
            .extract_all(&dest, true)
            .expect("extract_all should work");
        assert_eq!(report.errors.len(), 0);
        assert!(dest.join("dir").is_dir());
        assert_eq!(
            fs::read(dest.join("dir/hello.txt")).unwrap(),
            b"hello from asar"
        );
        assert_eq!(fs::read(dest.join("root.txt")).unwrap(), b"root file");
    }

    #[test]
    fn asar_symlink_alias_extracts_target_contents() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "app.asar", &create_symlink_asar());

        let mut reader = AsarReader::new(&archive);
        let entries = reader.entries().expect("entries should load");
        let alias = entries
            .iter()
            .find(|entry| entry.path == "alias.txt")
            .expect("alias entry should exist")
            .clone();
        let mut out = Vec::new();
        reader
            .extract(&alias, &mut out)
            .expect("alias extract should work");
        assert_eq!(out, b"hello from asar");
    }

    #[test]
    fn asar_truncated_payload_fails() {
        let mut bytes = create_basic_asar();
        bytes.truncate(bytes.len() - 4);
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "broken.asar", &bytes);

        let mut reader = AsarReader::new(&archive);
        let err = reader.entries().expect_err("truncated asar should fail");
        let msg = err.to_string();
        assert!(msg.contains("truncated ASAR entry") || msg.contains("truncated ASAR archive"));
    }

    #[test]
    fn asar_traversal_entry_is_blocked_on_extract_all() {
        let header = r#"{"files":{"../evil.txt":{"size":5,"offset":"0"}}}"#;
        let bytes = build_raw_asar(header, b"owned");
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "evil.asar", &bytes);
        let dest = temp.path().join("out");
        let outside = temp.path().join("evil.txt");

        let mut reader = AsarReader::new(&archive);
        let report = reader
            .extract_all(&dest, true)
            .expect("extract_all should complete");
        assert_eq!(report.files_extracted, 0);
        assert!(!report.errors.is_empty());
        assert!(!outside.exists());
    }

    #[test]
    fn asar_windows_style_absolute_entry_is_blocked() {
        let header = r#"{"files":{"C:\\evil.txt":{"size":5,"offset":"0"}}}"#;
        let bytes = build_raw_asar(header, b"owned");
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "evil.asar", &bytes);
        let dest = temp.path().join("out");

        let mut reader = AsarReader::new(&archive);
        let report = reader
            .extract_all(&dest, true)
            .expect("extract_all should complete");
        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        let msg = report.errors[0].1.to_string();
        assert!(msg.contains("path traversal"), "msg: {msg}");
    }

    #[test]
    fn asar_unsafe_symlink_target_fails_closed() {
        let header = r#"{"files":{"alias.txt":{"link":"../../outside.txt"}}}"#;
        let bytes = build_raw_asar(header, &[]);
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "symlink.asar", &bytes);

        let mut reader = AsarReader::new(&archive);
        let alias = reader
            .entries()
            .expect("entries should load")
            .into_iter()
            .find(|entry| entry.path == "alias.txt")
            .expect("alias should exist");
        let err = reader
            .extract(&alias, &mut Vec::new())
            .expect_err("unsafe symlink should fail");
        assert!(err.to_string().contains("unsafe ASAR symlink target"));
    }

    #[cfg(unix)]
    #[test]
    fn asar_unpacked_symlink_escape_fails_closed() {
        let header = r#"{"files":{"safe.txt":{"size":7,"unpacked":true}}}"#;
        let bytes = build_raw_asar(header, &[]);
        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "app.asar", &bytes);
        let unpacked_root = archive.with_extension("asar.unpacked");
        fs::create_dir_all(&unpacked_root).unwrap();

        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, unpacked_root.join("safe.txt")).unwrap();

        let mut reader = AsarReader::new(&archive);
        let safe = reader
            .entries()
            .expect("entries should load")
            .into_iter()
            .find(|entry| entry.path == "safe.txt")
            .expect("safe.txt should exist");
        let err = reader
            .extract(&safe, &mut Vec::new())
            .expect_err("symlinked unpacked file should fail");
        assert!(err.to_string().contains("traverses symlink"));
    }

    #[test]
    fn asar_trait_object_safety() {
        fn use_reader(_reader: &mut dyn ArchiveReader) {}

        let temp = tempfile::TempDir::new().unwrap();
        let archive = write_archive(&temp, "app.asar", &create_basic_asar());
        let mut reader = AsarReader::new(&archive);
        use_reader(&mut reader);
    }

    #[test]
    fn asar_writer_roundtrip() {
        use std::io::Cursor;

        let mut buf = Cursor::new(Vec::new());
        let writer = AsarWriter::new(&mut buf);
        let mut boxed_writer: Box<dyn ArchiveWriter> = Box::new(writer);
        boxed_writer
            .add_entry_from_reader(Path::new("hello.txt"), &mut Cursor::new(b"hello asar"))
            .unwrap();
        boxed_writer.add_directory(Path::new("subdir")).unwrap();
        let total = boxed_writer.finish().unwrap();
        assert!(total > 0);

        let out_buf = buf.into_inner();

        let temp = tempfile::TempDir::new().unwrap();
        let archive_path = temp.path().join("test.asar");
        std::fs::write(&archive_path, &out_buf).unwrap();

        let mut reader = AsarReader::new(&archive_path);
        let entries = reader.entries().unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"hello.txt"));

        let hello = entries
            .iter()
            .find(|e| e.path == "hello.txt")
            .unwrap()
            .clone();
        let mut out = Vec::new();
        let bytes = reader.extract(&hello, &mut out).unwrap();
        assert_eq!(bytes, 10);
        assert_eq!(out, b"hello asar");
    }
}
