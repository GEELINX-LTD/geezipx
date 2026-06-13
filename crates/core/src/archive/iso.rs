//! ISO disc image (`.iso`) archive reader and writer (ISO 9660 Level 1).
//!
//! Reading is delegated to `isomage` for detection and parsing, while extraction
//! flows through GeeZipX's shared `ArchiveReader` interface so path traversal
//! protection stays centralized in `ArchiveReader::extract_all`.
//!
//! Writing produces ISO 9660 Level 1 images via `hadris-iso`. Entries are
//! buffered in memory during `add_entry_from_reader` and written in a single
//! pass when `finish()` is called (non-streaming by ISO format constraints).
//! Joliet, Rock Ridge, and El Torito are not currently exposed.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};

use isomage::TreeNode;

use crate::archive::{is_entry_path_dangerous, ArchiveReader, ArchiveWriter, CountWriter, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use std::sync::Arc;

use hadris_iso::read::PathSeparator;
use hadris_iso::write::options::{BaseIsoLevel, CreationFeatures, FormatOptions};
use hadris_iso::write::{File as HadrisFile, InputFiles, IsoImageWriter};

/// Maximum single file size for ISO 9660 Level 1 (4 GiB).
const ISO_MAX_ENTRY_SIZE: u64 = u32::MAX as u64;

fn validate_iso_entry_path(path: &std::path::Path) -> GeeZipResult<String> {
    let raw = path.to_str().ok_or_else(|| {
        GeeZipError::format(
            format!("non-UTF-8 path: {}", path.display()),
            ArchiveFormat::Iso,
        )
    })?;

    let normalized = raw.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');

    if normalized.is_empty() || normalized == "." || normalized == ".." {
        return Err(GeeZipError::format(
            format!("invalid ISO entry path: {raw}"),
            ArchiveFormat::Iso,
        ));
    }

    if is_entry_path_dangerous(std::path::Path::new(normalized)) {
        return Err(GeeZipError::format(
            format!("invalid ISO entry path: {raw}"),
            ArchiveFormat::Iso,
        ));
    }

    Ok(normalized.to_string())
}
/// Read-only ISO image reader.
pub struct IsoReader<R: Read + Seek + Send> {
    inner: R,
    root: Option<TreeNode>,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for IsoReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IsoReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> IsoReader<R> {
    /// Create an ISO reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        Self {
            inner: reader,
            root: None,
            format: ArchiveFormat::Iso,
        }
    }

    fn ensure_parsed(&mut self) -> GeeZipResult<()> {
        if self.root.is_some() {
            return Ok(());
        }

        self.inner.seek(SeekFrom::Start(0))?;
        let root = isomage::detect_and_parse_filesystem(&mut self.inner, "archive.iso")
            .map_err(|err| convert_iso_error(err, "parsing ISO filesystem"))?;
        self.root = Some(root);
        Ok(())
    }
}

impl IsoReader<std::io::Cursor<Vec<u8>>> {
    /// Create an ISO reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        Self::new(std::io::Cursor::new(buf))
    }
}

// ---------------------------------------------------------------------------
// IsoWriter
// ---------------------------------------------------------------------------

/// ISO 9660 disc image writer.
///
/// Entries are buffered in memory during `add_entry_from_reader` calls and
/// written in a single pass when `finish()` is called.
pub struct IsoWriter<W: std::io::Write + Send> {
    inner: Option<W>,
    entries: Vec<BufferedIsoEntry>,
    format: ArchiveFormat,
}

#[derive(Clone)]
struct BufferedIsoEntry {
    path: String,
    data: Vec<u8>,
    is_dir: bool,
}

impl<W: std::io::Write + Send> std::fmt::Debug for IsoWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsoWriter")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Build a tree of `HadrisFile` entries from a flat list of buffered entries.
///
/// Missing intermediate directory nodes are automatically created so that
/// deeply nested files (e.g. `a/b/c/f.txt`) are not silently dropped when
/// only the leaf entries were registered.
fn build_hadris_files(entries: &[BufferedIsoEntry]) -> GeeZipResult<Vec<HadrisFile>> {
    use std::collections::HashSet;

    // Collect all directory paths: explicit + synthetic (from file parent chains).
    let mut dir_paths: HashSet<String> = HashSet::new();
    for entry in entries {
        if entry.is_dir {
            dir_paths.insert(entry.path.clone());
        }
        let p = std::path::Path::new(&entry.path);
        let mut current = p.parent();
        while let Some(parent) = current {
            if let Some(s) = parent.to_str() {
                if s.is_empty() {
                    break;
                }
                dir_paths.insert(s.to_string());
                current = parent.parent();
            } else {
                break;
            }
        }
    }

    // Index entries by path for fast lookup.
    let mut entry_by_path: std::collections::HashMap<&str, &BufferedIsoEntry> =
        std::collections::HashMap::new();
    for entry in entries {
        entry_by_path.insert(&entry.path, entry);
    }

    /// Recursively build children for `prefix`.  Entries whose parent is
    /// `prefix` become direct children.  Synthetic directories (paths in
    /// `dir_paths` that have `prefix` as parent but have no explicit entry)
    /// are also emitted as empty directories.
    fn build_subtree(
        entry_by_path: &std::collections::HashMap<&str, &BufferedIsoEntry>,
        dir_paths: &HashSet<String>,
        prefix: &str,
    ) -> Vec<HadrisFile> {
        let mut children: Vec<HadrisFile> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();

        // 1) Entries whose parent is `prefix` (real entries).
        for (path, entry) in entry_by_path {
            let p = std::path::Path::new(path);
            let parent = p.parent().and_then(|x| x.to_str()).unwrap_or("");
            if parent != prefix {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.is_empty() || seen.contains(name) {
                continue;
            }
            seen.insert(name);

            if entry.is_dir {
                children.push(HadrisFile::Directory {
                    name: Arc::new(name.to_string()),
                    children: build_subtree(entry_by_path, dir_paths, path),
                });
            } else {
                children.push(HadrisFile::File {
                    name: Arc::new(name.to_string()),
                    contents: entry.data.clone(),
                });
            }
        }

        // 2) Synthetic directories: paths in `dir_paths` whose parent is
        //    `prefix` but that have no explicit entry.
        for dir_path in dir_paths {
            let p = std::path::Path::new(dir_path);
            let parent = p.parent().and_then(|x| x.to_str()).unwrap_or("");
            if parent != prefix {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.is_empty() || seen.contains(name) {
                continue;
            }
            seen.insert(name);

            children.push(HadrisFile::Directory {
                name: Arc::new(name.to_string()),
                children: build_subtree(entry_by_path, dir_paths, dir_path),
            });
        }

        children
    }

    Ok(build_subtree(&entry_by_path, &dir_paths, ""))
}

impl<W: std::io::Write + Send> IsoWriter<W> {
    /// Create a new ISO writer targeting the given output.
    pub fn new(writer: W) -> Self {
        Self {
            inner: Some(writer),
            entries: Vec::new(),
            format: ArchiveFormat::Iso,
        }
    }

    /// Finalise the archive and return the inner writer alongside the bytes written.
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        let mut writer = self.inner.take().ok_or_else(|| {
            GeeZipError::format("ISO writer already finalised", ArchiveFormat::Iso)
        })?;

        if self.entries.is_empty() {
            return Err(GeeZipError::format(
                "cannot create an empty ISO image",
                ArchiveFormat::Iso,
            ));
        }

        let hadris_files = build_hadris_files(&self.entries)?;

        let input_files = InputFiles {
            path_separator: PathSeparator::ForwardSlash,
            files: hadris_files,
        };

        let format_options = FormatOptions {
            volume_name: "GEEZIPX".to_string(),
            sector_size: 2048,
            system_id: None,
            volume_set_id: None,
            publisher_id: None,
            preparer_id: None,
            application_id: None,
            features: CreationFeatures {
                filenames: BaseIsoLevel::Level1 {
                    supports_lowercase: true,
                    supports_rrip: false,
                },
                long_filenames: false,
                joliet: None,
                rock_ridge: None,
                el_torito: None,
                hybrid_boot: None,
            },
            path_separator: PathSeparator::ForwardSlash,
            strict_charset: false,
        };

        // Write to a growable buffer so the underlying Vec can expand if the
        // estimate is too low.  Pre-allocate a generous lower bound.
        let total_data: usize = self.entries.iter().map(|e| e.data.len()).sum();
        let estimate = total_data
            .saturating_add(total_data / 8)
            .saturating_add(65536 * 16)
            .saturating_add(self.entries.len() * 512);
        // Pre-allocate with a conservative upper bound.  ISO 9660 directory
        // records are ~34-72 bytes each, plus path tables and descriptors.
        let buf = vec![0u8; estimate];
        let mut cursor = std::io::Cursor::new(buf);
        IsoImageWriter::format_new(&mut cursor, input_files, format_options).map_err(|err| {
            GeeZipError::format(format!("formatting ISO image: {err}"), ArchiveFormat::Iso)
        })?;
        let iso_data = cursor.into_inner();
        let byte_count = iso_data.len() as u64;

        writer
            .write_all(&iso_data)
            .map_err(|err| GeeZipError::io(err, "writing ISO image"))?;
        writer
            .flush()
            .map_err(|err| GeeZipError::io(err, "flushing ISO image"))?;

        Ok((byte_count, writer))
    }
}

impl<W: std::io::Write + Send> ArchiveWriter for IsoWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(
        &mut self,
        path: &std::path::Path,
        reader: &mut dyn std::io::Read,
    ) -> GeeZipResult<()> {
        let iso_path = validate_iso_entry_path(path)?;

        // Pre-allocate with a 64 KiB base — most ISO entries are small enough
        // to fit, and the Vec will grow geometrically for larger files anyway.
        let mut data = Vec::with_capacity(65536);
        let mut chunk = [0u8; 65536];
        loop {
            let n = reader.read(&mut chunk).map_err(|e| {
                GeeZipError::io(e, format!("reading data for ISO entry '{iso_path}'"))
            })?;
            if n == 0 {
                break;
            }
            if data.len() as u64 + n as u64 > ISO_MAX_ENTRY_SIZE {
                return Err(GeeZipError::format(
                    format!("ISO entry '{}' exceeds 4 GiB size limit", iso_path),
                    ArchiveFormat::Iso,
                ));
            }
            data.extend_from_slice(&chunk[..n]);
        }

        self.entries.push(BufferedIsoEntry {
            path: iso_path,
            data,
            is_dir: false,
        });
        Ok(())
    }

    fn add_directory(&mut self, path: &std::path::Path) -> GeeZipResult<()> {
        let iso_path = validate_iso_entry_path(path)?;
        self.entries.push(BufferedIsoEntry {
            path: iso_path,
            data: Vec::new(),
            is_dir: true,
        });
        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let (bytes, _writer) = (*self).finalize()?;
        Ok(bytes)
    }
}

fn convert_iso_io_error(err: std::io::Error, context: impl Into<String>) -> GeeZipError {
    match err.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            GeeZipError::format(format!("{}: {err}", context.into()), ArchiveFormat::Iso)
        }
        _ => GeeZipError::io(err, context),
    }
}

fn convert_iso_error(err: isomage::Error, context: impl Into<String>) -> GeeZipError {
    let context = context.into();
    match err.downcast::<std::io::Error>() {
        Ok(io_err) => convert_iso_io_error(*io_err, context),
        Err(err) => GeeZipError::format(format!("{context}: {err}"), ArchiveFormat::Iso),
    }
}

fn push_entries(node: &TreeNode, parent: &str, out: &mut Vec<Entry>) {
    let path = if parent.is_empty() {
        node.name.clone()
    } else if node.name.is_empty() {
        parent.to_owned()
    } else {
        format!("{parent}/{}", node.name)
    };

    if !path.is_empty() {
        out.push(Entry {
            path: path.clone(),
            size: if node.is_directory { 0 } else { node.size },
            compressed_size: if node.is_directory { 0 } else { node.size },
            crc32: None,
            modified: None,
            is_dir: node.is_directory,
        });
    }

    let next_parent = if path.is_empty() { parent } else { &path };
    for child in &node.children {
        push_entries(child, next_parent, out);
    }
}

fn find_node_by_path<'a>(node: &'a TreeNode, path: &str) -> Option<&'a TreeNode> {
    fn recurse<'a>(node: &'a TreeNode, segments: &[&str]) -> Option<&'a TreeNode> {
        match segments {
            [] => Some(node),
            [head, tail @ ..] => {
                let child = node.children.iter().find(|child| child.name == *head)?;
                recurse(child, tail)
            }
        }
    }

    let segments: Vec<_> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    recurse(node, &segments)
}

impl<R: Read + Seek + Send> ArchiveReader for IsoReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.ensure_parsed()?;
        let root = self.root.as_ref().expect("ISO tree should be cached");
        let mut entries = Vec::new();
        for child in &root.children {
            push_entries(child, "", &mut entries);
        }
        Ok(entries)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.ensure_parsed()?;

        let Self { inner, root, .. } = self;
        let root = root.as_ref().expect("ISO tree should be cached");
        let node =
            find_node_by_path(root, &entry.path).ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;

        if node.is_directory {
            return if entry.is_dir {
                Ok(0)
            } else {
                Err(GeeZipError::format(
                    format!("ISO entry '{}' resolves to a directory", entry.path),
                    ArchiveFormat::Iso,
                ))
            };
        }

        if entry.is_dir {
            return Err(GeeZipError::format(
                format!("ISO entry '{}' resolves to a file", entry.path),
                ArchiveFormat::Iso,
            ));
        }

        let mut counted = CountWriter {
            inner: writer,
            count: 0,
        };
        isomage::cat_node(inner, node, &mut counted).map_err(|err| {
            convert_iso_error(err, format!("extracting ISO entry '{}'", entry.path))
        })?;
        Ok(counted.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SECTOR_SIZE: usize = 2048;
    const PVD_SECTOR: u32 = 16;
    const TERMINATOR_SECTOR: u32 = 17;
    const L_PATH_TABLE_SECTOR: u32 = 18;
    const M_PATH_TABLE_SECTOR: u32 = 19;
    const ROOT_DIR_SECTOR: u32 = 20;
    const SUBDIR_SECTOR: u32 = 21;
    const HELLO_SECTOR: u32 = 22;
    const NESTED_SECTOR: u32 = 23;
    const TOTAL_SECTORS: u32 = 24;

    fn both_endian_u16(value: u16) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..2].copy_from_slice(&value.to_le_bytes());
        out[2..].copy_from_slice(&value.to_be_bytes());
        out
    }

    fn both_endian_u32(value: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&value.to_le_bytes());
        out[4..].copy_from_slice(&value.to_be_bytes());
        out
    }

    fn directory_record(name: &[u8], extent: u32, size: u32, is_dir: bool) -> Vec<u8> {
        let needs_padding = name.len().is_multiple_of(2);
        let record_len = 33 + name.len() + usize::from(needs_padding);
        let mut record = vec![0u8; record_len];
        record[0] = record_len as u8;
        record[1] = 0;
        record[2..10].copy_from_slice(&both_endian_u32(extent));
        record[10..18].copy_from_slice(&both_endian_u32(size));
        record[18..25].copy_from_slice(&[124, 1, 2, 3, 4, 5, 0]);
        record[25] = if is_dir { 0x02 } else { 0x00 };
        record[26] = 0;
        record[27] = 0;
        record[28..32].copy_from_slice(&both_endian_u16(1));
        record[32] = name.len() as u8;
        record[33..33 + name.len()].copy_from_slice(name);
        record
    }

    fn path_table_entry_le(name: &[u8], extent: u32, parent: u16) -> Vec<u8> {
        let needs_padding = name.len() % 2 == 1;
        let mut entry = vec![0u8; 8 + name.len() + usize::from(needs_padding)];
        entry[0] = name.len() as u8;
        entry[1] = 0;
        entry[2..6].copy_from_slice(&extent.to_le_bytes());
        entry[6..8].copy_from_slice(&parent.to_le_bytes());
        entry[8..8 + name.len()].copy_from_slice(name);
        entry
    }

    fn path_table_entry_be(name: &[u8], extent: u32, parent: u16) -> Vec<u8> {
        let needs_padding = name.len() % 2 == 1;
        let mut entry = vec![0u8; 8 + name.len() + usize::from(needs_padding)];
        entry[0] = name.len() as u8;
        entry[1] = 0;
        entry[2..6].copy_from_slice(&extent.to_be_bytes());
        entry[6..8].copy_from_slice(&parent.to_be_bytes());
        entry[8..8 + name.len()].copy_from_slice(name);
        entry
    }

    fn write_sector(image: &mut [u8], sector: u32, data: &[u8]) {
        let start = sector as usize * SECTOR_SIZE;
        let end = start + data.len();
        image[start..end].copy_from_slice(data);
    }

    fn write_padded_ascii(field: &mut [u8], text: &str) {
        let bytes = text.as_bytes();
        let len = bytes.len().min(field.len());
        field[..len].copy_from_slice(&bytes[..len]);
    }

    fn build_test_iso() -> Vec<u8> {
        let hello = b"hello iso\n";
        let nested = b"nested iso\n";

        let mut le_path_table = Vec::new();
        le_path_table.extend_from_slice(&path_table_entry_le(&[0], ROOT_DIR_SECTOR, 1));
        le_path_table.extend_from_slice(&path_table_entry_le(b"DIR", SUBDIR_SECTOR, 1));

        let mut be_path_table = Vec::new();
        be_path_table.extend_from_slice(&path_table_entry_be(&[0], ROOT_DIR_SECTOR, 1));
        be_path_table.extend_from_slice(&path_table_entry_be(b"DIR", SUBDIR_SECTOR, 1));

        let mut pvd = [0u8; SECTOR_SIZE];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        write_padded_ascii(&mut pvd[8..40], "GEEZIPX");
        write_padded_ascii(&mut pvd[40..72], "ISO TEST");
        pvd[80..88].copy_from_slice(&both_endian_u32(TOTAL_SECTORS));
        pvd[120..124].copy_from_slice(&both_endian_u16(1));
        pvd[124..128].copy_from_slice(&both_endian_u16(1));
        pvd[128..132].copy_from_slice(&both_endian_u16(SECTOR_SIZE as u16));
        pvd[132..140].copy_from_slice(&both_endian_u32(le_path_table.len() as u32));
        pvd[140..144].copy_from_slice(&L_PATH_TABLE_SECTOR.to_le_bytes());
        pvd[144..148].copy_from_slice(&0u32.to_le_bytes());
        pvd[148..152].copy_from_slice(&M_PATH_TABLE_SECTOR.to_be_bytes());
        pvd[152..156].copy_from_slice(&0u32.to_be_bytes());
        let root_record = directory_record(&[0], ROOT_DIR_SECTOR, SECTOR_SIZE as u32, true);
        pvd[156..156 + root_record.len()].copy_from_slice(&root_record);
        pvd[881] = 1;

        let mut terminator = [0u8; SECTOR_SIZE];
        terminator[0] = 255;
        terminator[1..6].copy_from_slice(b"CD001");
        terminator[6] = 1;

        let mut root_dir = Vec::new();
        root_dir.extend_from_slice(&directory_record(
            &[0],
            ROOT_DIR_SECTOR,
            SECTOR_SIZE as u32,
            true,
        ));
        root_dir.extend_from_slice(&directory_record(
            &[1],
            ROOT_DIR_SECTOR,
            SECTOR_SIZE as u32,
            true,
        ));
        root_dir.extend_from_slice(&directory_record(
            b"DIR",
            SUBDIR_SECTOR,
            SECTOR_SIZE as u32,
            true,
        ));
        root_dir.extend_from_slice(&directory_record(
            b"HELLO.TXT;1",
            HELLO_SECTOR,
            hello.len() as u32,
            false,
        ));

        let mut subdir = Vec::new();
        subdir.extend_from_slice(&directory_record(
            &[0],
            SUBDIR_SECTOR,
            SECTOR_SIZE as u32,
            true,
        ));
        subdir.extend_from_slice(&directory_record(
            &[1],
            ROOT_DIR_SECTOR,
            SECTOR_SIZE as u32,
            true,
        ));
        subdir.extend_from_slice(&directory_record(
            b"NEST.TXT;1",
            NESTED_SECTOR,
            nested.len() as u32,
            false,
        ));

        let mut image = vec![0u8; TOTAL_SECTORS as usize * SECTOR_SIZE];
        write_sector(&mut image, PVD_SECTOR, &pvd);
        write_sector(&mut image, TERMINATOR_SECTOR, &terminator);
        write_sector(&mut image, L_PATH_TABLE_SECTOR, &le_path_table);
        write_sector(&mut image, M_PATH_TABLE_SECTOR, &be_path_table);
        write_sector(&mut image, ROOT_DIR_SECTOR, &root_dir);
        write_sector(&mut image, SUBDIR_SECTOR, &subdir);
        write_sector(&mut image, HELLO_SECTOR, hello);
        write_sector(&mut image, NESTED_SECTOR, nested);
        image
    }

    fn normalize_test_path(path: &str) -> String {
        path.trim_end_matches('/')
            .replace(";1", "")
            .to_ascii_uppercase()
    }

    #[test]
    fn iso_entries_and_extract_file() {
        let mut reader = IsoReader::from_buf(build_test_iso());
        let entries = reader.entries().expect("ISO entries should parse");
        assert_eq!(entries.len(), 3);

        let hello = entries
            .iter()
            .find(|entry| normalize_test_path(&entry.path) == "HELLO.TXT")
            .expect("hello entry should exist")
            .clone();
        let nested = entries
            .iter()
            .find(|entry| normalize_test_path(&entry.path) == "DIR/NEST.TXT")
            .expect("nested entry should exist")
            .clone();
        let dir = entries
            .iter()
            .find(|entry| normalize_test_path(&entry.path) == "DIR")
            .expect("directory entry should exist")
            .clone();

        assert!(!hello.is_dir);
        assert!(!nested.is_dir);
        assert!(dir.is_dir);

        let mut out = Vec::new();
        let bytes = reader
            .extract(&hello, &mut out)
            .expect("hello entry should extract");
        assert_eq!(bytes, b"hello iso\n".len() as u64);
        assert_eq!(out, b"hello iso\n");

        let dir_bytes = reader
            .extract(&dir, &mut Vec::new())
            .expect("dir extract should work");
        assert_eq!(dir_bytes, 0);
    }

    #[test]
    fn iso_extract_all_nested_paths() {
        let mut reader = IsoReader::from_buf(build_test_iso());
        let temp = tempfile::TempDir::new().unwrap();
        let dest = temp.path().join("out");
        let report = reader
            .extract_all(&dest, true)
            .expect("extract_all should work");

        assert_eq!(report.files_extracted, 3);
        assert_eq!(report.files_skipped, 0);
        assert!(dest.join("DIR").is_dir());
        assert_eq!(
            std::fs::read(dest.join("HELLO.TXT")).unwrap(),
            b"hello iso\n"
        );
        assert_eq!(
            std::fs::read(dest.join("DIR").join("NEST.TXT")).unwrap(),
            b"nested iso\n"
        );
    }

    #[test]
    fn iso_invalid_image_fails_cleanly() {
        let mut reader = IsoReader::from_buf(b"not an iso".to_vec());
        let err = reader.entries().expect_err("invalid ISO should fail");
        assert!(err.to_string().contains("iso") || err.to_string().contains("ISO"));
    }

    #[test]
    fn iso_trait_object_is_supported() {
        let mut reader: Box<dyn ArchiveReader> = Box::new(IsoReader::from_buf(build_test_iso()));
        let entries = reader
            .entries()
            .expect("trait-object ISO reader should work");
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn iso_missing_entry_reports_not_found() {
        let mut reader = IsoReader::from_buf(build_test_iso());
        let missing = Entry {
            path: PathBuf::from("missing.txt").to_string_lossy().into_owned(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        };
        let err = reader
            .extract(&missing, &mut Vec::new())
            .expect_err("missing entry should error");
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    // ---------------------------------------------------------------
    // IsoWriter tests
    // ---------------------------------------------------------------

    #[test]
    fn iso_writer_single_file_roundtrip() {
        let content = b"hello iso writer";
        let mut writer = IsoWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .add_entry_from_reader(
                std::path::Path::new("HELLO.TXT"),
                &mut std::io::Cursor::new(content.to_vec()),
            )
            .expect("file should be added");

        let (bytes_written, cursor) = writer.finalize().expect("writer should finalize");
        let archive = cursor.into_inner();
        assert_eq!(bytes_written, archive.len() as u64);

        let mut reader = IsoReader::from_buf(archive);
        let entries = reader.entries().expect("entries should load");
        assert!(!entries.is_empty(), "should have at least one entry");

        let hello = entries
            .iter()
            .find(|e| normalize_test_path(&e.path) == "HELLO.TXT")
            .expect("hello entry should exist");
        assert!(!hello.is_dir);
        assert_eq!(hello.size, content.len() as u64);

        let mut out = Vec::new();
        let bytes = reader
            .extract(hello, &mut out)
            .expect("extract should work");
        assert_eq!(bytes, content.len() as u64);
        assert_eq!(out, content);
    }

    #[test]
    fn iso_writer_roundtrip_with_directories() {
        let mut writer = IsoWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .add_directory(std::path::Path::new("SUBDIR"))
            .expect("dir should be added");
        writer
            .add_entry_from_reader(
                std::path::Path::new("SUBDIR/FILE.TXT"),
                &mut std::io::Cursor::new(b"nested content"),
            )
            .expect("nested file should be added");

        let (bytes_written, cursor) = writer.finalize().expect("writer should finalize");
        let archive = cursor.into_inner();
        assert!(bytes_written > 0);

        let mut reader = IsoReader::from_buf(archive);
        let entries = reader.entries().expect("entries should load");

        assert!(
            entries
                .iter()
                .any(|e| normalize_test_path(&e.path) == "SUBDIR" && e.is_dir),
            "expected SUBDIR directory: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| normalize_test_path(&e.path) == "SUBDIR/FILE.TXT"),
            "expected SUBDIR/FILE.TXT: {entries:?}"
        );
    }

    #[test]
    fn iso_writer_trait_object_finish_returns_byte_count() {
        let temp = tempfile::TempDir::new().unwrap();
        let archive_path = temp.path().join("writer.iso");
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut writer: Box<dyn ArchiveWriter> = Box::new(IsoWriter::new(file));
        writer
            .add_entry_from_reader(
                std::path::Path::new("DATA.BIN"),
                &mut std::io::Cursor::new(b"payload"),
            )
            .expect("file should be added");

        let bytes_written = writer.finish().expect("trait writer should finish");
        assert_eq!(
            bytes_written,
            std::fs::metadata(&archive_path).unwrap().len(),
            "finish() should report the final archive size"
        );
    }

    #[test]
    fn iso_writer_rejects_absolute_paths() {
        let mut writer = IsoWriter::new(std::io::Cursor::new(Vec::new()));
        let err = writer
            .add_entry_from_reader(
                std::path::Path::new("/etc/passwd"),
                &mut std::io::Cursor::new(b"bad"),
            )
            .expect_err("absolute path should be rejected");
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn iso_writer_rejects_traversal_paths() {
        let mut writer = IsoWriter::new(std::io::Cursor::new(Vec::new()));
        let err = writer
            .add_entry_from_reader(
                std::path::Path::new("../evil.txt"),
                &mut std::io::Cursor::new(b"bad"),
            )
            .expect_err("traversal path should be rejected");
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn iso_writer_rejects_invalid_path_before_reading_payload() {
        struct PanicReader;
        impl std::io::Read for PanicReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                panic!("should not read payload for invalid path");
            }
        }

        let mut writer = IsoWriter::new(std::io::Cursor::new(Vec::new()));
        let err = writer
            .add_entry_from_reader(std::path::Path::new("../evil.txt"), &mut PanicReader)
            .expect_err("invalid path should fail before reading payload");
        assert!(err.to_string().contains("invalid"));
    }
}
